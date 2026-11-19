//! Persistence of the association snapshots (`backups.json`).

use crate::models::{app_data_dir, BackupStore, ExtensionBackup, RegistryKeySnapshot};
use std::path::{Path, PathBuf};

pub fn default_backup_store_path() -> PathBuf {
    app_data_dir().join("backups.json")
}

pub fn default_reg_backup_dir() -> PathBuf {
    app_data_dir().join("reg-backups")
}

/// A missing file is an empty store. A corrupt one is an error: returning an
/// empty store would make the next save erase every recorded snapshot.
pub fn load_backup_store(path: &Path) -> Result<BackupStore, String> {
    crate::persist::load_json(path)
}

/// Atomic save (write to a temp file, then rename over the old one).
pub fn save_backup_store(store: &BackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

/// Old backups (WinTune <= 0.2.0) did not snapshot the custom ProgID: it was
/// always created from scratch, so its original state is "absent".
pub(super) fn migrate_legacy_backup(backup: &mut ExtensionBackup) {
    if backup.prog_id_snapshots.is_empty() && !backup.custom_prog_id.is_empty() {
        backup.prog_id_snapshots.insert(
            backup.custom_prog_id.clone(),
            RegistryKeySnapshot::default(),
        );
    }
}
