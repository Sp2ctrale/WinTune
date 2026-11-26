//! Startup apps: the background scan that lists them, and the on/off
//! choices picked but not applied yet. Apply itself is
//! `jobs::start_startup_apply`, one job with the pending choices.

use super::{kit, WinTuneApp};
use crate::backups::Slot;
use crate::i18n::messages;
use crate::startup::{
    load_startup_store, scan, ScanOptions, StartupBackupStore, StartupChange, StartupItem,
    StartupScan, StartupState,
};
use crate::tasks::{BackgroundJob, JobPoll};
use eframe::egui;

pub(super) type ScanJob = BackgroundJob<StartupScan>;

/// The change that makes `item` match `wanted_on`, or `None` when the item
/// is locked (the engine would refuse the whole Apply for it) or already is
/// in that state.
pub(super) fn startup_change(item: &StartupItem, wanted_on: bool) -> Option<StartupChange> {
    if item.locked.is_some() || wanted_on == (item.state == StartupState::On) {
        return None;
    }
    Some(StartupChange::from_item(item, wanted_on))
}

impl WinTuneApp {
    pub(super) fn reload_startup_store(&mut self) {
        match load_startup_store(&self.env.startup_store_path) {
            Ok(store) => {
                self.startup_store = store;
                self.startup_store_error = None;
            }
            Err(e) => {
                self.startup_store = StartupBackupStore::default();
                let text = messages::load_error(self.tr, &e);
                self.notify(kit::ToastKind::Error, text);
                self.startup_store_error = Some(e);
            }
        }
    }

    /// Schedules a scan; it starts as soon as no registry job is running.
    pub(super) fn request_startup_scan(&mut self) {
        self.startup_scan_needed = true;
    }

    /// Called every frame: collects a finished scan and starts a pending one.
    /// The last result stays shown while the next scan runs.
    pub(super) fn pump_startup_scan(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.startup_scan_job.as_mut() {
            match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(scan) => {
                    // A choice the registry already matches (applied by a
                    // partial Apply, or changed elsewhere), or for an item the
                    // scan no longer lists, is no longer pending.
                    self.startup_pending.retain(|id, wanted_on| {
                        scan.find(id)
                            .is_some_and(|item| startup_change(item, *wanted_on).is_some())
                    });
                    self.startup_scan = Some(scan);
                }
                JobPoll::Panicked(msg) => {
                    let text = self.tr.t_args("startup.scan_failed", &[&msg]);
                    self.notify(kit::ToastKind::Error, text);
                }
            }
            self.startup_scan_job = None;
        }

        // Reading while a job writes would show transient states.
        if !self.startup_scan_needed || self.is_busy() {
            return;
        }
        self.startup_scan_needed = false;

        let env = self.env.clone();
        // Empty when the store is unreadable: rows show no snapshot, and
        // Apply stays blocked by `startup_blocked_reason`.
        let store = self.startup_store.clone();
        let repaint_ctx = ctx.clone();
        self.startup_scan_job = Some(BackgroundJob::spawn(
            "Reading the startup apps",
            move || repaint_ctx.request_repaint(),
            move |_| {
                let opts = ScanOptions {
                    user: &env.user_label,
                    product_name: &env.product_name,
                    switch_works: env.startup_switch_works,
                };
                scan(&(env.startup_target)(), &store, &opts)
            },
        ));
    }

    /// The changes Apply writes: one per pending id the last scan still
    /// lists and whose wish differs from the current state.
    pub(super) fn startup_changes(&self) -> Vec<StartupChange> {
        let Some(scan) = &self.startup_scan else {
            return Vec::new();
        };
        self.startup_pending
            .iter()
            .filter_map(|(id, wanted_on)| startup_change(scan.find(id)?, *wanted_on))
            .collect()
    }

    pub(super) fn startup_pending_count(&self) -> usize {
        self.startup_changes().len()
    }

    /// "Discard": forgets the pending choices.
    pub(super) fn discard_startup_changes(&mut self) {
        self.startup_pending.clear();
    }

    /// How the UI names a switch slot (as in History's saved originals): the
    /// scanned item's name, else the value name inside the slot id.
    pub(super) fn startup_slot_name(&self, slot_id: &str) -> String {
        if let Some(item) = self.startup_scan.as_ref().and_then(|s| s.by_slot(slot_id)) {
            return item.display_name.clone();
        }
        match Slot::from_id(slot_id) {
            Slot::Value { name, .. } => name,
            Slot::Key(path) => path,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::startup::{Approved, Origin, Source, StartupLock, StartupState};
    use egui::RawInput;

    fn item(state: StartupState, locked: Option<StartupLock>) -> StartupItem {
        StartupItem {
            id: "run:app".into(),
            approved: Approved::Run,
            name: "App".into(),
            display_name: "App".into(),
            program: r"C:\Apps\app.exe".into(),
            origins: vec![Origin {
                source: Source::UserRun,
                detail: r"C:\Apps\app.exe".into(),
            }],
            program_missing: false,
            state,
            locked,
            has_snapshot: false,
        }
    }

    #[test]
    fn the_startup_scan_runs_in_background_and_waits_for_jobs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        fx.seed_run(false, "App", r#""C:\Apps\app.exe""#);
        assert!(app.startup_scan.is_none());
        let release = hold_job(&mut app);

        app.request_startup_scan();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.startup_scan_job.is_none(), "scan started during a job");
        assert!(app.startup_scan_needed);

        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        run_until_startup_scanned(&mut app, &ctx);
        assert_eq!(app.startup_scan.as_ref().map(|s| s.items.len()), Some(1));
    }

    #[test]
    fn startup_change_is_none_for_locked_or_unchanged() {
        let on = item(StartupState::On, None);
        assert_eq!(startup_change(&on, true), None);
        let off_change = startup_change(&on, false).expect("turn off");
        assert!(!off_change.on);
        assert_eq!(off_change.name, "App");

        for state in [StartupState::Off, StartupState::OffElsewhere] {
            let off = item(state, None);
            assert_eq!(startup_change(&off, false), None, "{state:?}");
            assert!(
                startup_change(&off, true).is_some_and(|c| c.on),
                "{state:?}"
            );
        }

        for lock in [
            StartupLock::UnknownShape,
            StartupLock::OffForEveryone,
            StartupLock::AllUsersOnly,
            StartupLock::Unreadable("x".into()),
        ] {
            let locked = item(StartupState::On, Some(lock.clone()));
            assert_eq!(startup_change(&locked, false), None, "{lock:?}");
        }
    }

    #[test]
    fn a_scan_drops_the_choices_it_matches_or_no_longer_lists() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        fx.seed_run(false, "App", r#""C:\Apps\app.exe""#);
        app.startup_pending.insert("run:app".into(), true);
        app.startup_pending.insert("run:gone".into(), false);
        app.request_startup_scan();
        run_until_startup_scanned(&mut app, &ctx);
        // Already on: nothing to do. Not listed: pruned, so Discard goes too.
        assert!(!app.startup_pending.contains_key("run:app"));
        assert!(!app.startup_pending.contains_key("run:gone"));
        assert!(app.startup_pending.is_empty());
        assert_eq!(app.startup_pending_count(), 0);

        app.startup_pending.insert("run:app".into(), false);
        assert_eq!(app.startup_pending_count(), 1);
        assert_eq!(app.startup_changes().len(), 1);
        app.discard_startup_changes();
        assert!(app.startup_pending.is_empty());
    }

    #[test]
    fn a_panicking_startup_scan_is_a_toast() {
        use crate::tasks::BackgroundJob;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.startup_scan_job = Some(BackgroundJob::spawn(
            "Reading the startup apps",
            || {},
            |_| -> StartupScan { panic!("boom") },
        ));
        run_until_startup_scanned(&mut app, &ctx);
        assert!(
            app.last_notice()
                .starts_with("Could not read the startup apps: "),
            "{}",
            app.last_notice()
        );
        assert!(app.startup_scan.is_none());
    }

    #[test]
    fn a_slot_is_named_by_its_scanned_item_or_its_value_name() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        fx.seed_run(false, "Pretty", r#""C:\Apps\app.exe""#);
        let slot = Approved::Run.slot("Pretty").id();
        // Before any scan: the value name.
        assert_eq!(app.startup_slot_name(&slot), "Pretty");
        app.request_startup_scan();
        run_until_startup_scanned(&mut app, &ctx);
        assert_eq!(app.startup_slot_name(&slot.to_uppercase()), "Pretty");
        let other = Approved::Run.slot("Elsewhere").id();
        assert_eq!(app.startup_slot_name(&other), "Elsewhere");
    }
}
