//! Apply: validate, snapshot and persist, then write (see the module docs).

use super::store::{migrate_legacy_backup, save_backup_store};
use super::target::AssocTarget;
use super::validate::{has_forbidden_chars, validate_extension};
use super::OperationReport;
use crate::clock;
use crate::models::{
    BackupStore, DoubleClickMode, ExtensionBackup, ExtensionConfig, RegistryKeySnapshot,
    UserOverrideSnapshot,
};
use crate::regsnap::{self, RegExportEntry};
use std::collections::BTreeMap;
use std::path::PathBuf;
use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

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

    let key = store
        .key_for(&cfg.extension)
        .unwrap_or(&cfg.extension)
        .to_string();
    let mut backup = match store.backups.get(&key) {
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
            machine_restored: false,
        },
    };
    migrate_legacy_backup(&mut backup);
    // The machine keys are about to change again: a rollback must restore them.
    backup.machine_restored = false;
    items.push((m_name.clone(), ext_path, current_ext));

    let mut prog_ids: Vec<String> = backup.prog_id_snapshots.keys().cloned().collect();
    if !prog_ids.contains(&cfg.prog_id) {
        prog_ids.push(cfg.prog_id.clone());
    }
    for pid in prog_ids {
        let path = target.class_path(&pid);
        let current = regsnap::snapshot_key(&target.machine_root, &path)?;
        if let std::collections::btree_map::Entry::Vacant(slot) =
            backup.prog_id_snapshots.entry(pid)
        {
            // Another extension may already have written this ProgID: its
            // recorded original is the true one, not WinTune's current key.
            let wanted = slot.key().clone();
            let original = store
                .backups
                .values()
                .find_map(|b| {
                    b.prog_id_snapshots
                        .iter()
                        .find(|(p, _)| p.eq_ignore_ascii_case(&wanted))
                        .map(|(_, s)| s.clone())
                })
                .unwrap_or_else(|| current.clone());
            slot.insert(original);
        }
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

    store.backups.insert(key, backup);
    export.extend(items);
    Ok(icon_path)
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
pub(super) fn script_launcher(cfg: &ExtensionConfig, wsl_available: bool) -> Option<&str> {
    let launcher = cfg.launcher_command.as_deref()?;
    let wsl_missing = cfg.prog_id == "CustomShellFile" && !wsl_available;
    (cfg.mode == DoubleClickMode::ExecuteScript && !wsl_missing).then_some(launcher)
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
    // A ProgID WinTune created is rebuilt from scratch (no stale verbs). One
    // that existed before belongs to another app: only overwrite the values
    // WinTune sets, never delete what it holds (rollback restores it exactly).
    let created_by_wintune = backup
        .prog_id_snapshots
        .get(&cfg.prog_id)
        .is_none_or(|original| !original.exists);
    if created_by_wintune {
        regsnap::delete_tree(m, &prog_path)?;
    }
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
            .map_err(|e| e.to_string())
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
    let reg_file = regsnap::unique_reg_file(
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
        let key = store.key_for(&ext).unwrap_or(&ext).to_string();
        let Some(backup) = store.backups.get(&key).cloned() else {
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
        if let Some(b) = store.backups.get_mut(&key) {
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
