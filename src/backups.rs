//! Shared backup helpers for every per-user registry module (tweaks, context
//! menu): where it writes, what it snapshots, the snapshot store, the
//! save-then-export safety net and the restore loop.

use crate::associations::OperationReport;
use crate::clock;
use crate::regsnap::{self, RegExportEntry, RegValueExport, RegistryKeySnapshot, ValueSnapshot};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

/// Where a module reads and writes. Production: a hive itself; tests: a
/// sandbox key.
pub struct RegTarget {
    pub root: RegKey,
    pub root_name: String,
    /// Prefix for every path (empty in production).
    pub base: String,
}

impl RegTarget {
    pub fn current_user() -> Self {
        Self {
            root: RegKey::predef(HKEY_CURRENT_USER),
            root_name: "HKEY_CURRENT_USER".into(),
            base: String::new(),
        }
    }

    pub fn local_machine() -> Self {
        Self {
            root: RegKey::predef(HKEY_LOCAL_MACHINE),
            root_name: "HKEY_LOCAL_MACHINE".into(),
            base: String::new(),
        }
    }

    /// `rel` under this target's base.
    pub fn path(&self, rel: &str) -> String {
        if self.base.is_empty() {
            rel.to_string()
        } else {
            format!("{}\\{}", self.base, rel)
        }
    }
}

/// One thing a module snapshots: a whole key, or one value of a key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Key(String),
    Value { path: String, name: String },
}

/// Separates path and value name in a value slot id. Never occurs in a key
/// path, so a key path is its own id.
const VALUE_SEPARATOR: &str = "\\\\";

impl Slot {
    /// Stable id: the key path, or `{path}\\{name}` for a value.
    pub fn id(&self) -> String {
        match self {
            Slot::Key(path) => path.clone(),
            Slot::Value { path, name } => format!("{path}{VALUE_SEPARATOR}{name}"),
        }
    }

    pub fn from_id(id: &str) -> Slot {
        match id.split_once(VALUE_SEPARATOR) {
            Some((path, name)) => Slot::Value {
                path: path.into(),
                name: name.into(),
            },
            None => Slot::Key(id.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum Snapshot {
    Value(ValueSnapshot),
    Key(RegistryKeySnapshot),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    /// When the original state was captured (local time).
    pub timestamp: String,
    #[serde(deserialize_with = "snapshot_or_bare_key")]
    pub snapshot: Snapshot,
}

/// Accepts the tagged form, or a bare key snapshot as written by the
/// context menu store before values could be snapshotted.
fn snapshot_or_bare_key<'de, D: Deserializer<'de>>(d: D) -> Result<Snapshot, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Tagged(Snapshot),
        Bare(RegistryKeySnapshot),
    }
    Ok(match Stored::deserialize(d)? {
        Stored::Tagged(s) => s,
        Stored::Bare(k) => Snapshot::Key(k),
    })
}

/// Original states, per user then per id. HKCU is per user, so a snapshot
/// only applies to the account that took it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserBackups {
    pub users: BTreeMap<String, BTreeMap<String, Backup>>,
}

impl UserBackups {
    pub fn backups_of(&self, user: &str) -> Option<&BTreeMap<String, Backup>> {
        self.users.get(user)
    }

    pub fn has_backup(&self, user: &str, id: &str) -> bool {
        self.backups_of(user).is_some_and(|b| b.contains_key(id))
    }

    /// Records `snapshot` unless `id` already has one: the first is the
    /// original.
    pub fn record_first(&mut self, user: &str, id: &str, timestamp: &str, snapshot: Snapshot) {
        self.users
            .entry(user.to_string())
            .or_default()
            .entry(id.to_string())
            .or_insert_with(|| Backup {
                timestamp: timestamp.to_string(),
                snapshot,
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

/// Read-only.
pub fn take_snapshot(t: &RegTarget, slot: &Slot) -> Result<Snapshot, String> {
    match slot {
        Slot::Key(path) => Ok(Snapshot::Key(regsnap::snapshot_key(
            &t.root,
            &t.path(path),
        )?)),
        Slot::Value { path, name } => Ok(Snapshot::Value(regsnap::snapshot_value(
            &t.root,
            &t.path(path),
            name,
        )?)),
    }
}

pub fn restore_snapshot(t: &RegTarget, slot: &Slot, s: &Snapshot) -> Result<(), String> {
    match (slot, s) {
        (Slot::Key(path), Snapshot::Key(k)) => regsnap::restore_key(&t.root, &t.path(path), k),
        (Slot::Value { path, name }, Snapshot::Value(v)) => {
            regsnap::restore_value(&t.root, &t.path(path), name, v)
        }
        _ => Err("the recorded snapshot does not match this key or value".to_string()),
    }
}

/// The safety net before any write: saves `store`, then exports every taken
/// snapshot to `<reg_dir>\<file_stamp>-<stem>.reg`. Returns that file.
pub fn save_then_export(
    store: &UserBackups,
    store_path: &Path,
    t: &RegTarget,
    taken: &[(Slot, Snapshot)],
    reg_dir: &Path,
    stem: &str,
) -> Result<PathBuf, String> {
    crate::persist::save_json(store, store_path)
        .map_err(|e| format!("Nothing applied: cannot save snapshots ({})", e))?;
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for (slot, snapshot) in taken {
        match (slot, snapshot) {
            (Slot::Key(path), Snapshot::Key(k)) => keys.push(RegExportEntry {
                root_name: &t.root_name,
                path: t.path(path),
                snapshot: k,
            }),
            (Slot::Value { path, name }, Snapshot::Value(v)) => values.push(RegValueExport {
                root_name: &t.root_name,
                path: t.path(path),
                name: name.clone(),
                snapshot: v,
            }),
            _ => {}
        }
    }
    let reg_file = regsnap::unique_reg_file(reg_dir, &format!("{}-{}", clock::file_stamp(), stem));
    regsnap::write_reg_file(&reg_file, &regsnap::export_reg_full(&keys, &values))
        .map_err(|e| format!("Nothing applied: cannot write .reg backup ({})", e))?;
    Ok(reg_file)
}

/// Puts each id back exactly as it was first recorded for `user`, then
/// forgets its snapshot. A failed restore keeps the snapshot.
#[allow(clippy::too_many_arguments)]
pub fn restore_ids(
    t: &RegTarget,
    store: &mut UserBackups,
    user: &str,
    ids: &[String],
    store_path: &Path,
    progress: &dyn Fn(usize, usize, &str),
    slot_of: &dyn Fn(&str) -> Result<Slot, String>,
    name_of: &dyn Fn(&str) -> String,
) -> OperationReport {
    let mut report = OperationReport::default();
    let total = ids.len();

    for (i, id) in ids.iter().enumerate() {
        progress(i, total, &format!("Restoring {}", id));
        let slot = match slot_of(id) {
            Ok(slot) => slot,
            Err(e) => {
                report.errors.push(format!("{}: {}", id, e));
                continue;
            }
        };
        let Some(backup) = store.backups_of(user).and_then(|b| b.get(id)).cloned() else {
            report
                .errors
                .push(format!("{}: no original state recorded for {}", id, user));
            continue;
        };
        match restore_snapshot(t, &slot, &backup.snapshot) {
            Ok(()) => {
                store.forget(user, id);
                report.succeeded.push(name_of(id));
            }
            Err(e) => report.errors.push(format!(
                "{}: {} (snapshot kept, restore can be retried)",
                id, e
            )),
        }
    }

    if !report.succeeded.is_empty() {
        if let Err(e) = crate::persist::save_json(store, store_path) {
            report.errors.push(format!("Cannot save snapshots: {}", e));
        }
    }
    progress(total, total, "Restore finished");
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regsnap::test_support::Sandbox;

    const BLOCKED: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";
    const FOLDER: &str = r"Software\Classes\Directory\shell\WinTune.e1";

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    #[test]
    fn slot_ids_round_trip() {
        let slots = [
            Slot::Key(r"Software\Classes\Directory\shell\A".into()),
            Slot::Value {
                path: BLOCKED.into(),
                name: "{ABC}".into(),
            },
        ];
        for slot in slots {
            assert_eq!(Slot::from_id(&slot.id()), slot);
        }
        assert_eq!(
            Slot::Value {
                path: BLOCKED.into(),
                name: "{ABC}".into(),
            }
            .id(),
            format!("{BLOCKED}\\\\{{ABC}}")
        );
    }

    #[test]
    fn tweak_store_json_is_unchanged() {
        let mut store = UserBackups::default();
        store.record_first(
            "U",
            "t",
            "x",
            Snapshot::Value(ValueSnapshot {
                exists: true,
                kind: 4,
                data: vec![1, 0, 0, 0],
            }),
        );
        let fixture = concat!(
            r#"{"users":{"U":{"t":{"timestamp":"x","snapshot":"#,
            r#"{"kind":"Value","data":{"exists":true,"kind":4,"data":[1,0,0,0]}}}}}}"#,
        );
        assert_eq!(serde_json::to_string(&store).unwrap(), fixture);
        let back: UserBackups = serde_json::from_str(fixture).unwrap();
        assert_eq!(back, store);
    }

    #[test]
    fn a_pr1_menu_store_still_loads_and_restores() {
        let mut bare = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        bare.values
            .insert("MUIVerb".into(), (REG_SZ as u32, utf16("Before")));
        // PR 1 stored `snapshot` as a bare RegistryKeySnapshot.
        let json = serde_json::json!({
            "users": { "U": { FOLDER: {
                "timestamp": "x",
                "snapshot": serde_json::to_value(&bare).unwrap(),
            } } }
        })
        .to_string();
        let mut store: UserBackups = serde_json::from_str(&json).unwrap();
        assert_eq!(
            store.backups_of("U").unwrap()[FOLDER].snapshot,
            Snapshot::Key(bare)
        );

        let sb = Sandbox::new();
        let dir = std::env::temp_dir().join(format!(
            "wintune-backups-{}",
            sb.path.rsplit('\\').next().unwrap()
        ));
        let target = RegTarget {
            root: RegKey::predef(HKEY_CURRENT_USER),
            root_name: "HKEY_CURRENT_USER".into(),
            base: sb.path.clone(),
        };
        let report = restore_ids(
            &target,
            &mut store,
            "U",
            &[FOLDER.to_string()],
            &dir.join("store.json"),
            &|_, _, _| {},
            &|id| Ok(Slot::from_id(id)),
            &|id| id.to_string(),
        );
        let _ = std::fs::remove_dir_all(&dir);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, [FOLDER]);
        let verb: String = sb
            .root
            .open_subkey(sb.key(FOLDER))
            .unwrap()
            .get_value("MUIVerb")
            .unwrap();
        assert_eq!(verb, "Before");
        assert!(store.users.is_empty());
    }

    #[test]
    fn record_first_keeps_the_oldest_snapshot() {
        let mut store = UserBackups::default();
        let first = Snapshot::Value(ValueSnapshot::default());
        let later = Snapshot::Value(ValueSnapshot {
            exists: true,
            kind: 4,
            data: vec![2, 0, 0, 0],
        });
        store.record_first("U", "t", "first", first.clone());
        store.record_first("U", "t", "later", later);

        let kept = &store.backups_of("U").unwrap()["t"];
        assert_eq!(kept.timestamp, "first");
        assert_eq!(kept.snapshot, first);
        assert!(store.has_backup("U", "t"));
        assert!(!store.has_backup("V", "t"));
    }

    #[test]
    fn forget_drops_an_empty_user() {
        let mut store = UserBackups::default();
        let s = Snapshot::Key(RegistryKeySnapshot::default());
        store.record_first("U", "a", "x", s.clone());
        store.record_first("U", "b", "x", s);

        store.forget("U", "a");
        assert!(!store.has_backup("U", "a"));
        assert!(store.has_backup("U", "b"));
        store.forget("U", "b");
        assert!(store.users.is_empty());
        store.forget("nobody", "a");
        assert!(store.users.is_empty());
    }
}
