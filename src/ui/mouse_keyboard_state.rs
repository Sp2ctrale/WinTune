//! Mouse & keyboard: the background scan that reads every setting, and the
//! choices picked but not applied yet. Apply itself is
//! `jobs::start_mk_apply`, one job with the pending choices.

use super::{kit, WinTuneApp};
use crate::i18n::messages;
use crate::mouse_keyboard::{
    load_mk_store, resolve, scan_mouse_keyboard, wanted_matches, MkBackupStore, MkChange, MkScan,
    SettingId, SettingValue, Wanted,
};
use crate::tasks::{BackgroundJob, JobPoll};
use eframe::egui;

pub(super) type MkScanJob = BackgroundJob<MkScan>;

impl WinTuneApp {
    pub(super) fn reload_mk_store(&mut self) {
        match load_mk_store(&self.env.mk_store_path) {
            Ok(store) => {
                self.mk_store = store;
                self.mk_store_error = None;
            }
            Err(e) => {
                self.mk_store = MkBackupStore::default();
                let text = messages::load_error(self.tr, &e);
                self.notify(kit::ToastKind::Error, text);
                self.mk_store_error = Some(e);
            }
        }
    }

    /// Schedules a scan; it starts as soon as no job is running.
    pub(super) fn request_mk_scan(&mut self) {
        self.mk_scan_needed = true;
    }

    /// Called every frame: collects a finished scan and starts a pending one.
    /// The last result stays shown while the next scan runs.
    pub(super) fn pump_mk_scan(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.mk_scan_job.as_mut() {
            match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(scan) => {
                    // A choice the live value already matches (applied by a
                    // partial Apply, or changed in Settings), or for a
                    // setting Windows could not read, is no longer pending.
                    self.mk_pending.retain(|&id, wanted| {
                        scan.live(id)
                            .is_some_and(|live| !wanted_matches(id, wanted, &live))
                    });
                    self.mk_scan = Some(scan);
                }
                JobPoll::Panicked(msg) => {
                    let text = self.tr.t_args("mk.scan_failed", &[&msg]);
                    self.notify(kit::ToastKind::Error, text);
                }
            }
            self.mk_scan_job = None;
        }

        // Reading while a job writes would show transient values.
        if !self.mk_scan_needed || self.is_busy() {
            return;
        }
        self.mk_scan_needed = false;

        let env = self.env.clone();
        let repaint_ctx = ctx.clone();
        self.mk_scan_job = Some(BackgroundJob::spawn(
            "Reading the mouse and keyboard settings",
            move || repaint_ctx.request_repaint(),
            move |_| scan_mouse_keyboard(&*env.input, &*env.input_account),
        ));
    }

    /// The value this user had before WinTune first changed `id`.
    pub(super) fn mk_original(&self, id: SettingId) -> Option<SettingValue> {
        self.mk_store
            .original(&self.env.user_label, id.key())
            .copied()
    }

    /// The value the page shows: the pending one (a pending toggle previewed
    /// as Apply would resolve it now), else the live one. `None` before the
    /// first scan and for a setting Windows could not read.
    pub(super) fn mk_display(&self, id: SettingId) -> Option<SettingValue> {
        let live = self.mk_scan.as_ref()?.live(id)?;
        match self.mk_pending.get(&id) {
            None => Some(live),
            Some(Wanted::Value(v)) => Some(*v),
            Some(toggle) => {
                let original = self.mk_original(id).unwrap_or(live);
                Some(resolve(id, toggle, &live, &original).unwrap_or(live))
            }
        }
    }

    /// Records a choice; one the live value already matches is dropped.
    pub(super) fn set_mk_wanted(&mut self, id: SettingId, wanted: Wanted) {
        let live = self.mk_scan.as_ref().and_then(|s| s.live(id));
        if live.is_some_and(|live| wanted_matches(id, &wanted, &live)) {
            self.mk_pending.remove(&id);
        } else {
            self.mk_pending.insert(id, wanted);
        }
    }

    /// The changes Apply writes, in table order: one per pending setting the
    /// last scan read and whose live value differs. Empty before the first
    /// scan and while WinTune runs as another account than the session's.
    pub(super) fn mk_changes(&self) -> Vec<MkChange> {
        let Some(scan) = self.mk_scan.as_ref().filter(|s| s.account_ok()) else {
            return Vec::new();
        };
        self.mk_pending
            .iter()
            .filter(|&(&id, wanted)| {
                scan.live(id)
                    .is_some_and(|live| !wanted_matches(id, wanted, &live))
            })
            .map(|(&id, &wanted)| MkChange { id, wanted })
            .collect()
    }

    pub(super) fn mk_pending_count(&self) -> usize {
        self.mk_changes().len()
    }

    /// "Discard": forgets the pending choices.
    pub(super) fn discard_mk_changes(&mut self) {
        self.mk_pending.clear();
    }

    /// How the UI names a saved setting (as in History's saved originals):
    /// its title, else the raw key of a setting this version does not know.
    pub(super) fn mk_slot_name(&self, key: &str) -> String {
        match SettingId::from_key(key) {
            Some(id) => messages::mk_title(self.tr, id),
            None => key.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::mouse_keyboard::SettingId::*;
    use crate::mouse_keyboard::SettingValue::*;
    use crate::mouse_keyboard::{AccountCheck, MkChange, SettingId, Wanted};
    use egui::RawInput;
    use std::collections::BTreeMap;

    fn scanned(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_mk_scan();
        run_until_mk_scanned(&mut app, ctx);
        app
    }

    fn rescan(app: &mut WinTuneApp, ctx: &egui::Context) {
        app.request_mk_scan();
        run_until_mk_scanned(app, ctx);
    }

    #[test]
    fn the_mouse_keyboard_scan_runs_in_background_and_waits_for_jobs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        assert!(app.mk_scan.is_none());
        let release = hold_job(&mut app);

        app.request_mk_scan();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.mk_scan_job.is_none(), "scan started during a job");
        assert!(app.mk_scan_needed);
        assert_eq!(fx.fake_input().calls(), [], "read during a job");

        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        run_until_mk_scanned(&mut app, &ctx);
        let scan = app.mk_scan.as_ref().expect("scanned");
        assert_eq!(scan.values.len(), 18);
        assert!(scan.account_ok());
    }

    #[test]
    fn a_panicking_mouse_keyboard_scan_is_a_toast() {
        use crate::tasks::BackgroundJob;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.mk_scan_job = Some(BackgroundJob::spawn(
            "Reading the mouse and keyboard settings",
            || {},
            |_| -> crate::mouse_keyboard::MkScan { panic!("boom") },
        ));
        run_until_mk_scanned(&mut app, &ctx);
        assert!(
            app.last_notice()
                .starts_with("Could not read the mouse and keyboard settings: "),
            "{}",
            app.last_notice()
        );
        assert!(app.mk_scan.is_none());
    }

    #[test]
    fn a_scan_drops_choices_the_live_value_matches_or_cannot_read() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.mk_pending
            .insert(PointerSpeed, Wanted::Value(Number(14)));
        app.mk_pending
            .insert(PointerPrecision, Wanted::Toggle(false));
        app.mk_pending.insert(WheelChars, Wanted::Value(Number(5)));
        // Settings made the speed change itself; the wheel chars read fails.
        fx.fake_input().put(PointerSpeed, Number(14));
        fx.fake_input().fail_get(WheelChars);

        rescan(&mut app, &ctx);

        assert_eq!(
            app.mk_pending,
            BTreeMap::from([(PointerPrecision, Wanted::Toggle(false))])
        );
    }

    #[test]
    fn the_display_previews_a_pending_toggle() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        assert_eq!(app.mk_display(PointerSpeed), None, "before any scan");
        fx.fake_input().put(PointerPrecision, Mouse([4, 12, 2]));
        fx.fake_input().fail_get(HoverTime);
        rescan(&mut app, &ctx);

        app.set_mk_wanted(PointerPrecision, Wanted::Toggle(false));
        assert_eq!(
            app.mk_pending.get(&PointerPrecision),
            Some(&Wanted::Toggle(false))
        );
        assert_eq!(app.mk_display(PointerPrecision), Some(Mouse([4, 12, 0])));
        // Back on: the live value already is, so nothing is pending.
        app.set_mk_wanted(PointerPrecision, Wanted::Toggle(true));
        assert!(!app.mk_pending.contains_key(&PointerPrecision));
        assert_eq!(app.mk_display(PointerPrecision), Some(Mouse([4, 12, 2])));

        assert_eq!(app.mk_display(PointerSpeed), Some(Number(10)));
        app.set_mk_wanted(PointerSpeed, Wanted::Value(Number(14)));
        assert_eq!(app.mk_display(PointerSpeed), Some(Number(14)));
        app.set_mk_wanted(PointerSpeed, Wanted::Value(Number(10)));
        assert!(!app.mk_pending.contains_key(&PointerSpeed));

        assert_eq!(app.mk_display(HoverTime), None, "unread");
    }

    #[test]
    fn a_pending_precision_on_previews_this_users_original() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.fake_input().put(PointerPrecision, Mouse([4, 12, 0]));
        let mut app = scanned(&fx, &ctx);
        assert_eq!(app.mk_original(PointerPrecision), None);
        app.mk_store
            .record_first("OTHER\\user", "pointer_precision", "x", Mouse([7, 7, 1]));
        assert_eq!(app.mk_original(PointerPrecision), None, "another user's");
        app.mk_store
            .record_first("TEST\\user", "pointer_precision", "x", Mouse([5, 9, 2]));
        assert_eq!(app.mk_original(PointerPrecision), Some(Mouse([5, 9, 2])));

        app.set_mk_wanted(PointerPrecision, Wanted::Toggle(true));

        assert_eq!(app.mk_display(PointerPrecision), Some(Mouse([5, 9, 2])));
    }

    #[test]
    fn changes_are_empty_before_the_first_scan_and_for_the_wrong_account() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.mk_pending
            .insert(PointerSpeed, Wanted::Value(Number(14)));
        assert_eq!(app.mk_changes(), []);
        assert_eq!(app.mk_pending_count(), 0);

        rescan(&mut app, &ctx);
        assert_eq!(
            app.mk_changes(),
            [MkChange {
                id: PointerSpeed,
                wanted: Wanted::Value(Number(14))
            }]
        );
        assert_eq!(app.mk_pending_count(), 1);

        fx.set_account(AccountCheck::Different {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        });
        rescan(&mut app, &ctx);
        assert_eq!(app.mk_changes(), []);
        assert_eq!(app.mk_pending_count(), 0);
        assert_eq!(app.mk_pending.len(), 1, "the choice is kept");

        app.discard_mk_changes();
        assert!(app.mk_pending.is_empty());
    }

    #[test]
    fn changes_come_in_table_order() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        app.set_mk_wanted(SwapButtons, Wanted::Value(Flag(true)));
        app.set_mk_wanted(WheelLines, Wanted::Value(Number(5)));
        app.set_mk_wanted(PointerSpeed, Wanted::Value(Number(12)));
        let ids: Vec<SettingId> = app.mk_changes().iter().map(|c| c.id).collect();
        assert_eq!(ids, [PointerSpeed, SwapButtons, WheelLines]);
    }

    #[test]
    fn a_slot_is_named_by_its_title_or_its_raw_key() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        assert_eq!(app.mk_slot_name("pointer_speed"), "Pointer speed");
        assert_eq!(app.mk_slot_name("caret_blink"), "caret_blink");
    }
}
