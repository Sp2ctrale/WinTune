//! Explorer & Taskbar tweaks: per-user (HKCU) settings, each one value or one
//! key. Same safety model as file associations: the original state of every
//! value/key is snapshotted on first touch and persisted before any write, a
//! `.reg` safety net is exported, and "restore" puts it back exactly.

use crate::associations::OperationReport;
use crate::clock;
use crate::models::app_data_dir;
use crate::regsnap::{self, RegExportEntry, RegValueExport, RegistryKeySnapshot, ValueSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakGroup {
    FileExplorer,
    Taskbar,
    Start,
    Privacy,
}

impl TweakGroup {
    pub const ALL: [TweakGroup; 4] = [
        TweakGroup::FileExplorer,
        TweakGroup::Taskbar,
        TweakGroup::Start,
        TweakGroup::Privacy,
    ];

    /// Stable id, used for the `group.<id>` translation key.
    pub fn id(self) -> &'static str {
        match self {
            TweakGroup::FileExplorer => "file_explorer",
            TweakGroup::Taskbar => "taskbar",
            TweakGroup::Start => "start",
            TweakGroup::Privacy => "privacy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsRequirement {
    Any,
    Windows11,
}

/// When the change becomes visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Immediately (taskbar settings).
    Live,
    /// In newly opened Explorer windows (or after F5).
    NewWindows,
    /// After restarting Explorer or signing out.
    ExplorerRestart,
}

#[derive(Debug, Clone, Copy)]
pub enum TweakAction {
    /// A REG_DWORD under HKCU: `on` when enabled, `off` (the Windows
    /// default) when disabled.
    Dword {
        path: &'static str,
        name: &'static str,
        on: u32,
        off: u32,
    },
    /// Enabled while `key` exists (created with an empty default value);
    /// disabling deletes the `remove` subtree (`key` or one of its parents).
    Key {
        key: &'static str,
        remove: &'static str,
    },
}

impl TweakAction {
    /// The key whose state is snapshotted (Key tweaks) or that holds the value.
    fn snapshot_path(&self) -> &'static str {
        match self {
            TweakAction::Dword { path, .. } => path,
            TweakAction::Key { remove, .. } => remove,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Tweak {
    pub id: &'static str,
    pub group: TweakGroup,
    pub os: OsRequirement,
    pub refresh: Refresh,
    /// Relies on undocumented Windows behaviour.
    pub unofficial: bool,
    pub action: TweakAction,
}

const EXPLORER: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer";
const START: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Start";
const ADVANCED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const CLASSIC_MENU_CLSID: &str = "Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";

static CATALOG: &[Tweak] = &[
    Tweak {
        id: "show_extensions",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "HideFileExt",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "show_hidden",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "Hidden",
            on: 1,
            off: 2,
        },
    },
    Tweak {
        id: "show_protected_os_files",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "ShowSuperHidden",
            on: 1,
            off: 0,
        },
    },
    Tweak {
        id: "open_this_pc",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "LaunchTo",
            on: 1,
            off: 2,
        },
    },
    Tweak {
        id: "full_path_title",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::ExplorerRestart,
        unofficial: false,
        action: TweakAction::Dword {
            path: "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\CabinetState",
            name: "FullPath",
            on: 1,
            off: 0,
        },
    },
    Tweak {
        id: "compact_mode",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Windows11,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "UseCompactMode",
            on: 1,
            off: 0,
        },
    },
    Tweak {
        id: "item_checkboxes",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "AutoCheckSelect",
            on: 1,
            off: 0,
        },
    },
    Tweak {
        id: "hide_recent_files",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: EXPLORER,
            name: "ShowRecent",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "hide_frequent_folders",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Any,
        refresh: Refresh::NewWindows,
        unofficial: false,
        action: TweakAction::Dword {
            path: EXPLORER,
            name: "ShowFrequent",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "classic_context_menu",
        group: TweakGroup::FileExplorer,
        os: OsRequirement::Windows11,
        refresh: Refresh::ExplorerRestart,
        unofficial: true,
        action: TweakAction::Key {
            key: "Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32",
            remove: CLASSIC_MENU_CLSID,
        },
    },
    Tweak {
        id: "taskbar_left",
        group: TweakGroup::Taskbar,
        os: OsRequirement::Windows11,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "TaskbarAl",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "hide_task_view",
        group: TweakGroup::Taskbar,
        os: OsRequirement::Any,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "ShowTaskViewButton",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "hide_search",
        group: TweakGroup::Taskbar,
        os: OsRequirement::Any,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: "Software\\Microsoft\\Windows\\CurrentVersion\\Search",
            name: "SearchboxTaskbarMode",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "start_recommendations",
        group: TweakGroup::Start,
        os: OsRequirement::Windows11,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "Start_IrisRecommendations",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "start_more_pins",
        group: TweakGroup::Start,
        os: OsRequirement::Windows11,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "Start_Layout",
            on: 1,
            off: 0,
        },
    },
    Tweak {
        id: "start_hide_recent_apps",
        group: TweakGroup::Start,
        os: OsRequirement::Windows11,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: START,
            name: "ShowRecentList",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "start_show_all_pins",
        group: TweakGroup::Start,
        os: OsRequirement::Windows11,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: START,
            name: "ShowAllPinsList",
            on: 1,
            off: 0,
        },
    },
    Tweak {
        id: "no_app_launch_tracking",
        group: TweakGroup::Privacy,
        os: OsRequirement::Any,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "Start_TrackProgs",
            on: 0,
            off: 1,
        },
    },
    Tweak {
        id: "no_recent_documents",
        group: TweakGroup::Privacy,
        os: OsRequirement::Any,
        refresh: Refresh::Live,
        unofficial: false,
        action: TweakAction::Dword {
            path: ADVANCED,
            name: "Start_TrackDocs",
            on: 0,
            off: 1,
        },
    },
];

pub fn catalog() -> &'static [Tweak] {
    CATALOG
}

pub fn find_tweak(id: &str) -> Option<&'static Tweak> {
    CATALOG.iter().find(|t| t.id == id)
}

/// Where tweaks are written. Production: HKCU itself; tests: a sandbox key.
pub struct TweakTarget {
    pub root: RegKey,
    pub root_name: String,
    /// Prefix for every path (empty in production).
    pub base: String,
}

impl TweakTarget {
    pub fn current_user() -> Self {
        Self {
            root: RegKey::predef(HKEY_CURRENT_USER),
            root_name: "HKEY_CURRENT_USER".into(),
            base: String::new(),
        }
    }

    fn path(&self, rel: &str) -> String {
        if self.base.is_empty() {
            rel.to_string()
        } else {
            format!("{}\\{}", self.base, rel)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TweakStatus {
    On,
    /// Off, including "not set" (Windows then uses its default).
    Off,
    /// Set to something WinTune does not write, e.g. a value set by another tool.
    Custom(CustomReason),
}

/// Why a tweak reads as "Custom". Carries the values: the UI translates it
/// (`i18n::messages::custom_reason`); `Display` is the English text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomReason {
    UnexpectedValue {
        name: String,
        value: u32,
    },
    UnexpectedType {
        name: String,
    },
    /// The registry read failed; carries the error text.
    Unreadable(String),
}

impl std::fmt::Display for CustomReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CustomReason::UnexpectedValue { name, value } => write!(f, "{} = {}", name, value),
            CustomReason::UnexpectedType { name } => {
                write!(f, "{} has an unexpected type", name)
            }
            CustomReason::Unreadable(e) => write!(f, "unreadable: {}", e),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum TweakSnapshot {
    Value(ValueSnapshot),
    Key(RegistryKeySnapshot),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TweakBackup {
    /// When the original state was captured (local time).
    pub timestamp: String,
    pub snapshot: TweakSnapshot,
}

/// Original states, per user then per tweak id. HKCU is per user, so a
/// snapshot only applies to the account that took it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TweakBackupStore {
    pub users: BTreeMap<String, BTreeMap<String, TweakBackup>>,
}

impl TweakBackupStore {
    pub fn backups_of(&self, user: &str) -> Option<&BTreeMap<String, TweakBackup>> {
        self.users.get(user)
    }

    pub fn has_backup(&self, user: &str, id: &str) -> bool {
        self.backups_of(user).is_some_and(|b| b.contains_key(id))
    }
}

pub fn default_tweak_store_path() -> PathBuf {
    app_data_dir().join("tweaks-backups.json")
}

pub fn load_tweak_store(path: &Path) -> Result<TweakBackupStore, crate::persist::LoadError> {
    crate::persist::load_json(path)
}

pub fn save_tweak_store(store: &TweakBackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

/// `DOMAIN\user` of the account whose HKCU this process sees.
pub fn current_user_label() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "unknown".into());
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{}\\{}", domain, user),
        _ => user,
    }
}

pub struct TweakOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: String,
    pub is_windows_11: bool,
    pub store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    pub progress: &'a dyn Fn(usize, usize, &str),
}

/// Tells Explorer and the taskbar that settings changed, so "live" tweaks
/// show up immediately (others need new windows or an Explorer restart).
pub fn notify_explorer_settings() {
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let area: Vec<u16> = "TraySettings".encode_utf16().chain(Some(0)).collect();
    let mut result = 0usize;
    // SAFETY: `area` is a NUL-terminated UTF-16 string that outlives the
    // call; SMTO_ABORTIFHUNG + timeout keep a hung window from blocking us.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        );
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

/// Read-only.
pub fn tweak_status(target: &TweakTarget, tweak: &Tweak) -> Result<TweakStatus, String> {
    match tweak.action {
        TweakAction::Dword {
            path,
            name,
            on,
            off,
        } => {
            let value = regsnap::snapshot_value(&target.root, &target.path(path), name)?;
            if !value.exists {
                return Ok(TweakStatus::Off);
            }
            match dword_of(&value) {
                Some(v) if v == on => Ok(TweakStatus::On),
                Some(v) if v == off => Ok(TweakStatus::Off),
                Some(v) => Ok(TweakStatus::Custom(CustomReason::UnexpectedValue {
                    name: name.to_string(),
                    value: v,
                })),
                None => Ok(TweakStatus::Custom(CustomReason::UnexpectedType {
                    name: name.to_string(),
                })),
            }
        }
        TweakAction::Key { key, .. } => {
            match target
                .root
                .open_subkey_with_flags(target.path(key), KEY_READ)
            {
                Ok(_) => Ok(TweakStatus::On),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TweakStatus::Off),
                Err(e) => Err(format!("Cannot read {}: {}", key, e)),
            }
        }
    }
}

fn dword_of(value: &ValueSnapshot) -> Option<u32> {
    let bytes: [u8; 4] = value.data.as_slice().try_into().ok()?;
    (value.kind == REG_DWORD as u32).then(|| u32::from_le_bytes(bytes))
}

fn snapshot_tweak(target: &TweakTarget, tweak: &Tweak) -> Result<TweakSnapshot, String> {
    match tweak.action {
        TweakAction::Dword { path, name, .. } => Ok(TweakSnapshot::Value(regsnap::snapshot_value(
            &target.root,
            &target.path(path),
            name,
        )?)),
        TweakAction::Key { remove, .. } => Ok(TweakSnapshot::Key(regsnap::snapshot_key(
            &target.root,
            &target.path(remove),
        )?)),
    }
}

fn restore_tweak(
    target: &TweakTarget,
    tweak: &Tweak,
    snapshot: &TweakSnapshot,
) -> Result<(), String> {
    match (tweak.action, snapshot) {
        (TweakAction::Dword { path, name, .. }, TweakSnapshot::Value(v)) => {
            regsnap::restore_value(&target.root, &target.path(path), name, v)
        }
        (TweakAction::Key { remove, .. }, TweakSnapshot::Key(k)) => {
            regsnap::restore_key(&target.root, &target.path(remove), k)
        }
        _ => Err(format!(
            "{}: the recorded snapshot does not match this tweak",
            tweak.id
        )),
    }
}

fn write_tweak(target: &TweakTarget, tweak: &Tweak, enabled: bool) -> Result<(), String> {
    match tweak.action {
        TweakAction::Dword {
            path,
            name,
            on,
            off,
        } => {
            let full = target.path(path);
            let (key, _) = target
                .root
                .create_subkey(&full)
                .map_err(|e| format!("Cannot create {}: {}", path, e))?;
            key.set_value(name, &if enabled { on } else { off })
                .map_err(|e| format!("Cannot write {}\\{}: {}", path, name, e))
        }
        TweakAction::Key { key, remove } => {
            if enabled {
                let (k, _) = target
                    .root
                    .create_subkey(target.path(key))
                    .map_err(|e| format!("Cannot create {}: {}", key, e))?;
                k.set_value("", &"")
                    .map_err(|e| format!("Cannot write {}: {}", key, e))
            } else {
                regsnap::delete_tree(&target.root, &target.path(remove))
            }
        }
    }
}

/// Turns each `(id, enabled)` on or off. Disabling writes the Windows
/// default; the original state stays recorded until [`restore_tweaks`].
pub fn apply_tweaks(
    target: &TweakTarget,
    store: &mut TweakBackupStore,
    changes: &[(String, bool)],
    opts: &TweakOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let now = clock::now_local();
    let steps = changes.len();

    // Phase 1: validate and snapshot (first touch only), nothing written.
    let mut prepared: Vec<(&'static Tweak, bool)> = Vec::new();
    let mut value_exports: Vec<(String, String, ValueSnapshot)> = Vec::new();
    let mut key_exports: Vec<(String, RegistryKeySnapshot)> = Vec::new();
    for (i, (id, enabled)) in changes.iter().enumerate() {
        (opts.progress)(i + 1, steps * 2, &format!("Snapshotting {}", id));
        let Some(tweak) = find_tweak(id) else {
            report.errors.push(format!("{}: unknown tweak", id));
            continue;
        };
        if tweak.os == OsRequirement::Windows11 && !opts.is_windows_11 {
            report.errors.push(format!("{}: needs Windows 11", id));
            continue;
        }
        let current = match snapshot_tweak(target, tweak) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(format!("{}: {}", id, e));
                continue;
            }
        };
        match &current {
            TweakSnapshot::Value(v) => {
                if let TweakAction::Dword { path, name, .. } = tweak.action {
                    value_exports.push((target.path(path), name.to_string(), v.clone()));
                }
            }
            TweakSnapshot::Key(k) => {
                key_exports.push((target.path(tweak.action.snapshot_path()), k.clone()))
            }
        }
        store
            .users
            .entry(opts.user.clone())
            .or_default()
            .entry(tweak.id.to_string())
            .or_insert_with(|| TweakBackup {
                timestamp: now.clone(),
                snapshot: current,
            });
        prepared.push((tweak, *enabled));
    }
    if prepared.is_empty() {
        return report;
    }

    // Phase 2: persist snapshots + .reg safety net before any write.
    if let Err(e) = save_tweak_store(store, &opts.store_path) {
        report
            .errors
            .push(format!("Nothing applied: cannot save snapshots ({})", e));
        return report;
    }
    let keys: Vec<RegExportEntry> = key_exports
        .iter()
        .map(|(path, snapshot)| RegExportEntry {
            root_name: &target.root_name,
            path: path.clone(),
            snapshot,
        })
        .collect();
    let values: Vec<RegValueExport> = value_exports
        .iter()
        .map(|(path, name, snapshot)| RegValueExport {
            root_name: &target.root_name,
            path: path.clone(),
            name: name.clone(),
            snapshot,
        })
        .collect();
    let reg_file = regsnap::unique_reg_file(
        &opts.reg_backup_dir,
        &format!("{}-tweaks-before-apply", clock::file_stamp()),
    );
    if let Err(e) = regsnap::write_reg_file(&reg_file, &regsnap::export_reg_full(&keys, &values)) {
        report
            .errors
            .push(format!("Nothing applied: cannot write .reg backup ({})", e));
        return report;
    }
    report.reg_backup_file = Some(reg_file);

    // Phase 3: writes.
    let total = steps + prepared.len();
    for (i, (tweak, enabled)) in prepared.into_iter().enumerate() {
        match write_tweak(target, tweak, enabled) {
            Ok(()) => report.succeeded.push(tweak.id.to_string()),
            Err(e) => report.errors.push(format!("{}: {}", tweak.id, e)),
        }
        (opts.progress)(steps + i + 1, total, &format!("Writing {}", tweak.id));
    }
    report
}

/// Puts each tweak back exactly as it was before WinTune first changed it,
/// then forgets its snapshot. The caller notifies Explorer.
pub fn restore_tweaks(
    target: &TweakTarget,
    store: &mut TweakBackupStore,
    ids: &[String],
    opts: &TweakOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let total = ids.len();

    for (i, id) in ids.iter().enumerate() {
        (opts.progress)(i, total, &format!("Restoring {}", id));
        let Some(tweak) = find_tweak(id) else {
            report.errors.push(format!("{}: unknown tweak", id));
            continue;
        };
        let Some(backup) = store.users.get(&opts.user).and_then(|b| b.get(id)).cloned() else {
            report.errors.push(format!(
                "{}: no original state recorded for {}",
                id, opts.user
            ));
            continue;
        };
        match restore_tweak(target, tweak, &backup.snapshot) {
            Ok(()) => {
                if let Some(user) = store.users.get_mut(&opts.user) {
                    user.remove(id);
                }
                report.succeeded.push(id.clone());
            }
            Err(e) => report.errors.push(format!(
                "{}: {} (snapshot kept, restore can be retried)",
                id, e
            )),
        }
    }
    store.users.retain(|_, b| !b.is_empty());

    if !report.succeeded.is_empty() {
        if let Err(e) = save_tweak_store(store, &opts.store_path) {
            report.errors.push(format!("Cannot save snapshots: {}", e));
        }
    }
    (opts.progress)(total, total, "Restore finished");
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regsnap::test_support::Sandbox;

    struct Env {
        sb: Sandbox,
        dir: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let sb = Sandbox::new();
            let dir = std::env::temp_dir().join(format!(
                "wintune-tweaks-{}",
                sb.path.rsplit('\\').next().unwrap()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { sb, dir }
        }

        fn target(&self) -> TweakTarget {
            TweakTarget {
                root: RegKey::predef(HKEY_CURRENT_USER),
                root_name: "HKEY_CURRENT_USER".into(),
                base: self.sb.path.clone(),
            }
        }

        fn opts(&self, user: &str) -> TweakOptions<'static> {
            TweakOptions {
                user: user.into(),
                is_windows_11: true,
                store_path: self.dir.join("tweaks-backups.json"),
                reg_backup_dir: self.dir.join("reg-backups"),
                progress: &|_, _, _| {},
            }
        }

        fn abs(&self, rel: &str) -> String {
            format!("{}\\{}", self.sb.path, rel)
        }

        fn dword(&self, rel_path: &str, name: &str) -> Option<u32> {
            self.sb
                .root
                .open_subkey(self.abs(rel_path))
                .ok()?
                .get_value(name)
                .ok()
        }

        fn set_dword(&self, rel_path: &str, name: &str, v: u32) {
            let (k, _) = self.sb.root.create_subkey(self.abs(rel_path)).unwrap();
            k.set_value(name, &v).unwrap();
        }

        fn exists(&self, rel_path: &str) -> bool {
            self.sb.root.open_subkey(self.abs(rel_path)).is_ok()
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn on(id: &str) -> (String, bool) {
        (id.to_string(), true)
    }

    fn off(id: &str) -> (String, bool) {
        (id.to_string(), false)
    }

    fn ok(report: &OperationReport) {
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    fn status(env: &Env, id: &str) -> TweakStatus {
        tweak_status(&env.target(), find_tweak(id).unwrap()).unwrap()
    }

    #[test]
    fn the_catalog_includes_the_start_and_privacy_tweaks() {
        let start = [
            "start_recommendations",
            "start_more_pins",
            "start_hide_recent_apps",
            "start_show_all_pins",
        ];
        for id in start {
            assert_eq!(
                find_tweak(id).map(|t| t.group),
                Some(TweakGroup::Start),
                "{}",
                id
            );
        }
        for id in ["no_app_launch_tracking", "no_recent_documents"] {
            assert_eq!(
                find_tweak(id).map(|t| t.group),
                Some(TweakGroup::Privacy),
                "{}",
                id
            );
        }
    }

    #[test]
    fn the_catalog_includes_the_verified_explorer_tweaks() {
        for id in [
            "compact_mode",
            "item_checkboxes",
            "hide_recent_files",
            "hide_frequent_folders",
        ] {
            assert!(find_tweak(id).is_some(), "missing tweak {}", id);
        }
    }

    #[test]
    fn the_registry_reference_lists_every_tweak() {
        let doc = include_str!("../docs/registry-reference.md");
        for t in catalog() {
            assert!(
                doc.contains(&format!("`{}`", t.id)),
                "docs/registry-reference.md does not list `{}`",
                t.id
            );
            if let TweakAction::Dword { name, .. } = t.action {
                assert!(
                    doc.contains(&format!("`{}`", name)),
                    "docs/registry-reference.md does not name the value `{}` of `{}`",
                    name,
                    t.id
                );
            }
        }
    }

    #[test]
    fn every_dword_tweak_has_distinct_on_and_off_values() {
        for t in catalog() {
            if let TweakAction::Dword { on, off, .. } = t.action {
                assert_ne!(on, off, "{}", t.id);
            }
        }
    }

    /// The whole catalog, not only the hand-picked cases: on, off, then
    /// restore must give back exactly what was there (here: nothing).
    #[test]
    fn every_tweak_round_trips_on_off_restore_in_the_sandbox() {
        for t in catalog() {
            let env = Env::new();
            let before = regsnap::snapshot_key(&env.sb.root, &env.sb.path).unwrap();
            let mut store = TweakBackupStore::default();

            ok(&apply_tweaks(
                &env.target(),
                &mut store,
                &[on(t.id)],
                &env.opts("u"),
            ));
            assert_eq!(status(&env, t.id), TweakStatus::On, "{} not on", t.id);
            ok(&apply_tweaks(
                &env.target(),
                &mut store,
                &[off(t.id)],
                &env.opts("u"),
            ));
            assert_eq!(status(&env, t.id), TweakStatus::Off, "{} not off", t.id);
            ok(&restore_tweaks(
                &env.target(),
                &mut store,
                &[t.id.to_string()],
                &env.opts("u"),
            ));

            let after = regsnap::snapshot_key(&env.sb.root, &env.sb.path).unwrap();
            assert_eq!(
                values_of(&after),
                values_of(&before),
                "{} left values behind after restore",
                t.id
            );
            assert!(store.users.is_empty(), "{} snapshot not dropped", t.id);
        }
    }

    /// Every value in a snapshot tree, flattened (empty parent keys created by
    /// a write are harmless; leftover values are not).
    fn values_of(snap: &RegistryKeySnapshot) -> Vec<(String, u32, Vec<u8>)> {
        fn walk(prefix: &str, s: &RegistryKeySnapshot, out: &mut Vec<(String, u32, Vec<u8>)>) {
            for (name, (kind, data)) in &s.values {
                out.push((format!("{}\\{}", prefix, name), *kind, data.clone()));
            }
            for (child, sub) in &s.subkeys {
                walk(&format!("{}\\{}", prefix, child), sub, out);
            }
        }
        let mut out = Vec::new();
        walk("", snap, &mut out);
        out
    }

    #[test]
    fn catalog_ids_are_unique_and_paths_are_relative_to_hkcu() {
        let mut ids: Vec<&str> = catalog().iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), catalog().len());
        for t in catalog() {
            let path = t.action.snapshot_path();
            assert!(path.starts_with("Software\\"), "{}: {}", t.id, path);
            if let TweakAction::Key { key, remove } = t.action {
                assert!(key.starts_with(remove), "{}: remove must contain key", t.id);
            }
        }
    }

    #[test]
    fn an_unset_value_is_off() {
        let env = Env::new();
        assert_eq!(status(&env, "show_extensions"), TweakStatus::Off);
    }

    #[test]
    fn a_value_set_by_someone_else_is_custom() {
        let env = Env::new();
        env.set_dword(ADVANCED, "LaunchTo", 3);
        assert!(matches!(
            status(&env, "open_this_pc"),
            TweakStatus::Custom(_)
        ));
    }

    #[test]
    fn a_dword_with_an_unknown_value_is_custom_with_its_value() {
        let env = Env::new();
        env.set_dword(ADVANCED, "HideFileExt", 7);
        assert_eq!(
            status(&env, "show_extensions"),
            TweakStatus::Custom(CustomReason::UnexpectedValue {
                name: "HideFileExt".into(),
                value: 7
            })
        );
    }

    #[test]
    fn turning_on_writes_the_value_and_reads_back_on() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();

        let report = apply_tweaks(
            &env.target(),
            &mut store,
            &[on("show_extensions")],
            &env.opts("u"),
        );

        ok(&report);
        assert_eq!(report.succeeded, vec!["show_extensions".to_string()]);
        assert_eq!(env.dword(ADVANCED, "HideFileExt"), Some(0));
        assert_eq!(status(&env, "show_extensions"), TweakStatus::On);
    }

    #[test]
    fn turning_off_writes_the_windows_default() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();
        apply_tweaks(
            &env.target(),
            &mut store,
            &[on("show_hidden")],
            &env.opts("u"),
        );

        ok(&apply_tweaks(
            &env.target(),
            &mut store,
            &[off("show_hidden")],
            &env.opts("u"),
        ));

        assert_eq!(env.dword(ADVANCED, "Hidden"), Some(2));
        assert_eq!(status(&env, "show_hidden"), TweakStatus::Off);
        assert!(
            store.has_backup("u", "show_hidden"),
            "snapshot kept until restore"
        );
    }

    #[test]
    fn restore_deletes_a_value_that_did_not_exist() {
        let env = Env::new();
        env.set_dword(ADVANCED, "Sibling", 7);
        let mut store = TweakBackupStore::default();
        apply_tweaks(
            &env.target(),
            &mut store,
            &[on("show_extensions")],
            &env.opts("u"),
        );

        let report = restore_tweaks(
            &env.target(),
            &mut store,
            &["show_extensions".into()],
            &env.opts("u"),
        );

        ok(&report);
        assert_eq!(env.dword(ADVANCED, "HideFileExt"), None);
        assert_eq!(env.dword(ADVANCED, "Sibling"), Some(7));
        assert!(!store.has_backup("u", "show_extensions"));
    }

    #[test]
    fn restore_brings_back_the_first_value_not_an_intermediate_one() {
        let env = Env::new();
        env.set_dword(ADVANCED, "LaunchTo", 3);
        let mut store = TweakBackupStore::default();
        apply_tweaks(
            &env.target(),
            &mut store,
            &[on("open_this_pc")],
            &env.opts("u"),
        );
        apply_tweaks(
            &env.target(),
            &mut store,
            &[off("open_this_pc")],
            &env.opts("u"),
        );

        restore_tweaks(
            &env.target(),
            &mut store,
            &["open_this_pc".into()],
            &env.opts("u"),
        );

        assert_eq!(env.dword(ADVANCED, "LaunchTo"), Some(3));
    }

    #[test]
    fn key_tweak_creates_the_key_and_off_removes_the_whole_tree() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();
        let key =
            "Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32";

        ok(&apply_tweaks(
            &env.target(),
            &mut store,
            &[on("classic_context_menu")],
            &env.opts("u"),
        ));
        let default: String = env
            .sb
            .root
            .open_subkey(env.abs(key))
            .unwrap()
            .get_value("")
            .unwrap();
        assert_eq!(default, "");
        assert_eq!(status(&env, "classic_context_menu"), TweakStatus::On);

        ok(&apply_tweaks(
            &env.target(),
            &mut store,
            &[off("classic_context_menu")],
            &env.opts("u"),
        ));
        assert!(!env.exists(CLASSIC_MENU_CLSID));
        assert_eq!(status(&env, "classic_context_menu"), TweakStatus::Off);
    }

    #[test]
    fn key_tweak_restore_returns_to_the_original_tree() {
        let env = Env::new();
        let (k, _) = env
            .sb
            .root
            .create_subkey(env.abs(CLASSIC_MENU_CLSID))
            .unwrap();
        k.set_value("Marker", &"original").unwrap();
        let mut store = TweakBackupStore::default();
        apply_tweaks(
            &env.target(),
            &mut store,
            &[off("classic_context_menu")],
            &env.opts("u"),
        );
        assert!(!env.exists(CLASSIC_MENU_CLSID));

        ok(&restore_tweaks(
            &env.target(),
            &mut store,
            &["classic_context_menu".into()],
            &env.opts("u"),
        ));

        let marker: String = env
            .sb
            .root
            .open_subkey(env.abs(CLASSIC_MENU_CLSID))
            .unwrap()
            .get_value("Marker")
            .unwrap();
        assert_eq!(marker, "original");
    }

    #[test]
    fn snapshots_are_persisted_and_a_reg_safety_net_is_written() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();

        let report = apply_tweaks(
            &env.target(),
            &mut store,
            &[on("show_extensions")],
            &env.opts("u"),
        );

        let on_disk = load_tweak_store(&env.dir.join("tweaks-backups.json")).unwrap();
        assert!(on_disk.has_backup("u", "show_extensions"));
        let file = report.reg_backup_file.expect("no .reg written");
        let bytes = std::fs::read(file).unwrap();
        let words: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let text = String::from_utf16(&words).unwrap();
        assert!(text.contains("\"HideFileExt\"=-"), "{text}");
    }

    #[test]
    fn unknown_ids_and_windows_11_only_tweaks_on_windows_10_are_refused() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();
        let mut opts = env.opts("u");
        opts.is_windows_11 = false;

        let report = apply_tweaks(
            &env.target(),
            &mut store,
            &[on("nope"), on("taskbar_left")],
            &opts,
        );

        assert_eq!(report.errors.len(), 2, "{:?}", report.errors);
        assert_eq!(env.dword(ADVANCED, "TaskbarAl"), None);
        assert!(store.users.is_empty());
    }

    #[test]
    fn snapshots_belong_to_the_user_who_took_them() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();
        apply_tweaks(
            &env.target(),
            &mut store,
            &[on("show_extensions")],
            &env.opts("alice"),
        );

        let report = restore_tweaks(
            &env.target(),
            &mut store,
            &["show_extensions".into()],
            &env.opts("bob"),
        );

        assert!(!report.errors.is_empty());
        assert_eq!(
            env.dword(ADVANCED, "HideFileExt"),
            Some(0),
            "bob must not undo alice's change"
        );
    }

    #[test]
    fn reading_the_status_writes_nothing() {
        let env = Env::new();
        let before = regsnap::snapshot_key(&env.sb.root, &env.sb.path).unwrap();
        for t in catalog() {
            let _ = tweak_status(&env.target(), t);
        }
        assert_eq!(
            regsnap::snapshot_key(&env.sb.root, &env.sb.path).unwrap(),
            before
        );
    }

    #[test]
    fn progress_ends_complete() {
        let env = Env::new();
        let mut store = TweakBackupStore::default();
        let calls = std::cell::RefCell::new(Vec::new());
        let record = |d: usize, t: usize, _: &str| calls.borrow_mut().push((d, t));
        let mut opts = env.opts("u");
        opts.progress = &record;

        apply_tweaks(
            &env.target(),
            &mut store,
            &[on("show_extensions"), on("show_hidden")],
            &opts,
        );

        assert_eq!(calls.into_inner().last(), Some(&(4, 4)));
    }
}
