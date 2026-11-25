//! Registry engine of the context menu module: status, apply and restore.
//!
//! Shaped like `tweaks.rs`: validate and snapshot (first touch only), persist
//! the snapshots and a `.reg` export, then write. Everything lives under
//! `HKCU\Software\Classes`, in `WinTune.<id>` verb keys.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use winreg::enums::*;
use winreg::RegKey;

use super::{
    command_line, entry_problems, icon_value, key_name, MenuEntry, Place, Programs, KEY_PREFIX,
};
use crate::associations::OperationReport;
use crate::clock;
use crate::models::app_data_dir;
use crate::regsnap::{self, RegExportEntry, RegistryKeySnapshot};

/// Where entries are written. Production: HKCU itself; tests: a sandbox key.
pub struct MenuTarget {
    pub root: RegKey,
    pub root_name: String,
    /// Prefix for every path (empty in production).
    pub base: String,
}

impl MenuTarget {
    pub fn current_user() -> Self {
        Self {
            root: RegKey::predef(HKEY_CURRENT_USER),
            root_name: "HKEY_CURRENT_USER".into(),
            base: String::new(),
        }
    }

    pub(crate) fn path(&self, rel: &str) -> String {
        if self.base.is_empty() {
            rel.to_string()
        } else {
            format!("{}\\{}", self.base, rel)
        }
    }
}

/// Relative registry path of the verb key of `id` in `place`.
pub fn entry_key(place: Place, id: &str) -> String {
    format!(
        "Software\\Classes\\{}\\shell\\{}",
        place.class_path(),
        key_name(id)
    )
}

fn shell_key(place: Place) -> String {
    format!("Software\\Classes\\{}\\shell", place.class_path())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuBackup {
    /// When the original state was captured (local time).
    pub timestamp: String,
    pub snapshot: RegistryKeySnapshot,
}

/// Original states, per user then per `entry_key` path.
#[derive(Default, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct MenuBackupStore {
    pub users: BTreeMap<String, BTreeMap<String, MenuBackup>>,
}

impl MenuBackupStore {
    pub fn backups_of(&self, user: &str) -> Option<&BTreeMap<String, MenuBackup>> {
        self.users.get(user)
    }
}

pub fn default_menu_store_path() -> PathBuf {
    app_data_dir().join("context-menu-backups.json")
}

pub fn load_menu_store(path: &Path) -> Result<MenuBackupStore, crate::persist::LoadError> {
    crate::persist::load_json(path)
}

pub fn save_menu_store(store: &MenuBackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryStatus {
    Applied,
    NotApplied,
    /// Present, but different from what Apply would write, in these places.
    Changed(Vec<Place>),
}

pub struct MenuOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: String,
    pub store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    pub progress: &'a dyn Fn(usize, usize, &str),
}

/// The three things Apply writes for one entry in one place.
struct Wanted {
    label: String,
    icon: String,
    command: String,
}

/// The `MUIVerb` text for a label: `&` marks an accelerator there, so a
/// literal one is doubled.
fn verb_text(label: &str) -> String {
    label.trim().replace('&', "&&")
}

/// The place whose `shell` key is `key` (an `entry_key`-style path).
pub fn key_place(key: &str) -> Option<Place> {
    let rest = key.strip_prefix(r"Software\Classes\")?;
    Place::ALL.into_iter().find(|p| {
        rest.strip_prefix(p.class_path())
            .is_some_and(|rest| rest.starts_with(r"\shell\"))
    })
}

/// A key as a report names it: "<what> · <place>", never the registry path.
fn key_report_name(what: &str, key: &str) -> String {
    match key_place(key) {
        Some(place) => format!("{what} \u{b7} {}", place.english_name()),
        None => what.to_string(),
    }
}

fn wanted(entry: &MenuEntry, programs: &Programs, place: Place) -> Option<Wanted> {
    let exe = programs.resolve(&entry.program)?;
    Some(Wanted {
        label: verb_text(&entry.label),
        icon: icon_value(entry, &exe),
        command: command_line(&entry.program, programs, place)?,
    })
}

/// Read-only.
pub fn entry_status(target: &MenuTarget, entry: &MenuEntry, programs: &Programs) -> EntryStatus {
    let mut existing = 0;
    let mut differing = Vec::new();
    for &place in &entry.places {
        let path = target.path(&entry_key(place, &entry.id));
        let Ok(key) = target.root.open_subkey_with_flags(&path, KEY_READ) else {
            continue;
        };
        existing += 1;
        let matches = wanted(entry, programs, place).is_some_and(|w| {
            let command = target
                .root
                .open_subkey_with_flags(format!("{path}\\command"), KEY_READ)
                .and_then(|c| c.get_value::<String, _>(""))
                .ok();
            key.get_value::<String, _>("MUIVerb").ok().as_deref() == Some(w.label.as_str())
                && key.get_value::<String, _>("Icon").ok().as_deref() == Some(w.icon.as_str())
                && command.as_deref() == Some(w.command.as_str())
        });
        if !matches {
            differing.push(place);
        }
    }
    if existing == 0 {
        EntryStatus::NotApplied
    } else if differing.is_empty() && existing == entry.places.len() {
        EntryStatus::Applied
    } else {
        // A place that is chosen but has no key counts as differing too.
        let all: Vec<Place> = entry
            .places
            .iter()
            .copied()
            .filter(|p| {
                differing.contains(p)
                    || target
                        .root
                        .open_subkey_with_flags(target.path(&entry_key(*p, &entry.id)), KEY_READ)
                        .is_err()
            })
            .collect();
        EntryStatus::Changed(all)
    }
}

/// Existing `WinTune.*` keys under the four `shell` keys that belong to no
/// (entry, chosen place). Returned as `entry_key`-style relative paths.
pub fn orphan_keys(target: &MenuTarget, entries: &[MenuEntry]) -> Vec<String> {
    let expected: BTreeSet<String> = entries
        .iter()
        .flat_map(|e| e.places.iter().map(|p| entry_key(*p, &e.id).to_lowercase()))
        .collect();
    let prefix = KEY_PREFIX.to_lowercase();
    let mut orphans = Vec::new();
    for place in Place::ALL {
        let shell = shell_key(place);
        let Ok(key) = target
            .root
            .open_subkey_with_flags(target.path(&shell), KEY_READ)
        else {
            continue;
        };
        for name in key.enum_keys().flatten() {
            if !name.to_lowercase().starts_with(&prefix) {
                continue;
            }
            let rel = format!("{shell}\\{name}");
            if !expected.contains(&rel.to_lowercase()) {
                orphans.push(rel);
            }
        }
    }
    orphans
}

fn write_entry_key(target: &MenuTarget, rel: &str, w: &Wanted) -> Result<(), String> {
    let full = target.path(rel);
    regsnap::delete_tree(&target.root, &full)?;
    let (key, _) = target
        .root
        .create_subkey(&full)
        .map_err(|e| format!("Cannot create {}: {}", rel, e))?;
    key.set_value("MUIVerb", &w.label)
        .map_err(|e| format!("Cannot write {}\\MUIVerb: {}", rel, e))?;
    key.set_value("Icon", &w.icon)
        .map_err(|e| format!("Cannot write {}\\Icon: {}", rel, e))?;
    let (command, _) = key
        .create_subkey("command")
        .map_err(|e| format!("Cannot create {}\\command: {}", rel, e))?;
    command
        .set_value("", &w.command)
        .map_err(|e| format!("Cannot write {}\\command: {}", rel, e))
}

/// Makes the registry match `entries`: writes every (entry, place) key and
/// removes `WinTune.*` keys that belong to no entry. The original state of
/// each touched key stays recorded until [`restore_keys`].
pub fn apply_entries(
    target: &MenuTarget,
    store: &mut MenuBackupStore,
    entries: &[MenuEntry],
    programs: &Programs,
    opts: &MenuOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let now = clock::now_local();

    // Phase 1: validate, then snapshot (first touch only); nothing written.
    let problems = entry_problems(entries, programs);
    if !problems.is_empty() {
        for (id, problem) in problems {
            report.errors.push(format!("{}: {:?}", id, problem));
        }
        return report;
    }
    // (entry index, key path, what to write)
    let mut to_write: Vec<(usize, String, Wanted)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        for &place in &entry.places {
            match wanted(entry, programs, place) {
                Some(w) => to_write.push((index, entry_key(place, &entry.id), w)),
                None => {
                    // The program vanished after validation: same rule as a
                    // validation problem, nothing is written.
                    report
                        .errors
                        .push(format!("Nothing applied: {}: program not found", entry.id));
                    return report;
                }
            }
        }
    }
    let to_delete = orphan_keys(target, entries);
    let steps = to_write.len() + to_delete.len();
    if steps == 0 {
        return report;
    }

    let mut failed_entries: BTreeSet<usize> = BTreeSet::new();
    let mut snapshots: Vec<(String, RegistryKeySnapshot)> = Vec::new();
    let all_keys = to_write
        .iter()
        .map(|(i, k, _)| (Some(*i), k.clone()))
        .chain(to_delete.iter().map(|k| (None, k.clone())));
    let mut skipped: BTreeSet<String> = BTreeSet::new();
    for (n, (owner, key)) in all_keys.enumerate() {
        (opts.progress)(n + 1, steps * 2, &format!("Snapshotting {}", key));
        match regsnap::snapshot_key(&target.root, &target.path(&key)) {
            Ok(current) => {
                store
                    .users
                    .entry(opts.user.clone())
                    .or_default()
                    .entry(key.clone())
                    .or_insert_with(|| MenuBackup {
                        timestamp: now.clone(),
                        snapshot: current.clone(),
                    });
                snapshots.push((key, current));
            }
            Err(e) => {
                report.errors.push(format!("{}: {}", key, e));
                if let Some(i) = owner {
                    failed_entries.insert(i);
                }
                skipped.insert(key);
            }
        }
    }
    if snapshots.is_empty() {
        return report;
    }

    // Phase 2: persist snapshots + .reg safety net before any write.
    if let Err(e) = save_menu_store(store, &opts.store_path) {
        report
            .errors
            .push(format!("Nothing applied: cannot save snapshots ({})", e));
        return report;
    }
    let exports: Vec<RegExportEntry> = snapshots
        .iter()
        .map(|(key, snapshot)| RegExportEntry {
            root_name: &target.root_name,
            path: target.path(key),
            snapshot,
        })
        .collect();
    let reg_file = regsnap::unique_reg_file(
        &opts.reg_backup_dir,
        &format!("{}-context-menu-before-apply", clock::file_stamp()),
    );
    if let Err(e) = regsnap::write_reg_file(&reg_file, &regsnap::export_reg_full(&exports, &[])) {
        report
            .errors
            .push(format!("Nothing applied: cannot write .reg backup ({})", e));
        return report;
    }
    report.reg_backup_file = Some(reg_file);

    // Phase 3: writes (delete-then-write), then orphan removal.
    let mut done = steps;
    for (index, key, w) in &to_write {
        if skipped.contains(key) {
            continue;
        }
        if let Err(e) = write_entry_key(target, key, w) {
            report.errors.push(format!("{}: {}", entries[*index].id, e));
            failed_entries.insert(*index);
        }
        done += 1;
        (opts.progress)(done, steps * 2, &format!("Writing {}", key));
    }
    for (index, entry) in entries.iter().enumerate() {
        if !failed_entries.contains(&index) && !entry.places.is_empty() {
            report.succeeded.push(entry.label.clone());
        }
    }
    for key in &to_delete {
        if skipped.contains(key) {
            continue;
        }
        match regsnap::delete_tree(&target.root, &target.path(key)) {
            Ok(()) => report.removed.push(key_report_name("Removed entry", key)),
            Err(e) => report.errors.push(format!("{}: {}", key, e)),
        }
        done += 1;
        (opts.progress)(done, steps * 2, &format!("Removing {}", key));
    }
    report
}

/// Puts each key back exactly as it was before WinTune first changed it,
/// then forgets its snapshot.
pub fn restore_keys(
    target: &MenuTarget,
    store: &mut MenuBackupStore,
    keys: &[String],
    opts: &MenuOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let total = keys.len();

    for (i, key) in keys.iter().enumerate() {
        (opts.progress)(i, total, &format!("Restoring {}", key));
        let Some(backup) = store
            .users
            .get(&opts.user)
            .and_then(|b| b.get(key))
            .cloned()
        else {
            report.errors.push(format!(
                "{}: no original state recorded for {}",
                key, opts.user
            ));
            continue;
        };
        match regsnap::restore_key(&target.root, &target.path(key), &backup.snapshot) {
            Ok(()) => {
                if let Some(user) = store.users.get_mut(&opts.user) {
                    user.remove(key);
                }
                report.succeeded.push(key_report_name("Entry", key));
            }
            Err(e) => report.errors.push(format!(
                "{}: {} (snapshot kept, restore can be retried)",
                key, e
            )),
        }
    }
    store.users.retain(|_, b| !b.is_empty());

    if !report.succeeded.is_empty() {
        if let Err(e) = save_menu_store(store, &opts.store_path) {
            report.errors.push(format!("Cannot save snapshots: {}", e));
        }
    }
    (opts.progress)(total, total, "Restore finished");
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_menu::ProgramChoice;
    use crate::regsnap::test_support::Sandbox;

    struct Env {
        sb: Sandbox,
        dir: PathBuf,
        exe: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let sb = Sandbox::new();
            let dir = std::env::temp_dir().join(format!(
                "wintune-menu-{}",
                sb.path.rsplit('\\').next().unwrap()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let exe = dir.join("app.exe");
            std::fs::write(&exe, b"x").unwrap();
            Self { sb, dir, exe }
        }

        fn target(&self) -> MenuTarget {
            MenuTarget {
                root: RegKey::predef(HKEY_CURRENT_USER),
                root_name: "HKEY_CURRENT_USER".into(),
                base: self.sb.path.clone(),
            }
        }

        fn opts(&self, user: &str) -> MenuOptions<'static> {
            MenuOptions {
                user: user.into(),
                store_path: self.dir.join("context-menu-backups.json"),
                reg_backup_dir: self.dir.join("reg-backups"),
                progress: &|_, _, _| {},
            }
        }

        fn entry(&self, id: &str, label: &str, places: &[Place]) -> MenuEntry {
            MenuEntry {
                id: id.into(),
                label: label.into(),
                program: ProgramChoice::Custom(self.exe.display().to_string()),
                places: places.iter().copied().collect(),
                icon: None,
            }
        }

        fn abs(&self, rel: &str) -> String {
            format!("{}\\{}", self.sb.path, rel)
        }

        fn text(&self, rel: &str, name: &str) -> Option<String> {
            self.sb
                .root
                .open_subkey(self.abs(rel))
                .ok()?
                .get_value(name)
                .ok()
        }

        fn exists(&self, rel: &str) -> bool {
            self.sb.root.open_subkey(self.abs(rel)).is_ok()
        }

        fn set_text(&self, rel: &str, name: &str, value: &str) {
            let (k, _) = self.sb.root.create_subkey(self.abs(rel)).unwrap();
            k.set_value(name, &value).unwrap();
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn apply(
        env: &Env,
        user: &str,
        store: &mut MenuBackupStore,
        entries: &[MenuEntry],
    ) -> OperationReport {
        apply_entries(
            &env.target(),
            store,
            entries,
            &Programs::default(),
            &env.opts(user),
        )
    }

    const FOLDER: &str = r"Software\Classes\Directory\shell\WinTune.e1";

    #[test]
    fn apply_writes_one_key_per_place_with_label_icon_and_command() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let entry = env.entry("e1", "Open with App", &[Place::Folder, Place::Background]);
        let report = apply(&env, "A", &mut store, std::slice::from_ref(&entry));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.reg_backup_file.is_some());
        assert_eq!(report.succeeded, ["Open with App"]);

        let exe = env.exe.display();
        assert_eq!(env.text(FOLDER, "MUIVerb").unwrap(), "Open with App");
        assert_eq!(env.text(FOLDER, "Icon").unwrap(), format!("\"{exe}\""));
        let command = format!("{FOLDER}\\command");
        assert_eq!(env.text(&command, "").unwrap(), format!("\"{exe}\" \"%1\""));
        let background = r"Software\Classes\Directory\Background\shell\WinTune.e1\command";
        assert_eq!(
            env.text(background, "").unwrap(),
            format!(r#""{exe}" "%V\.""#)
        );
        assert!(!env.exists(r"Software\Classes\Drive\shell\WinTune.e1"));
        assert!(!env.exists(r"Software\Classes\*\shell\WinTune.e1"));
        assert_eq!(
            entry_status(&env.target(), &entry, &Programs::default()),
            EntryStatus::Applied
        );
    }

    #[test]
    fn status_is_not_applied_before_apply_and_changed_after_an_outside_edit() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let entry = env.entry("e1", "Open", &[Place::Folder, Place::Files]);
        let programs = Programs::default();
        assert_eq!(
            entry_status(&env.target(), &entry, &programs),
            EntryStatus::NotApplied
        );
        apply(&env, "A", &mut store, std::slice::from_ref(&entry));
        env.set_text(FOLDER, "MUIVerb", "Edited");
        assert_eq!(
            entry_status(&env.target(), &entry, &programs),
            EntryStatus::Changed(vec![Place::Folder])
        );
        // A chosen place whose key vanished counts as changed too.
        let wider = env.entry("e1", "Edited", &[Place::Folder, Place::Files, Place::Drive]);
        env.set_text(FOLDER, "MUIVerb", "Edited");
        assert_eq!(
            entry_status(&env.target(), &wider, &programs),
            EntryStatus::Changed(vec![Place::Drive, Place::Files])
        );
    }

    #[test]
    fn shrinking_places_or_deleting_an_entry_removes_its_keys_after_apply() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let files = r"Software\Classes\*\shell\WinTune.e1";
        apply(
            &env,
            "A",
            &mut store,
            &[env.entry("e1", "Open", &[Place::Folder, Place::Files])],
        );
        assert!(env.exists(FOLDER) && env.exists(files));
        let one = env.entry("e1", "Open", &[Place::Folder]);
        let report = apply(&env, "A", &mut store, std::slice::from_ref(&one));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(env.exists(FOLDER) && !env.exists(files));
        let report = apply(&env, "A", &mut store, &[]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!env.exists(FOLDER));
        assert!(orphan_keys(&env.target(), &[]).is_empty());
    }

    #[test]
    fn apply_never_touches_keys_without_the_prefix() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let other = r"Software\Classes\Directory\shell\Other";
        env.set_text(other, "MUIVerb", "Keep");
        apply(
            &env,
            "A",
            &mut store,
            &[env.entry("e1", "Open", &[Place::Folder])],
        );
        assert_eq!(env.text(other, "MUIVerb").unwrap(), "Keep");
        apply(&env, "A", &mut store, &[]);
        assert_eq!(env.text(other, "MUIVerb").unwrap(), "Keep");
        assert!(orphan_keys(&env.target(), &[]).is_empty());
    }

    #[test]
    fn a_problem_blocks_the_whole_apply() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let entries = [
            env.entry("e1", "Fine", &[Place::Folder]),
            env.entry("e2", "  ", &[Place::Folder]),
        ];
        let report = apply(&env, "A", &mut store, &entries);
        assert!(!report.errors.is_empty());
        assert!(report.succeeded.is_empty());
        assert!(!env.exists(FOLDER));
        assert!(!env.opts("A").store_path.exists());
    }

    #[test]
    fn restore_puts_back_the_state_before_the_first_apply() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.set_text(FOLDER, "MUIVerb", "Before");
        let files = r"Software\Classes\*\shell\WinTune.e1";
        apply(
            &env,
            "A",
            &mut store,
            &[env.entry("e1", "Open", &[Place::Folder, Place::Files])],
        );
        assert_eq!(env.text(FOLDER, "MUIVerb").unwrap(), "Open");

        let keys = vec![FOLDER.to_string(), files.to_string()];
        let report = restore_keys(&env.target(), &mut store, &keys, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(FOLDER, "MUIVerb").unwrap(), "Before");
        assert!(!env.exists(files));
        assert!(store.backups_of("A").is_none());
        let again = restore_keys(&env.target(), &mut store, &keys[..1], &env.opts("A"));
        assert_eq!(again.errors.len(), 1);
    }

    #[test]
    fn snapshots_are_per_user() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.set_text(FOLDER, "MUIVerb", "Original");
        let first = env.entry("e1", "First", &[Place::Folder]);
        let second = env.entry("e1", "Second", &[Place::Folder]);
        apply(&env, "A", &mut store, &[first]);
        apply(&env, "B", &mut store, &[second]);
        let of = |user: &str| {
            store.users[user][FOLDER]
                .snapshot
                .values
                .get("MUIVerb")
                .map(|(_, b)| b.clone())
        };
        let utf16 = |s: &str| -> Vec<u8> {
            s.encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect()
        };
        assert_eq!(of("A"), Some(utf16("Original")));
        assert_eq!(of("B"), Some(utf16("First")));
        assert!(store.backups_of("C").is_none());
    }

    #[test]
    fn an_unwritable_store_blocks_apply() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let blocker = env.dir.join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let mut opts = env.opts("A");
        opts.store_path = blocker.join("store.json");
        let entry = env.entry("e1", "Open", &[Place::Folder]);
        let report = apply_entries(
            &env.target(),
            &mut store,
            &[entry],
            &Programs::default(),
            &opts,
        );
        assert!(
            report.errors[0].starts_with("Nothing applied"),
            "{:?}",
            report.errors
        );
        assert!(!env.exists(FOLDER));
    }

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn saved_verb(store: &MenuBackupStore, user: &str, key: &str) -> Option<Vec<u8>> {
        store.users[user][key]
            .snapshot
            .values
            .get("MUIVerb")
            .map(|(_, b)| b.clone())
    }

    #[test]
    fn a_second_apply_keeps_the_snapshot_from_before_the_first() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.set_text(FOLDER, "MUIVerb", "Before");
        let first = env.entry("e1", "First", &[Place::Folder]);
        apply(&env, "A", &mut store, &[first]);
        let second = env.entry("e1", "Second", &[Place::Folder]);
        let report = apply(&env, "A", &mut store, &[second]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(FOLDER, "MUIVerb").unwrap(), "Second");
        assert_eq!(saved_verb(&store, "A", FOLDER), Some(utf16("Before")));

        let keys = vec![FOLDER.to_string()];
        let report = restore_keys(&env.target(), &mut store, &keys, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(FOLDER, "MUIVerb").unwrap(), "Before");
    }

    #[test]
    fn an_orphan_with_content_is_deleted_snapshotted_and_restorable() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let orphan = r"Software\Classes\Directory\shell\WinTune.e9";
        env.set_text(orphan, "MUIVerb", "X");
        let report = apply(&env, "A", &mut store, &[]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!env.exists(orphan));
        assert_eq!(saved_verb(&store, "A", orphan), Some(utf16("X")));
        // Named for people, not counted as an applied entry, no registry path.
        assert!(report.succeeded.is_empty(), "{:?}", report.succeeded);
        assert_eq!(report.removed, ["Removed entry \u{b7} Folders"]);

        let keys = vec![orphan.to_string()];
        let report = restore_keys(&env.target(), &mut store, &keys, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(orphan, "MUIVerb").unwrap(), "X");
        assert_eq!(report.succeeded, ["Entry \u{b7} Folders"]);
    }

    #[test]
    fn a_failed_reg_backup_blocks_every_write() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let blocker = env.dir.join("reg-blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let mut opts = env.opts("A");
        opts.reg_backup_dir = blocker.join("sub");
        let entry = env.entry("e1", "Open", &[Place::Folder]);
        let report = apply_entries(
            &env.target(),
            &mut store,
            &[entry],
            &Programs::default(),
            &opts,
        );
        assert!(
            report.errors[0].starts_with("Nothing applied"),
            "{:?}",
            report.errors
        );
        assert!(!env.exists(FOLDER));
        assert!(report.succeeded.is_empty());
    }

    #[test]
    fn an_ampersand_in_a_label_is_doubled_and_compared_the_same_way() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let entry = env.entry("e1", "Tom & Jerry", &[Place::Folder]);
        let report = apply(&env, "A", &mut store, std::slice::from_ref(&entry));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(FOLDER, "MUIVerb").unwrap(), "Tom && Jerry");
        assert_eq!(
            entry_status(&env.target(), &entry, &Programs::default()),
            EntryStatus::Applied
        );
    }

    #[test]
    fn key_place_reads_the_class_of_a_verb_key() {
        for place in Place::ALL {
            assert_eq!(key_place(&entry_key(place, "e1")), Some(place));
        }
        assert_eq!(key_place(r"Software\Classes\Other\shell\WinTune.e1"), None);
    }
}
