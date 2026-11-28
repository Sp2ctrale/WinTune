//! Scan, Apply and Restore. Apply follows spec 5.4 (account check, validate,
//! read, snapshot: store then `.reg`, write with readback, forget) with the
//! master's companions of spec 5.3; Restore follows spec 5.5.

use super::{
    account_problem, check_change, export_desktop, save_ve_store, write_order, ApiError, EffectId,
    EffectsApi, SettingValue, VeBackupStore, VeChange, VeFailure, VeIssue, VeProblem,
    MASTER_COMPANIONS,
};
use crate::associations::OperationReport;
use crate::backups::RegTarget;
use crate::clock;
pub use crate::sysparams::AccountCheck;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// What a scan read: every effect (or its read error) and the account check.
#[derive(Debug, Clone, PartialEq)]
pub struct VeScan {
    pub values: BTreeMap<EffectId, Result<SettingValue, ApiError>>,
    pub account: AccountCheck,
}

impl VeScan {
    pub fn live(&self, id: EffectId) -> Option<SettingValue> {
        self.values.get(&id)?.as_ref().ok().copied()
    }

    pub fn read_error(&self, id: EffectId) -> Option<ApiError> {
        self.values.get(&id)?.as_ref().err().copied()
    }

    pub fn account_ok(&self) -> bool {
        self.account == AccountCheck::Same
    }
}

/// One GET per effect in table order, then the account check. Never a SET.
pub fn scan_visual_effects(api: &dyn EffectsApi, account: &dyn Fn() -> AccountCheck) -> VeScan {
    let values = EffectId::ALL
        .into_iter()
        .map(|id| (id, api.get(id)))
        .collect();
    VeScan {
        values,
        account: account(),
    }
}

pub struct VeOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: String,
    pub store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    /// Where the `.reg` net reads `Control Panel\Desktop` (read only).
    pub reg_source: &'a RegTarget,
    pub account: &'a dyn Fn() -> AccountCheck,
    pub progress: &'a dyn Fn(usize, usize, &str),
}

/// `report.errors[i]` is `english_issue(&issues[i])`, one line per issue and
/// nothing else; `report.succeeded` holds `EffectId::key` strings.
#[derive(Debug, Default)]
pub struct VeReport {
    pub report: OperationReport,
    pub issues: Vec<VeIssue>,
}

impl VeReport {
    fn push(&mut self, issue: VeIssue) {
        self.report.errors.push(english_issue(&issue));
        self.issues.push(issue);
    }

    fn problem(&mut self, problem: VeProblem) {
        self.push(VeIssue::Problem(problem));
    }

    fn failure(&mut self, failure: VeFailure) {
        self.push(VeIssue::Failure(failure));
    }
}

fn english_value(v: &SettingValue) -> String {
    match *v {
        SettingValue::Number(n) => n.to_string(),
        SettingValue::Flag(true) => "on".to_string(),
        SettingValue::Flag(false) => "off".to_string(),
        SettingValue::Mouse([t1, t2, a]) => format!("{{{t1}, {t2}, {a}}}"),
    }
}

/// The English line for History and logs; the UI translates the enum.
pub fn english_issue(issue: &VeIssue) -> String {
    match issue {
        VeIssue::Problem(p) => match p {
            VeProblem::OutOfRange(id) => {
                format!("{}: this value is outside what WinTune allows", id.key())
            }
            VeProblem::UnknownSetting(key) => {
                format!("{key} is a setting this version of WinTune does not know")
            }
            VeProblem::Unreadable { id, code } => format!(
                "{}: Windows could not read this setting (error {code})",
                id.key()
            ),
            VeProblem::WrongAccount { process, session } => {
                format!("WinTune runs as {process}, but {session} is signed in")
            }
            VeProblem::AccountUnknown(why) => format!(
                "WinTune cannot tell which account is signed in ({why}), \
                 so it does not change visual effects"
            ),
            VeProblem::NotSaved(why) => format!(
                "Nothing applied: the originals or the .reg backup could not be saved ({why})"
            ),
            VeProblem::NoOriginal(key) => format!("{key}: no original value is saved"),
        },
        VeIssue::Failure(f) => match f {
            VeFailure::SetFailed { id, code } => {
                format!("{}: Windows refused the change (error {code})", id.key())
            }
            VeFailure::Mismatch { id, wanted, read } => format!(
                "{}: Windows kept {} instead of {}",
                id.key(),
                english_value(read),
                english_value(wanted)
            ),
            VeFailure::Unverified { id, code } => format!(
                "{}: WinTune could not read the setting back (error {code})",
                id.key()
            ),
            VeFailure::HeldByMaster { id, .. } => format!(
                "{}: Windows keeps this effect off while UI effects is off",
                id.key()
            ),
            VeFailure::StoreNotSaved(why) => {
                format!("The change was made, but the saved originals could not be updated: {why}")
            }
        },
    }
}

/// The failure for a readback that differs: a master companion read while
/// `ui_effects` is live off is held by the master (one GET of the master),
/// anything else is a plain mismatch (spec 5.3).
fn held_or_mismatch(
    api: &dyn EffectsApi,
    id: EffectId,
    wanted: SettingValue,
    read: SettingValue,
) -> VeFailure {
    if !MASTER_COMPANIONS.contains(&id) {
        return VeFailure::Mismatch { id, wanted, read };
    }
    match api.get(EffectId::UiEffects) {
        Ok(SettingValue::Flag(false)) => VeFailure::HeldByMaster { id, wanted, read },
        Ok(_) => VeFailure::Mismatch { id, wanted, read },
        // Cannot tell held from mismatch: unverified, snapshot kept (T3-C3).
        Err(e) => VeFailure::Unverified { id, code: e.code },
    }
}

/// What one SET plus its readback left behind.
enum Written {
    /// The readback equals the value set.
    Kept(SettingValue),
    /// The SET was refused; the live value is unchanged.
    Refused,
    /// The readback failed: the live value is unknown.
    Unknown,
    /// Windows kept another value.
    Other(SettingValue),
    /// The master holds the effect off: the value set is not known yet.
    Held,
}

/// SET `value`, then GET it back, reporting a failure in `out`.
fn write_and_read_back(
    api: &dyn EffectsApi,
    id: EffectId,
    value: SettingValue,
    out: &mut VeReport,
) -> Written {
    if let Err(e) = api.set(id, &value) {
        out.failure(VeFailure::SetFailed { id, code: e.code });
        return Written::Refused;
    }
    match api.get(id) {
        Err(e) => {
            out.failure(VeFailure::Unverified { id, code: e.code });
            Written::Unknown
        }
        Ok(read) if read != value => {
            let failure = held_or_mismatch(api, id, value, read);
            let written = match failure {
                VeFailure::HeldByMaster { .. } => Written::Held,
                VeFailure::Unverified { .. } => Written::Unknown,
                _ => Written::Other(read),
            };
            out.failure(failure);
            written
        }
        Ok(read) => Written::Kept(read),
    }
}

/// Saves the store after the writes, when they changed it. A save failure
/// already reported in this job is not reported twice.
fn save_if_changed(
    store: &VeBackupStore,
    saved: &VeBackupStore,
    opts: &VeOptions,
    out: &mut VeReport,
) {
    if store != saved {
        if let Err(e) = save_ve_store(store, &opts.store_path) {
            let reported = out
                .issues
                .iter()
                .any(|i| matches!(i, VeIssue::Failure(VeFailure::StoreNotSaved(_))));
            if !reported {
                out.failure(VeFailure::StoreNotSaved(e));
            }
        }
    }
}

/// Ruling T3-C1 (spec 5.3 call): while the master is live off, a companion's
/// GET may read off whatever its value (case 2), so the snapshots this job
/// took for `fresh` may be wrong. Once the master's SET on is read back
/// (`master_on`), each is read again and replaced, and the store is saved
/// before any gated row is written. A master that did not end on leaves
/// them unreliable: they are dropped and their rows not written. Returns the
/// ids not to write.
fn reread_after_master_on(
    api: &dyn EffectsApi,
    store: &mut VeBackupStore,
    fresh: &[EffectId],
    master_on: bool,
    saved: &mut VeBackupStore,
    opts: &VeOptions,
    out: &mut VeReport,
) -> BTreeSet<EffectId> {
    let user = opts.user.as_str();
    let mut skip = BTreeSet::new();
    if fresh.is_empty() {
        return skip;
    }
    if !master_on {
        for &id in fresh {
            store.forget(user, id.key());
            skip.insert(id);
        }
        return skip;
    }
    for &id in fresh {
        match api.get(id) {
            Ok(v) => {
                if let Some(b) = store.users.get_mut(user).and_then(|b| b.get_mut(id.key())) {
                    b.value = v;
                }
            }
            Err(e) => {
                out.problem(VeProblem::Unreadable { id, code: e.code });
                store.forget(user, id.key());
                skip.insert(id);
            }
        }
    }
    match save_ve_store(store, &opts.store_path) {
        Ok(()) => *saved = store.clone(),
        Err(e) => {
            out.failure(VeFailure::StoreNotSaved(e));
            skip.extend(MASTER_COMPANIONS);
        }
    }
    skip
}

/// After the master's snapshot was forgotten: a companion back at its
/// original (one GET each, table order) is forgotten; one that still
/// differs, or cannot be read, is kept as its own row (spec 5.3).
fn settle_companions(api: &dyn EffectsApi, store: &mut VeBackupStore, user: &str) {
    let master = EffectId::UiEffects.key();
    let mut keys = store.companions_of(user, master);
    keys.sort_by_key(|key| EffectId::from_key(key));
    for key in keys {
        let back = EffectId::from_key(&key)
            .is_some_and(|id| api.get(id).ok().as_ref() == store.original(user, &key));
        if back {
            store.forget(user, &key);
        } else {
            store.clear_companion(user, &key);
        }
    }
}

/// Applies `changes` in the order of spec 5.4, with the master's companions
/// of spec 5.3. Every refusal before the first SET leaves the settings, the
/// store (file and memory) and the `.reg` folder as they were. A later
/// change of the same id replaces an earlier one; no change, no call.
pub fn apply_visual_effects(
    api: &dyn EffectsApi,
    store: &mut VeBackupStore,
    changes: &[VeChange],
    opts: &VeOptions,
) -> VeReport {
    let mut out = VeReport::default();
    let user = opts.user.as_str();
    let master = EffectId::UiEffects;

    // 1. The account, before any call; then validate every change.
    if let Some(p) = account_problem(&(opts.account)()) {
        out.problem(p);
        return out;
    }
    for c in changes {
        if let Some(p) = check_change(c) {
            out.problem(p);
        }
    }
    if !out.issues.is_empty() || changes.is_empty() {
        return out;
    }
    let wanted: BTreeMap<EffectId, SettingValue> =
        changes.iter().map(|c| (c.id, c.value)).collect();
    let master_touched = wanted.contains_key(&master);

    // 2. Read each touched effect, and every companion when the master is
    // touched, in table order: these are the snapshots.
    let mut live: BTreeMap<EffectId, SettingValue> = BTreeMap::new();
    for id in EffectId::ALL {
        let companion = master_touched && MASTER_COMPANIONS.contains(&id);
        if !wanted.contains_key(&id) && !companion {
            continue;
        }
        match api.get(id) {
            Ok(v) => {
                live.insert(id, v);
            }
            Err(e) => out.problem(VeProblem::Unreadable { id, code: e.code }),
        }
    }
    if !out.issues.is_empty() {
        return out;
    }

    // The gated rows whose snapshot this batch takes (T3-C1).
    let fresh: Vec<EffectId> = MASTER_COMPANIONS
        .into_iter()
        .filter(|id| live.contains_key(id) && !store.has_backup(user, id.key()))
        .collect();

    // 3. Snapshot on first touch, save the store, then write the `.reg`.
    let before = store.clone();
    let timestamp = clock::now_local();
    for &id in wanted.keys() {
        store.record_first(user, id.key(), &timestamp, live[&id]);
        store.clear_companion(user, id.key());
    }
    if master_touched {
        for id in MASTER_COMPANIONS {
            if !wanted.contains_key(&id) {
                store.record_companion(user, id.key(), &timestamp, live[&id], master.key());
            }
        }
    }
    if let Err(e) = save_ve_store(store, &opts.store_path) {
        // Keep the store in memory as the file still is.
        *store = before;
        out.problem(VeProblem::NotSaved(e));
        return out;
    }
    match export_desktop(opts.reg_source, &opts.reg_backup_dir) {
        Ok(file) => out.report.reg_backup_file = Some(file),
        Err(e) => {
            // Nothing is applied: put the store back, in memory and in its
            // file; a failure of that save is part of the reason.
            *store = before;
            let why = match save_ve_store(store, &opts.store_path) {
                Ok(()) => e,
                Err(back) => {
                    format!("{e}; the saved originals could not be put back either: {back}")
                }
            };
            out.problem(VeProblem::NotSaved(why));
            return out;
        }
    }
    let mut saved = store.clone();

    // 4. Write, the master first going on and last going off, each SET
    // followed by its GET. A master going from live off to on is followed
    // by the re-read of T3-C1 before any gated row.
    let master_target = match wanted.get(&master) {
        Some(SettingValue::Flag(on)) => Some(*on),
        _ => None,
    };
    let reread =
        master_target == Some(true) && live.get(&master) == Some(&SettingValue::Flag(false));
    let ids: Vec<EffectId> = wanted.keys().copied().collect();
    let order = write_order(&ids, master_target);
    let total = order.len();
    let mut after: Vec<(EffectId, SettingValue)> = Vec::new();
    let mut skip: BTreeSet<EffectId> = BTreeSet::new();
    for (n, &id) in order.iter().enumerate() {
        if skip.contains(&id) {
            continue;
        }
        (opts.progress)(n, total, &format!("Writing {}", id.key()));
        let written = write_and_read_back(api, id, wanted[&id], &mut out);
        if id == master && reread {
            let on = matches!(written, Written::Kept(_));
            skip = reread_after_master_on(api, store, &fresh, on, &mut saved, opts, &mut out);
        }
        match written {
            Written::Kept(read) => {
                out.report.succeeded.push(id.key().to_string());
                after.push((id, read));
            }
            Written::Other(read) => after.push((id, read)),
            Written::Refused => after.push((id, live[&id])),
            // Unverified (ruling F5) or held by the master: the snapshot
            // stays.
            Written::Unknown | Written::Held => {}
        }
    }

    // 5. Forget each snapshot back at its original; the master's companions
    // go with it, or become their own rows (spec 5.3). While the master
    // stays changed, they are kept. A touched gated row this batch took a
    // snapshot of but did not write is forgotten (T3-C1).
    for &id in &skip {
        if wanted.contains_key(&id) && fresh.contains(&id) {
            store.forget(user, id.key());
        }
    }
    let mut master_forgotten = false;
    for (id, value) in after {
        // T3-C2: the master's SET off, written last, may clear a gated row
        // read back before it.
        if master_target == Some(false) && MASTER_COMPANIONS.contains(&id) {
            continue;
        }
        if store.original(user, id.key()) == Some(&value) {
            store.forget(user, id.key());
            master_forgotten |= id == master;
        }
    }
    if master_forgotten {
        settle_companions(api, store, user);
    }
    save_if_changed(store, &saved, opts, &mut out);
    (opts.progress)(total, total, "Apply finished");
    out
}

/// Sets each id back to its stored original exactly (no range check), in
/// the master order of spec 5.4 using the stored master value, and forgets
/// the snapshot when the readback matches. Restoring the master snapshots
/// the companions it lacks first, then restores every companion with it.
/// No `.reg`.
pub fn restore_visual_effects(
    api: &dyn EffectsApi,
    store: &mut VeBackupStore,
    ids: &[String],
    opts: &VeOptions,
) -> VeReport {
    let mut out = VeReport::default();
    let user = opts.user.as_str();
    let master = EffectId::UiEffects;

    if let Some(p) = account_problem(&(opts.account)()) {
        out.problem(p);
        return out;
    }

    let mut targets: Vec<EffectId> = Vec::new();
    for key in ids {
        let Some(id) = EffectId::from_key(key) else {
            out.problem(VeProblem::UnknownSetting(key.clone()));
            continue;
        };
        if !store.has_backup(user, key) {
            out.problem(VeProblem::NoOriginal(key.clone()));
            continue;
        }
        if !targets.contains(&id) {
            targets.push(id);
        }
    }

    let master_target = match store.original(user, master.key()) {
        Some(SettingValue::Flag(on)) if targets.contains(&master) => Some(*on),
        _ => None,
    };
    let mut fresh: Vec<EffectId> = Vec::new();
    if targets.contains(&master) {
        // Companion snapshots first, all read before any write.
        let mut read: Vec<(EffectId, SettingValue)> = Vec::new();
        let mut unreadable = false;
        for id in MASTER_COMPANIONS {
            if store.has_backup(user, id.key()) {
                continue;
            }
            match api.get(id) {
                Ok(v) => read.push((id, v)),
                Err(e) => {
                    out.problem(VeProblem::Unreadable { id, code: e.code });
                    unreadable = true;
                }
            }
        }
        if unreadable {
            return out;
        }
        fresh = read.iter().map(|(id, _)| *id).collect();
        if !read.is_empty() {
            let before = store.clone();
            let timestamp = clock::now_local();
            for (id, v) in read {
                store.record_companion(user, id.key(), &timestamp, v, master.key());
            }
            if let Err(e) = save_ve_store(store, &opts.store_path) {
                *store = before;
                out.problem(VeProblem::NotSaved(e));
                return out;
            }
        }
        for key in store.companions_of(user, master.key()) {
            match EffectId::from_key(&key) {
                Some(id) if !targets.contains(&id) => targets.push(id),
                Some(_) => {}
                None => out.problem(VeProblem::UnknownSetting(key)),
            }
        }
    }

    let mut saved = store.clone();
    let order = write_order(&targets, master_target);
    let total = order.len();
    let mut master_forgotten = false;
    let mut skip: BTreeSet<EffectId> = BTreeSet::new();
    for (n, &id) in order.iter().enumerate() {
        let key = id.key();
        if skip.contains(&id) {
            continue;
        }
        (opts.progress)(n, total, &format!("Restoring {key}"));
        let Some(&original) = store.original(user, key) else {
            continue;
        };
        let kept = matches!(
            write_and_read_back(api, id, original, &mut out),
            Written::Kept(_)
        );
        if kept {
            store.forget(user, key);
            out.report.succeeded.push(key.to_string());
            master_forgotten |= id == master;
        }
        // T3-C1: the companions just snapshotted are read again once the
        // master is on, before any of them is written.
        if id == master && master_target == Some(true) {
            skip = reread_after_master_on(api, store, &fresh, kept, &mut saved, opts, &mut out);
        }
    }
    // A companion snapshotted for this restore but not written is dropped.
    for &id in &skip {
        if fresh.contains(&id) {
            store.forget(user, id.key());
        }
    }
    if master_forgotten {
        for key in store.companions_of(user, master.key()) {
            store.clear_companion(user, &key);
        }
    }
    save_if_changed(store, &saved, opts, &mut out);
    (opts.progress)(total, total, "Restore finished");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual_effects::store::test_support::{
        control_panel, fixture, mask, store_path, Fixture, MASK, USER,
    };
    use crate::visual_effects::EffectId::*;
    use crate::visual_effects::SettingValue::*;
    use crate::visual_effects::*;

    fn same() -> AccountCheck {
        AccountCheck::Same
    }

    fn no_progress(_: usize, _: usize, _: &str) {}

    fn opts<'a>(f: &'a Fixture, account: &'a dyn Fn() -> AccountCheck) -> VeOptions<'a> {
        VeOptions {
            user: USER.into(),
            store_path: store_path(f),
            reg_backup_dir: f.reg_dir(),
            reg_source: &f.source,
            account,
            progress: &no_progress,
        }
    }

    fn change(id: EffectId, value: SettingValue) -> VeChange {
        VeChange { id, value }
    }

    fn apply(
        fake: &dyn EffectsApi,
        store: &mut VeBackupStore,
        f: &Fixture,
        changes: &[VeChange],
    ) -> VeReport {
        apply_visual_effects(fake, store, changes, &opts(f, &same))
    }

    fn restore(
        fake: &dyn EffectsApi,
        store: &mut VeBackupStore,
        f: &Fixture,
        ids: &[&str],
    ) -> VeReport {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        restore_visual_effects(fake, store, &ids, &opts(f, &same))
    }

    fn assert_errors_match(r: &VeReport) {
        assert_eq!(r.report.errors.len(), r.issues.len());
        for (line, issue) in r.report.errors.iter().zip(&r.issues) {
            assert_eq!(line, &english_issue(issue));
        }
    }

    /// The calls made before the first SET (the snapshot reads).
    fn before_first_set(calls: &[VeCall]) -> Vec<VeCall> {
        calls
            .iter()
            .take_while(|c| matches!(c, VeCall::Get(_)))
            .cloned()
            .collect()
    }

    fn set_ids(sets: &[(EffectId, SettingValue)]) -> Vec<EffectId> {
        sets.iter().map(|(id, _)| *id).collect()
    }

    fn gets(ids: impl IntoIterator<Item = EffectId>) -> Vec<VeCall> {
        ids.into_iter().map(VeCall::Get).collect()
    }

    /// Every live value of `fake`, in table order.
    fn values(fake: &FakeEffects) -> Vec<SettingValue> {
        EffectId::ALL.into_iter().map(|id| fake.value(id)).collect()
    }

    /// The seed value of each companion, in table order.
    fn companion_seeds() -> Vec<(EffectId, SettingValue)> {
        let seed = FakeEffects::new();
        MASTER_COMPANIONS
            .into_iter()
            .map(|id| (id, seed.value(id)))
            .collect()
    }

    // ---- basics ----

    #[test]
    fn the_scan_reads_every_effect_once() {
        let fake = FakeEffects::new();
        fake.fail_get(CaretWidth);

        let scan = scan_visual_effects(&fake, &same);

        assert_eq!(scan.read_error(CaretWidth), Some(ApiError { code: 5 }));
        assert_eq!(scan.live(CaretWidth), None);
        assert_eq!(scan.read_error(CleartypeContrast), None);
        assert_eq!(scan.live(CleartypeContrast), Some(Number(1200)));
        let live = EffectId::ALL
            .iter()
            .filter(|&&id| scan.live(id).is_some())
            .count();
        assert_eq!(live, 17);
        assert!(scan.account_ok());
        assert_eq!(fake.calls(), gets(EffectId::ALL));

        let other = scan_visual_effects(&fake, &|| AccountCheck::Unknown("x".into()));
        assert!(!other.account_ok());
    }

    #[test]
    fn apply_sets_reads_back_and_keeps_the_original() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[change(CursorShadow, Flag(true))]);

        assert_eq!(r.issues, []);
        assert_errors_match(&r);
        assert_eq!(fake.value(CursorShadow), Flag(true));
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(false)));
        assert_eq!(store.companion_of(USER, "cursor_shadow"), None);
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
        assert_eq!(f.reg_files().len(), 1);
        assert_eq!(r.report.reg_backup_file.as_ref(), Some(&f.reg_files()[0]));
        assert_eq!(r.report.succeeded, ["cursor_shadow"]);
        assert_eq!(
            fake.calls(),
            [
                VeCall::Get(CursorShadow),
                VeCall::Set(CursorShadow, Flag(true)),
                VeCall::Get(CursorShadow),
            ]
        );
    }

    #[test]
    fn the_whole_apply_is_refused_on_a_bad_value() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(CleartypeContrast, Number(2300)),
                change(CaretWidth, Number(2)),
            ],
        );

        assert_eq!(
            r.issues,
            [VeIssue::Problem(VeProblem::OutOfRange(CleartypeContrast))]
        );
        assert_errors_match(&r);
        assert_eq!(fake.calls(), []);
        assert!(!store_path(&f).exists());
        assert!(f.reg_files().is_empty());
        assert!(r.report.succeeded.is_empty());
    }

    #[test]
    fn an_empty_apply_makes_no_call_and_writes_no_file() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[]);

        assert_eq!(r.issues, []);
        assert_eq!(fake.calls(), []);
        assert!(!store_path(&f).exists());
        assert!(f.reg_files().is_empty());
    }

    #[test]
    fn a_later_change_of_the_same_id_replaces_the_earlier_one() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[change(CaretWidth, Number(3)), change(CaretWidth, Number(5))],
        );

        assert_eq!(r.issues, []);
        assert_eq!(fake.sets(), [(CaretWidth, Number(5))]);
        assert_eq!(r.report.succeeded, ["caret_width"]);
        assert_eq!(store.original(USER, "caret_width"), Some(&Number(1)));
    }

    #[test]
    fn windows_keeping_another_value_with_the_master_on_is_a_plain_mismatch() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.keep(CursorShadow, Flag(false));
        let mut store = VeBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[change(CursorShadow, Flag(true))]);

        assert_eq!(
            r.issues,
            [VeIssue::Failure(VeFailure::Mismatch {
                id: CursorShadow,
                wanted: Flag(true),
                read: Flag(false),
            })]
        );
        assert!(
            r.report.errors[0].contains("Windows kept off instead of on"),
            "{}",
            r.report.errors[0]
        );
        assert_errors_match(&r);
        assert!(r.report.succeeded.is_empty());
        // The master is live on, so nothing hides or clears the value: the
        // readback is the true live value, back at its original, and the
        // snapshot is forgotten.
        assert!(!store.has_backup(USER, "cursor_shadow"));
    }

    #[test]
    fn a_failed_set_is_reported_and_the_next_is_still_tried() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.fail_set(CaretWidth);
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(CaretWidth, Number(2)),
                change(DropShadow, Flag(false)),
            ],
        );

        assert_eq!(
            r.issues,
            [VeIssue::Failure(VeFailure::SetFailed {
                id: CaretWidth,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert_eq!(fake.value(DropShadow), Flag(false));
        assert_eq!(fake.value(CaretWidth), Number(1));
        assert_eq!(r.report.succeeded, ["drop_shadow"]);
        // The value read before the SET is the original: forgotten.
        assert!(!store.has_backup(USER, "caret_width"));
        assert!(store.has_backup(USER, "drop_shadow"));
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    /// Passes everything through, but every GET after a SET fails.
    struct ReadbackFails<'a> {
        inner: &'a FakeEffects,
    }

    impl EffectsApi for ReadbackFails<'_> {
        fn get(&self, id: EffectId) -> Result<SettingValue, ApiError> {
            if !self.inner.sets().is_empty() {
                return Err(ApiError { code: 5 });
            }
            self.inner.get(id)
        }

        fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError> {
            self.inner.set(id, value)
        }
    }

    #[test]
    fn an_unverified_write_is_never_forgotten() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();
        let api = ReadbackFails { inner: &fake };

        // A SET equal to the live value is still made and read back.
        let r = apply(&api, &mut store, &f, &[change(CaretWidth, Number(1))]);

        assert_eq!(
            r.issues,
            [VeIssue::Failure(VeFailure::Unverified {
                id: CaretWidth,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert_eq!(fake.sets(), [(CaretWidth, Number(1))]);
        assert!(r.report.succeeded.is_empty());
        assert_eq!(store.original(USER, "caret_width"), Some(&Number(1)));

        let r = restore(&api, &mut store, &f, &["caret_width"]);
        assert_eq!(
            r.issues,
            [VeIssue::Failure(VeFailure::Unverified {
                id: CaretWidth,
                code: 5
            })]
        );
        assert!(store.has_backup(USER, "caret_width"));
    }

    #[test]
    fn restore_sets_the_stored_value_exactly_even_outside_the_page_range() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();
        fake.put(FontSmoothingType, Number(3));
        fake.put(CaretWidth, Number(25));
        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(FontSmoothingType, Number(2)),
                change(CaretWidth, Number(2)),
            ],
        );
        assert_eq!(r.issues, []);
        let reg_files = f.reg_files();

        let r = restore(
            &fake,
            &mut store,
            &f,
            &["font_smoothing_type", "caret_width"],
        );

        assert_eq!(r.issues, []);
        assert_eq!(r.report.succeeded, ["font_smoothing_type", "caret_width"]);
        assert_eq!(fake.value(FontSmoothingType), Number(3));
        assert_eq!(fake.value(CaretWidth), Number(25));
        assert!(store.users.is_empty());
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
        assert_eq!(f.reg_files(), reg_files);
        assert!(r.report.reg_backup_file.is_none());
    }

    #[test]
    fn restore_of_an_unknown_or_unrecorded_id_reports_it() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();
        store.record_first(USER, "hot_tracking", "x", Flag(true));

        let r = restore(&fake, &mut store, &f, &["hot_tracking", "drop_shadow"]);

        assert_eq!(
            r.issues,
            [
                VeIssue::Problem(VeProblem::UnknownSetting("hot_tracking".into())),
                VeIssue::Problem(VeProblem::NoOriginal("drop_shadow".into())),
            ]
        );
        assert_errors_match(&r);
        assert_eq!(fake.calls(), []);
        assert!(store.has_backup(USER, "hot_tracking"));
    }

    #[test]
    fn the_wrong_account_refuses_apply_and_restore_before_any_call() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();
        store.record_first(USER, "ui_effects", "x", Flag(true));
        let different = || AccountCheck::Different {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        };
        let wrong = [VeIssue::Problem(VeProblem::WrongAccount {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        })];

        let r = apply_visual_effects(
            &fake,
            &mut store,
            &[change(UiEffects, Flag(false))],
            &opts(&f, &different),
        );
        assert_eq!(r.issues, wrong);
        assert_eq!(
            r.report.errors,
            ["WinTune runs as PC\\Admin, but PC\\Ana is signed in"]
        );
        let ids = vec!["ui_effects".to_string()];
        let r = restore_visual_effects(&fake, &mut store, &ids, &opts(&f, &different));
        assert_eq!(r.issues, wrong);
        assert_errors_match(&r);
        assert_eq!(fake.calls(), []);
        assert!(!store_path(&f).exists());
        assert!(f.reg_files().is_empty());

        let unknown = || AccountCheck::Unknown("x".into());
        let want = [VeIssue::Problem(VeProblem::AccountUnknown("x".into()))];
        let r = apply_visual_effects(
            &fake,
            &mut store,
            &[change(UiEffects, Flag(false))],
            &opts(&f, &unknown),
        );
        assert_eq!(r.issues, want);
        let r = restore_visual_effects(&fake, &mut store, &ids, &opts(&f, &unknown));
        assert_eq!(r.issues, want);
        assert_eq!(fake.calls(), []);
    }

    fn is_not_saved(r: &VeReport) -> bool {
        matches!(
            r.issues.as_slice(),
            [VeIssue::Problem(VeProblem::NotSaved(_))]
        )
    }

    #[test]
    fn an_unsavable_store_or_reg_refuses_before_any_write() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();
        let file = f.tmp.join("a-file");
        std::fs::write(&file, "x").unwrap();
        let changes = [change(UiEffects, Flag(false))];

        let mut o = opts(&f, &same);
        o.store_path = file.join("store.json");
        let r = apply_visual_effects(&fake, &mut store, &changes, &o);
        assert!(is_not_saved(&r), "{:?}", r.issues);
        assert_errors_match(&r);
        assert_eq!(r.report.errors[0].matches("Nothing applied").count(), 1);
        assert_eq!(fake.sets(), []);
        assert!(f.reg_files().is_empty());
        assert!(store.users.is_empty(), "the store mirrors the file");

        let mut o = opts(&f, &same);
        o.reg_backup_dir = file.clone();
        let r = apply_visual_effects(&fake, &mut store, &changes, &o);
        assert!(is_not_saved(&r), "{:?}", r.issues);
        assert_errors_match(&r);
        assert_eq!(fake.sets(), []);
        assert!(r.report.reg_backup_file.is_none());
        assert!(store.users.is_empty(), "the store is put back");
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    #[test]
    fn an_unsavable_companion_snapshot_refuses_the_restore() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();
        store.record_first(USER, "ui_effects", "x", Flag(true));
        let before = store.clone();
        let file = f.tmp.join("a-file");
        std::fs::write(&file, "x").unwrap();

        let mut o = opts(&f, &same);
        o.store_path = file.join("store.json");
        let ids = vec!["ui_effects".to_string()];
        let r = restore_visual_effects(&fake, &mut store, &ids, &o);

        assert!(is_not_saved(&r), "{:?}", r.issues);
        assert_errors_match(&r);
        assert_eq!(fake.sets(), []);
        assert_eq!(store, before);
    }

    #[test]
    fn the_reg_export_reads_control_panel_without_writing_it() {
        let f = fixture();
        let before = control_panel(&f);
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(WindowAnimation, Flag(false)),
                change(CursorShadow, Flag(true)),
                change(DropShadow, Flag(false)),
                change(CleartypeContrast, Number(1600)),
                change(CaretWidth, Number(3)),
                change(FocusBorderWidth, Number(2)),
            ],
        );
        assert_eq!(r.issues, []);
        assert_eq!(r.report.succeeded.len(), 6);
        assert_eq!(f.reg_files().len(), 1);
        let ids: Vec<String> = store.backups_of(USER).unwrap().keys().cloned().collect();
        let r = restore_visual_effects(&fake, &mut store, &ids, &opts(&f, &same));
        assert_eq!(r.issues, []);
        assert!(store.users.is_empty());

        assert_eq!(f.reg_files().len(), 1);
        assert_eq!(mask(&f), MASK);
        assert_eq!(control_panel(&f), before);
    }

    #[test]
    fn english_lines_name_the_effect_and_its_values() {
        let line = |i: VeIssue| english_issue(&i);
        assert_eq!(
            line(VeIssue::Failure(VeFailure::Mismatch {
                id: CaretWidth,
                wanted: Number(3),
                read: Number(1),
            })),
            "caret_width: Windows kept 1 instead of 3"
        );
        assert_eq!(
            line(VeIssue::Failure(VeFailure::HeldByMaster {
                id: CursorShadow,
                wanted: Flag(true),
                read: Flag(false),
            })),
            "cursor_shadow: Windows keeps this effect off while UI effects is off"
        );
        assert_eq!(
            line(VeIssue::Problem(VeProblem::NotSaved("disk full".into()))),
            "Nothing applied: the originals or the .reg backup could not be saved (disk full)"
        );
        assert_eq!(
            line(VeIssue::Problem(VeProblem::Unreadable {
                id: MenuFade,
                code: 5
            })),
            "menu_fade: Windows could not read this setting (error 5)"
        );
    }

    // ---- write order and readback while the master is off ----

    #[test]
    fn the_master_is_written_first_when_it_goes_on() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(CursorShadow, Flag(true)),
                change(UiEffects, Flag(true)),
                change(MenuAnimation, Flag(false)),
            ],
        );

        assert_eq!(r.issues, []);
        let mut snapshot = gets([UiEffects]);
        snapshot.extend(gets(MASTER_COMPANIONS));
        assert_eq!(before_first_set(&fake.calls()), snapshot);
        assert_eq!(
            set_ids(&fake.sets()),
            [UiEffects, MenuAnimation, CursorShadow]
        );
    }

    #[test]
    fn the_master_is_written_last_when_it_goes_off() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(UiEffects, Flag(false)),
                change(DropShadow, Flag(false)),
                change(CaretWidth, Number(2)),
            ],
        );

        assert_eq!(r.issues, []);
        assert_eq!(set_ids(&fake.sets()), [DropShadow, CaretWidth, UiEffects]);
    }

    #[test]
    fn a_gated_write_under_a_master_that_is_off_is_held_by_master() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[change(CursorShadow, Flag(true))]);

        assert_eq!(
            r.issues,
            [VeIssue::Failure(VeFailure::HeldByMaster {
                id: CursorShadow,
                wanted: Flag(true),
                read: Flag(false),
            })]
        );
        assert!(
            r.report.errors[0].contains("keeps this effect off while UI effects is off"),
            "{}",
            r.report.errors[0]
        );
        assert_errors_match(&r);
        assert_eq!(fake.calls().last(), Some(&VeCall::Get(UiEffects)));
        // Kept although the value read equals the original.
        assert!(store.has_backup(USER, "cursor_shadow"));
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(false)));
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    #[test]
    fn a_gated_row_written_with_the_master_going_on_reads_back_after_it() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(MenuAnimation, Flag(true)),
                change(UiEffects, Flag(true)),
            ],
        );

        assert_eq!(r.issues, []);
        assert_eq!(r.report.succeeded, ["ui_effects", "menu_animation"]);
        let calls = fake.calls();
        let snapshot = calls
            .iter()
            .position(|c| *c == VeCall::Get(MenuAnimation))
            .unwrap();
        let master_set = calls
            .iter()
            .position(|c| *c == VeCall::Set(UiEffects, Flag(true)))
            .unwrap();
        assert!(snapshot < master_set, "{calls:?}");
    }

    // ---- companion snapshots (spec 5.3) ----

    #[test]
    fn turning_the_master_off_snapshots_its_companions_first() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);

        assert_eq!(r.issues, []);
        let mut snapshot = gets([UiEffects]);
        snapshot.extend(gets(MASTER_COMPANIONS));
        assert_eq!(before_first_set(&fake.calls()), snapshot);
        assert_eq!(fake.sets(), [(UiEffects, Flag(false))]);
        assert_eq!(store.original(USER, "ui_effects"), Some(&Flag(true)));
        assert_eq!(store.companion_of(USER, "ui_effects"), None);
        for (id, seed) in companion_seeds() {
            assert_eq!(store.original(USER, id.key()), Some(&seed), "{}", id.key());
            assert_eq!(
                store.companion_of(USER, id.key()),
                Some("ui_effects"),
                "{}",
                id.key()
            );
        }
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(false)));
        assert_eq!(store.original(USER, "menu_fade"), Some(&Flag(true)));
        assert_eq!(store.backups_of(USER).unwrap().len(), 9);
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    #[test]
    fn a_failed_companion_get_refuses_the_whole_apply() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.fail_get(CursorShadow);
        let mut store = VeBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);

        assert_eq!(
            r.issues,
            [VeIssue::Problem(VeProblem::Unreadable {
                id: CursorShadow,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert_eq!(fake.sets(), []);
        assert!(!store_path(&f).exists());
        assert!(f.reg_files().is_empty());
        assert!(store.users.is_empty());
    }

    #[test]
    fn a_row_with_its_own_snapshot_keeps_it() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        apply(&fake, &mut store, &f, &[change(CursorShadow, Flag(true))]);
        let r = apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);

        assert_eq!(r.issues, []);
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(false)));
        assert_eq!(store.companion_of(USER, "cursor_shadow"), None);
    }

    #[test]
    fn changing_a_companion_directly_clears_its_mark() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);
        assert_eq!(
            store.companion_of(USER, "menu_animation"),
            Some("ui_effects")
        );
        let r = apply(&fake, &mut store, &f, &[change(MenuAnimation, Flag(false))]);

        assert_eq!(r.issues, []);
        assert_eq!(store.original(USER, "menu_animation"), Some(&Flag(true)));
        assert_eq!(store.companion_of(USER, "menu_animation"), None);
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    #[test]
    fn companions_are_read_at_apply_time_not_from_the_scan() {
        let f = fixture();
        let fake = FakeEffects::new();
        let mut store = VeBackupStore::default();

        let scan = scan_visual_effects(&fake, &same);
        assert_eq!(scan.live(MenuAnimation), Some(Flag(true)));
        fake.put(MenuAnimation, Flag(false));
        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);

        assert_eq!(store.original(USER, "menu_animation"), Some(&Flag(false)));
    }

    // ---- restore and forget, both FOLKLORE cases ----

    #[test]
    fn restoring_the_master_restores_its_companions_master_first() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::Clears);
        let mut store = VeBackupStore::default();
        let seed = values(&fake);

        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);
        for id in MASTER_COMPANIONS {
            assert_eq!(fake.value(id), Flag(false), "{}", id.key());
        }
        let applied = fake.sets().len();
        let r = restore(&fake, &mut store, &f, &["ui_effects"]);

        assert_eq!(r.issues, []);
        assert_errors_match(&r);
        let mut want = vec![(UiEffects, Flag(true))];
        want.extend(companion_seeds());
        assert_eq!(fake.sets()[applied..], want);
        assert!(want.contains(&(CursorShadow, Flag(false))));
        assert_eq!(values(&fake), seed);
        assert!(store.users.is_empty());
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    #[test]
    fn restoring_a_master_that_was_off_writes_it_last() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();

        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(true))]);
        let applied = fake.sets().len();
        let r = restore(&fake, &mut store, &f, &["ui_effects"]);

        assert_eq!(r.issues, []);
        let mut want = companion_seeds();
        want.push((UiEffects, Flag(false)));
        assert_eq!(fake.sets()[applied..], want);
        assert!(store.users.is_empty());
    }

    #[test]
    fn case_2_loses_nothing() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        let mut store = VeBackupStore::default();
        let seed = values(&fake);

        let r = apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);
        assert_eq!(r.issues, []);
        let scan = scan_visual_effects(&fake, &same);
        for id in MASTER_COMPANIONS {
            assert_eq!(scan.live(id), Some(Flag(false)), "{}", id.key());
        }
        let r = restore(&fake, &mut store, &f, &["ui_effects"]);

        assert_eq!(r.issues, []);
        assert_eq!(r.report.succeeded.len(), 9);
        assert!(store.users.is_empty());
        assert_eq!(values(&fake), seed);
    }

    #[test]
    fn turning_the_master_back_on_forgets_it_and_its_companions() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::Gates);
        let mut store = VeBackupStore::default();

        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);
        let r = apply(&fake, &mut store, &f, &[change(UiEffects, Flag(true))]);

        assert_eq!(r.issues, []);
        assert!(store.users.is_empty());
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
        let calls = fake.calls();
        let last_set = calls
            .iter()
            .rposition(|c| matches!(c, VeCall::Set(..)))
            .unwrap();
        let mut want = gets([UiEffects]);
        want.extend(gets(MASTER_COMPANIONS));
        assert_eq!(calls[last_set + 1..], want);
    }

    #[test]
    fn case_1_a_companion_still_off_becomes_its_own_row() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::Clears);
        let mut store = VeBackupStore::default();

        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);
        let r = apply(&fake, &mut store, &f, &[change(UiEffects, Flag(true))]);

        assert_eq!(r.issues, []);
        assert!(!store.has_backup(USER, "ui_effects"));
        assert!(!store.has_backup(USER, "cursor_shadow"));
        let kept: Vec<EffectId> = MASTER_COMPANIONS
            .into_iter()
            .filter(|id| *id != CursorShadow)
            .collect();
        assert_eq!(kept.len(), 7);
        for id in kept {
            assert_eq!(
                store.original(USER, id.key()),
                Some(&Flag(true)),
                "{}",
                id.key()
            );
            assert_eq!(store.companion_of(USER, id.key()), None, "{}", id.key());
        }
        assert_eq!(store.backups_of(USER).unwrap().len(), 7);
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }

    #[test]
    fn restore_all_writes_each_id_once_in_master_order() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::Clears);
        let mut store = VeBackupStore::default();
        let seed = values(&fake);

        apply(&fake, &mut store, &f, &[change(UiEffects, Flag(false))]);
        apply(&fake, &mut store, &f, &[change(CursorShadow, Flag(true))]);
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(false)));
        assert_eq!(store.companion_of(USER, "cursor_shadow"), None);
        let applied = fake.sets().len();

        let ids: Vec<String> = store.backups_of(USER).unwrap().keys().cloned().collect();
        let r = restore_visual_effects(&fake, &mut store, &ids, &opts(&f, &same));

        assert_eq!(r.issues, []);
        let restored = set_ids(&fake.sets()[applied..]);
        let mut want = vec![UiEffects];
        want.extend(MASTER_COMPANIONS);
        assert_eq!(restored, want);
        assert!(store.users.is_empty());
        assert_eq!(values(&fake), seed);
    }

    #[test]
    fn a_restore_sets_a_gated_id_while_its_gate_is_off() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();

        apply(&fake, &mut store, &f, &[change(MenuAnimation, Flag(false))]);
        let r = restore(&fake, &mut store, &f, &["menu_animation"]);

        assert_eq!(r.issues, []);
        assert_eq!(fake.sets().last(), Some(&(MenuAnimation, Flag(true))));
        assert!(store.users.is_empty());
    }

    #[test]
    fn restoring_the_master_snapshots_the_companions_it_lacks_first() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();
        store.record_first(USER, "ui_effects", "x", Flag(true));
        store.record_first(USER, "cursor_shadow", "x", Flag(true));

        let r = restore(&fake, &mut store, &f, &["ui_effects"]);

        assert_eq!(r.issues, []);
        let lacking: Vec<EffectId> = MASTER_COMPANIONS
            .into_iter()
            .filter(|id| *id != CursorShadow)
            .collect();
        assert_eq!(before_first_set(&fake.calls()), gets(lacking.clone()));
        // The master first; its new companions with the values just read;
        // cursor shadow is its own row and was not asked for.
        let mut want = vec![(UiEffects, Flag(true))];
        want.extend(lacking.iter().map(|&id| (id, Flag(true))));
        assert_eq!(fake.sets(), want);
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(true)));
        assert_eq!(store.backups_of(USER).unwrap().len(), 1);

        let fake = FakeEffects::new();
        fake.fail_get(MenuFade);
        let mut store = VeBackupStore::default();
        store.record_first(USER, "ui_effects", "x", Flag(true));
        let r = restore(&fake, &mut store, &f, &["ui_effects"]);
        assert_eq!(
            r.issues,
            [VeIssue::Problem(VeProblem::Unreadable {
                id: MenuFade,
                code: 5
            })]
        );
        assert_eq!(fake.sets(), []);
        assert_eq!(store.backups_of(USER).unwrap().len(), 1);
    }

    // ---- review T3-C1, T3-C2, T3-C3: originals the master hides or clears ----

    /// T3-C1: in case 2, the snapshot GETs taken while the master is off all
    /// read off. Turning the master on, and touching menu fade in the same
    /// batch, then restoring everything must give back the hidden values.
    #[test]
    fn case_2_a_master_turned_on_from_off_keeps_the_hidden_originals() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        fake.put(UiEffects, Flag(false));
        let seed = values(&fake);
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[change(UiEffects, Flag(true)), change(MenuFade, Flag(false))],
        );
        assert_eq!(r.issues, []);
        assert_eq!(r.report.succeeded, ["ui_effects", "menu_fade"]);
        for (id, seed) in companion_seeds() {
            assert_eq!(store.original(USER, id.key()), Some(&seed), "{}", id.key());
        }
        assert_eq!(store.companion_of(USER, "menu_fade"), None);
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);

        let ids: Vec<String> = store.backups_of(USER).unwrap().keys().cloned().collect();
        let r = restore_visual_effects(&fake, &mut store, &ids, &opts(&f, &same));

        assert_eq!(r.issues, []);
        assert!(store.users.is_empty());
        assert_eq!(values(&fake), seed);
    }

    /// T3-C1, restore side: a restore that sets the master on reads the
    /// companions it lacks again after that SET.
    #[test]
    fn case_2_a_master_on_restore_rereads_the_companions_it_snapshotted() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        fake.put(UiEffects, Flag(false));
        let mut store = VeBackupStore::default();
        store.record_first(USER, "ui_effects", "x", Flag(true));

        let r = restore(&fake, &mut store, &f, &["ui_effects"]);

        assert_eq!(r.issues, []);
        assert!(store.users.is_empty());
        assert_eq!(values(&fake), values(&FakeEffects::new()));
        let mut want = vec![(UiEffects, Flag(true))];
        want.extend(companion_seeds());
        assert_eq!(fake.sets(), want);
    }

    /// T3-C2: the master's SET, written last, clears a touched companion
    /// that was read back before it; its snapshot must survive.
    #[test]
    fn case_1_a_companion_touched_with_the_master_going_off_keeps_its_snapshot() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::Clears);
        let seed = values(&fake);
        let mut store = VeBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                change(UiEffects, Flag(false)),
                change(MenuAnimation, Flag(true)),
            ],
        );
        assert_eq!(r.issues, []);
        assert_eq!(fake.value(MenuAnimation), Flag(false));
        assert_eq!(store.original(USER, "menu_animation"), Some(&Flag(true)));
        assert_eq!(store.companion_of(USER, "menu_animation"), None);

        let ids: Vec<String> = store.backups_of(USER).unwrap().keys().cloned().collect();
        let r = restore_visual_effects(&fake, &mut store, &ids, &opts(&f, &same));

        assert_eq!(r.issues, []);
        assert_eq!(fake.value(MenuAnimation), Flag(true));
        assert!(store.users.is_empty());
        assert_eq!(values(&fake), seed);
    }

    /// Passes everything through, but the first GET of the master fails.
    struct MasterGetFailsOnce<'a> {
        inner: &'a FakeEffects,
        failed: std::sync::atomic::AtomicBool,
    }

    impl EffectsApi for MasterGetFailsOnce<'_> {
        fn get(&self, id: EffectId) -> Result<SettingValue, ApiError> {
            use std::sync::atomic::Ordering;
            if id == UiEffects && !self.failed.swap(true, Ordering::SeqCst) {
                return Err(ApiError { code: 5 });
            }
            self.inner.get(id)
        }

        fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError> {
            self.inner.set(id, value)
        }
    }

    /// T3-C3: a differing companion readback whose master GET fails cannot
    /// tell held from mismatch, so the snapshot is kept.
    #[test]
    fn a_failed_master_get_on_a_differing_companion_keeps_the_snapshot() {
        let f = fixture();
        let fake = FakeEffects::new();
        fake.master_mode(MasterMode::ReadsOff);
        fake.put(UiEffects, Flag(false));
        let api = MasterGetFailsOnce {
            inner: &fake,
            failed: std::sync::atomic::AtomicBool::new(false),
        };
        let mut store = VeBackupStore::default();

        let r = apply(&api, &mut store, &f, &[change(CursorShadow, Flag(true))]);

        assert_eq!(
            r.issues,
            [VeIssue::Failure(VeFailure::Unverified {
                id: CursorShadow,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert!(r.report.succeeded.is_empty());
        assert_eq!(store.original(USER, "cursor_shadow"), Some(&Flag(false)));
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
    }
}
