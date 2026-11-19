//! Where the association keys live (real machine or test sandbox).

use crate::detector::get_loaded_user_sids;
use winreg::enums::*;
use winreg::RegKey;

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

    pub(super) fn class_path(&self, name: &str) -> String {
        format!("{}\\{}", self.machine_classes, name)
    }

    pub(super) fn user_classes_path(&self, hive: &str, ext: &str) -> String {
        format!("{}\\{}\\{}", hive, USER_CLASSES, ext)
    }

    pub(super) fn user_file_exts_path(&self, hive: &str, ext: &str) -> String {
        format!("{}\\{}\\{}", hive, USER_FILE_EXTS, ext)
    }
}
