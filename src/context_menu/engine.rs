//! Registry engine of the context menu module: status, apply and restore.
//!
//! Shaped like `tweaks.rs`: validate and snapshot (first touch only), persist
//! the snapshots and a `.reg` export, then write. Everything written lives in
//! HKCU: added entries in `WinTune.<id>` verb keys under `Software\Classes`;
//! hidden or renamed existing entries as two values of a verb's HKCU copy, or
//! a value under [`BLOCKED_KEY`]. HKLM is only read.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use winreg::enums::*;

use super::existing::PROGRAMMATIC_ACCESS_ONLY;
use super::{
    command_line, entry_problems, icon_value, key_name, rename_problem, ExistingChange, Mechanism,
    MenuEntry, Place, Programs, Rename, BLOCKED_KEY, KEY_PREFIX,
};
use crate::associations::OperationReport;
use crate::backups::{self, RegTarget, Slot};
use crate::clock;
use crate::models::app_data_dir;
use crate::regsnap::{self, RegistryKeySnapshot, ValueSnapshot};

/// Where the menu lives. Production: HKCU and HKLM themselves; tests:
/// sandbox keys. Writes only ever go to `user`.
pub struct MenuTarget {
    pub user: RegTarget,
    pub machine: RegTarget,
}

impl MenuTarget {
    pub fn current_user() -> Self {
        Self {
            user: RegTarget::current_user(),
            machine: RegTarget::local_machine(),
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

pub type MenuBackup = backups::Backup;
/// Original states, per user then per slot id (`entry_key` path for a key).
pub type MenuBackupStore = backups::UserBackups;

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
    let target = &target.user;
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
    let target = &target.user;
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

fn write_entry_key(target: &RegTarget, rel: &str, w: &Wanted) -> Result<(), String> {
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

/// Whether `clsid` is in the machine-wide `Blocked` list. Only an
/// administrator can undo that, so WinTune never changes such an entry.
fn blocked_for_everyone(machine: &RegTarget, clsid: &str) -> Result<bool, String> {
    let not_found = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    let cannot = |e| format!("Cannot read the machine-wide Blocked list: {e}");
    match machine
        .root
        .open_subkey_with_flags(machine.path(BLOCKED_KEY), KEY_READ)
    {
        Ok(key) => match key.get_raw_value(clsid) {
            Ok(_) => Ok(true),
            Err(e) if not_found(&e) => Ok(false),
            Err(e) => Err(cannot(e)),
        },
        Err(e) if not_found(&e) => Ok(false),
        Err(e) => Err(cannot(e)),
    }
}

/// Everything that stops `changes` from being applied, as report lines.
fn change_problems(target: &MenuTarget, changes: &[ExistingChange]) -> Vec<String> {
    let mut problems = Vec::new();
    for change in changes {
        let name = change.mechanism.name();
        if let Rename::To(label) = &change.rename {
            if !change.mechanism.renamable() {
                problems.push(format!("{name}: a shell extension cannot be renamed"));
            } else if let Some(problem) = rename_problem(label) {
                problems.push(format!("{name}: {problem:?}"));
            }
        }
        match &change.mechanism {
            Mechanism::Verbs(refs) if refs.is_empty() => {
                problems.push(format!("{name}: no verb key to change"));
            }
            Mechanism::Verbs(_) => {}
            Mechanism::Clsid(clsid) => match blocked_for_everyone(&target.machine, clsid) {
                Ok(true) => problems.push(format!(
                    "{clsid}: hidden for every account; WinTune changes only yours"
                )),
                Ok(false) => {}
                Err(e) => problems.push(format!("{clsid}: {e}")),
            },
        }
    }
    problems
}

/// Writes the wanted state of one existing entry. Only HKCU is written: the
/// `Blocked` value of a shell extension, or for a verb the two values of its
/// HKCU copy (`ProgrammaticAccessOnly`, `MUIVerb`). `recorded` gives the
/// original state of each slot, as first snapshotted.
fn write_change(
    t: &RegTarget,
    change: &ExistingChange,
    recorded: &dyn Fn(&Slot) -> Option<backups::Snapshot>,
) -> Result<(), String> {
    match &change.mechanism {
        Mechanism::Clsid(clsid) => {
            let full = t.path(BLOCKED_KEY);
            if change.hidden {
                let (key, _) = t
                    .root
                    .create_subkey(&full)
                    .map_err(|e| format!("Cannot create {BLOCKED_KEY}: {e}"))?;
                key.set_value(clsid, &"")
                    .map_err(|e| format!("Cannot write {BLOCKED_KEY}\\{clsid}: {e}"))
            } else {
                regsnap::restore_value(&t.root, &full, clsid, &ValueSnapshot::default())
            }
        }
        Mechanism::Verbs(refs) => {
            let errors: Vec<String> = refs
                .iter()
                .filter_map(|v| {
                    let rel = v.user_key();
                    let original = recorded(&Slot::Key(rel.clone()));
                    write_verb_copy(t, &rel, change, original.as_ref()).err()
                })
                .collect();
            match errors.is_empty() {
                true => Ok(()),
                false => Err(errors.join("; ")),
            }
        }
    }
}

/// One verb's HKCU copy `rel`. On a key that existed before WinTune, only
/// `ProgrammaticAccessOnly` and `MUIVerb` ever change; a copy WinTune
/// created is deleted again once nothing is left in it.
fn write_verb_copy(
    t: &RegTarget,
    rel: &str,
    change: &ExistingChange,
    original: Option<&backups::Snapshot>,
) -> Result<(), String> {
    let full = t.path(rel);
    let (key, _) = t
        .root
        .create_subkey(&full)
        .map_err(|e| format!("Cannot create {rel}: {e}"))?;
    let absent = ValueSnapshot::default();
    if change.hidden {
        key.set_value(PROGRAMMATIC_ACCESS_ONLY, &"")
            .map_err(|e| format!("Cannot write {rel}\\{PROGRAMMATIC_ACCESS_ONLY}: {e}"))?;
    } else {
        regsnap::restore_value(&t.root, &full, PROGRAMMATIC_ACCESS_ONLY, &absent)?;
    }
    let original_key = match original {
        Some(backups::Snapshot::Key(k)) => Some(k),
        _ => None,
    };
    match &change.rename {
        Rename::Keep => {}
        Rename::To(label) => key
            .set_value("MUIVerb", &verb_text(label))
            .map_err(|e| format!("Cannot write {rel}\\MUIVerb: {e}"))?,
        Rename::Original => {
            // Value names are case-insensitive: a vendor's `MuiVerb` counts.
            let verb = original_key
                .and_then(|k| {
                    k.values
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("MUIVerb"))
                })
                .map(|(_, (kind, data))| ValueSnapshot {
                    exists: true,
                    kind: *kind,
                    data: data.clone(),
                })
                .unwrap_or_default();
            regsnap::restore_value(&t.root, &full, "MUIVerb", &verb)?;
        }
    }
    drop(key);
    if original_key.is_some_and(|k| !k.exists) {
        delete_if_empty(t, rel)?;
    }
    Ok(())
}

/// The two values WinTune ever changes on a verb's HKCU copy.
const COPY_VALUES: [&str; 2] = [PROGRAMMATIC_ACCESS_ONLY, "MUIVerb"];

/// Deletes the key `rel` when it holds no value and no subkey. Never
/// `delete_tree`: only an empty key can ever go. A missing key is fine.
fn delete_if_empty(t: &RegTarget, rel: &str) -> Result<(), String> {
    let full = t.path(rel);
    let info = match t.root.open_subkey_with_flags(&full, KEY_READ) {
        Ok(key) => key
            .query_info()
            .map_err(|e| format!("Cannot read {rel}: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Cannot read {rel}: {e}")),
    };
    if info.values > 0 || info.sub_keys > 0 {
        return Ok(());
    }
    match t.root.delete_subkey(&full) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Cannot delete {rel}: {e}")),
    }
}

/// Value `name` of a key snapshot, matched ignoring case like the registry
/// does: its recorded spelling (else `name`) and state (absent when the key
/// or the value was).
fn recorded_value(original: &RegistryKeySnapshot, name: &str) -> (String, ValueSnapshot) {
    original
        .values
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(n, (kind, data))| {
            let value = ValueSnapshot {
                exists: true,
                kind: *kind,
                data: data.clone(),
            };
            (n.clone(), value)
        })
        .unwrap_or_else(|| (name.to_string(), ValueSnapshot::default()))
}

/// Whether slot `rel`'s key is a `WinTune.*` key (an added entry), as
/// opposed to the HKCU copy of a verb another program registered.
fn own_key(rel: &str) -> bool {
    rel.rsplit('\\')
        .next()
        .is_some_and(|name| name.to_lowercase().starts_with(&KEY_PREFIX.to_lowercase()))
}

/// Puts one slot back for Restore original. A `WinTune.*` key and a value
/// come back exactly as recorded; a verb's HKCU copy only gets its two
/// values back ([`restore_verb_copy`]): the rest of it is the program's.
fn restore_menu_slot(
    t: &RegTarget,
    slot: &Slot,
    snapshot: &backups::Snapshot,
) -> Result<(), String> {
    match (slot, snapshot) {
        (Slot::Key(rel), backups::Snapshot::Key(original)) if !own_key(rel) => {
            restore_verb_copy(t, rel, original)
        }
        _ => backups::restore_snapshot(t, slot, snapshot),
    }
}

/// `ProgrammaticAccessOnly` and `MUIVerb` of the copy `rel` back to
/// `original` (deleted when it had none), then the copy itself deleted when
/// WinTune created it and nothing is left in it. Whatever else the program
/// wrote there since, command and icon included, stays. A copy that is gone
/// holds nothing of WinTune's: it is not recreated.
fn restore_verb_copy(
    t: &RegTarget,
    rel: &str,
    original: &RegistryKeySnapshot,
) -> Result<(), String> {
    let full = t.path(rel);
    match t.root.open_subkey_with_flags(&full, KEY_READ) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Cannot read {rel}: {e}")),
    }
    for name in COPY_VALUES {
        let (spelling, value) = recorded_value(original, name);
        regsnap::restore_value(&t.root, &full, &spelling, &value)?;
    }
    if !original.exists {
        delete_if_empty(t, rel)?;
    }
    Ok(())
}

/// The slots of `change` that Apply brought back to their recorded original,
/// by stored id: a verb copy whose two values equal the snapshot's (a copy
/// WinTune created and deleted again counts), a `Blocked` value present or
/// absent as before. A slot that cannot be read stays recorded.
fn slots_at_original(
    t: &RegTarget,
    change: &ExistingChange,
    store: &MenuBackupStore,
    user: &str,
) -> Vec<String> {
    change
        .mechanism
        .slots()
        .into_iter()
        .filter_map(|slot| {
            let id = store.find_id(user, &slot.id())?;
            let backup = store.backups_of(user)?.get(id)?;
            let back = match (&slot, &backup.snapshot) {
                (Slot::Key(rel), backups::Snapshot::Key(original)) => {
                    COPY_VALUES.iter().all(|name| {
                        let now = regsnap::snapshot_value(&t.root, &t.path(rel), name);
                        now.is_ok_and(|now| now == recorded_value(original, name).1)
                    })
                }
                (Slot::Value { path, name }, backups::Snapshot::Value(original)) => {
                    regsnap::snapshot_value(&t.root, &t.path(path), name)
                        .is_ok_and(|now| now.exists == original.exists)
                }
                _ => false,
            };
            back.then(|| id.to_string())
        })
        .collect()
}

/// What Apply touches, and on whose behalf.
#[derive(Clone, Copy)]
enum Owner {
    Entry(usize),
    Orphan,
    Change(usize),
}

/// Makes the registry match `entries` and `changes` in one pass: writes
/// every (entry, place) key, removes `WinTune.*` keys that belong to no
/// entry, then writes the wanted state of each existing entry in `changes`.
/// The original state of each touched key or value stays recorded until
/// [`restore_keys`]. Any problem refuses the whole apply.
pub fn apply_menu(
    target: &MenuTarget,
    store: &mut MenuBackupStore,
    entries: &[MenuEntry],
    changes: &[ExistingChange],
    programs: &Programs,
    opts: &MenuOptions,
) -> OperationReport {
    let mut report = OperationReport::default();
    let now = clock::now_local();

    // Phase 1: validate, then snapshot (first touch only); nothing written.
    let problems = entry_problems(entries, programs);
    for (id, problem) in problems {
        report.errors.push(format!("{}: {:?}", id, problem));
    }
    report.errors.extend(change_problems(target, changes));
    if !report.errors.is_empty() {
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
    let steps = to_write.len() + to_delete.len() + changes.len();
    if steps == 0 {
        return report;
    }
    let target = &target.user;

    let touched: Vec<(Owner, Slot)> = to_write
        .iter()
        .map(|(i, k, _)| (Owner::Entry(*i), Slot::Key(k.clone())))
        .chain(
            to_delete
                .iter()
                .map(|k| (Owner::Orphan, Slot::Key(k.clone()))),
        )
        .chain(changes.iter().enumerate().flat_map(|(i, c)| {
            c.mechanism
                .slots()
                .into_iter()
                .map(move |s| (Owner::Change(i), s))
        }))
        .collect();
    let total = touched.len() + steps;
    let mut failed_entries: BTreeSet<usize> = BTreeSet::new();
    let mut failed_changes: BTreeSet<usize> = BTreeSet::new();
    let mut snapshots: Vec<(Slot, backups::Snapshot)> = Vec::new();
    // Slot ids taken this pass (registry paths are case-insensitive).
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut skipped: BTreeSet<String> = BTreeSet::new();
    for (n, (owner, slot)) in touched.into_iter().enumerate() {
        let id = slot.id();
        (opts.progress)(n + 1, total, &format!("Snapshotting {}", id));
        if taken.contains(&id.to_lowercase()) {
            continue;
        }
        match backups::take_snapshot(target, &slot) {
            Ok(current) => {
                // A slot recorded under another spelling keeps that original.
                let id = store
                    .find_id(&opts.user, &id)
                    .map(str::to_string)
                    .unwrap_or(id);
                store.record_first(&opts.user, &id, &now, current.clone());
                taken.insert(id.to_lowercase());
                snapshots.push((slot, current));
            }
            Err(e) => {
                report.errors.push(format!("{}: {}", id, e));
                match owner {
                    Owner::Entry(i) => _ = failed_entries.insert(i),
                    Owner::Change(i) => _ = failed_changes.insert(i),
                    Owner::Orphan => {}
                }
                skipped.insert(id);
            }
        }
    }
    if snapshots.is_empty() {
        return report;
    }

    // Phase 2: persist snapshots + .reg safety net before any write.
    match backups::save_then_export(
        store,
        &opts.store_path,
        target,
        &snapshots,
        &opts.reg_backup_dir,
        "context-menu-before-apply",
    ) {
        Ok(reg_file) => report.reg_backup_file = Some(reg_file),
        Err(e) => {
            report.errors.push(e);
            return report;
        }
    }

    // Phase 3: writes (delete-then-write), orphan removal, then changes.
    let mut done = total - steps;
    for (index, key, w) in &to_write {
        if skipped.contains(key) {
            continue;
        }
        if let Err(e) = write_entry_key(target, key, w) {
            report.errors.push(format!("{}: {}", entries[*index].id, e));
            failed_entries.insert(*index);
        }
        done += 1;
        (opts.progress)(done, total, &format!("Writing {}", key));
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
        (opts.progress)(done, total, &format!("Removing {}", key));
    }
    // Every change compares against the original, never the state the last
    // Apply left: that is what Reset name and a WinTune-made copy go back to.
    let held = &*store;
    let recorded = |slot: &Slot| {
        let id = held.find_id(&opts.user, &slot.id())?;
        held.backups_of(&opts.user)?
            .get(id)
            .map(|b| b.snapshot.clone())
    };
    // Slots a change brought back to their original: nothing is left to
    // restore, so their snapshots go.
    let mut at_original: Vec<String> = Vec::new();
    for (index, change) in changes.iter().enumerate() {
        let name = change.report_name();
        if failed_changes.contains(&index) {
            continue;
        }
        match write_change(target, change, &recorded) {
            Ok(()) => {
                report.succeeded.push(name.clone());
                at_original.extend(slots_at_original(target, change, held, &opts.user));
            }
            Err(e) => report.errors.push(format!("{}: {}", name, e)),
        }
        done += 1;
        (opts.progress)(done, total, &format!("Changing {}", name));
    }
    if !at_original.is_empty() {
        for id in &at_original {
            store.forget(&opts.user, id);
        }
        if let Err(e) = save_menu_store(store, &opts.store_path) {
            report.errors.push(format!("Cannot save snapshots: {e}"));
        }
    }
    report
}

/// Puts each slot (by id, see [`Slot::from_id`]; a key path is its own id)
/// back as it was before WinTune first changed it, then forgets its
/// snapshot. A `WinTune.*` key or a `Blocked` value comes back exactly; a
/// verb's HKCU copy only gets back the two values WinTune changes
/// (`ProgrammaticAccessOnly`, `MUIVerb`), and goes once empty if WinTune
/// created it.
pub fn restore_keys(
    target: &MenuTarget,
    store: &mut MenuBackupStore,
    ids: &[String],
    opts: &MenuOptions,
) -> OperationReport {
    let user = &target.user;
    backups::restore_ids(
        &|slot, snapshot| restore_menu_slot(user, slot, snapshot),
        store,
        &opts.user,
        ids,
        &opts.store_path,
        opts.progress,
        &|id| Ok(Slot::from_id(id)),
        &|id| key_report_name("Entry", id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_menu::{
        rename_problem, Mechanism, ProgramChoice, Rename, RenameProblem, VerbRef, BLOCKED_KEY,
    };
    use crate::regsnap::test_support::Sandbox;
    use winreg::RegKey;

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
            let sandbox = |base: String| RegTarget {
                root: RegKey::predef(HKEY_CURRENT_USER),
                root_name: "HKEY_CURRENT_USER".into(),
                base,
            };
            MenuTarget {
                user: sandbox(format!("{}\\HKCU", self.sb.path)),
                machine: sandbox(format!("{}\\HKLM", self.sb.path)),
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

        /// `rel` on the user side, relative to HKEY_CURRENT_USER.
        fn abs(&self, rel: &str) -> String {
            format!("{}\\HKCU\\{}", self.sb.path, rel)
        }

        fn machine_abs(&self, rel: &str) -> String {
            format!("{}\\HKLM\\{}", self.sb.path, rel)
        }

        fn text_at(&self, full: &str, name: &str) -> Option<String> {
            self.sb.root.open_subkey(full).ok()?.get_value(name).ok()
        }

        fn text(&self, rel: &str, name: &str) -> Option<String> {
            self.text_at(&self.abs(rel), name)
        }

        fn machine_text(&self, rel: &str, name: &str) -> Option<String> {
            self.text_at(&self.machine_abs(rel), name)
        }

        fn exists(&self, rel: &str) -> bool {
            self.sb.root.open_subkey(self.abs(rel)).is_ok()
        }

        /// Value names of a user key, sorted; empty when it does not exist.
        fn value_names(&self, rel: &str) -> Vec<String> {
            let Ok(key) = self.sb.root.open_subkey(self.abs(rel)) else {
                return Vec::new();
            };
            let mut names: Vec<String> = key.enum_values().map(|v| v.unwrap().0).collect();
            names.sort();
            names
        }

        fn subkey_count(&self, rel: &str) -> usize {
            self.sb
                .root
                .open_subkey(self.abs(rel))
                .map(|k| k.enum_keys().count())
                .unwrap_or(0)
        }

        fn set_at(&self, full: &str, name: &str, value: &str) {
            let (k, _) = self.sb.root.create_subkey(full).unwrap();
            k.set_value(name, &value).unwrap();
        }

        fn set_text(&self, rel: &str, name: &str, value: &str) {
            self.set_at(&self.abs(rel), name, value);
        }

        fn set_machine(&self, rel: &str, name: &str, value: &str) {
            self.set_at(&self.machine_abs(rel), name, value);
        }

        /// A machine-wide verb on folders, as a per-machine install writes it.
        fn machine_verb(&self) {
            self.set_machine(APP_VERB, "MUIVerb", "App verb");
            self.set_machine(&format!(r"{APP_VERB}\command"), "", "app.exe \"%1\"");
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
        apply_menu(
            &env.target(),
            store,
            entries,
            &[],
            &Programs::default(),
            &env.opts(user),
        )
    }

    fn change(
        env: &Env,
        user: &str,
        store: &mut MenuBackupStore,
        changes: &[ExistingChange],
    ) -> OperationReport {
        apply_menu(
            &env.target(),
            store,
            &[],
            changes,
            &Programs::default(),
            &env.opts(user),
        )
    }

    const FOLDER: &str = r"Software\Classes\Directory\shell\WinTune.e1";
    const APP_VERB: &str = r"Software\Classes\Directory\shell\AppVerb";
    const CLSID: &str = "{23170F69-40C1-278A-1000-000100020000}";

    fn app_verb() -> Mechanism {
        Mechanism::Verbs(vec![VerbRef {
            class: "Directory".into(),
            verb: "AppVerb".into(),
        }])
    }

    fn wanted_state(mechanism: Mechanism, hidden: bool, rename: Rename) -> ExistingChange {
        ExistingChange {
            mechanism,
            hidden,
            rename,
        }
    }

    fn to(label: &str) -> Rename {
        Rename::To(label.to_string())
    }

    fn clsid_slot() -> String {
        Mechanism::Clsid(CLSID.into()).slots()[0].id()
    }

    /// A per-user install whose vendor spelled the value `MuiVerb`.
    fn vendor_copy(env: &Env) {
        env.set_text(APP_VERB, "MuiVerb", "Vendor");
        env.set_text(&format!(r"{APP_VERB}\command"), "", "app.exe");
    }

    #[test]
    fn a_plain_hide_keeps_a_vendor_label() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        vendor_copy(&env);
        let report = change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), true, Rename::Keep)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            env.value_names(APP_VERB),
            ["MuiVerb", "ProgrammaticAccessOnly"]
        );
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Vendor");

        let report = change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), false, Rename::Keep)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.value_names(APP_VERB), ["MuiVerb"]);
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Vendor");
        assert_eq!(env.subkey_count(APP_VERB), 1);
    }

    #[test]
    fn reset_finds_a_vendor_label_whatever_its_case() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        vendor_copy(&env);
        change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), false, to("Mine"))],
        );
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Mine");
        let reset = wanted_state(app_verb(), false, Rename::Original);
        let report = change(&env, "A", &mut store, &[reset]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Vendor");
        assert_eq!(env.value_names(APP_VERB).len(), 1);
    }

    #[test]
    fn showing_a_verb_hidden_elsewhere_and_restoring_it() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        env.set_text(APP_VERB, "ProgrammaticAccessOnly", "");
        let show = wanted_state(app_verb(), false, Rename::Keep);
        let report = change(&env, "A", &mut store, &[show]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        // The copy existed before WinTune: emptied, never deleted.
        assert!(env.exists(APP_VERB));
        assert!(env.value_names(APP_VERB).is_empty());

        let ids = vec![APP_VERB.to_string()];
        let report = restore_keys(&env.target(), &mut store, &ids, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(APP_VERB, "ProgrammaticAccessOnly").unwrap(), "");
        assert!(store.backups_of("A").is_none());
    }

    #[test]
    fn a_ref_spelled_in_another_case_finds_the_first_snapshot() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), true, Rename::Keep)],
        );
        let lower = Mechanism::Verbs(vec![VerbRef {
            class: "directory".into(),
            verb: "appverb".into(),
        }]);
        let report = change(
            &env,
            "A",
            &mut store,
            &[wanted_state(lower, false, Rename::Keep)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        // Still the snapshot from before the hide (the copy did not exist):
        // the copy goes, and with it the snapshot, under its first spelling.
        assert!(!env.exists(APP_VERB));
        assert!(store.backups_of("A").is_none());
    }

    #[test]
    fn restore_keeps_a_vendor_key_written_after_show() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        let hide = wanted_state(app_verb(), true, Rename::Keep);
        change(&env, "A", &mut store, &[hide]);
        assert!(store.find_id("A", APP_VERB).is_some());
        let show = wanted_state(app_verb(), false, Rename::Keep);
        let report = change(&env, "A", &mut store, &[show]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        // Back to the original: copy gone, snapshot forgotten, on disk too.
        assert!(!env.exists(APP_VERB));
        assert!(store.find_id("A", APP_VERB).is_none());
        let on_disk = load_menu_store(&env.opts("A").store_path).unwrap();
        assert!(on_disk.users.is_empty(), "{on_disk:?}");
        // The vendor now writes its own per-user key: nothing offers to
        // restore it.
        env.set_text(APP_VERB, "MUIVerb", "Vendor");
        env.set_text(&format!(r"{APP_VERB}\command"), "", "app.exe");
        assert!(store.find_id("A", APP_VERB).is_none());

        // Hidden and renamed, then the vendor writes into the copy: Restore
        // original takes back only the two values WinTune wrote.
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        let both = wanted_state(app_verb(), true, to("Renamed"));
        change(&env, "A", &mut store, &[both]);
        env.set_text(APP_VERB, "Icon", "vendor.ico");
        env.set_text(&format!(r"{APP_VERB}\command"), "", "vendor.exe");

        let ids = vec![APP_VERB.to_string()];
        let report = restore_keys(&env.target(), &mut store, &ids, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.value_names(APP_VERB), ["Icon"]);
        assert_eq!(env.text(APP_VERB, "Icon").unwrap(), "vendor.ico");
        let command = format!(r"{APP_VERB}\command");
        assert_eq!(env.text(&command, "").unwrap(), "vendor.exe");
        assert!(store.backups_of("A").is_none());
    }

    #[test]
    fn restore_of_a_per_user_verb_only_touches_the_two_values() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let command = format!(r"{APP_VERB}\command");
        env.set_text(APP_VERB, "MUIVerb", "Vendor");
        env.set_text(APP_VERB, "Icon", "old.ico");
        env.set_text(&command, "", "app.exe");
        let both = wanted_state(app_verb(), true, to("Mine"));
        let report = change(&env, "A", &mut store, &[both]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Mine");
        // An update of the program since.
        env.set_text(APP_VERB, "Icon", "new.ico");
        env.set_text(&command, "", "app2.exe");

        let ids = vec![APP_VERB.to_string()];
        let report = restore_keys(&env.target(), &mut store, &ids, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.value_names(APP_VERB), ["Icon", "MUIVerb"]);
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Vendor");
        assert_eq!(env.text(APP_VERB, "Icon").unwrap(), "new.ico");
        assert_eq!(env.text(&command, "").unwrap(), "app2.exe");
        assert!(store.backups_of("A").is_none());
    }

    #[test]
    fn show_again_on_two_places_forgets_both_snapshots() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let background = r"Software\Classes\Directory\Background\shell\AppVerb";
        env.machine_verb();
        env.set_machine(background, "MUIVerb", "App verb");
        let both_places = || {
            Mechanism::Verbs(vec![
                VerbRef {
                    class: "Directory".into(),
                    verb: "AppVerb".into(),
                },
                VerbRef {
                    class: r"Directory\Background".into(),
                    verb: "AppVerb".into(),
                },
            ])
        };
        let hide = wanted_state(both_places(), true, Rename::Keep);
        change(&env, "A", &mut store, &[hide]);
        assert!(env.exists(APP_VERB) && env.exists(background));
        assert_eq!(store.backups_of("A").unwrap().len(), 2);

        let show = wanted_state(both_places(), false, Rename::Keep);
        let report = change(&env, "A", &mut store, &[show]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!env.exists(APP_VERB) && !env.exists(background));
        assert!(store.backups_of("A").is_none());
    }

    #[test]
    fn showing_a_clsid_again_forgets_its_snapshot() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let hide = wanted_state(Mechanism::Clsid(CLSID.into()), true, Rename::Keep);
        change(&env, "A", &mut store, &[hide]);
        assert!(store.backups_of("A").is_some());
        let show = wanted_state(Mechanism::Clsid(CLSID.into()), false, Rename::Keep);
        let report = change(&env, "A", &mut store, &[show]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(env.text(BLOCKED_KEY, CLSID).is_none());
        assert!(store.backups_of("A").is_none());
    }

    #[test]
    fn hiding_a_machine_verb_writes_only_an_hkcu_copy() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        let report = change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), true, Rename::Keep)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["AppVerb"]);
        assert_eq!(env.value_names(APP_VERB), ["ProgrammaticAccessOnly"]);
        assert_eq!(env.text(APP_VERB, "ProgrammaticAccessOnly").unwrap(), "");
        assert_eq!(env.subkey_count(APP_VERB), 0);

        assert_eq!(env.machine_text(APP_VERB, "MUIVerb").unwrap(), "App verb");
        let command = format!(r"{APP_VERB}\command");
        assert_eq!(env.machine_text(&command, "").unwrap(), "app.exe \"%1\"");
        assert!(env
            .machine_text(APP_VERB, "ProgrammaticAccessOnly")
            .is_none());
        assert!(report.reg_backup_file.unwrap().is_file());
    }

    #[test]
    fn showing_again_deletes_the_copy_wintune_created() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), true, Rename::Keep)],
        );
        assert!(env.exists(APP_VERB));
        let report = change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), false, Rename::Keep)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!env.exists(APP_VERB));
        assert_eq!(env.machine_text(APP_VERB, "MUIVerb").unwrap(), "App verb");
    }

    #[test]
    fn rename_then_reset_puts_the_original_label_back() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        // A per-user install: the verb lives in HKCU only.
        env.set_text(APP_VERB, "MUIVerb", "Vendor");
        env.set_text(&format!(r"{APP_VERB}\command"), "", "app.exe");
        let renamed = wanted_state(app_verb(), false, to("Mine & Co"));
        let report = change(&env, "A", &mut store, &[renamed]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["Mine & Co"]);
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Mine && Co");

        let report = change(
            &env,
            "A",
            &mut store,
            &[wanted_state(app_verb(), false, Rename::Original)],
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Vendor");
        assert_eq!(env.value_names(APP_VERB), ["MUIVerb"]);
        assert_eq!(env.subkey_count(APP_VERB), 1);
    }

    #[test]
    fn hide_and_rename_then_restore_is_exact() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        let both = wanted_state(app_verb(), true, to("Renamed"));
        let report = change(&env, "A", &mut store, &[both]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            env.value_names(APP_VERB),
            ["MUIVerb", "ProgrammaticAccessOnly"]
        );
        assert_eq!(env.text(APP_VERB, "MUIVerb").unwrap(), "Renamed");

        let ids = vec![APP_VERB.to_string()];
        let report = restore_keys(&env.target(), &mut store, &ids, &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!env.exists(APP_VERB));
        assert!(store.backups_of("A").is_none());
        assert_eq!(env.machine_text(APP_VERB, "MUIVerb").unwrap(), "App verb");
    }

    #[test]
    fn blocking_a_clsid_and_restoring_it() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        let hide = wanted_state(Mechanism::Clsid(CLSID.into()), true, Rename::Keep);
        let report = change(&env, "A", &mut store, std::slice::from_ref(&hide));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, [CLSID]);
        assert_eq!(env.text(BLOCKED_KEY, CLSID).unwrap(), "");
        let report = restore_keys(&env.target(), &mut store, &[clsid_slot()], &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(env.text(BLOCKED_KEY, CLSID).is_none());
        assert!(store.backups_of("A").is_none());

        // Hidden elsewhere: showing deletes the value, restore puts it back.
        env.set_text(BLOCKED_KEY, CLSID, "");
        let show = wanted_state(Mechanism::Clsid(CLSID.into()), false, Rename::Keep);
        let report = change(&env, "A", &mut store, &[show]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(env.text(BLOCKED_KEY, CLSID).is_none());
        let report = restore_keys(&env.target(), &mut store, &[clsid_slot()], &env.opts("A"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(env.text(BLOCKED_KEY, CLSID).unwrap(), "");
    }

    #[test]
    fn a_machine_wide_block_refuses_the_apply() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        env.set_machine(BLOCKED_KEY, CLSID, "");
        let changes = [
            wanted_state(app_verb(), true, Rename::Keep),
            wanted_state(Mechanism::Clsid(CLSID.into()), false, Rename::Keep),
        ];
        let report = change(&env, "A", &mut store, &changes);
        assert_eq!(
            report.errors,
            [format!(
                "{CLSID}: hidden for every account; WinTune changes only yours"
            )]
        );
        assert!(report.succeeded.is_empty());
        assert!(!env.exists(APP_VERB));
        assert!(!env.exists(BLOCKED_KEY));
        assert_eq!(env.machine_text(BLOCKED_KEY, CLSID).unwrap(), "");
        assert!(!env.opts("A").store_path.exists());
        assert!(store.users.is_empty());
    }

    #[test]
    fn a_bad_rename_blocks_the_whole_apply() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        let changes = [
            wanted_state(Mechanism::Clsid(CLSID.into()), true, Rename::Keep),
            wanted_state(app_verb(), true, to("@x")),
        ];
        let report = change(&env, "A", &mut store, &changes);
        assert!(!report.errors.is_empty());
        assert!(report.succeeded.is_empty());
        assert!(!env.exists(APP_VERB));
        assert!(!env.exists(BLOCKED_KEY));
        assert!(!env.opts("A").store_path.exists());

        // A shell extension has no name of its own to change.
        let renamed = wanted_state(Mechanism::Clsid(CLSID.into()), true, to("Mine"));
        let report = change(&env, "A", &mut store, &[renamed]);
        assert!(!report.errors.is_empty());
        assert!(!env.exists(BLOCKED_KEY));
        assert!(!env.opts("A").store_path.exists());

        let long = "x".repeat(81);
        let max = "x".repeat(80);
        let table = [
            ("", Some(RenameProblem::Empty)),
            ("  ", Some(RenameProblem::Empty)),
            (long.as_str(), Some(RenameProblem::TooLong)),
            ("@a", Some(RenameProblem::LeadingAt)),
            (max.as_str(), None),
        ];
        for (label, problem) in table {
            assert_eq!(rename_problem(label), problem, "{label:?}");
        }
    }

    #[test]
    fn entries_and_changes_share_one_reg_file() {
        let env = Env::new();
        let mut store = MenuBackupStore::default();
        env.machine_verb();
        let entry = env.entry("e1", "Open", &[Place::Folder]);
        let report = apply_menu(
            &env.target(),
            &mut store,
            &[entry],
            &[wanted_state(app_verb(), true, Rename::Keep)],
            &Programs::default(),
            &env.opts("A"),
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.succeeded, ["Open", "AppVerb"]);
        assert!(env.exists(FOLDER));
        assert_eq!(env.text(APP_VERB, "ProgrammaticAccessOnly").unwrap(), "");

        let files: Vec<_> = std::fs::read_dir(&env.opts("A").reg_backup_dir)
            .unwrap()
            .map(|f| f.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        let bytes = std::fs::read(&files[0]).unwrap();
        let words: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let content = String::from_utf16(&words).unwrap();
        for rel in [FOLDER, APP_VERB] {
            let full = format!("HKEY_CURRENT_USER\\{}", env.abs(rel));
            assert!(content.contains(&full), "{full} not in {content}");
        }
    }

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
        let of = |user: &str| saved_verb(&store, user, FOLDER);
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
        let report = apply_menu(
            &env.target(),
            &mut store,
            &[entry],
            &[],
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
        let backups::Snapshot::Key(snapshot) = &store.users[user][key].snapshot else {
            panic!("{key}: not a key snapshot");
        };
        snapshot.values.get("MUIVerb").map(|(_, b)| b.clone())
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
        let report = apply_menu(
            &env.target(),
            &mut store,
            &[entry],
            &[],
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
