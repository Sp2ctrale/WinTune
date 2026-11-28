//! Visual effects: the background scan that reads every effect, the choices
//! picked but not applied yet, the gates that disable rows and the two
//! presets. Apply itself is `jobs::start_ve_apply`, one job with the pending
//! choices.

use super::{kit, WinTuneApp};
use crate::i18n::messages;
use crate::tasks::{BackgroundJob, JobPoll};
use crate::visual_effects::{
    load_ve_store, scan_visual_effects, EffectId, Gate, SettingValue, VeBackupStore, VeChange,
    VeControl, VeScan, VeSection, FE_CLEARTYPE, MASTER_COMPANIONS,
};
use eframe::egui;

pub(super) type VeScanJob = BackgroundJob<VeScan>;

/// The page's two header buttons (spec 5.7). They set pending values only.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VePreset {
    /// Every effect of sections 1 to 4 on, fades as Fade.
    Appearance,
    /// The master on, every other toggle of sections 1 to 4 off; the fade
    /// choices untouched.
    Performance,
}

/// The rows the presets change: sections 1 to 4, never Text or Caret and
/// focus.
fn preset_rows() -> impl Iterator<Item = EffectId> {
    EffectId::ALL
        .into_iter()
        .filter(|id| !matches!(id.section(), VeSection::Text | VeSection::CaretFocus))
}

/// The hover text of a row whose gate `g` (a `Gate::On` effect) is off.
fn gate_key(g: EffectId) -> Option<&'static str> {
    match g {
        EffectId::MenuAnimation => Some("ve.gate.menu"),
        EffectId::TooltipAnimation => Some("ve.gate.tooltip"),
        EffectId::FontSmoothing => Some("ve.gate.smoothing"),
        _ => None,
    }
}

impl WinTuneApp {
    pub(super) fn reload_ve_store(&mut self) {
        match load_ve_store(&self.env.ve_store_path) {
            Ok(store) => {
                self.ve_store = store;
                self.ve_store_error = None;
            }
            Err(e) => {
                self.ve_store = VeBackupStore::default();
                let text = messages::load_error(self.tr, &e);
                self.notify(kit::ToastKind::Error, text);
                self.ve_store_error = Some(e);
            }
        }
    }

    /// Schedules a scan; it starts as soon as no job is running.
    pub(super) fn request_ve_scan(&mut self) {
        self.ve_scan_needed = true;
    }

    /// Called every frame: collects a finished scan and starts a pending one.
    /// The last result stays shown while the next scan runs.
    pub(super) fn pump_ve_scan(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.ve_scan_job.as_mut() {
            match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(scan) => {
                    self.ve_scan = Some(scan);
                    // A choice the live value already matches (applied by a
                    // partial Apply, or changed in Performance Options), or
                    // for an effect Windows could not read, is no longer
                    // pending; nor is one whose row the new values disable
                    // (spec 5.3, review T4-I1).
                    let stale: Vec<EffectId> = self
                        .ve_pending
                        .iter()
                        .filter(|&(&id, wanted)| !self.ve_differs_from_live(id, wanted))
                        .map(|(&id, _)| id)
                        .collect();
                    for id in stale {
                        self.ve_pending.remove(&id);
                    }
                    self.drop_gated_choices();
                }
                JobPoll::Panicked(msg) => {
                    let text = self.tr.t_args("ve.scan_failed", &[&msg]);
                    self.notify(kit::ToastKind::Error, text);
                }
            }
            self.ve_scan_job = None;
        }

        // Reading while a job writes would show transient values.
        if !self.ve_scan_needed || self.is_busy() {
            return;
        }
        self.ve_scan_needed = false;

        let env = self.env.clone();
        let repaint_ctx = ctx.clone();
        self.ve_scan_job = Some(BackgroundJob::spawn(
            "Reading the visual effects",
            move || repaint_ctx.request_repaint(),
            move |_| scan_visual_effects(&*env.effects, &*env.input_account),
        ));
    }

    /// The value this user had before WinTune first changed `id`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn ve_original(&self, id: EffectId) -> Option<SettingValue> {
        self.ve_store
            .original(&self.env.user_label, id.key())
            .copied()
    }

    /// The value the page shows: the pending one, else the live one. `None`
    /// before the first scan and for an effect Windows could not read.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn ve_display(&self, id: EffectId) -> Option<SettingValue> {
        let live = self.ve_scan.as_ref()?.live(id)?;
        Some(self.ve_pending.get(&id).copied().unwrap_or(live))
    }

    /// Why the row of `id` is disabled (its hover text), or `None` while it
    /// is enabled. A gate's state is the value the page shows for it (spec
    /// 5.3); a row under a disabled gate gives that gate's reason first.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn ve_gate_reason(&self, id: EffectId) -> Option<String> {
        let off = |g: EffectId| self.ve_display(g) == Some(SettingValue::Flag(false));
        match id.gate()? {
            Gate::Master => off(EffectId::UiEffects).then(|| self.tr.t("ve.gate.master")),
            Gate::On(g) => self
                .ve_gate_reason(g)
                .or_else(|| gate_key(g).filter(|_| off(g)).map(|key| self.tr.t(key))),
            Gate::ClearType => {
                let not_cleartype = matches!(
                    self.ve_display(EffectId::FontSmoothingType),
                    Some(SettingValue::Number(n)) if n != FE_CLEARTYPE
                );
                if off(EffectId::FontSmoothing) {
                    Some(self.tr.t("ve.gate.smoothing"))
                } else if not_cleartype {
                    Some(self.tr.t("ve.gate.cleartype"))
                } else {
                    None
                }
            }
        }
    }

    /// Whether `wanted` is a change for `id`: the last scan read it and the
    /// value differs. Under a master live off that the batch turns on, a
    /// companion's live read may be masked (case 2 of spec 5.3), so it
    /// counts as a change even when it reads the same (review T4-M1). Not
    /// a change before the first scan.
    fn ve_differs_from_live(&self, id: EffectId, wanted: &SettingValue) -> bool {
        let Some(scan) = self.ve_scan.as_ref() else {
            return false;
        };
        let Some(live) = scan.live(id) else {
            return false;
        };
        let master = EffectId::UiEffects;
        let masked = MASTER_COMPANIONS.contains(&id)
            && scan.live(master) == Some(SettingValue::Flag(false))
            && self.ve_pending.get(&master) == Some(&SettingValue::Flag(true));
        masked || live != *wanted
    }

    /// Drops every choice whose row is disabled, so a disabled row never
    /// shows "Not applied" (spec 5.3). Dropping one can disable another
    /// row: until nothing changes.
    fn drop_gated_choices(&mut self) {
        loop {
            let disabled: Vec<EffectId> = self
                .ve_pending
                .keys()
                .copied()
                .filter(|&other| self.ve_gate_reason(other).is_some())
                .collect();
            if disabled.is_empty() {
                break;
            }
            for other in disabled {
                self.ve_pending.remove(&other);
            }
        }
    }

    /// Records a choice; one that is no change (`ve_differs_from_live`) is
    /// dropped. Then every choice whose row is now disabled is dropped too.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn set_ve_wanted(&mut self, id: EffectId, v: SettingValue) {
        self.ve_pending.insert(id, v);
        if !self.ve_differs_from_live(id, &v) {
            self.ve_pending.remove(&id);
        }
        self.drop_gated_choices();
    }

    /// "For appearance" / "For performance": pending values only, through
    /// `set_ve_wanted`, the master first so its rows are enabled.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn apply_ve_preset(&mut self, preset: VePreset) {
        let on = SettingValue::Flag(true);
        self.set_ve_wanted(EffectId::UiEffects, on);
        for id in preset_rows().filter(|&id| id != EffectId::UiEffects) {
            match preset {
                VePreset::Appearance => self.set_ve_wanted(id, on),
                VePreset::Performance if id.control() == VeControl::Toggle => {
                    self.set_ve_wanted(id, SettingValue::Flag(false))
                }
                VePreset::Performance => {}
            }
        }
    }

    /// The changes Apply writes, in table order: one per pending effect the
    /// last scan read and whose live value differs (`ve_differs_from_live`).
    /// Empty before the first scan and while WinTune runs as another account
    /// than the session's.
    pub(super) fn ve_changes(&self) -> Vec<VeChange> {
        if !self.ve_scan.as_ref().is_some_and(|s| s.account_ok()) {
            return Vec::new();
        }
        self.ve_pending
            .iter()
            .filter(|&(&id, wanted)| self.ve_differs_from_live(id, wanted))
            .map(|(&id, &value)| VeChange { id, value })
            .collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn ve_pending_count(&self) -> usize {
        self.ve_changes().len()
    }

    /// "Discard": forgets the pending choices.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn discard_ve_changes(&mut self) {
        self.ve_pending.clear();
    }

    /// How the UI names a saved effect (as in History's saved originals):
    /// its title, else the raw key of an effect this version does not know.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn ve_slot_name(&self, key: &str) -> String {
        match EffectId::from_key(key) {
            Some(id) => messages::ve_title(self.tr, id),
            None => key.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::visual_effects::EffectId::*;
    use crate::visual_effects::SettingValue::*;
    use crate::visual_effects::{AccountCheck, EffectId, VeChange, VeSection};
    use egui::RawInput;
    use std::collections::BTreeMap;

    fn scanned(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_ve_scan();
        run_until_ve_scanned(&mut app, ctx);
        app
    }

    fn rescan(app: &mut WinTuneApp, ctx: &egui::Context) {
        app.request_ve_scan();
        run_until_ve_scanned(app, ctx);
    }

    fn off_all(ids: &[EffectId]) -> BTreeMap<EffectId, SettingValue> {
        ids.iter().map(|&id| (id, Flag(false))).collect()
    }

    const PERFORMANCE_OFF: [EffectId; 8] = [
        WindowAnimation,
        ClientAreaAnimation,
        ComboboxAnimation,
        ListboxSmoothScroll,
        MenuAnimation,
        SelectionFade,
        TooltipAnimation,
        DropShadow,
    ];

    fn no_text_or_caret(pending: &BTreeMap<EffectId, SettingValue>) {
        for id in pending.keys() {
            assert!(
                !matches!(id.section(), VeSection::Text | VeSection::CaretFocus),
                "{} in {pending:?}",
                id.key()
            );
        }
    }

    #[test]
    fn the_visual_effects_scan_runs_in_background_and_waits_for_jobs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        assert!(app.ve_scan.is_none());
        let release = hold_job(&mut app);

        app.request_ve_scan();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.ve_scan_job.is_none(), "scan started during a job");
        assert!(app.ve_scan_needed);
        assert_eq!(fx.fake_effects().calls(), [], "read during a job");

        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        run_until_ve_scanned(&mut app, &ctx);
        let scan = app.ve_scan.as_ref().expect("scanned");
        assert_eq!(scan.values.len(), 18);
        assert!(scan.account_ok());
        assert_eq!(fx.fake_effects().sets(), [], "a scan never writes");
    }

    #[test]
    fn a_panicking_visual_effects_scan_is_a_toast() {
        use crate::tasks::BackgroundJob;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.ve_scan_job = Some(BackgroundJob::spawn(
            "Reading the visual effects",
            || {},
            |_| -> crate::visual_effects::VeScan { panic!("boom") },
        ));
        run_until_ve_scanned(&mut app, &ctx);
        assert!(
            app.last_notice()
                .starts_with("Could not read the visual effects: "),
            "{}",
            app.last_notice()
        );
        assert!(app.ve_scan.is_none());
    }

    #[test]
    fn a_scan_drops_choices_the_live_value_matches_or_cannot_read() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.ve_pending.insert(CaretWidth, Number(2));
        app.ve_pending.insert(CleartypeContrast, Number(1600));
        app.ve_pending.insert(DropShadow, Flag(false));
        // Settings made the caret change itself; the contrast read fails.
        fx.fake_effects().put(CaretWidth, Number(2));
        fx.fake_effects().fail_get(CleartypeContrast);

        rescan(&mut app, &ctx);

        assert_eq!(app.ve_pending, BTreeMap::from([(DropShadow, Flag(false))]));
    }

    #[test]
    fn a_scan_drops_choices_the_master_now_disables() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        app.set_ve_wanted(CursorShadow, Flag(true));
        assert_eq!(app.ve_pending_count(), 1);
        // Performance Options turns the master off while WinTune is open.
        fx.fake_effects().put(UiEffects, Flag(false));

        rescan(&mut app, &ctx);

        assert!(app.ve_pending.is_empty(), "{:?}", app.ve_pending);
        assert_eq!(app.ve_pending_count(), 0);
    }

    #[test]
    fn a_scan_drops_a_menu_effect_choice_when_menu_animation_reads_off() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        app.set_ve_wanted(MenuFade, Flag(false));
        app.set_ve_wanted(CaretWidth, Number(2));
        fx.fake_effects().put(MenuAnimation, Flag(false));

        rescan(&mut app, &ctx);

        assert_eq!(app.ve_pending, BTreeMap::from([(CaretWidth, Number(2))]));
    }

    #[test]
    fn for_performance_under_a_hidden_master_queues_every_gated_toggle_off() {
        use crate::visual_effects::MasterMode;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        // Case 2: with the master off, every companion reads off whatever
        // its stored value, so "off" may not be the real value.
        fx.fake_effects().master_mode(MasterMode::ReadsOff);
        fx.fake_effects().put(UiEffects, Flag(false));
        let mut app = scanned(&fx, &ctx);
        assert_eq!(app.ve_display(MenuAnimation), Some(Flag(false)), "masked");

        app.apply_ve_preset(VePreset::Performance);

        let mut want = off_all(&PERFORMANCE_OFF);
        want.insert(CursorShadow, Flag(false));
        want.insert(UiEffects, Flag(true));
        assert_eq!(app.ve_pending, want);
        assert_eq!(app.ve_pending_count(), 10, "Apply writes them all");
        no_text_or_caret(&app.ve_pending);

        // A rescan reads the same masked values: the choices stay.
        rescan(&mut app, &ctx);
        assert_eq!(app.ve_pending, want);
    }

    #[test]
    fn the_display_is_the_pending_value_else_the_live_one() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        assert_eq!(app.ve_display(CaretWidth), None, "before any scan");
        fx.fake_effects().fail_get(FocusBorderWidth);
        rescan(&mut app, &ctx);

        assert_eq!(app.ve_display(CaretWidth), Some(Number(1)));
        app.set_ve_wanted(CaretWidth, Number(3));
        assert_eq!(app.ve_display(CaretWidth), Some(Number(3)));
        // Back to the live value: nothing is pending.
        app.set_ve_wanted(CaretWidth, Number(1));
        assert!(app.ve_pending.is_empty());
        assert_eq!(app.ve_display(FocusBorderWidth), None, "unread");

        assert_eq!(app.ve_original(CaretWidth), None);
        app.ve_store
            .record_first("OTHER\\user", "caret_width", "x", Number(7));
        assert_eq!(app.ve_original(CaretWidth), None, "another user's");
        app.ve_store
            .record_first("TEST\\user", "caret_width", "x", Number(4));
        assert_eq!(app.ve_original(CaretWidth), Some(Number(4)));
    }

    #[test]
    fn turning_a_gate_off_drops_the_choices_it_disables() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        let tr = app.tr;
        app.ve_pending.insert(MenuFade, Flag(false));
        app.ve_pending.insert(CursorShadow, Flag(true));
        app.ve_pending.insert(FontSmoothingType, Number(1));

        app.set_ve_wanted(MenuAnimation, Flag(false));
        assert!(!app.ve_pending.contains_key(&MenuFade), "menu fade kept");
        assert_eq!(app.ve_pending.get(&CursorShadow), Some(&Flag(true)));
        assert_eq!(app.ve_gate_reason(MenuFade), Some(tr.t("ve.gate.menu")));

        // Menu animation back on (its live value), menu fade as Slide again.
        app.set_ve_wanted(MenuAnimation, Flag(true));
        assert!(!app.ve_pending.contains_key(&MenuAnimation));
        app.set_ve_wanted(MenuFade, Flag(false));
        assert_eq!(app.ve_pending.get(&MenuFade), Some(&Flag(false)));

        app.set_ve_wanted(UiEffects, Flag(false));
        assert!(!app.ve_pending.contains_key(&CursorShadow));
        assert!(!app.ve_pending.contains_key(&MenuFade));
        assert_eq!(
            app.ve_gate_reason(MenuFade),
            Some(tr.t("ve.gate.master")),
            "the master's reason comes first"
        );
        assert_eq!(
            app.ve_gate_reason(TooltipFade),
            Some(tr.t("ve.gate.master"))
        );
        assert_eq!(app.ve_gate_reason(DropShadow), None, "not gated");
        assert_eq!(app.ve_pending.get(&FontSmoothingType), Some(&Number(1)));

        app.set_ve_wanted(FontSmoothing, Flag(false));
        assert!(!app.ve_pending.contains_key(&FontSmoothingType));
        assert_eq!(
            app.ve_gate_reason(CleartypeContrast),
            Some(tr.t("ve.gate.smoothing"))
        );
        assert_eq!(
            app.ve_gate_reason(FontSmoothingType),
            Some(tr.t("ve.gate.smoothing"))
        );
    }

    #[test]
    fn turning_the_master_on_in_the_batch_enables_its_rows_at_once() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        let master = app.tr.t("ve.gate.master");
        fx.fake_effects().put(UiEffects, Flag(false));
        rescan(&mut app, &ctx);
        assert_eq!(app.ve_gate_reason(CursorShadow), Some(master.clone()));
        app.set_ve_wanted(CursorShadow, Flag(true));
        assert!(app.ve_pending.is_empty(), "a disabled row took a choice");

        app.set_ve_wanted(UiEffects, Flag(true));
        assert_eq!(app.ve_gate_reason(CursorShadow), None);
        app.set_ve_wanted(CursorShadow, Flag(true));
        assert_eq!(
            app.ve_pending,
            BTreeMap::from([(UiEffects, Flag(true)), (CursorShadow, Flag(true))])
        );
    }

    #[test]
    fn contrast_needs_smoothing_on_and_type_cleartype() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        assert_eq!(app.ve_gate_reason(CleartypeContrast), None);

        app.set_ve_wanted(CleartypeContrast, Number(1600));
        assert_eq!(app.ve_pending.get(&CleartypeContrast), Some(&Number(1600)));
        app.set_ve_wanted(FontSmoothingType, Number(1));

        assert!(!app.ve_pending.contains_key(&CleartypeContrast));
        assert_eq!(
            app.ve_gate_reason(CleartypeContrast),
            Some(app.tr.t("ve.gate.cleartype"))
        );
    }

    #[test]
    fn for_performance_turns_every_effect_off_and_the_master_on() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);
        app.ve_pending.insert(MenuFade, Flag(false));

        app.apply_ve_preset(VePreset::Performance);

        assert_eq!(app.ve_pending, off_all(&PERFORMANCE_OFF));
        no_text_or_caret(&app.ve_pending);

        app.discard_ve_changes();
        fx.fake_effects().put(UiEffects, Flag(false));
        rescan(&mut app, &ctx);
        app.apply_ve_preset(VePreset::Performance);

        // T4-M1: under a master live off, a gated row's live read may be
        // masked, so its "off" is queued even though it reads off already.
        let mut want = off_all(&PERFORMANCE_OFF);
        want.insert(CursorShadow, Flag(false));
        want.insert(UiEffects, Flag(true));
        assert_eq!(app.ve_pending, want);
        no_text_or_caret(&app.ve_pending);
    }

    #[test]
    fn for_appearance_turns_everything_on_with_fades() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned(&fx, &ctx);

        app.apply_ve_preset(VePreset::Appearance);
        assert_eq!(app.ve_pending, BTreeMap::from([(CursorShadow, Flag(true))]));

        app.discard_ve_changes();
        fx.fake_effects().put(MenuFade, Flag(false));
        fx.fake_effects().put(FontSmoothing, Flag(false));
        rescan(&mut app, &ctx);
        app.apply_ve_preset(VePreset::Appearance);
        assert_eq!(
            app.ve_pending,
            BTreeMap::from([(MenuFade, Flag(true)), (CursorShadow, Flag(true))])
        );
        no_text_or_caret(&app.ve_pending);
    }

    #[test]
    fn changes_are_empty_before_the_first_scan_and_for_the_wrong_account() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.ve_pending.insert(CaretWidth, Number(2));
        assert_eq!(app.ve_changes(), []);
        assert_eq!(app.ve_pending_count(), 0);

        rescan(&mut app, &ctx);
        app.ve_pending.insert(DropShadow, Flag(false));
        assert_eq!(
            app.ve_changes(),
            [
                VeChange {
                    id: DropShadow,
                    value: Flag(false)
                },
                VeChange {
                    id: CaretWidth,
                    value: Number(2)
                }
            ],
            "in table order"
        );
        assert_eq!(app.ve_pending_count(), 2);

        fx.set_account(AccountCheck::Different {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        });
        rescan(&mut app, &ctx);
        assert_eq!(app.ve_changes(), []);
        assert_eq!(app.ve_pending_count(), 0);
        assert_eq!(app.ve_pending.len(), 2, "the choices are kept");

        app.discard_ve_changes();
        assert!(app.ve_pending.is_empty());
    }

    #[test]
    fn a_slot_is_named_by_its_title_or_its_raw_key() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        assert_eq!(app.ve_slot_name("caret_width"), "Text cursor width");
        assert_eq!(app.ve_slot_name("hot_tracking"), "hot_tracking");
    }
}
