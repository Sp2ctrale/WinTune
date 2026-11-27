//! Scan, Apply and Restore. Apply follows spec 5.4: account check, validate,
//! read, snapshot (store, then `.reg`), write with readback, forget.

use super::{
    check_change, export_control_panel, resolve, save_mk_store, AccountCheck, ApiError, InputApi,
    MkBackupStore, MkChange, MkFailure, MkIssue, MkProblem, SettingId, SettingValue, Wanted,
    WHEEL_PAGESCROLL,
};
use crate::associations::OperationReport;
use crate::backups::RegTarget;
use crate::clock;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What a scan read: every setting (or its read error), Filter Keys and the
/// account check.
#[derive(Debug, Clone, PartialEq)]
pub struct MkScan {
    pub values: BTreeMap<SettingId, Result<SettingValue, ApiError>>,
    pub filter_keys_on: bool,
    pub account: AccountCheck,
}

impl MkScan {
    pub fn live(&self, id: SettingId) -> Option<SettingValue> {
        self.values.get(&id)?.as_ref().ok().copied()
    }

    pub fn read_error(&self, id: SettingId) -> Option<ApiError> {
        self.values.get(&id)?.as_ref().err().copied()
    }

    pub fn account_ok(&self) -> bool {
        self.account == AccountCheck::Same
    }
}

/// One GET per setting in table order, then Filter Keys (an error counts as
/// off), then the account check. Never a SET.
pub fn scan_mouse_keyboard(api: &dyn InputApi, account: &dyn Fn() -> AccountCheck) -> MkScan {
    let values = SettingId::ALL
        .into_iter()
        .map(|id| (id, api.get(id)))
        .collect();
    let filter_keys_on = api.filter_keys_on().unwrap_or(false);
    MkScan {
        values,
        filter_keys_on,
        account: account(),
    }
}

pub struct MkOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: String,
    pub store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    /// Where the `.reg` net reads `Control Panel` (read only).
    pub reg_source: &'a RegTarget,
    pub account: &'a dyn Fn() -> AccountCheck,
    pub progress: &'a dyn Fn(usize, usize, &str),
}

/// `report.errors[i]` is `english_issue(&issues[i])`, one line per issue and
/// nothing else; `report.succeeded` holds `SettingId::key` strings.
#[derive(Debug, Default)]
pub struct MkReport {
    pub report: OperationReport,
    pub issues: Vec<MkIssue>,
}

impl MkReport {
    fn push(&mut self, issue: MkIssue) {
        self.report.errors.push(english_issue(&issue));
        self.issues.push(issue);
    }

    fn problem(&mut self, problem: MkProblem) {
        self.push(MkIssue::Problem(problem));
    }

    fn failure(&mut self, failure: MkFailure) {
        self.push(MkIssue::Failure(failure));
    }
}

fn english_value(id: SettingId, v: &SettingValue) -> String {
    match *v {
        SettingValue::Number(WHEEL_PAGESCROLL) if id == SettingId::WheelLines => {
            "one screen at a time".to_string()
        }
        SettingValue::Number(n) => n.to_string(),
        SettingValue::Flag(true) => "on".to_string(),
        SettingValue::Flag(false) => "off".to_string(),
        SettingValue::Mouse([t1, t2, a]) => format!("{{{t1}, {t2}, {a}}}"),
    }
}

/// The English line for History and logs; the UI translates the enum.
pub fn english_issue(issue: &MkIssue) -> String {
    match issue {
        MkIssue::Problem(p) => match p {
            MkProblem::OutOfRange(id) => {
                format!("{}: this value is outside what WinTune allows", id.key())
            }
            MkProblem::UnknownSetting(key) => {
                format!("{key} is a setting this version of WinTune does not know")
            }
            MkProblem::Unreadable { id, code } => format!(
                "{}: Windows could not read this setting (error {code})",
                id.key()
            ),
            MkProblem::WrongAccount { process, session } => {
                format!("WinTune runs as {process}, but {session} is signed in")
            }
            MkProblem::AccountUnknown(why) => format!(
                "WinTune cannot tell which account is signed in ({why}), \
                 so it does not change mouse and keyboard settings"
            ),
            MkProblem::NotSaved(why) => format!(
                "Nothing applied: the originals or the .reg backup could not be saved ({why})"
            ),
            MkProblem::NoOriginal(key) => format!("{key}: no original value is saved"),
        },
        MkIssue::Failure(f) => match f {
            MkFailure::SetFailed { id, code } => {
                format!("{}: Windows refused the change (error {code})", id.key())
            }
            MkFailure::Mismatch { id, wanted, read } => format!(
                "{}: Windows kept {} instead of {}",
                id.key(),
                english_value(*id, read),
                english_value(*id, wanted)
            ),
            MkFailure::Unverified { id, code } => format!(
                "{}: WinTune could not read the setting back (error {code})",
                id.key()
            ),
            MkFailure::StoreNotSaved(why) => {
                format!("The change was made, but the saved originals could not be updated: {why}")
            }
        },
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
}

/// SET `value`, then GET it back, reporting a failure in `out`.
fn write_and_read_back(
    api: &dyn InputApi,
    id: SettingId,
    value: SettingValue,
    out: &mut MkReport,
) -> Written {
    if let Err(e) = api.set(id, &value) {
        out.failure(MkFailure::SetFailed { id, code: e.code });
        return Written::Refused;
    }
    match api.get(id) {
        Err(e) => {
            out.failure(MkFailure::Unverified { id, code: e.code });
            Written::Unknown
        }
        Ok(read) if read != value => {
            out.failure(MkFailure::Mismatch {
                id,
                wanted: value,
                read,
            });
            Written::Other(read)
        }
        Ok(read) => Written::Kept(read),
    }
}

/// Saves the store after snapshots were forgotten.
fn save_after_forget(store: &MkBackupStore, opts: &MkOptions, out: &mut MkReport) {
    if let Err(e) = save_mk_store(store, &opts.store_path) {
        out.failure(MkFailure::StoreNotSaved(e));
    }
}

/// `swap_buttons` last (spec 5.4 step 4), the rest in table order.
fn write_order(id: SettingId) -> (bool, SettingId) {
    (id == SettingId::SwapButtons, id)
}

/// Applies `changes` in the order of spec 5.4. Every refusal before the
/// first SET leaves the settings, the store file and the `.reg` folder as
/// they were. A later change of the same id replaces an earlier one.
pub fn apply_mouse_keyboard(
    api: &dyn InputApi,
    store: &mut MkBackupStore,
    changes: &[MkChange],
    opts: &MkOptions,
) -> MkReport {
    let mut out = MkReport::default();
    let user = opts.user.as_str();

    // 1. The account, before any call.
    if let Some(p) = (opts.account)().problem() {
        out.problem(p);
        return out;
    }

    // 2. Validate every change; write nothing.
    for c in changes {
        if let Some(p) = check_change(c) {
            out.problem(p);
        }
    }
    if !out.issues.is_empty() || changes.is_empty() {
        return out;
    }
    let wanted: BTreeMap<SettingId, Wanted> = changes.iter().map(|c| (c.id, c.wanted)).collect();

    // 3. Read each touched setting now: it is the snapshot.
    let mut live: BTreeMap<SettingId, SettingValue> = BTreeMap::new();
    for &id in wanted.keys() {
        match api.get(id) {
            Ok(v) => {
                live.insert(id, v);
            }
            Err(e) => out.problem(MkProblem::Unreadable { id, code: e.code }),
        }
    }
    if !out.issues.is_empty() {
        return out;
    }

    // The value to set, from the live read and the original (the stored
    // one, else the live value about to be recorded). Resolved before any
    // save so an unresolvable pair still refuses with nothing written.
    let mut targets: BTreeMap<SettingId, SettingValue> = BTreeMap::new();
    for (&id, w) in &wanted {
        let now = &live[&id];
        let original = store.original(user, id.key()).unwrap_or(now);
        match resolve(id, w, now, original) {
            Some(v) => {
                targets.insert(id, v);
            }
            None => out.problem(MkProblem::OutOfRange(id)),
        }
    }
    if !out.issues.is_empty() {
        return out;
    }

    // 4. Snapshot on first touch, save the store, then write the `.reg`.
    let before = store.clone();
    let timestamp = clock::now_local();
    for (&id, v) in &live {
        store.record_first(user, id.key(), &timestamp, *v);
    }
    if let Err(e) = save_mk_store(store, &opts.store_path) {
        // Keep the store in memory as the file still is.
        *store = before;
        out.problem(MkProblem::NotSaved(e));
        return out;
    }
    match export_control_panel(opts.reg_source, &opts.reg_backup_dir) {
        Ok(file) => out.report.reg_backup_file = Some(file),
        Err(e) => {
            out.problem(MkProblem::NotSaved(e));
            return out;
        }
    }

    // 5. Write, each SET followed by its GET.
    let mut order: Vec<SettingId> = targets.keys().copied().collect();
    order.sort_by_key(|&id| write_order(id));
    let total = order.len();
    let mut after: Vec<(SettingId, SettingValue)> = Vec::new();
    for (n, &id) in order.iter().enumerate() {
        (opts.progress)(n, total, &format!("Writing {}", id.key()));
        match write_and_read_back(api, id, targets[&id], &mut out) {
            Written::Kept(read) => {
                out.report.succeeded.push(id.key().to_string());
                after.push((id, read));
            }
            Written::Other(read) => after.push((id, read)),
            Written::Refused => after.push((id, live[&id])),
            Written::Unknown => {}
        }
    }

    // 6. Forget each snapshot back at its original.
    let mut forgot = false;
    for (id, value) in after {
        if store.original(user, id.key()) == Some(&value) {
            store.forget(user, id.key());
            forgot = true;
        }
    }
    if forgot {
        save_after_forget(store, opts, &mut out);
    }
    (opts.progress)(total, total, "Apply finished");
    out
}

/// Sets each id back to its stored original exactly (no range check),
/// `swap_buttons` last, and forgets the snapshot when the readback matches.
/// No `.reg`.
pub fn restore_mouse_keyboard(
    api: &dyn InputApi,
    store: &mut MkBackupStore,
    ids: &[String],
    opts: &MkOptions,
) -> MkReport {
    let mut out = MkReport::default();
    let user = opts.user.as_str();

    if let Some(p) = (opts.account)().problem() {
        out.problem(p);
        return out;
    }

    let mut order: Vec<&String> = Vec::new();
    for key in ids {
        if !order.contains(&key) {
            order.push(key);
        }
    }
    order.sort_by_key(|key| key.as_str() == SettingId::SwapButtons.key());

    let total = order.len();
    let mut forgot = false;
    for (n, key) in order.into_iter().enumerate() {
        (opts.progress)(n, total, &format!("Restoring {key}"));
        let Some(id) = SettingId::from_key(key) else {
            out.problem(MkProblem::UnknownSetting(key.clone()));
            continue;
        };
        let Some(&original) = store.original(user, key) else {
            out.problem(MkProblem::NoOriginal(key.clone()));
            continue;
        };
        if let Written::Kept(_) = write_and_read_back(api, id, original, &mut out) {
            store.forget(user, key);
            out.report.succeeded.push(key.clone());
            forgot = true;
        }
    }
    if forgot {
        save_after_forget(store, opts, &mut out);
    }
    (opts.progress)(total, total, "Restore finished");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mouse_keyboard::store::test_support::{Fixture, USER};
    use crate::mouse_keyboard::SettingId::*;
    use crate::mouse_keyboard::SettingValue::*;
    use crate::mouse_keyboard::*;

    fn same() -> AccountCheck {
        AccountCheck::Same
    }

    fn no_progress(_: usize, _: usize, _: &str) {}

    fn opts<'a>(f: &'a Fixture, account: &'a dyn Fn() -> AccountCheck) -> MkOptions<'a> {
        MkOptions {
            user: USER.into(),
            store_path: f.store_path(),
            reg_backup_dir: f.reg_dir(),
            reg_source: &f.source,
            account,
            progress: &no_progress,
        }
    }

    fn value(id: SettingId, v: SettingValue) -> MkChange {
        MkChange {
            id,
            wanted: Wanted::Value(v),
        }
    }

    fn toggle(id: SettingId, on: bool) -> MkChange {
        MkChange {
            id,
            wanted: Wanted::Toggle(on),
        }
    }

    fn apply(
        fake: &FakeInput,
        store: &mut MkBackupStore,
        f: &Fixture,
        changes: &[MkChange],
    ) -> MkReport {
        apply_mouse_keyboard(fake, store, changes, &opts(f, &same))
    }

    fn restore(fake: &FakeInput, store: &mut MkBackupStore, f: &Fixture, ids: &[&str]) -> MkReport {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        restore_mouse_keyboard(fake, store, &ids, &opts(f, &same))
    }

    fn assert_errors_match(r: &MkReport) {
        assert_eq!(r.report.errors.len(), r.issues.len());
        for (line, issue) in r.report.errors.iter().zip(&r.issues) {
            assert_eq!(line, &english_issue(issue));
        }
    }

    fn last_set(fake: &FakeInput) -> (SettingId, SettingValue) {
        *fake.sets().last().expect("a SET")
    }

    #[test]
    fn the_scan_reads_every_setting_once_and_records_failed_reads() {
        let fake = FakeInput::new();
        fake.fail_get(ClickLockTime);

        let scan = scan_mouse_keyboard(&fake, &same);

        assert_eq!(scan.read_error(ClickLockTime), Some(ApiError { code: 5 }));
        assert_eq!(scan.live(ClickLockTime), None);
        assert_eq!(scan.read_error(PointerSpeed), None);
        assert_eq!(scan.live(PointerSpeed), Some(Number(10)));
        let live = SettingId::ALL
            .iter()
            .filter(|&&id| scan.live(id).is_some())
            .count();
        assert_eq!(live, 17);
        assert!(!scan.filter_keys_on);
        assert_eq!(scan.account, AccountCheck::Same);
        assert!(scan.account_ok());
        let mut want: Vec<FakeCall> = SettingId::ALL.into_iter().map(FakeCall::Get).collect();
        want.push(FakeCall::FilterKeys);
        assert_eq!(fake.calls(), want);

        let other = scan_mouse_keyboard(&fake, &|| AccountCheck::Unknown("x".into()));
        assert!(!other.account_ok());
    }

    #[test]
    fn apply_sets_reads_back_and_keeps_the_original() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();

        let r = apply(&fake, &mut store, &f, &[value(PointerSpeed, Number(14))]);

        assert_eq!(r.issues, []);
        assert_errors_match(&r);
        assert_eq!(fake.value(PointerSpeed), Number(14));
        assert_eq!(store.original(USER, "pointer_speed"), Some(&Number(10)));
        assert_eq!(load_mk_store(&f.store_path()).unwrap(), store);
        assert_eq!(f.reg_files().len(), 1);
        assert_eq!(r.report.reg_backup_file.as_ref(), Some(&f.reg_files()[0]));
        assert_eq!(r.report.succeeded, ["pointer_speed"]);
        assert_eq!(
            fake.calls(),
            [
                FakeCall::Get(PointerSpeed),
                FakeCall::Set(PointerSpeed, Number(14)),
                FakeCall::Get(PointerSpeed),
            ]
        );
    }

    #[test]
    fn the_whole_apply_is_refused_on_a_bad_value() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(PointerSpeed, Number(21)),
                value(WheelChars, Number(5)),
            ],
        );

        assert_eq!(
            r.issues,
            [MkIssue::Problem(MkProblem::OutOfRange(PointerSpeed))]
        );
        assert_errors_match(&r);
        assert_eq!(fake.calls(), []);
        assert!(!f.store_path().exists());
        assert!(f.reg_files().is_empty());
        assert!(r.report.succeeded.is_empty());
    }

    #[test]
    fn a_failed_get_refuses_the_whole_apply() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        fake.fail_get(HoverTime);
        let mut store = MkBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(HoverTime, Number(600)),
                value(PointerSpeed, Number(14)),
            ],
        );

        assert_eq!(
            r.issues,
            [MkIssue::Problem(MkProblem::Unreadable {
                id: HoverTime,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert_eq!(fake.sets(), []);
        assert!(!f.store_path().exists());
        assert!(f.reg_files().is_empty());
        assert!(store.users.is_empty());
    }

    #[test]
    fn windows_keeping_another_value_fails_that_setting_only() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        fake.keep(DoubleClickTime, Number(500));
        let mut store = MkBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(DoubleClickTime, Number(700)),
                value(PointerSpeed, Number(14)),
            ],
        );

        assert_eq!(
            r.issues,
            [MkIssue::Failure(MkFailure::Mismatch {
                id: DoubleClickTime,
                wanted: Number(700),
                read: Number(500),
            })]
        );
        assert!(
            r.report.errors[0].contains("Windows kept 500 instead of 700"),
            "{}",
            r.report.errors[0]
        );
        assert_eq!(r.report.succeeded, ["pointer_speed"]);
        assert!(!store.has_backup(USER, "double_click_time"));
        assert!(store.has_backup(USER, "pointer_speed"));
        assert_eq!(load_mk_store(&f.store_path()).unwrap(), store);
        assert_eq!(r.report.errors.len(), r.issues.len());
        assert_errors_match(&r);
    }

    #[test]
    fn a_failed_set_is_reported_and_the_next_is_still_tried() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        fake.fail_set(PointerSpeed);
        let mut store = MkBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(PointerSpeed, Number(14)),
                value(KeyRepeatRate, Number(20)),
            ],
        );

        assert_eq!(
            r.issues,
            [MkIssue::Failure(MkFailure::SetFailed {
                id: PointerSpeed,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert_eq!(fake.value(KeyRepeatRate), Number(20));
        assert_eq!(fake.value(PointerSpeed), Number(10));
        assert_eq!(r.report.succeeded, ["key_repeat_rate"]);
        assert!(!store.has_backup(USER, "pointer_speed"));
        assert!(store.has_backup(USER, "key_repeat_rate"));
    }

    #[test]
    fn a_failed_readback_keeps_the_snapshot() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        let api = ReadbackFails { inner: &fake };

        // A SET equal to the live value is still made and read back.
        let r = apply_mouse_keyboard(
            &api,
            &mut store,
            &[value(PointerSpeed, Number(10))],
            &opts(&f, &same),
        );

        assert_eq!(
            r.issues,
            [MkIssue::Failure(MkFailure::Unverified {
                id: PointerSpeed,
                code: 5
            })]
        );
        assert_errors_match(&r);
        assert_eq!(fake.sets(), [(PointerSpeed, Number(10))]);
        assert!(r.report.succeeded.is_empty());
        assert!(store.has_backup(USER, "pointer_speed"));
    }

    /// Passes everything through, but every GET after a SET fails.
    struct ReadbackFails<'a> {
        inner: &'a FakeInput,
    }

    impl InputApi for ReadbackFails<'_> {
        fn get(&self, id: SettingId) -> Result<SettingValue, ApiError> {
            let set_before = self
                .inner
                .calls()
                .iter()
                .any(|c| matches!(c, FakeCall::Set(..)));
            if set_before {
                return Err(ApiError { code: 5 });
            }
            self.inner.get(id)
        }

        fn set(&self, id: SettingId, value: &SettingValue) -> Result<(), ApiError> {
            self.inner.set(id, value)
        }

        fn filter_keys_on(&self) -> Result<bool, ApiError> {
            self.inner.filter_keys_on()
        }
    }

    #[test]
    fn swap_buttons_is_written_last() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(SwapButtons, Flag(true)),
                value(PointerSpeed, Number(12)),
                value(KeyRepeatRate, Number(20)),
            ],
        );

        assert_eq!(r.issues, []);
        let ids: Vec<SettingId> = fake.sets().into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, [PointerSpeed, KeyRepeatRate, SwapButtons]);
    }

    #[test]
    fn precision_off_uses_the_thresholds_read_at_apply_time() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        fake.put(PointerPrecision, Mouse([4, 12, 2]));

        let r = apply(&fake, &mut store, &f, &[toggle(PointerPrecision, false)]);
        assert_eq!(r.issues, []);
        assert_eq!(last_set(&fake), (PointerPrecision, Mouse([4, 12, 0])));
        assert_eq!(
            store.original(USER, "pointer_precision"),
            Some(&Mouse([4, 12, 2]))
        );

        let r = apply(&fake, &mut store, &f, &[toggle(PointerPrecision, true)]);
        assert_eq!(r.issues, []);
        assert_eq!(last_set(&fake), (PointerPrecision, Mouse([4, 12, 2])));
        assert!(!store.has_backup(USER, "pointer_precision"));
    }

    #[test]
    fn precision_on_without_an_accelerated_original_and_the_windows_default() {
        let cases = [
            ([6, 10, 0], Wanted::Toggle(true), [6, 10, 1]),
            ([0, 0, 0], Wanted::Toggle(true), [6, 10, 1]),
            ([4, 12, 2], Wanted::Value(Mouse([6, 10, 1])), [6, 10, 1]),
        ];
        for (live, wanted, written) in cases {
            let f = Fixture::new();
            let fake = FakeInput::new();
            let mut store = MkBackupStore::default();
            fake.put(PointerPrecision, Mouse(live));

            let change = MkChange {
                id: PointerPrecision,
                wanted,
            };
            let r = apply(&fake, &mut store, &f, &[change]);

            assert_eq!(r.issues, [], "{live:?}");
            assert_eq!(
                fake.sets(),
                [(PointerPrecision, Mouse(written))],
                "{live:?}"
            );
        }
    }

    #[test]
    fn scroll_inactive_on_brings_back_an_original_one() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        fake.put(ScrollInactive, Number(1));

        apply(&fake, &mut store, &f, &[toggle(ScrollInactive, false)]);
        assert_eq!(fake.value(ScrollInactive), Number(0));
        apply(&fake, &mut store, &f, &[toggle(ScrollInactive, true)]);
        assert_eq!(fake.value(ScrollInactive), Number(1));
        assert!(!store.has_backup(USER, "scroll_inactive"));

        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        fake.put(ScrollInactive, Number(0));
        apply(&fake, &mut store, &f, &[toggle(ScrollInactive, true)]);
        assert_eq!(fake.value(ScrollInactive), Number(2));
    }

    #[test]
    fn editing_back_to_the_original_forgets_the_snapshot() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();

        apply(&fake, &mut store, &f, &[value(PointerSpeed, Number(14))]);
        assert!(store.has_backup(USER, "pointer_speed"));
        let r = apply(&fake, &mut store, &f, &[value(PointerSpeed, Number(10))]);

        assert_eq!(r.issues, []);
        assert!(store.users.is_empty());
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(f.store_path()).unwrap()).unwrap();
        assert_eq!(saved, serde_json::json!({ "users": {} }));
    }

    #[test]
    fn restore_sets_the_stored_value_exactly_even_outside_the_page_range() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        fake.put(DoubleClickTime, Number(1200));
        fake.put(PointerPrecision, Mouse([4, 12, 2]));
        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(DoubleClickTime, Number(500)),
                toggle(PointerPrecision, false),
            ],
        );
        assert_eq!(r.issues, []);
        let reg_files = f.reg_files();

        let r = restore(
            &fake,
            &mut store,
            &f,
            &["double_click_time", "pointer_precision"],
        );

        assert_eq!(r.issues, []);
        assert_eq!(
            r.report.succeeded,
            ["double_click_time", "pointer_precision"]
        );
        assert_eq!(fake.value(DoubleClickTime), Number(1200));
        assert_eq!(fake.value(PointerPrecision), Mouse([4, 12, 2]));
        assert!(store.users.is_empty());
        assert_eq!(load_mk_store(&f.store_path()).unwrap(), store);
        assert_eq!(f.reg_files(), reg_files);
        assert!(r.report.reg_backup_file.is_none());
    }

    #[test]
    fn a_restore_windows_does_not_keep_keeps_the_snapshot() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        apply(&fake, &mut store, &f, &[value(PointerSpeed, Number(14))]);
        fake.keep(PointerSpeed, Number(14));

        let r = restore(&fake, &mut store, &f, &["pointer_speed"]);

        assert_eq!(
            r.issues,
            [MkIssue::Failure(MkFailure::Mismatch {
                id: PointerSpeed,
                wanted: Number(10),
                read: Number(14),
            })]
        );
        assert_errors_match(&r);
        assert!(r.report.succeeded.is_empty());
        assert_eq!(store.original(USER, "pointer_speed"), Some(&Number(10)));
    }

    #[test]
    fn restore_of_an_unknown_or_unrecorded_id_reports_it() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        store.record_first(USER, "caret_blink", "x", Number(530));

        let r = restore(&fake, &mut store, &f, &["caret_blink", "pointer_speed"]);

        assert_eq!(
            r.issues,
            [
                MkIssue::Problem(MkProblem::UnknownSetting("caret_blink".into())),
                MkIssue::Problem(MkProblem::NoOriginal("pointer_speed".into())),
            ]
        );
        assert_errors_match(&r);
        assert_eq!(fake.sets(), []);
        assert!(store.has_backup(USER, "caret_blink"));
    }

    #[test]
    fn the_wrong_account_refuses_apply_and_restore_before_any_call() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        store.record_first(USER, "pointer_speed", "x", Number(12));
        let different = || AccountCheck::Different {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        };
        let wrong = [MkIssue::Problem(MkProblem::WrongAccount {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        })];

        let r = apply_mouse_keyboard(
            &fake,
            &mut store,
            &[value(PointerSpeed, Number(14))],
            &opts(&f, &different),
        );
        assert_eq!(r.issues, wrong);
        assert_eq!(
            r.report.errors,
            ["WinTune runs as PC\\Admin, but PC\\Ana is signed in"]
        );
        let ids = vec!["pointer_speed".to_string()];
        let r = restore_mouse_keyboard(&fake, &mut store, &ids, &opts(&f, &different));
        assert_eq!(r.issues, wrong);
        assert_errors_match(&r);
        assert_eq!(fake.calls(), []);
        assert!(!f.store_path().exists());
        assert!(f.reg_files().is_empty());

        let unknown = || AccountCheck::Unknown("x".into());
        let want = [MkIssue::Problem(MkProblem::AccountUnknown("x".into()))];
        let r = apply_mouse_keyboard(
            &fake,
            &mut store,
            &[value(PointerSpeed, Number(14))],
            &opts(&f, &unknown),
        );
        assert_eq!(r.issues, want);
        let r = restore_mouse_keyboard(&fake, &mut store, &ids, &opts(&f, &unknown));
        assert_eq!(r.issues, want);
        assert_eq!(fake.calls(), []);
    }

    #[test]
    fn an_unsavable_store_or_reg_refuses_before_any_write() {
        let f = Fixture::new();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();
        let file = f.tmp.join("a-file");
        std::fs::write(&file, "x").unwrap();
        let changes = [value(PointerSpeed, Number(14))];

        let mut o = opts(&f, &same);
        o.store_path = file.join("store.json");
        let r = apply_mouse_keyboard(&fake, &mut store, &changes, &o);
        assert!(
            matches!(
                r.issues.as_slice(),
                [MkIssue::Problem(MkProblem::NotSaved(_))]
            ),
            "{:?}",
            r.issues
        );
        assert_errors_match(&r);
        assert_eq!(r.report.errors[0].matches("Nothing applied").count(), 1);
        assert_eq!(fake.sets(), []);
        assert!(f.reg_files().is_empty());
        assert!(store.users.is_empty(), "the store mirrors the file");

        let mut o = opts(&f, &same);
        o.reg_backup_dir = file.clone();
        let r = apply_mouse_keyboard(&fake, &mut store, &changes, &o);
        assert!(
            matches!(
                r.issues.as_slice(),
                [MkIssue::Problem(MkProblem::NotSaved(_))]
            ),
            "{:?}",
            r.issues
        );
        assert_errors_match(&r);
        assert_eq!(r.report.errors[0].matches("Nothing applied").count(), 1);
        assert_eq!(fake.sets(), []);
        assert!(r.report.reg_backup_file.is_none());
    }

    #[test]
    fn nothing_under_control_panel_is_written() {
        let f = Fixture::new();
        f.key(r"Control Panel\Desktop\WindowMetrics")
            .set_value("BorderWidth", &"-15")
            .unwrap();
        f.key(r"Control Panel\Keyboard")
            .set_value("KeyboardSpeed", &"31")
            .unwrap();
        let before = f.control_panel();
        let fake = FakeInput::new();
        let mut store = MkBackupStore::default();

        let r = apply(
            &fake,
            &mut store,
            &f,
            &[
                value(PointerSpeed, Number(14)),
                toggle(PointerPrecision, false),
                value(DoubleClickTime, Number(700)),
                value(WheelLines, Number(WHEEL_PAGESCROLL)),
                value(KeyRepeatRate, Number(20)),
                value(SwapButtons, Flag(true)),
            ],
        );
        assert_eq!(r.issues, []);
        assert_eq!(r.report.succeeded.len(), 6);
        let ids: Vec<String> = store.backups_of(USER).unwrap().keys().cloned().collect();
        let r = restore_mouse_keyboard(&fake, &mut store, &ids, &opts(&f, &same));
        assert_eq!(r.issues, []);
        assert!(store.users.is_empty());

        assert_eq!(f.control_panel(), before);
    }

    #[test]
    fn english_lines_name_the_setting_and_its_values() {
        let line = |i: MkIssue| english_issue(&i);
        assert_eq!(
            line(MkIssue::Failure(MkFailure::Mismatch {
                id: WheelLines,
                wanted: Number(WHEEL_PAGESCROLL),
                read: Number(3),
            })),
            "wheel_lines: Windows kept 3 instead of one screen at a time"
        );
        assert_eq!(
            line(MkIssue::Failure(MkFailure::Mismatch {
                id: PointerPrecision,
                wanted: Mouse([6, 10, 1]),
                read: Mouse([6, 10, 0]),
            })),
            "pointer_precision: Windows kept {6, 10, 0} instead of {6, 10, 1}"
        );
        assert_eq!(
            line(MkIssue::Failure(MkFailure::Mismatch {
                id: SwapButtons,
                wanted: Flag(true),
                read: Flag(false),
            })),
            "swap_buttons: Windows kept off instead of on"
        );
        assert_eq!(
            line(MkIssue::Problem(MkProblem::NotSaved("disk full".into()))),
            "Nothing applied: the originals or the .reg backup could not be saved (disk full)"
        );
    }
}
