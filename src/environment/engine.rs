//! Apply and restore for the Environment variables module.
//!
//! Shaped like the Startup apps engine: validate (any problem refuses the whole
//! Apply), snapshot on first touch, save the store and a `.reg` export, then
//! write. Only values of `HKCU\Environment` are written; HKLM and
//! `Volatile Environment` are never touched. The change is announced to other
//! programs with one `WM_SETTINGCHANGE` broadcast, only when something was
//! written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use winreg::enums::KEY_READ;

use super::{
    encode_text, env_slot, kind_to_write, read_vars, validate_changes, EnvChange, EnvProblem,
    EnvTarget, EnvVar, PathBasis, USER_ENV_KEY,
};
use crate::associations::OperationReport;
use crate::backups::{self, RegTarget, Slot, Snapshot, UserBackups};
use crate::clock;
use crate::models::app_data_dir;
use crate::regsnap::{restore_value, snapshot_value, ValueSnapshot};

/// Original states per user, plus the slots a rename restores together.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EnvBackupStore {
    #[serde(flatten)]
    pub snapshots: UserBackups,
    /// Per user: slot ids restored together (a rename's two slots).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub renames: BTreeMap<String, Vec<Vec<String>>>,
}

fn holds(group: &[String], id: &str) -> bool {
    group.iter().any(|g| g.eq_ignore_ascii_case(id))
}

fn push_unique(ids: &mut Vec<String>, id: &str) {
    if !holds(ids, id) {
        ids.push(id.to_string());
    }
}

impl EnvBackupStore {
    /// Merges any groups holding `a` or `b` into one group holding both.
    pub fn link(&mut self, user: &str, a: &str, b: &str) {
        let groups = self.renames.entry(user.to_string()).or_default();
        let mut merged: Vec<String> = Vec::new();
        let mut rest = Vec::new();
        for group in std::mem::take(groups) {
            if holds(&group, a) || holds(&group, b) {
                for id in &group {
                    push_unique(&mut merged, id);
                }
            } else {
                rest.push(group);
            }
        }
        push_unique(&mut merged, a);
        push_unique(&mut merged, b);
        rest.push(merged);
        *groups = rest;
    }

    /// `ids` plus every member of their groups, without duplicates (ignoring
    /// ASCII case), the input order first.
    pub fn with_partners(&self, user: &str, ids: &[String]) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        for id in ids {
            push_unique(&mut all, id);
        }
        if let Some(groups) = self.renames.get(user) {
            for id in ids {
                for group in groups.iter().filter(|g| holds(g, id)) {
                    for member in group {
                        push_unique(&mut all, member);
                    }
                }
            }
        }
        all
    }

    /// Drops ids with no snapshot left, groups with fewer than two ids and a
    /// user with no group left. Says whether anything changed.
    pub fn prune(&mut self, user: &str) -> bool {
        let Some(groups) = self.renames.get_mut(user) else {
            return false;
        };
        let before = groups.clone();
        for group in groups.iter_mut() {
            group.retain(|id| self.snapshots.find_id(user, id).is_some());
        }
        groups.retain(|g| g.len() >= 2);
        let changed = *groups != before;
        if groups.is_empty() {
            self.renames.remove(user);
        }
        changed
    }
}

pub fn default_env_store_path() -> PathBuf {
    app_data_dir().join("environment-backups.json")
}

pub fn load_env_store(path: &Path) -> Result<EnvBackupStore, crate::persist::LoadError> {
    crate::persist::load_json(path)
}

pub fn save_env_store(store: &EnvBackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

pub struct EnvOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: String,
    pub store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    pub progress: &'a dyn Fn(usize, usize, &str),
    /// Tells other programs the environment changed; called at most once.
    pub broadcast: &'a dyn Fn(),
}

/// Sends `WM_SETTINGCHANGE` with "Environment" to every top-level window so
/// programs reread their variables. Never called by tests.
pub fn broadcast_environment() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let area: Vec<u16> = "Environment".encode_utf16().chain(Some(0)).collect();
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
            5000,
            &mut result,
        );
    }
}

/// The spelling `name` is stored under (matched ignoring ASCII case), if any.
fn stored_spelling(t: &RegTarget, name: &str) -> Result<Option<String>, String> {
    let key = match t
        .root
        .open_subkey_with_flags(t.path(USER_ENV_KEY), KEY_READ)
    {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    for item in key.enum_values() {
        let (stored, _) = item.map_err(|e| e.to_string())?;
        if stored.eq_ignore_ascii_case(name) {
            return Ok(Some(stored));
        }
    }
    Ok(None)
}

/// Writes `name` with exactly this spelling. The registry keeps the first
/// spelling of a name, so a different stored spelling is deleted first.
fn write_exact(t: &RegTarget, name: &str, kind: u32, data: Vec<u8>) -> Result<(), String> {
    let path = t.path(USER_ENV_KEY);
    if let Some(stored) = stored_spelling(t, name)? {
        if stored != name {
            restore_value(&t.root, &path, &stored, &ValueSnapshot::default())?;
        }
    }
    let snapshot = ValueSnapshot {
        exists: true,
        kind,
        data,
    };
    restore_value(&t.root, &path, name, &snapshot)
}

fn restore_exact(t: &RegTarget, slot: &Slot, snapshot: &Snapshot) -> Result<(), String> {
    match (slot, snapshot) {
        (Slot::Value { name, .. }, Snapshot::Value(v)) if v.exists => {
            write_exact(t, name, v.kind, v.data.clone())
        }
        _ => backups::restore_snapshot(t, slot, snapshot),
    }
}

/// The slot of a stored id, refused unless it is a value of
/// `HKCU\Environment` (key path matched ignoring ASCII case): a store edited
/// by hand must not make Restore write or delete anywhere else.
fn env_slot_of(id: &str) -> Result<Slot, String> {
    match Slot::from_id(id) {
        Slot::Value { path, name } if path.eq_ignore_ascii_case(USER_ENV_KEY) => {
            Ok(Slot::Value { path, name })
        }
        _ => Err(format!(
            "not a value of HKCU\\{USER_ENV_KEY}, left alone (snapshot kept)"
        )),
    }
}

fn value_name(id: &str) -> String {
    match Slot::from_id(id) {
        Slot::Value { name, .. } => name,
        Slot::Key(path) => path,
    }
}

fn english(problem: &EnvProblem) -> String {
    match problem {
        EnvProblem::EmptyName => "the name is empty".to_string(),
        EnvProblem::NameHasEquals => "a name cannot contain '='".to_string(),
        EnvProblem::NameHasNul => "a name cannot contain a NUL character".to_string(),
        EnvProblem::NameTooLong => "the name is longer than 255 characters".to_string(),
        EnvProblem::NameTaken(name) => format!("a variable named {name} already exists"),
        EnvProblem::PathName => "this name is reserved for the Path list".to_string(),
        EnvProblem::EmptyValue => "the value is empty".to_string(),
        EnvProblem::ValueTooLong => "the value is longer than 32,767 characters".to_string(),
        EnvProblem::ValueHasNul => "the value cannot contain a NUL character".to_string(),
        EnvProblem::EmptyEntry => "an entry is empty".to_string(),
        EnvProblem::EntryHasSemicolon => "an entry cannot contain ';'".to_string(),
        EnvProblem::Gone(name) => {
            format!("{name} no longer exists; it changed since the last scan")
        }
        EnvProblem::NotText(name) => format!("{name} is not a text value, WinTune leaves it alone"),
        EnvProblem::Twice(name) => format!("{name} is changed more than once"),
        EnvProblem::ChangedSinceScan(name) => format!(
            "{name} was changed by another program since the last scan; nothing was written"
        ),
    }
}

fn existing<'a>(vars: &'a [EnvVar], name: &str) -> Option<&'a EnvVar> {
    vars.iter().find(|v| v.name.eq_ignore_ascii_case(name))
}

fn write_change(t: &RegTarget, vars: &[EnvVar], change: &EnvChange) -> Result<(), String> {
    let path = t.path(USER_ENV_KEY);
    let delete = |name: &str| restore_value(&t.root, &path, name, &ValueSnapshot::default());
    match change {
        EnvChange::Add { name, value } => write_exact(
            t,
            name,
            kind_to_write(None, name, value),
            encode_text(value),
        ),
        EnvChange::Edit { name, value } => {
            // Only a rename changes the spelling the registry stores.
            let current = existing(vars, name);
            let spelled = current.map_or(name.as_str(), |v| v.name.as_str());
            write_exact(
                t,
                spelled,
                kind_to_write(current.map(|v| v.kind), name, value),
                encode_text(value),
            )
        }
        EnvChange::Delete { name } => delete(name),
        EnvChange::Rename { from, to, value } => {
            let kind = existing(vars, from).map(|v| v.kind);
            let data = encode_text(value);
            if from.eq_ignore_ascii_case(to) {
                return write_exact(t, to, kind_to_write(kind, to, value), data);
            }
            write_exact(t, to, kind_to_write(kind, to, value), data)?;
            delete(from)
        }
    }
}

fn same_value(a: &ValueSnapshot, b: &ValueSnapshot) -> bool {
    a.exists == b.exists && (!a.exists || (a.kind == b.kind && a.data == b.data))
}

/// Whether the slot of `id` is exactly as first recorded, spelling included.
fn back_at_original(t: &RegTarget, store: &EnvBackupStore, user: &str, id: &str) -> bool {
    let Some(Snapshot::Value(original)) = store
        .snapshots
        .backups_of(user)
        .and_then(|b| b.get(id))
        .map(|b| b.snapshot.clone())
    else {
        return false;
    };
    let name = value_name(id);
    let Ok(now) = snapshot_value(&t.root, &t.path(USER_ENV_KEY), &name) else {
        return false;
    };
    if !same_value(&original, &now) {
        return false;
    }
    !original.exists || matches!(stored_spelling(t, &name), Ok(Some(s)) if s == name)
}

/// Applies every change to the current user's variables in one pass. The
/// original of each touched value stays recorded until it is restored or
/// found back at its original. Any problem refuses the whole Apply with
/// nothing written, a `Path` that is no longer `path_basis` (what the changes
/// and their confirmation were computed from) included.
pub fn apply_environment(
    target: &EnvTarget,
    store: &mut EnvBackupStore,
    changes: &[EnvChange],
    path_basis: &PathBasis,
    opts: &EnvOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let user = opts.user.as_str();
    let now = clock::now_local();

    // Validate everything against the registry as it is now; write nothing.
    let current = match read_vars(&target.user, USER_ENV_KEY) {
        Ok(vars) => vars,
        Err(e) => {
            report
                .errors
                .push(format!("Cannot read the variables: {e}"));
            return report;
        }
    };
    for (name, problem) in validate_changes(&current, changes, path_basis) {
        report.errors.push(format!("{name}: {}", english(&problem)));
    }
    if !report.errors.is_empty() || changes.is_empty() {
        return report;
    }

    // Snapshot each slot on first touch.
    let total = changes.len() * 2;
    let mut touched: Vec<String> = Vec::new();
    let mut taken: Vec<(Slot, Snapshot)> = Vec::new();
    for (n, change) in changes.iter().enumerate() {
        (opts.progress)(n, total, &format!("Snapshotting {}", change.display_name()));
        let mut ids: Vec<String> = Vec::new();
        for name in change.slot_names() {
            let spelled = existing(&current, name).map_or(name, |v| v.name.as_str());
            let slot = env_slot(spelled);
            match backups::take_snapshot(&target.user, &slot) {
                Ok(snapshot) => {
                    let id = slot.id();
                    let id = store
                        .snapshots
                        .find_id(user, &id)
                        .map(str::to_string)
                        .unwrap_or(id);
                    store
                        .snapshots
                        .record_first(user, &id, &now, snapshot.clone());
                    ids.push(id);
                    taken.push((slot, snapshot));
                }
                Err(e) => report
                    .errors
                    .push(format!("{}: {e}", change.display_name())),
            }
        }
        if let [from, to] = ids.as_slice() {
            store.link(user, from, to);
        }
        touched.extend(ids);
    }
    if !report.errors.is_empty() {
        return report;
    }

    // Save the store and the .reg before any write.
    match backups::save_then_export(
        &*store,
        &opts.store_path,
        &target.user,
        &taken,
        &opts.reg_backup_dir,
        "environment-before-apply",
    ) {
        Ok(reg_file) => report.reg_backup_file = Some(reg_file),
        Err(e) => {
            report.errors.push(e);
            return report;
        }
    }

    // Write, in change order.
    for (n, change) in changes.iter().enumerate() {
        (opts.progress)(
            changes.len() + n,
            total,
            &format!("Writing {}", change.display_name()),
        );
        match write_change(&target.user, &current, change) {
            Ok(()) => report.succeeded.push(change.display_name()),
            Err(e) => report
                .errors
                .push(format!("{}: {e}", change.display_name())),
        }
    }

    // A slot back at its original has nothing left to restore.
    let mut changed = false;
    for id in &touched {
        if back_at_original(&target.user, store, user, id) {
            store.snapshots.forget(user, id);
            changed = true;
        }
    }
    changed |= store.prune(user);
    if changed {
        if let Err(e) = save_env_store(store, &opts.store_path) {
            report.errors.push(format!("Cannot save snapshots: {e}"));
        }
    }

    if !report.succeeded.is_empty() {
        (opts.broadcast)();
    }
    (opts.progress)(total, total, "Apply finished");
    report
}

/// Puts each variable (by slot id) back as it was before WinTune first changed
/// it, the exact spelling included, then forgets its snapshot. The partners of
/// a rename are restored with it.
pub fn restore_environment(
    target: &EnvTarget,
    store: &mut EnvBackupStore,
    ids: &[String],
    opts: &EnvOptions,
) -> OperationReport {
    let user = opts.user.as_str();
    let stored: Vec<String> = ids
        .iter()
        .map(|id| {
            store
                .snapshots
                .find_id(user, id)
                .map_or_else(|| id.clone(), str::to_string)
        })
        .collect();
    let mut all: Vec<String> = Vec::new();
    for id in store.with_partners(user, &stored) {
        let id = store
            .snapshots
            .find_id(user, &id)
            .map_or(id.clone(), str::to_string);
        push_unique(&mut all, &id);
    }
    // The old name comes back before the new one goes.
    let present = |id: &String| {
        store
            .snapshots
            .backups_of(user)
            .and_then(|b| b.get(id))
            .is_some_and(|b| matches!(&b.snapshot, Snapshot::Value(v) if v.exists))
    };
    all.sort_by_key(|id| !present(id));

    let t = &target.user;
    let mut report = backups::restore_ids(
        &|slot, snapshot| restore_exact(t, slot, snapshot),
        &mut store.snapshots,
        user,
        &all,
        &opts.store_path,
        opts.progress,
        &env_slot_of,
        &value_name,
    );
    if !report.succeeded.is_empty() {
        store.prune(user);
        // `restore_ids` saved only the snapshots, not the rename groups.
        if let Err(e) = save_env_store(store, &opts.store_path) {
            report.errors.push(format!("Cannot save snapshots: {e}"));
        }
        (opts.broadcast)();
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups::Backup;
    use crate::environment::{SYSTEM_ENV_KEY, VOLATILE_ENV_KEY};
    use crate::regsnap::test_support::Sandbox;
    use std::cell::Cell;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    const USER: &str = "TEST\\user";

    struct Env {
        _sb: Sandbox,
        tmp: PathBuf,
        target: EnvTarget,
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
        let tmp = std::env::temp_dir().join(format!("wintune-env-engine-{leaf}"));
        std::fs::create_dir_all(&tmp).unwrap();
        let target = EnvTarget {
            user: reg_target(&sb, "HKCU"),
            machine: reg_target(&sb, "HKLM"),
        };
        Env {
            _sb: sb,
            tmp,
            target,
        }
    }

    fn no_progress(_: usize, _: usize, _: &str) {}

    impl Env {
        fn store_path(&self) -> PathBuf {
            self.tmp.join("environment-backups.json")
        }

        fn reg_dir(&self) -> PathBuf {
            self.tmp.join("reg")
        }

        fn reg_files(&self) -> usize {
            std::fs::read_dir(self.reg_dir()).map_or(0, |d| d.count())
        }

        /// The `Path` as a scan would read it now.
        fn basis(&self) -> PathBasis {
            let vars = read_vars(&self.target.user, USER_ENV_KEY).unwrap();
            PathBasis::of(vars.iter().find(|v| v.is_path()))
        }

        fn opts<'a>(&self, broadcast: &'a dyn Fn()) -> EnvOptions<'a> {
            EnvOptions {
                user: USER.into(),
                store_path: self.store_path(),
                reg_backup_dir: self.reg_dir(),
                progress: &no_progress,
                broadcast,
            }
        }

        fn put(&self, name: &str, vtype: RegType, bytes: Vec<u8>) {
            let t = &self.target.user;
            let (key, _) = t.root.create_subkey(t.path(USER_ENV_KEY)).unwrap();
            key.set_raw_value(name, &RegValue { bytes, vtype }).unwrap();
        }

        fn put_text(&self, name: &str, vtype: RegType, text: &str) {
            self.put(name, vtype, encode_text(text));
        }

        fn get(&self, name: &str) -> Option<(u32, Vec<u8>)> {
            let t = &self.target.user;
            let v = snapshot_value(&t.root, &t.path(USER_ENV_KEY), name).unwrap();
            v.exists.then_some((v.kind, v.data))
        }

        fn names(&self) -> Vec<String> {
            read_vars(&self.target.user, USER_ENV_KEY)
                .unwrap()
                .into_iter()
                .map(|v| v.name)
                .collect()
        }
    }

    fn add(name: &str, value: &str) -> EnvChange {
        EnvChange::Add {
            name: name.into(),
            value: value.into(),
        }
    }

    fn edit(name: &str, value: &str) -> EnvChange {
        EnvChange::Edit {
            name: name.into(),
            value: value.into(),
        }
    }

    fn rename(from: &str, to: &str, value: &str) -> EnvChange {
        EnvChange::Rename {
            from: from.into(),
            to: to.into(),
            value: value.into(),
        }
    }

    fn id(name: &str) -> String {
        env_slot(name).id()
    }

    fn assert_refused(e: &Env, changes: &[EnvChange]) {
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let before = e.names();
        let report = apply_environment(&e.target, &mut store, changes, &e.basis(), &e.opts(&bc));
        assert!(!report.errors.is_empty(), "{changes:?}");
        assert!(report.succeeded.is_empty(), "{changes:?}");
        assert_eq!(e.names(), before, "{changes:?}");
        assert!(!e.store_path().exists(), "{changes:?}");
        assert_eq!(e.reg_files(), 0, "{changes:?}");
        assert_eq!(count.get(), 0, "{changes:?}");
        assert_eq!(store, EnvBackupStore::default(), "{changes:?}");
    }

    #[test]
    fn adding_a_variable_writes_sz_or_expand_sz_and_keeps_the_original() {
        let e = env();
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[add("FOO", "bar"), add("BAR", r"%FOO%\x")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["FOO", "BAR"]);
        assert_eq!(e.get("FOO"), Some((1, encode_text("bar"))));
        assert_eq!(e.get("BAR"), Some((2, encode_text(r"%FOO%\x"))));
        let saved = store.snapshots.backups_of(USER).unwrap();
        for name in ["FOO", "BAR"] {
            assert_eq!(
                saved[&id(name)].snapshot,
                Snapshot::Value(ValueSnapshot::default())
            );
        }
        assert_eq!(saved.len(), 2);
        assert_eq!(e.reg_files(), 1);
        assert!(report.reg_backup_file.is_some());
        assert_eq!(load_env_store(&e.store_path()).unwrap(), store);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn editing_keeps_the_type_and_the_unexpanded_text() {
        let e = env();
        e.put_text("X", REG_EXPAND_SZ, r"%USERPROFILE%\a");
        e.put_text("Y", REG_SZ, "50% done");
        e.put_text("Z", REG_SZ, "plain");
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[
                edit("X", r"%USERPROFILE%\b"),
                edit("Y", "60% done"),
                edit("Z", r"%TEMP%\z"),
            ],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("X"), Some((2, encode_text(r"%USERPROFILE%\b"))));
        assert_eq!(e.get("Y"), Some((1, encode_text("60% done"))));
        assert_eq!(e.get("Z"), Some((2, encode_text(r"%TEMP%\z"))));
    }

    #[test]
    fn deleting_and_restoring_brings_the_exact_bytes_back() {
        let e = env();
        // Odd bytes: a double terminator, which an edit would never write.
        let mut raw = encode_text("%X%");
        raw.extend([0, 0]);
        e.put("Odd", REG_EXPAND_SZ, raw);
        let original = e.get("Odd").unwrap();
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[EnvChange::Delete { name: "Odd".into() }],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("Odd"), None);

        let report = restore_environment(&e.target, &mut store, &[id("Odd")], &e.opts(&bc));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("Odd"), Some(original));
        assert_eq!(e.get("Odd").unwrap().0, 2);
        assert!(store.snapshots.users.is_empty());
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn a_rename_writes_the_new_name_first_and_restores_both_from_either_half() {
        let e = env();
        e.put_text("OLD", REG_EXPAND_SZ, "v");
        let original = e.get("OLD").unwrap();
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[rename("OLD", "NEW", "v2")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("NEW"), Some((2, encode_text("v2"))));
        assert_eq!(e.get("OLD"), None);
        let groups = &store.renames[USER];
        assert_eq!(groups.len(), 1);
        let mut group = groups[0].clone();
        group.sort();
        assert_eq!(group, [id("NEW"), id("OLD")]);

        let report = restore_environment(&e.target, &mut store, &[id("NEW")], &e.opts(&bc));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("OLD"), Some(original));
        assert_eq!(e.get("NEW"), None);
        assert_eq!(store, EnvBackupStore::default());
        assert_eq!(
            load_env_store(&e.store_path()).unwrap(),
            EnvBackupStore::default()
        );
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn a_case_only_rename_changes_the_spelling_and_restore_brings_it_back() {
        let e = env();
        e.put_text("foo", REG_SZ, "1");
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[rename("foo", "Foo", "1")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.names(), ["Foo"]);
        let saved = store.snapshots.backups_of(USER).unwrap();
        assert_eq!(saved.keys().collect::<Vec<_>>(), [&id("foo")]);

        let report = restore_environment(&e.target, &mut store, &[id("Foo")], &e.opts(&bc));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.names(), ["foo"]);
        assert_eq!(e.get("foo"), Some((1, encode_text("1"))));
        assert_eq!(store, EnvBackupStore::default());
    }

    #[test]
    fn a_path_stored_in_capitals_keeps_its_spelling_through_restore() {
        let e = env();
        e.put_text("PATH", REG_EXPAND_SZ, r"C:\a");
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[edit("PATH", r"C:\a;C:\b")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(store.snapshots.has_backup(USER, &id("PATH")));

        let report = restore_environment(&e.target, &mut store, &[id("Path")], &e.opts(&bc));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.names(), ["PATH"]);
        assert_eq!(e.get("PATH"), Some((2, encode_text(r"C:\a"))));
        assert_eq!(store, EnvBackupStore::default());
    }

    #[test]
    fn editing_back_to_the_original_forgets_the_snapshot() {
        let e = env();
        e.put_text("X", REG_SZ, "a");
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        apply_environment(
            &e.target,
            &mut store,
            &[edit("X", "b")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(!store.snapshots.users.is_empty());
        let report = apply_environment(
            &e.target,
            &mut store,
            &[edit("X", "a")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(store.snapshots.users.is_empty());
        assert_eq!(
            load_env_store(&e.store_path()).unwrap(),
            EnvBackupStore::default()
        );
    }

    #[test]
    fn the_apply_is_refused_when_the_registry_changed_since_the_scan() {
        let e = env();
        assert_refused(&e, &[edit("GONE", "1"), add("NEW", "1")]);
        assert_eq!(e.get("NEW"), None);
    }

    #[test]
    fn a_path_edit_is_refused_when_path_changed_since_the_scan() {
        let e = env();
        e.put_text("Path", REG_EXPAND_SZ, r"C:\a;C:\b");
        e.put_text("X", REG_SZ, "1");
        let scanned = e.basis();
        // An installer appends its folder after the scan.
        e.put_text("Path", REG_EXPAND_SZ, r"C:\a;C:\b;C:\new");
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[edit("X", "2"), edit("Path", r"C:\b;C:\a")],
            &scanned,
            &e.opts(&bc),
        );
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(
            report.errors[0].contains("changed by another program"),
            "{:?}",
            report.errors
        );
        assert!(report.succeeded.is_empty());
        assert_eq!(e.get("Path"), Some((2, encode_text(r"C:\a;C:\b;C:\new"))));
        assert_eq!(e.get("X"), Some((1, encode_text("1"))));
        assert!(!e.store_path().exists());
        assert_eq!(e.reg_files(), 0);
        assert_eq!(count.get(), 0);
        assert_eq!(store, EnvBackupStore::default());

        // Computed from what the registry holds now, it applies.
        let report = apply_environment(
            &e.target,
            &mut store,
            &[edit("Path", r"C:\b;C:\a")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("Path"), Some((2, encode_text(r"C:\b;C:\a"))));
    }

    #[test]
    fn invalid_changes_are_refused_whole() {
        let e = env();
        e.put_text("a", REG_SZ, "1");
        e.put_text("b", REG_SZ, "2");
        e.put("D", REG_DWORD, vec![1, 0, 0, 0]);
        assert_refused(&e, &[add("A=B", "1")]);
        assert_refused(&e, &[add("x", "1"), add("X", "2")]);
        assert_refused(&e, &[rename("a", "b", "1")]);
        assert_refused(&e, &[edit("D", "1")]);
    }

    #[test]
    fn only_hkcu_environment_is_written() {
        let e = env();
        let m = &e.target.machine;
        let (key, _) = m.root.create_subkey(m.path(SYSTEM_ENV_KEY)).unwrap();
        key.set_raw_value(
            "Path",
            &RegValue {
                bytes: encode_text(r"C:\s"),
                vtype: REG_EXPAND_SZ,
            },
        )
        .unwrap();
        let u = &e.target.user;
        let (vol, _) = u.root.create_subkey(u.path(VOLATILE_ENV_KEY)).unwrap();
        vol.set_raw_value(
            "USERNAME",
            &RegValue {
                bytes: encode_text("u"),
                vtype: REG_SZ,
            },
        )
        .unwrap();
        for (n, v) in [("X", "1"), ("E", "2"), ("D", "3"), ("R", "4")] {
            e.put_text(n, REG_SZ, v);
        }
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[
                add("N", "n"),
                edit("E", "22"),
                EnvChange::Delete { name: "D".into() },
                rename("R", "R2", "44"),
            ],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let ids: Vec<String> = store
            .snapshots
            .backups_of(USER)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let report = restore_environment(&e.target, &mut store, &ids, &e.opts(&bc));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(store, EnvBackupStore::default());

        let values = |t: &RegTarget, rel: &str| -> Vec<(String, Vec<u8>)> {
            let key = t.root.open_subkey(t.path(rel)).unwrap();
            key.enum_values()
                .map(|v| {
                    let (n, v) = v.unwrap();
                    (n, v.bytes)
                })
                .collect()
        };
        assert_eq!(
            values(m, SYSTEM_ENV_KEY),
            [("Path".to_string(), encode_text(r"C:\s"))]
        );
        assert_eq!(
            values(u, VOLATILE_ENV_KEY),
            [("USERNAME".to_string(), encode_text("u"))]
        );
        let mut names = e.names();
        names.sort();
        assert_eq!(names, ["D", "E", "R", "X"]);
    }

    #[test]
    fn the_store_round_trips_with_its_rename_groups() {
        let e = env();
        assert_eq!(
            load_env_store(&e.store_path()).unwrap(),
            EnvBackupStore::default()
        );
        let mut store = EnvBackupStore::default();
        for name in ["A", "B"] {
            store.snapshots.record_first(
                USER,
                &id(name),
                "t",
                Snapshot::Value(ValueSnapshot::default()),
            );
        }
        store.link(USER, &id("A"), &id("B"));
        save_env_store(&store, &e.store_path()).unwrap();
        assert_eq!(load_env_store(&e.store_path()).unwrap(), store);

        std::fs::write(e.store_path(), r#"{"users":{}}"#).unwrap();
        let loaded = load_env_store(&e.store_path()).unwrap();
        assert!(loaded.renames.is_empty());
        assert_eq!(loaded, EnvBackupStore::default());
    }

    #[test]
    fn store_groups_link_find_partners_and_prune() {
        let mut store = EnvBackupStore::default();
        for name in ["a", "b", "c"] {
            store.snapshots.record_first(
                USER,
                name,
                "t",
                Snapshot::Value(ValueSnapshot::default()),
            );
        }
        store.link(USER, "a", "b");
        store.link(USER, "b", "c");
        assert_eq!(store.renames[USER].len(), 1);
        let mut group = store.renames[USER][0].clone();
        group.sort();
        assert_eq!(group, ["a", "b", "c"]);

        let mut partners = store.with_partners(USER, &["B".to_string()]);
        assert_eq!(partners[0], "B");
        partners.sort_by_key(|s| s.to_lowercase());
        assert_eq!(partners, ["a", "B", "c"]);
        assert_eq!(store.with_partners("other", &["B".to_string()]), ["B"]);

        assert!(!store.prune(USER));
        store.snapshots.forget(USER, "a");
        store.snapshots.forget(USER, "b");
        assert!(store.prune(USER));
        assert!(store.renames.is_empty());
        assert!(!store.prune(USER));
        let backup: &Backup = &store.snapshots.backups_of(USER).unwrap()["c"];
        assert_eq!(backup.timestamp, "t");
    }

    #[test]
    fn an_edit_keeps_the_spelling_the_registry_stores() {
        let e = env();
        e.put_text("FOO", REG_SZ, "1");
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = apply_environment(
            &e.target,
            &mut store,
            &[edit("foo", "2")],
            &e.basis(),
            &e.opts(&bc),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.names(), ["FOO"]);
        assert_eq!(e.get("FOO"), Some((1, encode_text("2"))));
    }

    #[test]
    fn restore_refuses_a_slot_outside_the_environment_key() {
        use crate::regsnap::RegistryKeySnapshot;
        let e = env();
        let t = &e.target.user;
        let (other, _) = t.root.create_subkey(t.path(r"Software\X")).unwrap();
        other.set_value("v", &"keep").unwrap();
        e.put_text("FOO", REG_SZ, "1");
        let mut store = EnvBackupStore::default();
        let absent = || Snapshot::Value(ValueSnapshot::default());
        let value_id = Slot::Value {
            path: r"Software\X".into(),
            name: "v".into(),
        }
        .id();
        let key_id = Slot::Key(r"Software\X".into()).id();
        let upper_id = Slot::Value {
            path: "ENVIRONMENT".into(),
            name: "FOO".into(),
        }
        .id();
        store.snapshots.record_first(USER, &value_id, "t", absent());
        store.snapshots.record_first(
            USER,
            &key_id,
            "t",
            Snapshot::Key(RegistryKeySnapshot::default()),
        );
        store.snapshots.record_first(USER, &upper_id, "t", absent());
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);

        let report = restore_environment(
            &e.target,
            &mut store,
            &[value_id.clone(), key_id.clone()],
            &e.opts(&bc),
        );
        assert_eq!(report.errors.len(), 2, "{:?}", report.errors);
        assert!(report.succeeded.is_empty());
        assert_eq!(count.get(), 0);
        let kept: String = t
            .root
            .open_subkey(t.path(r"Software\X"))
            .unwrap()
            .get_value("v")
            .unwrap();
        assert_eq!(kept, "keep");
        assert!(store.snapshots.has_backup(USER, &value_id));
        assert!(store.snapshots.has_backup(USER, &key_id));

        // The key path is matched ignoring ASCII case, like the registry.
        let report = restore_environment(&e.target, &mut store, &[upper_id], &e.opts(&bc));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(e.get("FOO"), None);
    }

    #[test]
    fn restore_without_a_snapshot_reports_it() {
        let e = env();
        let count = Cell::new(0);
        let bc = || count.set(count.get() + 1);
        let mut store = EnvBackupStore::default();
        let report = restore_environment(&e.target, &mut store, &[id("NOPE")], &e.opts(&bc));
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("no original state recorded"));
        assert!(report.succeeded.is_empty());
        assert_eq!(count.get(), 0);
    }
}
