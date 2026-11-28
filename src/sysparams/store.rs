//! The snapshot store shared by the SPI modules, and the read-only `.reg`
//! export taken before every Apply.

use super::SettingValue;
use crate::backups::RegTarget;
use crate::clock;
use crate::regsnap::{self, RegValueExport, ValueSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winreg::enums::KEY_READ;

/// One original value, as first read before WinTune changed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpiBackup {
    /// When it was read (local time).
    pub timestamp: String,
    pub value: SettingValue,
    /// The id of the setting whose gate made this snapshot necessary, when
    /// it was taken only as that setting's companion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion_of: Option<String>,
}

/// Original values, per user then per setting key. Same shape and rules as
/// `backups::UserBackups` (spec 5.3); a key this version does not know still
/// loads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpiBackupStore {
    pub users: BTreeMap<String, BTreeMap<String, SpiBackup>>,
}

impl SpiBackupStore {
    pub fn backups_of(&self, user: &str) -> Option<&BTreeMap<String, SpiBackup>> {
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
            .or_insert_with(|| SpiBackup {
                timestamp: timestamp.to_string(),
                value,
                companion_of: None,
            });
    }

    /// Records `value` as a companion of `master` unless `id` already has a
    /// snapshot (own or companion): an existing one is kept as it is.
    pub fn record_companion(
        &mut self,
        user: &str,
        id: &str,
        timestamp: &str,
        value: SettingValue,
        master: &str,
    ) {
        self.users
            .entry(user.to_string())
            .or_default()
            .entry(id.to_string())
            .or_insert_with(|| SpiBackup {
                timestamp: timestamp.to_string(),
                value,
                companion_of: Some(master.to_string()),
            });
    }

    /// The master `id` is a companion of, if its snapshot is one.
    pub fn companion_of(&self, user: &str, id: &str) -> Option<&str> {
        self.backups_of(user)?.get(id)?.companion_of.as_deref()
    }

    /// Makes the snapshot of `id` an ordinary one.
    pub fn clear_companion(&mut self, user: &str, id: &str) {
        if let Some(b) = self.users.get_mut(user).and_then(|b| b.get_mut(id)) {
            b.companion_of = None;
        }
    }

    /// The ids whose snapshot is a companion of `master`, in key order.
    pub fn companions_of(&self, user: &str, master: &str) -> Vec<String> {
        self.backups_of(user)
            .map(|b| {
                b.iter()
                    .filter(|(_, v)| v.companion_of.as_deref() == Some(master))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The snapshots that are not companions, in key order.
    pub fn own_backups(&self, user: &str) -> Vec<(&String, &SpiBackup)> {
        self.backups_of(user)
            .map(|b| b.iter().filter(|(_, v)| v.companion_of.is_none()).collect())
            .unwrap_or_default()
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

/// Exports every value directly in each of `keys` (never a subkey, never a
/// key delete line) to `<reg_dir>\<file_stamp>-<label>.reg`; a missing key
/// is skipped. Read only. The error is the bare cause (pre-flight ruling F8).
pub fn export_values(
    source: &RegTarget,
    keys: &[&str],
    reg_dir: &Path,
    label: &str,
) -> Result<PathBuf, String> {
    let mut read: Vec<(String, String, ValueSnapshot)> = Vec::new();
    for key in keys {
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
    let file = regsnap::unique_reg_file(reg_dir, &format!("{}-{label}", clock::file_stamp()));
    regsnap::write_reg_file(&file, &regsnap::export_reg_full(&[], &values))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysparams::test_support::Fixture;
    use SettingValue::*;

    #[test]
    fn a_backup_leaves_companion_of_out_when_empty() {
        let mut store = SpiBackupStore::default();
        store.record_first("U", "menu_fade", "x", Flag(true));
        let first = r#"{"users":{"U":{"menu_fade":{"timestamp":"x","value":{"kind":"Flag","data":true}}}}}"#;
        assert_eq!(serde_json::to_string(&store).unwrap(), first);
        assert_eq!(
            serde_json::from_str::<SpiBackupStore>(first).unwrap(),
            store
        );

        store.record_companion("U", "cursor_shadow", "x", Flag(false), "ui_effects");
        let both = concat!(
            r#"{"users":{"U":{"#,
            r#""cursor_shadow":{"timestamp":"x","value":{"kind":"Flag","data":false},"companion_of":"ui_effects"},"#,
            r#""menu_fade":{"timestamp":"x","value":{"kind":"Flag","data":true}}}}}"#,
        );
        assert_eq!(serde_json::to_string(&store).unwrap(), both);
        assert_eq!(serde_json::from_str::<SpiBackupStore>(both).unwrap(), store);
    }

    #[test]
    fn record_companion_keeps_an_existing_snapshot() {
        let mut store = SpiBackupStore::default();
        store.record_first("U", "cursor_shadow", "own", Flag(true));
        store.record_companion("U", "cursor_shadow", "later", Flag(false), "ui_effects");
        assert_eq!(store.original("U", "cursor_shadow"), Some(&Flag(true)));
        assert_eq!(store.companion_of("U", "cursor_shadow"), None);

        store.record_companion("U", "menu_fade", "a", Flag(true), "ui_effects");
        store.record_companion("U", "menu_fade", "b", Flag(false), "other");
        assert_eq!(store.original("U", "menu_fade"), Some(&Flag(true)));
        assert_eq!(store.companion_of("U", "menu_fade"), Some("ui_effects"));
    }

    #[test]
    fn companions_are_listed_cleared_and_left_out_of_own_backups() {
        let mut store = SpiBackupStore::default();
        store.record_companion("U", "menu_fade", "x", Flag(true), "ui_effects");
        store.record_companion("U", "combo_slide", "x", Flag(true), "ui_effects");
        store.record_first("U", "caret_width", "x", Number(1));

        assert_eq!(
            store.companions_of("U", "ui_effects"),
            vec!["combo_slide".to_string(), "menu_fade".to_string()]
        );
        assert_eq!(store.companions_of("V", "ui_effects"), Vec::<String>::new());
        let own: Vec<&str> = store
            .own_backups("U")
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(own, ["caret_width"]);

        store.clear_companion("U", "menu_fade");
        assert_eq!(store.companion_of("U", "menu_fade"), None);
        assert_eq!(
            store.companions_of("U", "ui_effects"),
            vec!["combo_slide".to_string()]
        );
        let own: Vec<&str> = store
            .own_backups("U")
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(own, ["caret_width", "menu_fade"]);
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
    fn export_values_takes_the_key_list_and_the_label() {
        let f = Fixture::new();
        f.key("A").set_value("V1", &"1").unwrap();
        f.key(r"A\Sub").set_value("V2", &"2").unwrap();

        let file = export_values(&f.source, &["A", r"A\Sub", "B"], &f.reg_dir(), "label").unwrap();
        assert_eq!(f.reg_files(), std::slice::from_ref(&file));
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with("-label.reg"), "{name}");
        let text = utf16_file(&file);
        assert!(text.contains(r#""V1"="1""#), "{text}");
        assert!(text.contains(r#""V2""#), "{text}");
        assert!(!text.contains("[-"), "{text}");

        let only_a = export_values(&f.source, &["A"], &f.reg_dir(), "label").unwrap();
        let text = utf16_file(&only_a);
        assert!(text.contains(r#""V1"="1""#), "{text}");
        assert!(!text.contains("V2"), "{text}");
    }
}
