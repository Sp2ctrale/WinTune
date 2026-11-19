//! File-association engine: writes custom ProgIDs under `HKLM\SOFTWARE\Classes`,
//! cleans per-user overrides, and undoes everything from lossless snapshots.
//!
//! Safety model of [`apply_extensions`]:
//! 1. validate every rule and install its icon — nothing written yet;
//! 2. snapshot every key about to change, persist the snapshots to
//!    `backups.json` and export a `.reg` safety net — abort if either fails;
//! 3. only then write to the registry, reporting every failure.

use crate::clock;
use crate::detector::get_loaded_user_sids;
use crate::models::{
    app_data_dir, BackupStore, DoubleClickMode, ExtensionBackup, ExtensionConfig,
    RegistryKeySnapshot, UserOverrideSnapshot,
};
use crate::regsnap::{self, RegExportEntry};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const USER_CLASSES: &str = "Software\\Classes";
const USER_FILE_EXTS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts";

/// Where the association keys live. Production uses [`AssocTarget::system`];
/// tests point every root at a throw-away key under HKCU.
pub struct AssocTarget {
    pub machine_root: RegKey,
    pub machine_root_name: String,
    /// Path of the machine-wide `Classes` key below `machine_root`.
    pub machine_classes: String,
    pub users_root: RegKey,
    pub users_root_name: String,
    /// User hives (SIDs) below `users_root` whose overrides are managed.
    pub user_hives: Vec<String>,
}

impl AssocTarget {
    /// The real machine: `HKLM\SOFTWARE\Classes` + every loaded user hive.
    pub fn system() -> Self {
        Self {
            machine_root: RegKey::predef(HKEY_LOCAL_MACHINE),
            machine_root_name: "HKEY_LOCAL_MACHINE".into(),
            machine_classes: "SOFTWARE\\Classes".into(),
            users_root: RegKey::predef(HKEY_USERS),
            users_root_name: "HKEY_USERS".into(),
            user_hives: get_loaded_user_sids(),
        }
    }

    fn class_path(&self, name: &str) -> String {
        format!("{}\\{}", self.machine_classes, name)
    }

    fn user_classes_path(&self, hive: &str, ext: &str) -> String {
        format!("{}\\{}\\{}", hive, USER_CLASSES, ext)
    }

    fn user_file_exts_path(&self, hive: &str, ext: &str) -> String {
        format!("{}\\{}\\{}", hive, USER_FILE_EXTS, ext)
    }
}

pub struct ApplyOptions<'a> {
    pub editor_command: String,
    /// Shown in the "Edit with …" context-menu entry of script types.
    pub editor_label: String,
    pub wsl_available: bool,
    /// Installs the icon of an extension and returns its file path.
    pub resolve_icon: &'a dyn Fn(&ExtensionConfig) -> Result<String, String>,
    pub backup_store_path: PathBuf,
    /// Folder receiving a `.reg` safety-net export before every apply.
    pub reg_backup_dir: PathBuf,
    /// `(done, total, label)`: called as each extension is snapshotted, then
    /// written (`total` = 2 × the number of valid rules).
    pub progress: &'a dyn Fn(usize, usize, &str),
}

#[derive(Debug, Default)]
pub struct OperationReport {
    pub succeeded: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub reg_backup_file: Option<PathBuf>,
}

pub fn default_backup_store_path() -> PathBuf {
    app_data_dir().join("backups.json")
}

pub fn default_reg_backup_dir() -> PathBuf {
    app_data_dir().join("reg-backups")
}

/// A missing file is an empty store. A corrupt one is an error: returning an
/// empty store would make the next save erase every recorded snapshot.
pub fn load_backup_store(path: &Path) -> Result<BackupStore, String> {
    match fs::read_to_string(path) {
        Ok(data) => serde_json::from_str(&data).map_err(|e| {
            format!(
                "Backup file {} is unreadable ({}). It was left untouched: fix or move it before applying changes.",
                path.display(),
                e
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BackupStore::default()),
        Err(e) => Err(format!("Cannot read {}: {}", path.display(), e)),
    }
}

/// Atomic save (write to a temp file, then rename over the old one).
pub fn save_backup_store(store: &BackupStore, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| format!("Cannot replace {}: {}", path.display(), e))
}

const FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

fn has_forbidden_chars(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_whitespace() || c.is_control() || FORBIDDEN_CHARS.contains(&c))
}

/// Rejects values that would write to an unexpected registry location.
pub fn validate_extension(cfg: &ExtensionConfig) -> Result<(), String> {
    let ext = &cfg.extension;
    if !ext.starts_with('.') || ext.trim_matches('.').is_empty() || has_forbidden_chars(ext) {
        return Err(format!(
            "invalid extension '{}' (expected e.g. '.rs', without spaces or path characters)",
            ext
        ));
    }
    let pid = &cfg.prog_id;
    if pid.trim().is_empty() || pid.starts_with('.') || has_forbidden_chars(pid) {
        return Err(format!(
            "invalid ProgID '{}' (expected e.g. 'CustomRustFile', without spaces or path characters)",
            pid
        ));
    }
    Ok(())
}

/// Everything that must be fixed before applying `rules`: invalid values
/// (see [`validate_extension`]) and extensions listed more than once
/// (case-insensitive). Returns `(rule index, message)`, in index order.
pub fn find_rule_problems(rules: &[ExtensionConfig]) -> Vec<(usize, String)> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for rule in rules {
        *seen.entry(rule.extension.to_lowercase()).or_default() += 1;
    }
    rules
        .iter()
        .enumerate()
        .filter_map(|(i, rule)| {
            if let Err(e) = validate_extension(rule) {
                return Some((i, e));
            }
            let count = seen
                .get(&rule.extension.to_lowercase())
                .copied()
                .unwrap_or(0);
            (count > 1).then(|| (i, format!("{} is listed more than once", rule.extension)))
        })
        .collect()
}

/// Old backups (WinTune <= 0.2.0) did not snapshot the custom ProgID: it was
/// always created from scratch, so its original state is "absent".
fn migrate_legacy_backup(backup: &mut ExtensionBackup) {
    if backup.prog_id_snapshots.is_empty() && !backup.custom_prog_id.is_empty() {
        backup.prog_id_snapshots.insert(
            backup.custom_prog_id.clone(),
            RegistryKeySnapshot::default(),
        );
    }
}

struct Prepared<'c> {
    cfg: &'c ExtensionConfig,
    icon_path: String,
}

type ExportItem = (String, String, RegistryKeySnapshot);

/// Phase 1 for one extension: install its icon, record the pre-WinTune state of
/// every key it will touch (first time only), and collect the *current* state
/// for the `.reg` safety net. Writes nothing to the registry.
fn prepare_extension(
    target: &AssocTarget,
    store: &mut BackupStore,
    cfg: &ExtensionConfig,
    opts: &ApplyOptions,
    now: &str,
    export: &mut Vec<ExportItem>,
) -> Result<String, String> {
    let icon_path = (opts.resolve_icon)(cfg)?;

    let m_name = &target.machine_root_name;
    let u_name = &target.users_root_name;
    let mut items: Vec<ExportItem> = Vec::new();

    let ext_path = target.class_path(&cfg.extension);
    let current_ext = regsnap::snapshot_key(&target.machine_root, &ext_path)?;

    let mut backup = match store.backups.get(&cfg.extension) {
        Some(existing) => existing.clone(),
        None => ExtensionBackup {
            extension: cfg.extension.clone(),
            custom_prog_id: String::new(),
            icon_installed_path: String::new(),
            timestamp: now.to_string(),
            last_applied: None,
            hklm_extension_key: current_ext.clone(),
            prog_id_snapshots: BTreeMap::new(),
            user_overrides: Vec::new(),
        },
    };
    migrate_legacy_backup(&mut backup);
    items.push((m_name.clone(), ext_path, current_ext));

    let mut prog_ids: Vec<String> = backup.prog_id_snapshots.keys().cloned().collect();
    if !prog_ids.contains(&cfg.prog_id) {
        prog_ids.push(cfg.prog_id.clone());
    }
    for pid in prog_ids {
        let path = target.class_path(&pid);
        let current = regsnap::snapshot_key(&target.machine_root, &path)?;
        backup
            .prog_id_snapshots
            .entry(pid)
            .or_insert_with(|| current.clone());
        items.push((m_name.clone(), path, current));
    }

    for hive in &target.user_hives {
        let cls_path = target.user_classes_path(hive, &cfg.extension);
        let fe_path = target.user_file_exts_path(hive, &cfg.extension);
        let cls = regsnap::snapshot_key(&target.users_root, &cls_path)?;
        let fe = regsnap::snapshot_key(&target.users_root, &fe_path)?;
        if !backup.user_overrides.iter().any(|u| &u.sid == hive) {
            backup.user_overrides.push(UserOverrideSnapshot {
                sid: hive.clone(),
                user_classes_ext: cls.clone(),
                user_file_exts: fe.clone(),
            });
        }
        items.push((u_name.clone(), cls_path, cls));
        items.push((u_name.clone(), fe_path, fe));
    }

    store.backups.insert(cfg.extension.clone(), backup);
    export.extend(items);
    Ok(icon_path)
}

fn unique_reg_file(dir: &Path, stem: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{}.reg", stem));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{}-{}.reg", stem, n));
        n += 1;
    }
    candidate
}

fn create_key(root: &RegKey, path: &str) -> Result<RegKey, String> {
    root.create_subkey(path)
        .map(|(k, _)| k)
        .map_err(|e| format!("cannot create {}: {}", path, e))
}

fn set_sz(key: &RegKey, path: &str, name: &str, value: &str) -> Result<(), String> {
    key.set_value(name, &value)
        .map_err(|e| format!("cannot write {}\\{}: {}", path, name, e))
}

fn set_expand_sz(key: &RegKey, path: &str, name: &str, value: &str) -> Result<(), String> {
    let bytes = value
        .encode_utf16()
        .chain(Some(0))
        .flat_map(|w| w.to_le_bytes())
        .collect();
    key.set_raw_value(
        name,
        &RegValue {
            bytes,
            vtype: REG_EXPAND_SZ,
        },
    )
    .map_err(|e| format!("cannot write {}\\{}: {}", path, name, e))
}

fn set_command(root: &RegKey, path: &str, command: &str, expand: bool) -> Result<(), String> {
    let key = create_key(root, path)?;
    if expand {
        set_expand_sz(&key, path, "", command)
    } else {
        set_sz(&key, path, "", command)
    }
}

/// The double-click command of a script rule, or `None` when the file opens
/// in the editor (editor mode, no launcher, or `.sh` without WSL).
fn script_launcher(cfg: &ExtensionConfig, wsl_available: bool) -> Option<&str> {
    let launcher = cfg.launcher_command.as_deref()?;
    let wsl_missing = cfg.prog_id == "CustomShellFile" && !wsl_available;
    (cfg.mode == DoubleClickMode::ExecuteScript && !wsl_missing).then_some(launcher)
}

/// Live state of a rule in the registry, compared with what applying the
/// rule as currently configured would write.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleStatus {
    /// WinTune never applied it and the extension does not use its ProgID.
    NotApplied,
    /// The registry matches the rule.
    Applied,
    /// Applied, but something differs now (changed by Windows, another app,
    /// a per-user override, or the rule was edited since). Reasons included.
    Drifted(Vec<String>),
}

/// Rule-independent inputs of [`rule_status`].
pub struct StatusContext {
    pub editor_command: String,
    pub wsl_available: bool,
    /// Folder icons are installed into (`<folder>\<icon_name>.ico`).
    pub icon_dir: String,
}

/// Read-only: never writes to the registry.
pub fn rule_status(
    target: &AssocTarget,
    cfg: &ExtensionConfig,
    backup: Option<&ExtensionBackup>,
    ctx: &StatusContext,
) -> RuleStatus {
    let m = &target.machine_root;
    let ext = &cfg.extension;
    let current = read_string(m, &target.class_path(ext), "");
    if backup.is_none() && current.as_deref() != Some(cfg.prog_id.as_str()) {
        return RuleStatus::NotApplied;
    }

    let mut reasons = Vec::new();
    match current.as_deref() {
        Some(p) if p == cfg.prog_id => {}
        Some(p) if !p.is_empty() => reasons.push(format!(
            "{} now opens with {} instead of {}",
            ext, p, cfg.prog_id
        )),
        _ => reasons.push(format!(
            "{} has no default ProgID any more (expected {})",
            ext, cfg.prog_id
        )),
    }

    let prog_path = target.class_path(&cfg.prog_id);
    if m.open_subkey_with_flags(&prog_path, KEY_READ).is_err() {
        reasons.push(format!("ProgID {} is missing", cfg.prog_id));
    } else {
        let icon = Path::new(&ctx.icon_dir)
            .join(format!("{}.ico", cfg.icon_name))
            .to_string_lossy()
            .into_owned();
        let open_command = script_launcher(cfg, ctx.wsl_available).unwrap_or(&ctx.editor_command);
        let expected = [
            (prog_path.clone(), "description", cfg.description.as_str()),
            (format!("{}\\DefaultIcon", prog_path), "icon", icon.as_str()),
            (
                format!("{}\\shell\\open\\command", prog_path),
                "open command",
                open_command,
            ),
        ];
        for (path, label, want) in expected {
            let found = read_string(m, &path, "").unwrap_or_default();
            if found != want {
                reasons.push(format!(
                    "{} differs: expected \"{}\", found \"{}\"",
                    label, want, found
                ));
            }
        }
    }

    // Per-user settings win over HKLM in Explorer.
    let u = &target.users_root;
    for hive in &target.user_hives {
        let user = hive.rsplit('\\').next().unwrap_or(hive);
        for choice in ["UserChoice", "UserChoiceLatest"] {
            let path = format!("{}\\{}", target.user_file_exts_path(hive, ext), choice);
            if let Some(p) = read_string(u, &path, "ProgId").filter(|p| p != &cfg.prog_id) {
                reasons.push(format!(
                    "Explorer {} of user {} opens {} with {}",
                    choice, user, ext, p
                ));
            }
        }
        let classes = target.user_classes_path(hive, ext);
        if let Some(p) = read_string(u, &classes, "").filter(|p| !p.is_empty() && p != &cfg.prog_id)
        {
            reasons.push(format!(
                "a per-user override of user {} points {} to {}",
                user, ext, p
            ));
        }
    }

    if reasons.is_empty() {
        RuleStatus::Applied
    } else {
        RuleStatus::Drifted(reasons)
    }
}

fn read_string(root: &RegKey, path: &str, name: &str) -> Option<String> {
    root.open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value::<String, _>(name)
        .ok()
}

/// Phase 3 for one extension: the actual registry writes.
fn write_extension(
    target: &AssocTarget,
    backup: &ExtensionBackup,
    cfg: &ExtensionConfig,
    icon_path: &str,
    opts: &ApplyOptions,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let m = &target.machine_root;
    let ext = &cfg.extension;

    for hive in &target.user_hives {
        if cfg.reset_user_choice {
            if let Err(e) =
                regsnap::delete_tree(&target.users_root, &target.user_file_exts_path(hive, ext))
            {
                warnings.push(format!("{}: Explorer UserChoice not reset ({})", ext, e));
            }
        }
        if cfg.reset_existing {
            if let Err(e) =
                regsnap::delete_tree(&target.users_root, &target.user_classes_path(hive, ext))
            {
                warnings.push(format!("{}: per-user override not removed ({})", ext, e));
            }
        }
    }

    // ProgIDs written by an earlier apply under another name go back to their
    // pre-WinTune state, so a rename leaves no orphan key behind.
    for (pid, snap) in backup
        .prog_id_snapshots
        .iter()
        .filter(|(p, _)| *p != &cfg.prog_id)
    {
        regsnap::restore_key(m, &target.class_path(pid), snap)?;
    }

    let prog_path = target.class_path(&cfg.prog_id);
    regsnap::delete_tree(m, &prog_path)?;
    let prog = create_key(m, &prog_path)?;
    set_sz(&prog, &prog_path, "", &cfg.description)?;

    let icon_key_path = format!("{}\\DefaultIcon", prog_path);
    set_sz(
        &create_key(m, &icon_key_path)?,
        &icon_key_path,
        "",
        icon_path,
    )?;

    let open_cmd = format!("{}\\shell\\open\\command", prog_path);
    match script_launcher(cfg, opts.wsl_available) {
        Some(launcher) => {
            set_command(m, &open_cmd, launcher, false)?;
            let edit_path = format!("{}\\shell\\edit", prog_path);
            let label = format!("Edit with {}", opts.editor_label);
            set_sz(&create_key(m, &edit_path)?, &edit_path, "", &label)?;
            set_command(
                m,
                &format!("{}\\command", edit_path),
                &opts.editor_command,
                true,
            )?;
        }
        None => set_command(m, &open_cmd, &opts.editor_command, true)?,
    }

    if let Some(elevated) = cfg
        .elevated_command
        .as_deref()
        .filter(|c| !c.trim().is_empty())
    {
        set_command(
            m,
            &format!("{}\\shell\\runas\\command", prog_path),
            elevated,
            false,
        )?;
    }

    for verb in &cfg.extra_verbs {
        if verb.verb_key.trim().is_empty() || verb.command.trim().is_empty() {
            continue;
        }
        if has_forbidden_chars(&verb.verb_key) {
            warnings.push(format!(
                "{}: context verb '{}' skipped (invalid key name)",
                ext, verb.verb_key
            ));
            continue;
        }
        let verb_path = format!("{}\\shell\\{}", prog_path, verb.verb_key);
        set_sz(
            &create_key(m, &verb_path)?,
            &verb_path,
            "",
            &verb.menu_label,
        )?;
        set_command(m, &format!("{}\\command", verb_path), &verb.command, true)?;
    }

    let ext_path = target.class_path(ext);
    let previous_prog_id: Option<String> = m
        .open_subkey_with_flags(&ext_path, KEY_READ)
        .ok()
        .and_then(|k| k.get_value("").ok())
        .filter(|v: &String| !v.is_empty() && v != &cfg.prog_id);

    if cfg.reset_existing && !cfg.preserve_existing_properties {
        regsnap::delete_tree(m, &ext_path)?;
    }
    let ext_key = create_key(m, &ext_path)?;
    set_sz(&ext_key, &ext_path, "", &cfg.prog_id)?;
    if !cfg.perceived_type.trim().is_empty() {
        set_sz(&ext_key, &ext_path, "PerceivedType", &cfg.perceived_type)?;
    }
    if !cfg.content_type.trim().is_empty() {
        set_sz(&ext_key, &ext_path, "Content Type", &cfg.content_type)?;
    }

    if cfg.preserve_existing_properties {
        if let Some(prev) = previous_prog_id {
            let owp_path = format!("{}\\OpenWithProgids", ext_path);
            create_key(m, &owp_path)?
                .set_raw_value(
                    &prev,
                    &RegValue {
                        bytes: Vec::new(),
                        vtype: REG_NONE,
                    },
                )
                .map_err(|e| format!("cannot write {}\\{}: {}", owp_path, prev, e))?;
        }
    }

    Ok(())
}

/// Applies every enabled rule. See the module docs for the safety model.
/// The caller is expected to notify the shell afterwards.
pub fn apply_extensions(
    target: &AssocTarget,
    store: &mut BackupStore,
    extensions: &[ExtensionConfig],
    opts: &ApplyOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let now = clock::now_local();

    let enabled: Vec<&ExtensionConfig> = extensions.iter().filter(|e| e.enabled).collect();
    let snapshot_steps = enabled.len();

    // Phase 1: validate, install icons, snapshot.
    let mut prepared: Vec<Prepared> = Vec::new();
    let mut export: Vec<ExportItem> = Vec::new();
    for (i, cfg) in enabled.into_iter().enumerate() {
        let result = validate_extension(cfg)
            .and_then(|_| prepare_extension(target, store, cfg, opts, &now, &mut export));
        match result {
            Ok(icon_path) => prepared.push(Prepared { cfg, icon_path }),
            Err(e) => report.errors.push(format!("{}: {}", cfg.extension, e)),
        }
        (opts.progress)(
            i + 1,
            snapshot_steps * 2,
            &format!("Snapshotting {}", cfg.extension),
        );
    }
    if prepared.is_empty() {
        return report;
    }

    // Phase 2: persist snapshots + .reg safety net before any registry write.
    if let Err(e) = save_backup_store(store, &opts.backup_store_path) {
        report
            .errors
            .push(format!("Nothing applied: cannot save snapshots ({})", e));
        return report;
    }
    let entries: Vec<RegExportEntry> = export
        .iter()
        .map(|(root_name, path, snapshot)| RegExportEntry {
            root_name,
            path: path.clone(),
            snapshot,
        })
        .collect();
    let reg_file = unique_reg_file(
        &opts.reg_backup_dir,
        &format!("{}-before-apply", clock::file_stamp()),
    );
    if let Err(e) = regsnap::write_reg_file(&reg_file, &regsnap::export_reg(&entries)) {
        report
            .errors
            .push(format!("Nothing applied: cannot write .reg backup ({})", e));
        return report;
    }
    report.reg_backup_file = Some(reg_file);

    // Phase 3: registry writes.
    let total_steps = snapshot_steps + prepared.len();
    for (i, p) in prepared.into_iter().enumerate() {
        let ext = p.cfg.extension.clone();
        (opts.progress)(snapshot_steps + i, total_steps, &format!("Writing {}", ext));
        let Some(backup) = store.backups.get(&ext).cloned() else {
            continue;
        };
        match write_extension(
            target,
            &backup,
            p.cfg,
            &p.icon_path,
            opts,
            &mut report.warnings,
        ) {
            Ok(()) => report.succeeded.push(ext.clone()),
            Err(e) => report.errors.push(format!(
                "{}: {} (may be partially applied, use Rollback to restore it)",
                ext, e
            )),
        }
        if let Some(b) = store.backups.get_mut(&ext) {
            b.custom_prog_id = p.cfg.prog_id.clone();
            b.icon_installed_path = p.icon_path.clone();
            b.last_applied = Some(now.clone());
        }
        (opts.progress)(
            snapshot_steps + i + 1,
            total_steps,
            &format!("Writing {}", ext),
        );
    }

    if let Err(e) = save_backup_store(store, &opts.backup_store_path) {
        report
            .errors
            .push(format!("Cannot save snapshots after apply: {}", e));
    }
    report
}

/// Tells Explorer that file associations changed so icons refresh immediately.
pub fn notify_shell_associations() {
    // SAFETY: documented usage of SHChangeNotify with no item pointers.
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

/// Puts each extension back to its pre-WinTune state. A snapshot is dropped
/// only once it has been fully restored; on any error it is kept so the
/// rollback can be retried. The caller saves the store and notifies the shell.
pub fn rollback_extensions(
    target: &AssocTarget,
    store: &mut BackupStore,
    extensions: &[String],
    progress: &dyn Fn(usize, usize, &str),
) -> OperationReport {
    let mut report = OperationReport::default();
    let total = extensions.len();
    progress(0, total, "Restoring");

    for (i, ext) in extensions.iter().enumerate() {
        progress(i, total, &format!("Restoring {}", ext));
        let Some(mut backup) = store.backups.get(ext).cloned() else {
            report.errors.push(format!("{}: no snapshot recorded", ext));
            continue;
        };
        migrate_legacy_backup(&mut backup);
        let mut errors = Vec::new();

        for user in &backup.user_overrides {
            if target.users_root.open_subkey(&user.sid).is_err() {
                report.warnings.push(format!(
                    "{}: user hive {} is not loaded, its overrides were not restored",
                    ext, user.sid
                ));
                continue;
            }
            let pairs = [
                (
                    target.user_classes_path(&user.sid, ext),
                    &user.user_classes_ext,
                ),
                (
                    target.user_file_exts_path(&user.sid, ext),
                    &user.user_file_exts,
                ),
            ];
            for (path, snap) in pairs {
                if let Err(e) = regsnap::restore_key(&target.users_root, &path, snap) {
                    errors.push(e);
                }
            }
        }

        for (pid, snap) in &backup.prog_id_snapshots {
            if let Err(e) =
                regsnap::restore_key(&target.machine_root, &target.class_path(pid), snap)
            {
                errors.push(e);
            }
        }

        if let Err(e) = regsnap::restore_key(
            &target.machine_root,
            &target.class_path(ext),
            &backup.hklm_extension_key,
        ) {
            errors.push(e);
        }

        if errors.is_empty() {
            store.backups.remove(ext);
            report.succeeded.push(ext.clone());
        } else {
            report.errors.push(format!(
                "{}: {} (snapshot kept, rollback can be retried)",
                ext,
                errors.join("; ")
            ));
        }
    }

    progress(total, total, "Rollback finished");
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DoubleClickMode, ExtensionConfig, RegistryKeySnapshot};
    use crate::regsnap::snapshot_key;
    use crate::regsnap::test_support::Sandbox;

    const HIVE_A: &str = "S-1-5-21-1000";
    const HIVE_B: &str = "S-1-5-21-2000";
    const USER_CHOICE_REL: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.foo";

    struct Env {
        sb: Sandbox,
        dir: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let sb = Sandbox::new();
            let dir = std::env::temp_dir().join(format!(
                "wintune-assoc-{}",
                sb.path.rsplit('\\').next().unwrap()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { sb, dir }
        }

        fn target(&self, hives: &[&str]) -> AssocTarget {
            AssocTarget {
                machine_root: RegKey::predef(HKEY_CURRENT_USER),
                machine_root_name: "HKEY_CURRENT_USER".into(),
                machine_classes: self.sb.key("Machine\\Classes"),
                users_root: RegKey::predef(HKEY_CURRENT_USER),
                users_root_name: "HKEY_CURRENT_USER".into(),
                user_hives: hives
                    .iter()
                    .map(|h| self.sb.key(&format!("Users\\{}", h)))
                    .collect(),
            }
        }

        fn classes(&self, rel: &str) -> String {
            self.sb.key(&format!("Machine\\Classes\\{}", rel))
        }

        fn user(&self, hive: &str, rel: &str) -> String {
            self.sb.key(&format!("Users\\{}\\{}", hive, rel))
        }

        fn snap(&self, path: &str) -> RegistryKeySnapshot {
            snapshot_key(&self.sb.root, path).unwrap()
        }

        fn exists(&self, path: &str) -> bool {
            self.sb.root.open_subkey(path).is_ok()
        }

        fn default_of(&self, path: &str) -> String {
            self.sb
                .root
                .open_subkey(path)
                .unwrap()
                .get_value("")
                .unwrap()
        }

        fn opts<'a>(
            &self,
            resolve_icon: &'a dyn Fn(&ExtensionConfig) -> Result<String, String>,
        ) -> ApplyOptions<'a> {
            ApplyOptions {
                editor_command: "\"C:\\Editor\\editor.exe\" \"%1\"".into(),
                editor_label: "VS Code".into(),
                wsl_available: false,
                resolve_icon,
                backup_store_path: self.dir.join("backups.json"),
                reg_backup_dir: self.dir.join("reg-backups"),
                progress: &no_progress,
            }
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn icon_ok(_: &ExtensionConfig) -> Result<String, String> {
        Ok("C:\\ProgramData\\ico\\foo.ico".into())
    }

    fn no_progress(_: usize, _: usize, _: &str) {}

    fn with_ext(ext: &str) -> ExtensionConfig {
        let mut c = foo_cfg(&format!("Custom{}File", ext.trim_start_matches('.')));
        c.extension = ext.into();
        c
    }

    fn assert_progress_ends_complete(calls: &[(usize, usize, String)], total: usize) {
        assert!(!calls.is_empty(), "no progress reported");
        assert!(calls.iter().all(|c| c.1 == total), "{:?}", calls);
        assert!(calls.windows(2).all(|w| w[0].0 <= w[1].0), "{:?}", calls);
        assert_eq!(calls.last().unwrap().0, total, "{:?}", calls);
    }

    #[test]
    fn apply_reports_progress_through_snapshot_and_write_phases() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let calls = std::cell::RefCell::new(Vec::new());
        let record = |d: usize, t: usize, l: &str| calls.borrow_mut().push((d, t, l.to_string()));
        let mut opts = env.opts(&icon_ok);
        opts.progress = &record;

        apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[with_ext(".foo"), with_ext(".bar")],
            &opts,
        );

        let calls = calls.into_inner();
        assert_progress_ends_complete(&calls, 4);
        assert!(calls.iter().any(|c| c.2.contains(".bar")), "{:?}", calls);
    }

    #[test]
    fn rollback_reports_progress_per_extension() {
        let env = Env::new();
        let target = env.target(&[HIVE_A]);
        let mut store = BackupStore::default();
        apply_extensions(
            &target,
            &mut store,
            &[with_ext(".foo"), with_ext(".bar")],
            &env.opts(&icon_ok),
        );
        let calls = std::cell::RefCell::new(Vec::new());
        let record = |d: usize, t: usize, l: &str| calls.borrow_mut().push((d, t, l.to_string()));

        rollback_extensions(
            &target,
            &mut store,
            &[".foo".into(), ".bar".into()],
            &record,
        );

        assert_progress_ends_complete(&calls.into_inner(), 2);
    }

    fn foo_cfg(prog_id: &str) -> ExtensionConfig {
        ExtensionConfig {
            enabled: true,
            extension: ".foo".into(),
            prog_id: prog_id.into(),
            description: "Foo File".into(),
            icon_name: "foo".into(),
            reset_existing: true,
            reset_user_choice: true,
            mode: DoubleClickMode::OpenInEditor,
            launcher_command: None,
            elevated_command: None,
            perceived_type: "text".into(),
            content_type: "text/plain".into(),
            preserve_existing_properties: true,
            extra_verbs: Vec::new(),
        }
    }

    /// A realistic "before WinTune" state for `.foo`.
    fn seed_original_state(env: &Env) {
        let root = &env.sb.root;
        let (ext, _) = root.create_subkey(env.classes(".foo")).unwrap();
        ext.set_value("", &"Old.File").unwrap();
        ext.set_value("Content Type", &"text/x-foo").unwrap();
        let (owp, _) = ext.create_subkey("OpenWithProgids").unwrap();
        owp.set_raw_value(
            "Other.App",
            &winreg::RegValue {
                bytes: Vec::new(),
                vtype: REG_NONE,
            },
        )
        .unwrap();
        let (old, _) = root
            .create_subkey(env.classes("Old.File\\shell\\open\\command"))
            .unwrap();
        old.set_value("", &"old.exe \"%1\"").unwrap();
        let (uc, _) = root
            .create_subkey(env.user(HIVE_A, &format!("{}\\UserChoice", USER_CHOICE_REL)))
            .unwrap();
        uc.set_value("ProgId", &"Old.File").unwrap();
        let (ucls, _) = root
            .create_subkey(env.user(HIVE_A, "Software\\Classes\\.foo"))
            .unwrap();
        ucls.set_value("", &"User.Foo").unwrap();
    }

    #[test]
    fn apply_registers_progid_and_points_extension_to_it() {
        let env = Env::new();
        let mut store = BackupStore::default();

        let report = apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, vec![".foo".to_string()]);
        assert_eq!(env.default_of(&env.classes(".foo")), "CustomFooFile");
        assert_eq!(env.default_of(&env.classes("CustomFooFile")), "Foo File");
        assert_eq!(
            env.default_of(&env.classes("CustomFooFile\\DefaultIcon")),
            "C:\\ProgramData\\ico\\foo.ico"
        );
        let cmd = env.snap(&env.classes("CustomFooFile\\shell\\open\\command"));
        assert_eq!(cmd.values.get("").map(|v| v.0), Some(REG_EXPAND_SZ as u32));
        assert_eq!(
            cmd.default_value.as_deref(),
            Some("\"C:\\Editor\\editor.exe\" \"%1\"")
        );
    }

    #[test]
    fn apply_cleans_user_overrides_and_keeps_previous_progid_in_open_with() {
        let env = Env::new();
        seed_original_state(&env);
        let mut store = BackupStore::default();

        apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );

        assert!(!env.exists(&env.user(HIVE_A, USER_CHOICE_REL)));
        assert!(!env.exists(&env.user(HIVE_A, "Software\\Classes\\.foo")));
        let owp = env.snap(&env.classes(".foo\\OpenWithProgids"));
        assert!(owp.values.contains_key("Old.File"));
        assert!(owp.values.contains_key("Other.App"));
    }

    #[test]
    fn rollback_restores_machine_and_user_keys_exactly() {
        let env = Env::new();
        seed_original_state(&env);
        let before_ext = env.snap(&env.classes(".foo"));
        let before_old = env.snap(&env.classes("Old.File"));
        let before_uc = env.snap(&env.user(HIVE_A, USER_CHOICE_REL));
        let before_ucls = env.snap(&env.user(HIVE_A, "Software\\Classes\\.foo"));
        let target = env.target(&[HIVE_A]);
        let mut store = BackupStore::default();
        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );

        let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.snap(&env.classes(".foo")), before_ext);
        assert_eq!(env.snap(&env.classes("Old.File")), before_old);
        assert_eq!(env.snap(&env.user(HIVE_A, USER_CHOICE_REL)), before_uc);
        assert_eq!(
            env.snap(&env.user(HIVE_A, "Software\\Classes\\.foo")),
            before_ucls
        );
        assert!(!env.exists(&env.classes("CustomFooFile")));
        assert!(!store.backups.contains_key(".foo"));
    }

    #[test]
    fn renaming_the_progid_removes_the_previously_written_one() {
        let env = Env::new();
        let target = env.target(&[HIVE_A]);
        let mut store = BackupStore::default();
        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );

        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("RenamedFooFile")],
            &env.opts(&icon_ok),
        );

        assert!(!env.exists(&env.classes("CustomFooFile")));
        assert_eq!(env.default_of(&env.classes(".foo")), "RenamedFooFile");
    }

    #[test]
    fn rollback_after_progid_rename_leaves_no_orphan_progid() {
        let env = Env::new();
        let target = env.target(&[HIVE_A]);
        let mut store = BackupStore::default();
        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );
        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("RenamedFooFile")],
            &env.opts(&icon_ok),
        );

        rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

        assert!(!env.exists(&env.classes("CustomFooFile")));
        assert!(!env.exists(&env.classes("RenamedFooFile")));
        assert!(!env.exists(&env.classes(".foo")));
    }

    #[test]
    fn reusing_an_existing_progid_restores_it_on_rollback() {
        let env = Env::new();
        seed_original_state(&env);
        let before_old = env.snap(&env.classes("Old.File"));
        let target = env.target(&[HIVE_A]);
        let mut store = BackupStore::default();
        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("Old.File")],
            &env.opts(&icon_ok),
        );
        assert_eq!(env.default_of(&env.classes("Old.File")), "Foo File");

        rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

        assert_eq!(env.snap(&env.classes("Old.File")), before_old);
    }

    #[test]
    fn user_hive_seen_only_on_a_later_apply_is_snapshotted_before_cleanup() {
        let env = Env::new();
        let mut store = BackupStore::default();
        apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );
        let (uc, _) = env
            .sb
            .root
            .create_subkey(env.user(HIVE_B, &format!("{}\\UserChoice", USER_CHOICE_REL)))
            .unwrap();
        uc.set_value("ProgId", &"B.File").unwrap();
        let before_b = env.snap(&env.user(HIVE_B, USER_CHOICE_REL));
        let both = env.target(&[HIVE_A, HIVE_B]);

        apply_extensions(
            &both,
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );
        assert!(!env.exists(&env.user(HIVE_B, USER_CHOICE_REL)));
        rollback_extensions(&both, &mut store, &[".foo".to_string()], &no_progress);

        assert_eq!(env.snap(&env.user(HIVE_B, USER_CHOICE_REL)), before_b);
    }

    #[test]
    fn missing_icon_skips_the_extension_and_reports_it() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let no_icon =
            |_: &ExtensionConfig| -> Result<String, String> { Err("icon not found".into()) };

        let report = apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&no_icon),
        );

        assert!(report.succeeded.is_empty());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains(".foo") && e.contains("icon not found")));
        assert!(!env.exists(&env.classes(".foo")));
        assert!(!env.exists(&env.classes("CustomFooFile")));
    }

    #[test]
    fn invalid_extension_or_progid_is_rejected_without_writing() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let mut bad_ext = foo_cfg("CustomFooFile");
        bad_ext.extension = "foo\\..\\bar".into();
        let mut bad_prog = foo_cfg("");
        bad_prog.extension = ".bar".into();

        let report = apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[bad_ext, bad_prog],
            &env.opts(&icon_ok),
        );

        assert_eq!(report.errors.len(), 2, "{:?}", report.errors);
        assert!(!env.exists(&env.classes(".bar")));
        assert!(store.backups.is_empty());
    }

    fn status_ctx() -> StatusContext {
        StatusContext {
            editor_command: "\"C:\\Editor\\editor.exe\" \"%1\"".into(),
            wsl_available: false,
            icon_dir: "C:\\ProgramData\\ico".into(),
        }
    }

    /// Applies `.foo` (→ CustomFooFile) in the sandbox and returns its status context.
    fn applied_foo(env: &Env, store: &mut BackupStore) -> AssocTarget {
        let target = env.target(&[HIVE_A]);
        let report = apply_extensions(
            &target,
            store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        target
    }

    fn reasons(status: RuleStatus) -> Vec<String> {
        match status {
            RuleStatus::Drifted(r) => r,
            other => panic!("expected Drifted, got {:?}", other),
        }
    }

    #[test]
    fn a_rule_never_applied_is_not_applied() {
        let env = Env::new();
        seed_original_state(&env);

        let status = rule_status(
            &env.target(&[HIVE_A]),
            &foo_cfg("CustomFooFile"),
            None,
            &status_ctx(),
        );

        assert_eq!(status, RuleStatus::NotApplied);
    }

    #[test]
    fn a_freshly_applied_rule_is_applied() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let target = applied_foo(&env, &mut store);

        let status = rule_status(
            &target,
            &foo_cfg("CustomFooFile"),
            store.backups.get(".foo"),
            &status_ctx(),
        );

        assert_eq!(status, RuleStatus::Applied);
    }

    #[test]
    fn an_extension_repointed_by_another_app_has_drifted() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let target = applied_foo(&env, &mut store);
        let (k, _) = env.sb.root.create_subkey(env.classes(".foo")).unwrap();
        k.set_value("", &"Other.App").unwrap();

        let r = reasons(rule_status(
            &target,
            &foo_cfg("CustomFooFile"),
            store.backups.get(".foo"),
            &status_ctx(),
        ));

        assert!(r.iter().any(|m| m.contains("Other.App")), "{:?}", r);
    }

    #[test]
    fn an_explorer_user_choice_override_has_drifted() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let target = applied_foo(&env, &mut store);
        let (uc, _) = env
            .sb
            .root
            .create_subkey(env.user(HIVE_A, &format!("{}\\UserChoice", USER_CHOICE_REL)))
            .unwrap();
        uc.set_value("ProgId", &"Other.App").unwrap();

        let r = reasons(rule_status(
            &target,
            &foo_cfg("CustomFooFile"),
            store.backups.get(".foo"),
            &status_ctx(),
        ));

        assert!(
            r.iter()
                .any(|m| m.contains("UserChoice") && m.contains("Other.App")),
            "{:?}",
            r
        );
    }

    #[test]
    fn a_deleted_progid_has_drifted() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let target = applied_foo(&env, &mut store);
        env.sb
            .root
            .delete_subkey_all(env.classes("CustomFooFile"))
            .unwrap();

        let r = reasons(rule_status(
            &target,
            &foo_cfg("CustomFooFile"),
            store.backups.get(".foo"),
            &status_ctx(),
        ));

        assert!(r.iter().any(|m| m.contains("CustomFooFile")), "{:?}", r);
    }

    #[test]
    fn a_rule_edited_since_apply_has_drifted() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let target = applied_foo(&env, &mut store);
        let mut edited = foo_cfg("CustomFooFile");
        edited.description = "Renamed".into();
        let mut ctx = status_ctx();
        ctx.editor_command = "\"C:\\Other\\other.exe\" \"%1\"".into();

        let r = reasons(rule_status(
            &target,
            &edited,
            store.backups.get(".foo"),
            &ctx,
        ));

        assert!(r.iter().any(|m| m.contains("description")), "{:?}", r);
        assert!(r.iter().any(|m| m.contains("command")), "{:?}", r);
    }

    #[test]
    fn computing_the_status_writes_nothing() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let target = applied_foo(&env, &mut store);
        let before = env.snap(&env.sb.path);

        let _ = rule_status(
            &target,
            &foo_cfg("CustomFooFile"),
            store.backups.get(".foo"),
            &status_ctx(),
        );
        let _ = rule_status(&target, &with_ext(".bar"), None, &status_ctx());

        assert_eq!(env.snap(&env.sb.path), before);
    }

    #[test]
    fn find_rule_problems_reports_invalid_values_with_their_index() {
        let mut bad = foo_cfg("CustomFooFile");
        bad.extension = ".a b".into();
        let rules = [with_ext(".ok"), bad];

        let problems = find_rule_problems(&rules);

        assert_eq!(problems.len(), 1, "{:?}", problems);
        assert_eq!(problems[0].0, 1);
        assert!(problems[0].1.contains(".a b"), "{:?}", problems);
    }

    #[test]
    fn find_rule_problems_flags_every_duplicate_case_insensitively() {
        let rules = [with_ext(".foo"), with_ext(".bar"), with_ext(".FOO")];

        let problems = find_rule_problems(&rules);

        let indexes: Vec<usize> = problems.iter().map(|p| p.0).collect();
        assert_eq!(indexes, vec![0, 2], "{:?}", problems);
        assert!(problems.iter().all(|p| p.1.contains("more than once")));
    }

    #[test]
    fn find_rule_problems_is_empty_for_the_default_rules() {
        assert!(find_rule_problems(&crate::models::default_extensions()).is_empty());
    }

    #[test]
    fn validate_extension_accepts_normal_and_rejects_dangerous_values() {
        assert!(validate_extension(&foo_cfg("CustomFooFile")).is_ok());
        for ext in ["", ".", "foo", ".a\\b", ".a b"] {
            let mut c = foo_cfg("CustomFooFile");
            c.extension = ext.into();
            assert!(
                validate_extension(&c).is_err(),
                "extension {:?} accepted",
                ext
            );
        }
        for pid in ["", "  ", "A\\B", ".foo"] {
            assert!(
                validate_extension(&foo_cfg(pid)).is_err(),
                "progid {:?} accepted",
                pid
            );
        }
    }

    #[test]
    fn disabled_extensions_are_ignored() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let mut cfg = foo_cfg("CustomFooFile");
        cfg.enabled = false;

        let report = apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[cfg],
            &env.opts(&icon_ok),
        );

        assert!(report.succeeded.is_empty() && report.errors.is_empty());
        assert!(!env.exists(&env.classes(".foo")));
    }

    #[test]
    fn script_types_get_an_edit_verb_named_after_the_configured_editor() {
        let env = Env::new();
        let mut store = BackupStore::default();
        let mut cfg = foo_cfg("CustomFooFile");
        cfg.mode = DoubleClickMode::ExecuteScript;
        cfg.launcher_command = Some("\"%1\" %*".into());
        let mut opts = env.opts(&icon_ok);
        opts.editor_label = "Notepad++".into();

        apply_extensions(&env.target(&[HIVE_A]), &mut store, &[cfg], &opts);

        assert_eq!(
            env.default_of(&env.classes("CustomFooFile\\shell\\open\\command")),
            "\"%1\" %*"
        );
        assert_eq!(
            env.default_of(&env.classes("CustomFooFile\\shell\\edit")),
            "Edit with Notepad++"
        );
    }

    #[test]
    fn apply_writes_a_reg_safety_net_of_the_previous_state() {
        let env = Env::new();
        seed_original_state(&env);
        let mut store = BackupStore::default();

        let report = apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );

        let file = report.reg_backup_file.expect("no .reg written");
        let bytes = std::fs::read(&file).unwrap();
        let words: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let text = String::from_utf16(&words).unwrap();
        assert!(
            text.contains(&format!("[-HKEY_CURRENT_USER\\{}]", env.classes(".foo"))),
            "{text}"
        );
        assert!(text.contains("@=\"Old.File\""), "{text}");
        assert!(
            text.contains(&format!(
                "[-HKEY_CURRENT_USER\\{}]",
                env.classes("CustomFooFile")
            )),
            "{text}"
        );
    }

    #[test]
    fn apply_persists_backups_with_timestamps() {
        let env = Env::new();
        let mut store = BackupStore::default();

        apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );

        let on_disk = load_backup_store(&env.dir.join("backups.json")).unwrap();
        let b = on_disk.backups.get(".foo").expect("backup not persisted");
        assert_eq!(b.custom_prog_id, "CustomFooFile");
        assert!(b.timestamp.starts_with("20"), "timestamp: {}", b.timestamp);
        assert!(b.last_applied.is_some());
    }

    #[test]
    fn failed_rollback_keeps_the_backup() {
        let env = Env::new();
        let target = env.target(&[HIVE_A]);
        let mut store = BackupStore::default();
        apply_extensions(
            &target,
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );
        // Corrupt the snapshot so restoring it must fail (key name > 255 chars).
        let bad = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        let b = store.backups.get_mut(".foo").unwrap();
        b.hklm_extension_key.exists = true;
        b.hklm_extension_key.subkeys.insert("x".repeat(300), bad);

        let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

        assert!(!report.errors.is_empty());
        assert!(store.backups.contains_key(".foo"));
    }

    #[test]
    fn legacy_backup_without_progid_snapshots_still_removes_the_custom_progid() {
        let env = Env::new();
        env.sb
            .root
            .create_subkey(env.classes("CustomFooFile\\DefaultIcon"))
            .unwrap();
        let legacy = r#"{"backups":{".foo":{
            "extension":".foo","custom_prog_id":"CustomFooFile",
            "icon_installed_path":"C:\\ico\\foo.ico","timestamp":"Initial Snapshot",
            "hklm_extension_key":{"exists":false,"default_value":null,"values":{},"subkeys":{}},
            "hklm_previous_prog_id":null,"user_overrides":[]}}}"#;
        let mut store: BackupStore = serde_json::from_str(legacy).unwrap();

        let report = rollback_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[".foo".to_string()],
            &no_progress,
        );

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!env.exists(&env.classes("CustomFooFile")));
    }

    #[test]
    fn missing_backup_file_loads_as_empty_store() {
        let env = Env::new();
        assert!(load_backup_store(&env.dir.join("none.json"))
            .unwrap()
            .backups
            .is_empty());
    }

    #[test]
    fn corrupt_backup_file_is_an_error_not_an_empty_store() {
        // Returning an empty store would make the next save erase every snapshot.
        let env = Env::new();
        let path = env.dir.join("backups.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_backup_store(&path).is_err());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let env = Env::new();
        let path = env.dir.join("sub\\backups.json");
        let mut store = BackupStore::default();
        apply_extensions(
            &env.target(&[HIVE_A]),
            &mut store,
            &[foo_cfg("CustomFooFile")],
            &env.opts(&icon_ok),
        );
        save_backup_store(&store, &path).unwrap();
        let back = load_backup_store(&path).unwrap();
        assert_eq!(back.backups.keys().collect::<Vec<_>>(), vec![".foo"]);
    }
}
