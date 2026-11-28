//! The snapshot store (`mouse-keyboard-backups.json`) and the `.reg` net
//! taken before every Apply.

use super::SettingValue;
use crate::backups::RegTarget;
use crate::clock;
use crate::models::app_data_dir;
use crate::persist::LoadError;
use crate::regsnap::{self, RegValueExport, ValueSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winreg::enums::KEY_READ;

/// One original value, as first read before WinTune changed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkBackup {
    /// When it was read (local time).
    pub timestamp: String,
    pub value: SettingValue,
}

/// Original values, per user then per `SettingId::key`. Same shape and
/// rules as `backups::UserBackups` (spec 5.3); a key this version does not
/// know still loads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MkBackupStore {
    pub users: BTreeMap<String, BTreeMap<String, MkBackup>>,
}

impl MkBackupStore {
    pub fn backups_of(&self, user: &str) -> Option<&BTreeMap<String, MkBackup>> {
        self.users.get(user)
    }

    pub fn has_backup(&self, user: &str, id: &str) -> bool {
        self.backups_of(user).is_some_and(|b| b.contains_key(id))
    }

    pub fn original(&self, user: &str, id: &str) -> Option<&SettingValue> {
        self.backups_of(user)?.get(id).map(|b| &b.value)
    }

    /// Records `value` unless `id` already has one: the first is the
    /// original.
    pub fn record_first(&mut self, user: &str, id: &str, timestamp: &str, value: SettingValue) {
        self.users
            .entry(user.to_string())
            .or_default()
            .entry(id.to_string())
            .or_insert_with(|| MkBackup {
                timestamp: timestamp.to_string(),
                value,
            });
    }

    /// Drops the snapshot of `id`, and the user once they have none left.
    pub fn forget(&mut self, user: &str, id: &str) {
        if let Some(backups) = self.users.get_mut(user) {
            backups.remove(id);
            if backups.is_empty() {
                self.users.remove(user);
            }
        }
    }
}

pub fn default_mk_store_path() -> PathBuf {
    app_data_dir().join("mouse-keyboard-backups.json")
}

pub fn load_mk_store(path: &Path) -> Result<MkBackupStore, LoadError> {
    crate::persist::load_json(path)
}

/// Atomic save. The error is the bare cause (`MkProblem::NotSaved` and
/// `MkFailure::StoreNotSaved` give the sentence around it).
pub fn save_mk_store(store: &MkBackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

/// The keys whose values the `.reg` net exports (spec 5.4).
pub const CONTROL_PANEL_KEYS: [&str; 3] = [
    r"Control Panel\Mouse",
    r"Control Panel\Desktop",
    r"Control Panel\Keyboard",
];

/// Exports every value directly in each of [`CONTROL_PANEL_KEYS`] (never a
/// subkey, never a key delete line) to
/// `<reg_dir>\<file_stamp>-mouse-keyboard.reg`; a missing key is skipped.
/// Read only. The error is the bare cause (pre-flight ruling F8).
pub fn export_control_panel(source: &RegTarget, reg_dir: &Path) -> Result<PathBuf, String> {
    let mut read: Vec<(String, String, ValueSnapshot)> = Vec::new();
    for key in CONTROL_PANEL_KEYS {
        let path = source.path(key);
        let opened = match source.root.open_subkey_with_flags(&path, KEY_READ) {
            Ok(opened) => opened,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("cannot read {}\\{path}: {e}", source.root_name)),
        };
        for item in opened.enum_values() {
            let (name, value) =
                item.map_err(|e| format!("cannot read {}\\{path}: {e}", source.root_name))?;
            let snapshot = ValueSnapshot {
                exists: true,
                kind: value.vtype as u32,
                data: value.bytes,
            };
            read.push((path.clone(), name, snapshot));
        }
    }
    let values: Vec<RegValueExport> = read
        .iter()
        .map(|(path, name, snapshot)| RegValueExport {
            root_name: &source.root_name,
            path: path.clone(),
            name: name.clone(),
            snapshot,
        })
        .collect();
    let file =
        regsnap::unique_reg_file(reg_dir, &format!("{}-mouse-keyboard", clock::file_stamp()));
    regsnap::write_reg_file(&file, &regsnap::export_reg_full(&[], &values))?;
    Ok(file)
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::backups::RegTarget;
    use crate::regsnap::test_support::Sandbox;
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    pub(crate) const USER: &str = "TEST\\user";

    /// A sandbox `{sandbox}\HKCU` whose `Control Panel\Mouse` holds
    /// `MouseSensitivity` = "10" and `Control Panel\Desktop` holds
    /// `UserPreferencesMask`, plus a temp folder for the store and `reg`.
    pub(crate) struct Fixture {
        pub(crate) sb: Sandbox,
        pub(crate) tmp: PathBuf,
        pub(crate) source: RegTarget,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.tmp);
        }
    }

    impl Fixture {
        pub(crate) fn new() -> Self {
            let sb = Sandbox::new();
            let leaf = sb.path.rsplit('\\').next().unwrap().to_string();
            let tmp = std::env::temp_dir().join(format!("wintune-mk-{leaf}"));
            std::fs::create_dir_all(&tmp).unwrap();
            let source = RegTarget {
                root: RegKey::predef(HKEY_CURRENT_USER),
                root_name: "HKEY_CURRENT_USER".into(),
                base: sb.key("HKCU"),
            };
            let f = Fixture { sb, tmp, source };
            f.key(r"Control Panel\Mouse")
                .set_value("MouseSensitivity", &"10")
                .unwrap();
            f.key(r"Control Panel\Desktop")
                .set_raw_value(
                    "UserPreferencesMask",
                    &RegValue {
                        bytes: vec![0x9E, 0x1E, 0x07, 0x80, 0x12, 0x00, 0x00, 0x00],
                        vtype: REG_BINARY,
                    },
                )
                .unwrap();
            f
        }

        /// Creates (or opens for writing) `rel` under the sandbox HKCU.
        pub(crate) fn key(&self, rel: &str) -> RegKey {
            self.sb.root.create_subkey(self.source.path(rel)).unwrap().0
        }

        /// Everything under the sandbox `Control Panel`, subkeys included.
        pub(crate) fn control_panel(&self) -> crate::regsnap::RegistryKeySnapshot {
            crate::regsnap::snapshot_key(&self.sb.root, &self.source.path("Control Panel")).unwrap()
        }

        pub(crate) fn store_path(&self) -> PathBuf {
            self.tmp.join("mouse-keyboard-backups.json")
        }

        pub(crate) fn reg_dir(&self) -> PathBuf {
            self.tmp.join("reg")
        }

        pub(crate) fn reg_files(&self) -> Vec<PathBuf> {
            std::fs::read_dir(self.reg_dir())
                .map(|d| d.map(|e| e.unwrap().path()).collect())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::Fixture;
    use super::*;
    use crate::mouse_keyboard::SettingValue::*;

    #[test]
    fn the_store_json_has_the_spec_shape() {
        let mut store = MkBackupStore::default();
        store.record_first("U", "pointer_precision", "x", Mouse([6, 10, 1]));
        store.record_first("U", "pointer_speed", "x", Number(10));
        store.record_first("U", "swap_buttons", "x", Flag(false));
        let json = concat!(
            r#"{"users":{"U":{"#,
            r#""pointer_precision":{"timestamp":"x","value":{"kind":"Mouse","data":[6,10,1]}},"#,
            r#""pointer_speed":{"timestamp":"x","value":{"kind":"Number","data":10}},"#,
            r#""swap_buttons":{"timestamp":"x","value":{"kind":"Flag","data":false}}}}}"#,
        );
        assert_eq!(serde_json::to_string(&store).unwrap(), json);
        assert_eq!(serde_json::from_str::<MkBackupStore>(json).unwrap(), store);

        let f = Fixture::new();
        assert_eq!(
            load_mk_store(&f.tmp.join("none.json")).unwrap(),
            MkBackupStore::default()
        );
        let unknown = r#"{"users":{"U":{"caret_blink":{"timestamp":"x","value":{"kind":"Number","data":530}}}}}"#;
        let path = f.tmp.join("unknown.json");
        std::fs::write(&path, unknown).unwrap();
        let loaded = load_mk_store(&path).unwrap();
        assert_eq!(loaded.original("U", "caret_blink"), Some(&Number(530)));

        save_mk_store(&store, &f.store_path()).unwrap();
        assert_eq!(load_mk_store(&f.store_path()).unwrap(), store);
        assert!(default_mk_store_path().ends_with("mouse-keyboard-backups.json"));
    }

    #[test]
    fn record_first_keeps_the_oldest_and_forget_drops_an_empty_user() {
        let mut store = MkBackupStore::default();
        store.record_first("U", "pointer_speed", "first", Number(10));
        store.record_first("U", "pointer_speed", "later", Number(14));
        store.record_first("U", "wheel_lines", "first", Number(3));

        let kept = &store.backups_of("U").unwrap()["pointer_speed"];
        assert_eq!(kept.timestamp, "first");
        assert_eq!(kept.value, Number(10));
        assert_eq!(store.original("U", "pointer_speed"), Some(&Number(10)));
        assert!(store.has_backup("U", "pointer_speed"));
        assert!(!store.has_backup("V", "pointer_speed"));
        assert_eq!(store.original("V", "pointer_speed"), None);

        store.forget("U", "pointer_speed");
        assert!(!store.has_backup("U", "pointer_speed"));
        assert!(store.has_backup("U", "wheel_lines"));
        store.forget("U", "wheel_lines");
        assert!(store.users.is_empty());
        store.forget("nobody", "wheel_lines");
        assert!(store.users.is_empty());
    }

    fn utf16_file(path: &std::path::Path) -> String {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE], "UTF-16LE BOM");
        let (pairs, rest) = bytes[2..].as_chunks::<2>();
        assert!(rest.is_empty());
        let units: Vec<u16> = pairs.iter().map(|&p| u16::from_le_bytes(p)).collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn the_reg_net_exports_top_level_values_only() {
        let f = Fixture::new();
        f.key(r"Control Panel\Desktop\WindowMetrics")
            .set_value("BorderWidth", &"-15")
            .unwrap();
        let before = f.control_panel();

        let file = export_control_panel(&f.source, &f.reg_dir()).unwrap();

        assert_eq!(f.reg_files(), std::slice::from_ref(&file));
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with("-mouse-keyboard.reg"), "{name}");
        let text = utf16_file(&file);
        assert!(text.contains(r#""MouseSensitivity"="10""#), "{text}");
        assert!(
            text.contains(r#""UserPreferencesMask"=hex:9e,1e,07,80,12,00,00,00"#),
            "{text}"
        );
        assert!(!text.contains("WindowMetrics"), "{text}");
        assert!(!text.contains("BorderWidth"), "{text}");
        assert!(!text.contains("[-"), "{text}");
        assert!(!text.contains(r"Control Panel\Keyboard]"), "{text}");
        assert!(
            text.contains(&format!(
                r"[HKEY_CURRENT_USER\{}]",
                f.source.path(r"Control Panel\Mouse")
            )),
            "{text}"
        );
        assert_eq!(f.control_panel(), before);
    }
}
