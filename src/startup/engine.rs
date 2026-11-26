//! Apply and restore for the Startup apps module.
//!
//! Shaped like the context menu engine: validate (any problem refuses the
//! whole Apply), snapshot on first touch, save the store and a `.reg` export,
//! then write. The only thing written is a `StartupApproved` value under HKCU;
//! HKLM, `Run` values and Startup folder files are never touched.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{
    off_value, on_value, read_switch, Approved, Source, StartupItem, StartupTarget, Switch,
};
use crate::associations::OperationReport;
use crate::backups::{self, Slot, Snapshot};
use crate::clock;
use crate::models::app_data_dir;
use crate::regsnap::{restore_value, snapshot_value, ValueSnapshot};

/// Original switch states, per user then per switch slot id.
pub type StartupBackupStore = backups::UserBackups;

pub fn default_startup_store_path() -> PathBuf {
    app_data_dir().join("startup-backups.json")
}

pub fn load_startup_store(path: &Path) -> Result<StartupBackupStore, crate::persist::LoadError> {
    crate::persist::load_json(path)
}

pub fn save_startup_store(store: &StartupBackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

/// One switch the user wants on or off.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupChange {
    pub approved: Approved,
    pub name: String,
    pub on: bool,
    pub sources: Vec<Source>,
}

impl StartupChange {
    pub fn slot(&self) -> Slot {
        self.approved.slot(&self.name)
    }

    pub fn from_item(item: &StartupItem, on: bool) -> Self {
        Self {
            approved: item.approved,
            name: item.name.clone(),
            on,
            sources: item.sources(),
        }
    }
}

pub struct StartupOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: String,
    pub store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    pub progress: &'a dyn Fn(usize, usize, &str),
    /// Whether a per-user switch turns items of this source off.
    pub switch_works: fn(Source) -> bool,
    /// The FILETIME a turn-off writes.
    pub now: u64,
}

/// Why `change` cannot be applied, from the registry as it is now.
fn problem(
    target: &StartupTarget,
    change: &StartupChange,
    opts: &StartupOptions,
) -> Option<String> {
    if change.name.is_empty() {
        return Some("a startup item has no name".to_string());
    }
    if change.sources.iter().any(|s| !(opts.switch_works)(*s)) {
        return Some(
            "this app starts for every account; WinTune cannot change it for your account alone yet"
                .to_string(),
        );
    }
    let key = change.approved.key();
    let read = |t: &crate::backups::RegTarget| {
        snapshot_value(&t.root, &t.path(&key), &change.name).map(|v| read_switch(&v))
    };
    match read(&target.user) {
        Ok(Switch::Unknown) => return Some("its on/off setting has an unknown format".into()),
        Err(e) => return Some(e),
        Ok(_) => {}
    }
    match read(&target.machine) {
        Ok(Switch::Unknown) => {
            Some("its on/off setting for all accounts has an unknown format".into())
        }
        Ok(s) if s.is_off() => {
            Some("turned off for every account; WinTune changes only yours".to_string())
        }
        Err(e) => Some(e),
        Ok(_) => None,
    }
}

/// Turns each change's switch on or off for the current user in one pass.
/// The original of each touched switch stays recorded until it is restored
/// or found back at its original after an Apply. Any problem refuses the whole
/// Apply with nothing written.
pub fn apply_startup(
    target: &StartupTarget,
    store: &mut StartupBackupStore,
    changes: &[StartupChange],
    opts: &StartupOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let now = clock::now_local();

    // Only the first change on a slot counts (ids compare ignoring case).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let changes: Vec<&StartupChange> = changes
        .iter()
        .filter(|c| seen.insert(c.slot().id().to_lowercase()))
        .collect();

    // Validate everything; write nothing.
    for c in &changes {
        if let Some(p) = problem(target, c, opts) {
            report.errors.push(format!("{}: {}", c.name, p));
        }
    }
    if !report.errors.is_empty() {
        return report;
    }

    // Snapshot each slot on first touch.
    let total = changes.len() * 2;
    let mut snapshotted: Vec<(&StartupChange, String)> = Vec::new();
    let mut taken: Vec<(Slot, Snapshot)> = Vec::new();
    for (n, c) in changes.iter().enumerate() {
        let slot = c.slot();
        (opts.progress)(n, total, &format!("Snapshotting {}", c.name));
        match backups::take_snapshot(&target.user, &slot) {
            Ok(current) => {
                let id = slot.id();
                let id = store
                    .find_id(&opts.user, &id)
                    .map(str::to_string)
                    .unwrap_or(id);
                store.record_first(&opts.user, &id, &now, current.clone());
                snapshotted.push((c, id));
                taken.push((slot, current));
            }
            Err(e) => report.errors.push(format!("{}: {}", c.name, e)),
        }
    }
    if taken.is_empty() {
        return report;
    }

    // Save the store and the .reg before any write.
    match backups::save_then_export(
        store,
        &opts.store_path,
        &target.user,
        &taken,
        &opts.reg_backup_dir,
        "startup-before-apply",
    ) {
        Ok(reg_file) => report.reg_backup_file = Some(reg_file),
        Err(e) => {
            report.errors.push(e);
            return report;
        }
    }

    // Write.
    let mut written: Vec<(&StartupChange, String)> = Vec::new();
    for (n, (c, id)) in snapshotted.into_iter().enumerate() {
        (opts.progress)(changes.len() + n, total, &format!("Writing {}", c.name));
        match write_switch(target, store, &opts.user, &id, c, opts.now) {
            Ok(()) => {
                report.succeeded.push(c.name.clone());
                written.push((c, id));
            }
            Err(e) => report.errors.push(format!("{}: {}", c.name, e)),
        }
    }

    // A slot back at its original has nothing left to restore.
    let mut forgotten = false;
    for (c, id) in &written {
        let original = original_value(store, &opts.user, id);
        let now_value = snapshot_value(
            &target.user.root,
            &target.user.path(&c.approved.key()),
            &c.name,
        );
        if let (Some(original), Ok(now_value)) = (original, now_value) {
            if same_value(&original, &now_value) {
                store.forget(&opts.user, id);
                forgotten = true;
            }
        }
    }
    if forgotten {
        if let Err(e) = save_startup_store(store, &opts.store_path) {
            report.errors.push(format!("Cannot save snapshots: {e}"));
        }
    }
    (opts.progress)(total, total, "Apply finished");
    report
}

fn original_value(store: &StartupBackupStore, user: &str, id: &str) -> Option<ValueSnapshot> {
    match &store.backups_of(user)?.get(id)?.snapshot {
        Snapshot::Value(v) => Some(v.clone()),
        Snapshot::Key(_) => None,
    }
}

fn same_value(a: &ValueSnapshot, b: &ValueSnapshot) -> bool {
    a.exists == b.exists && (!a.exists || (a.kind == b.kind && a.data == b.data))
}

fn write_switch(
    target: &StartupTarget,
    store: &StartupBackupStore,
    user: &str,
    id: &str,
    change: &StartupChange,
    now: u64,
) -> Result<(), String> {
    let root = &target.user;
    let path = root.path(&change.approved.key());
    if !change.on {
        return restore_value(&root.root, &path, &change.name, &off_value(now));
    }
    let original =
        original_value(store, user, id).ok_or_else(|| "no original state recorded".to_string())?;
    if matches!(read_switch(&original), Switch::Absent | Switch::On(_)) {
        restore_value(&root.root, &path, &change.name, &original)
    } else {
        restore_value(&root.root, &path, &change.name, &on_value())
    }
}

/// Puts each switch (by slot id) back as it was before WinTune first changed
/// it, then forgets its snapshot.
pub fn restore_startup(
    target: &StartupTarget,
    store: &mut StartupBackupStore,
    ids: &[String],
    opts: &StartupOptions,
) -> OperationReport {
    let user = &target.user;
    backups::restore_ids(
        &|slot, snapshot| backups::restore_snapshot(user, slot, snapshot),
        store,
        &opts.user,
        ids,
        &opts.store_path,
        opts.progress,
        &|id| Ok(Slot::from_id(id)),
        &|id| match Slot::from_id(id) {
            Slot::Value { name, .. } => name,
            Slot::Key(path) => path,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups::RegTarget;
    use crate::regsnap::test_support::Sandbox;
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    const NOW: u64 = 0x01DB_0000_0000_0001;
    const USER: &str = "TEST\\user";

    struct Env {
        sb: Sandbox,
        tmp: PathBuf,
        target: StartupTarget,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.tmp);
        }
    }

    fn reg_target(sb: &Sandbox, side: &str) -> RegTarget {
        RegTarget {
            root: RegKey::predef(HKEY_CURRENT_USER),
            root_name: "HKEY_CURRENT_USER".into(),
            base: format!("{}\\{side}", sb.path),
        }
    }

    fn env() -> Env {
        let sb = Sandbox::new();
        let leaf = sb.path.rsplit('\\').next().unwrap().to_string();
        let tmp = std::env::temp_dir().join(format!("wintune-startup-engine-{leaf}"));
        let (user, common) = (tmp.join("user"), tmp.join("common"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&common).unwrap();
        let target = StartupTarget {
            user: reg_target(&sb, "HKCU"),
            machine: reg_target(&sb, "HKLM"),
            user_folder: Some(user),
            common_folder: Some(common),
        };
        Env { sb, tmp, target }
    }

    impl Env {
        fn store_path(&self) -> PathBuf {
            self.tmp.join("startup-backups.json")
        }

        fn reg_dir(&self) -> PathBuf {
            self.tmp.join("reg")
        }

        fn reg_files(&self) -> usize {
            std::fs::read_dir(self.reg_dir()).map_or(0, |d| d.count())
        }
    }

    fn put(t: &RegTarget, rel: &str, name: &str, vtype: RegType, bytes: Vec<u8>) {
        let (key, _) = t.root.create_subkey(t.path(rel)).unwrap();
        key.set_raw_value(name, &RegValue { bytes, vtype }).unwrap();
    }

    fn get(t: &RegTarget, rel: &str, name: &str) -> Option<Vec<u8>> {
        let key = t.root.open_subkey_with_flags(t.path(rel), KEY_READ).ok()?;
        key.get_raw_value(name).ok().map(|v| v.bytes)
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain(Some(0))
            .flat_map(|w| w.to_le_bytes())
            .collect()
    }

    fn switch_bytes(flag: u8, tail: u8) -> Vec<u8> {
        let mut v = vec![flag, 0, 0, 0];
        v.extend([tail; 8]);
        v
    }

    fn set_switch(t: &RegTarget, approved: Approved, name: &str, bytes: Vec<u8>) {
        put(t, &approved.key(), name, REG_BINARY, bytes);
    }

    fn works(_: Source) -> bool {
        true
    }

    fn change(approved: Approved, name: &str, on: bool, sources: Vec<Source>) -> StartupChange {
        StartupChange {
            approved,
            name: name.into(),
            on,
            sources,
        }
    }

    fn run_change(name: &str, on: bool) -> StartupChange {
        change(Approved::Run, name, on, vec![Source::UserRun])
    }

    fn opts(e: &Env) -> StartupOptions<'static> {
        StartupOptions {
            user: USER.into(),
            store_path: e.store_path(),
            reg_backup_dir: e.reg_dir(),
            progress: &|_, _, _| {},
            switch_works: works,
            now: NOW,
        }
    }

    fn apply(
        e: &Env,
        store: &mut StartupBackupStore,
        changes: &[StartupChange],
    ) -> OperationReport {
        apply_startup(&e.target, store, changes, &opts(e))
    }

    fn user_switch(e: &Env, approved: Approved, name: &str) -> Option<Vec<u8>> {
        get(&e.target.user, &approved.key(), name)
    }

    fn off_bytes() -> Vec<u8> {
        let mut v = vec![3, 0, 0, 0];
        v.extend(NOW.to_le_bytes());
        v
    }

    #[test]
    fn turning_off_writes_three_and_the_time_and_keeps_the_original() {
        let e = env();
        put(
            &e.target.user,
            super::super::RUN_KEY,
            "App",
            REG_SZ,
            utf16(r"C:\a.exe"),
        );
        let mut store = StartupBackupStore::default();

        let report = apply(&e, &mut store, &[run_change("App", false)]);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["App"]);
        assert_eq!(user_switch(&e, Approved::Run, "App"), Some(off_bytes()));
        let id = Approved::Run.slot("App").id();
        let kept = &store.backups_of(USER).unwrap()[&id];
        assert!(matches!(
            kept.snapshot,
            Snapshot::Value(ValueSnapshot { exists: false, .. })
        ));
        assert_eq!(e.reg_files(), 1);
        assert!(report.reg_backup_file.is_some());
        assert_eq!(
            get(&e.target.user, super::super::RUN_KEY, "App"),
            Some(utf16(r"C:\a.exe"))
        );
        let saved = load_startup_store(&e.store_path()).unwrap();
        assert_eq!(saved, store);
    }

    #[test]
    fn turning_on_again_deletes_the_value_and_forgets_the_snapshot() {
        let e = env();
        put(
            &e.target.user,
            super::super::RUN_KEY,
            "App",
            REG_SZ,
            utf16(r"C:\a.exe"),
        );
        let mut store = StartupBackupStore::default();
        apply(&e, &mut store, &[run_change("App", false)]);

        let report = apply(&e, &mut store, &[run_change("App", true)]);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(user_switch(&e, Approved::Run, "App"), None);
        assert!(store.users.is_empty());
        let saved = load_startup_store(&e.store_path()).unwrap();
        assert!(saved.users.is_empty());
    }

    #[test]
    fn turning_on_an_item_off_elsewhere_writes_two_and_restore_puts_the_original_back() {
        let e = env();
        let original = switch_bytes(3, 0x55);
        set_switch(&e.target.user, Approved::Run, "App", original.clone());
        let mut store = StartupBackupStore::default();

        let report = apply(&e, &mut store, &[run_change("App", true)]);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            user_switch(&e, Approved::Run, "App"),
            Some(switch_bytes(2, 0))
        );
        let id = Approved::Run.slot("App").id();
        assert!(store.has_backup(USER, &id));

        let report = restore_startup(&e.target, &mut store, std::slice::from_ref(&id), &opts(&e));

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["App"]);
        assert_eq!(user_switch(&e, Approved::Run, "App"), Some(original));
        assert!(!store.has_backup(USER, &id));
    }

    #[test]
    fn an_original_six_comes_back_exactly() {
        let e = env();
        let original = switch_bytes(6, 0);
        set_switch(&e.target.user, Approved::Run, "App", original.clone());
        let mut store = StartupBackupStore::default();

        apply(&e, &mut store, &[run_change("App", false)]);
        assert_eq!(user_switch(&e, Approved::Run, "App"), Some(off_bytes()));
        let report = apply(&e, &mut store, &[run_change("App", true)]);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(user_switch(&e, Approved::Run, "App"), Some(original));
        assert!(store.users.is_empty());
    }

    #[test]
    fn turned_back_on_elsewhere_keeps_the_first_original() {
        let e = env();
        let mut store = StartupBackupStore::default();
        apply(&e, &mut store, &[run_change("App", false)]);
        set_switch(&e.target.user, Approved::Run, "App", switch_bytes(2, 0));

        let report = apply(&e, &mut store, &[run_change("App", false)]);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let id = Approved::Run.slot("App").id();
        assert!(matches!(
            store.backups_of(USER).unwrap()[&id].snapshot,
            Snapshot::Value(ValueSnapshot { exists: false, .. })
        ));
        let report = restore_startup(&e.target, &mut store, &[id], &opts(&e));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(user_switch(&e, Approved::Run, "App"), None);
    }

    #[test]
    fn the_apply_is_refused_when_the_registry_changed_since_the_scan() {
        let e = env();
        put(
            &e.target.user,
            &Approved::Run.key(),
            "Bad",
            REG_DWORD,
            vec![3, 0, 0, 0],
        );
        let mut store = StartupBackupStore::default();

        let report = apply(
            &e,
            &mut store,
            &[run_change("Bad", false), run_change("Good", false)],
        );

        assert!(!report.errors.is_empty());
        assert!(report.errors.iter().any(|l| l.starts_with("Bad: ")));
        assert!(report.succeeded.is_empty());
        assert_eq!(user_switch(&e, Approved::Run, "Good"), None);
        assert_eq!(
            user_switch(&e, Approved::Run, "Bad"),
            Some(vec![3, 0, 0, 0])
        );
        assert!(!e.store_path().exists());
        assert_eq!(e.reg_files(), 0);
        assert!(store.users.is_empty());
    }

    #[test]
    fn machine_off_or_all_users_only_refuses() {
        let e = env();
        set_switch(&e.target.machine, Approved::Run, "M", switch_bytes(3, 1));
        let mut store = StartupBackupStore::default();
        let report = apply(
            &e,
            &mut store,
            &[change(Approved::Run, "M", false, vec![Source::MachineRun])],
        );
        assert!(
            report.errors.iter().any(|l| l.contains("every account")),
            "{:?}",
            report.errors
        );
        assert_eq!(user_switch(&e, Approved::Run, "M"), None);

        let mut o = opts(&e);
        o.switch_works = |s| s != Source::MachineRun32;
        let report = apply_startup(
            &e.target,
            &mut store,
            &[change(
                Approved::Run32,
                "W",
                false,
                vec![Source::MachineRun32],
            )],
            &o,
        );
        assert!(
            report
                .errors
                .iter()
                .any(|l| l.starts_with("W: ") && l.contains("every account")),
            "{:?}",
            report.errors
        );
        assert_eq!(user_switch(&e, Approved::Run32, "W"), None);
        assert!(!e.store_path().exists());
        assert_eq!(e.reg_files(), 0);
    }

    #[test]
    fn an_empty_name_is_refused() {
        let e = env();
        let mut store = StartupBackupStore::default();
        let report = apply(&e, &mut store, &[run_change("", false)]);
        assert_eq!(report.errors.len(), 1);
        assert!(report.succeeded.is_empty());
    }

    #[test]
    fn only_hkcu_is_written() {
        let e = env();
        put(
            &e.target.machine,
            super::super::RUN_KEY,
            "M",
            REG_SZ,
            utf16(r"C:\m.exe"),
        );
        let lnk = e.target.common_folder.as_ref().unwrap().join("App.lnk");
        std::fs::write(&lnk, b"link bytes").unwrap();
        let mut store = StartupBackupStore::default();

        let report = apply(
            &e,
            &mut store,
            &[
                change(Approved::Run, "M", false, vec![Source::MachineRun]),
                change(
                    Approved::StartupFolder,
                    "App.lnk",
                    false,
                    vec![Source::CommonFolder],
                ),
            ],
        );

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let machine = &e.target.machine;
        for approved in [Approved::Run, Approved::Run32, Approved::StartupFolder] {
            let key = machine
                .root
                .open_subkey_with_flags(machine.path(&approved.key()), KEY_READ);
            if let Ok(key) = key {
                assert_eq!(key.enum_values().count(), 0, "{approved:?}");
            }
        }
        assert_eq!(
            get(machine, super::super::RUN_KEY, "M"),
            Some(utf16(r"C:\m.exe"))
        );
        assert_eq!(std::fs::read(&lnk).unwrap(), b"link bytes");
        assert_eq!(
            user_switch(&e, Approved::StartupFolder, "App.lnk"),
            Some(off_bytes())
        );
        assert_eq!(user_switch(&e, Approved::Run, "M"), Some(off_bytes()));
    }

    #[test]
    fn two_changes_on_one_slot_count_once() {
        let e = env();
        let mut store = StartupBackupStore::default();
        let report = apply(
            &e,
            &mut store,
            &[run_change("App", false), run_change("APP", true)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["App"]);
        assert_eq!(user_switch(&e, Approved::Run, "App"), Some(off_bytes()));
    }

    #[test]
    fn the_store_round_trips() {
        let e = env();
        let path = e.tmp.join("nested").join("s.json");
        assert_eq!(
            load_startup_store(&path).unwrap(),
            StartupBackupStore::default()
        );
        let mut store = StartupBackupStore::default();
        store.record_first(
            USER,
            "id",
            "t",
            Snapshot::Value(ValueSnapshot {
                exists: true,
                kind: 3,
                data: vec![2, 0, 0, 0],
            }),
        );
        save_startup_store(&store, &path).unwrap();
        assert_eq!(load_startup_store(&path).unwrap(), store);
    }

    #[test]
    fn restore_without_a_snapshot_reports_it() {
        let e = env();
        let mut store = StartupBackupStore::default();
        let id = Approved::Run.slot("Nope").id();
        let report = restore_startup(&e.target, &mut store, &[id], &opts(&e));
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("no original state recorded"));
        let _ = &e.sb;
    }
}
