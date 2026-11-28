//! The "Visual effects" page: the master switch, animations, menus and
//! tooltips, shadows, text and the caret, each row a toggle, a two-choice
//! control or a slider. Choices stay pending until Apply. The page is built
//! on top of the app wiring in `visual_effects_state.rs` and `jobs.rs`.

use super::visual_effects_state::VePreset;
use super::{kit, polish, ActiveTab, WinTuneApp};
use crate::i18n::messages;
use crate::visual_effects::{
    account_problem, EffectId, SettingValue, VeControl, VeProblem, VeSection, FE_CLEARTYPE,
    FE_STANDARD,
};
use eframe::egui::{self, RichText};
use kit::{primary_button, secondary_button, space, text_size, PillKind};

/// What the user did on a row this frame.
enum RowAction {
    Set(SettingValue),
    Restore,
}

/// `query` is trimmed and lower case; empty matches everything.
fn title_matches(query: &str, title: &str) -> bool {
    query.is_empty() || title.to_lowercase().contains(query)
}

fn secondary_line(ui: &mut egui::Ui, text: &str) {
    let p = kit::current_palette(ui.ctx());
    ui.label(
        RichText::new(text)
            .size(text_size::SECONDARY)
            .color(p.text_secondary),
    );
}

fn note_line(ui: &mut egui::Ui, text: &str) {
    let p = kit::current_palette(ui.ctx());
    ui.label(
        RichText::new(text)
            .size(text_size::BODY)
            .color(p.text_secondary),
    );
}

/// `kit::segmented`, greyed out and showing `reason` on hover while one is
/// given (ruling F8: the kit's control has no disabled form). Whether the
/// user picked another option.
fn segmented_with_reason<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: &[(T, &str)],
    reason: Option<&str>,
) -> bool {
    let resp = ui
        .add_enabled_ui(reason.is_none(), |ui| kit::segmented(ui, value, options))
        .inner;
    let resp = match reason {
        Some(reason) => resp.on_disabled_hover_text(reason),
        None => resp,
    };
    resp.changed()
}

impl WinTuneApp {
    pub(super) fn show_visual_effects_tab(&mut self, ctx: &egui::Context) {
        // Before the panel, so the bar spans the whole page width and the
        // progress of a running job shows.
        self.ve_action_bar(ctx);
        let tr = self.tr;
        let name = tr.t("nav.visual_effects");
        let subtitle = tr.t("ve.subtitle");
        let appearance = tr.t("ve.preset.appearance");
        let performance = tr.t("ve.preset.performance");
        let preset_reason = self.ve_preset_disabled_reason();
        let mut preset = None;
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |ui| {
                // Right to left: "For performance" sits at the far right.
                for (which, label) in [
                    (VePreset::Performance, &performance),
                    (VePreset::Appearance, &appearance),
                ] {
                    let resp = ui.add_enabled(preset_reason.is_none(), secondary_button(label));
                    let resp = match &preset_reason {
                        Some(reason) => resp.on_disabled_hover_text(reason),
                        None => resp,
                    };
                    if resp.clicked() {
                        preset = Some(which);
                    }
                }
            });
            secondary_line(ui, &tr.t("ve.note"));
            ui.add_space(space::S);
            self.ve_search_box(ui);
            ui.add_space(space::S);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.ve_sections(ui));
        });
        if let Some(which) = preset {
            self.apply_ve_preset(which);
        }
    }

    /// The read error of `id` in the last scan, as the page words it.
    fn ve_unreadable(&self, id: EffectId) -> Option<String> {
        let error = self.ve_scan.as_ref()?.read_error(id)?;
        let problem = VeProblem::Unreadable {
            id,
            code: error.code,
        };
        Some(messages::ve_problem(self.tr, &problem))
    }

    /// Why the control of `id` is disabled: Windows could not read it, a job
    /// runs or the store is unreadable, or its gate shows off (spec 5.3).
    pub(super) fn ve_control_disabled_reason(&self, id: EffectId) -> Option<String> {
        self.ve_unreadable(id)
            .or_else(|| self.busy_or_blocked(&self.ve_store_error))
            .or_else(|| self.ve_gate_reason(id))
    }

    /// The wrong-account (or unknown-account) text of the last scan.
    fn ve_account_text(&self) -> Option<String> {
        let problem = account_problem(&self.ve_scan.as_ref()?.account)?;
        Some(messages::ve_problem(self.tr, &problem))
    }

    /// Why the two preset buttons are disabled: a running job or an
    /// unreadable store, the wrong account, or no scan yet to build on.
    fn ve_preset_disabled_reason(&self) -> Option<String> {
        self.busy_or_blocked(&self.ve_store_error)
            .or_else(|| self.ve_account_text())
            .or_else(|| self.ve_scan.is_none().then(|| self.tr.t("ve.scanning")))
    }

    /// Why Apply is disabled: an unreadable store, the wrong account, a
    /// running job, or no change to write.
    fn ve_apply_disabled_reason(&self) -> Option<String> {
        let tr = self.tr;
        self.ve_blocked_reason()
            .or_else(|| self.ve_account_text())
            .or_else(|| self.busy_or_blocked(&None))
            .or_else(|| (self.ve_pending_count() == 0).then(|| tr.t("ve.nothing_to_apply")))
    }

    /// The page's bar: how much is not applied, Discard and Apply; or the
    /// job's progress while one runs. The count and Discard both come from
    /// the choices kept (each one a row marked "Not applied"), so a choice
    /// held while the account check fails is still counted and can be
    /// discarded; Apply's own reason says why it cannot be written.
    fn ve_action_bar(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let pending = self.ve_pending.len();
        let status = if pending > 0 {
            tr.t_args("ve.pending", &[&pending])
        } else {
            tr.t("ve.up_to_date")
        };
        let disabled_reason = self.ve_apply_disabled_reason();
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("tweaks.apply");
        let discard_label = tr.t("tweaks.discard");
        let can_discard = pending > 0;

        let (mut apply, mut discard) = (false, false);
        // Right to left: Apply sits at the far right.
        self.page_action_bar(ctx, Some(&status), |ui| {
            let resp = ui.add_enabled(disabled_reason.is_none(), primary_button(p, &apply_label));
            let resp = match &disabled_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            apply = resp.clicked();
            if can_discard {
                discard = ui.add(secondary_button(&discard_label)).clicked();
            }
        });
        if apply {
            self.start_ve_apply(ctx);
        }
        if discard {
            self.discard_ve_changes();
        }
    }

    fn ve_search_box(&mut self, ui: &mut egui::Ui) {
        let id = polish::search_box_id(ActiveTab::VisualEffects).expect("search box");
        let mut search = std::mem::take(&mut self.ve_search);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut search)
                .id(id)
                .hint_text(self.tr.t("ve.search"))
                .desired_width(kit::SEARCH_WIDTH),
        );
        self.ve_search = search;
        self.focus_search_if_requested(&resp);
    }

    /// The six sections, or the loading line, or the wrong-account banner.
    fn ve_sections(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let Some(scan) = &self.ve_scan else {
            kit::card(ui, |ui| note_line(ui, &tr.t("ve.scanning")));
            return;
        };
        if let Some(problem) = account_problem(&scan.account) {
            let text = messages::ve_problem(tr, &problem);
            kit::banner(ui, kit::BannerKind::Error, &text, None);
            return;
        }
        let query = self.ve_search.trim().to_lowercase();
        let mut actions = Vec::new();
        let mut open_mouse_keyboard = false;
        let mut shown = 0;
        for section in VeSection::ALL {
            let ids: Vec<EffectId> = EffectId::ALL
                .into_iter()
                .filter(|id| id.section() == section)
                .filter(|id| title_matches(&query, &messages::ve_title(tr, *id)))
                .collect();
            if ids.is_empty() {
                continue;
            }
            shown += ids.len();
            kit::section(ui, &messages::ve_section(tr, section), |ui| {
                if section == VeSection::Text {
                    secondary_line(ui, &tr.t("ve.text_note"));
                }
                for id in ids {
                    if let Some(action) = self.ve_row(ui, id) {
                        actions.push((id, action));
                    }
                }
                if section == VeSection::Animations {
                    ui.add_space(space::XS);
                    open_mouse_keyboard |= ui.link(tr.t("ve.drag_link")).clicked();
                    ui.add_space(space::XS);
                }
            });
        }
        if shown == 0 {
            note_line(ui, &tr.t("ve.no_match"));
        }

        // After drawing, as on the Startup rows.
        for (id, action) in actions {
            match action {
                RowAction::Set(value) => self.set_ve_wanted(id, value),
                RowAction::Restore => self.request_restore_visual_effects(vec![id.key().into()]),
            }
        }
        if open_mouse_keyboard {
            self.open_page(ActiveTab::MouseKeyboard);
        }
    }

    /// One effect: title and description on the left; then, right to left,
    /// the control, the `…` menu and the "Not applied" pill.
    fn ve_row(&self, ui: &mut egui::Ui, id: EffectId) -> Option<RowAction> {
        let tr = self.tr;
        let title = messages::ve_title(tr, id);
        let description = messages::ve_description(tr, id);
        let shown = self.ve_display(id);
        let reason = self.ve_control_disabled_reason(id);
        let busy = self.busy_or_blocked(&self.ve_store_error);
        let default = id.windows_default();
        // A gated-off row cannot take a choice; Restore original is never
        // gated (the engine writes what it is given, spec 5.3).
        let default_hint = reason
            .clone()
            .or_else(|| (shown == Some(default)).then(|| tr.t("ve.default_shown")));

        let windows_default = tr.t("ve.windows_default");
        let restore = tr.t("tweaks.restore_original");
        let mut items = vec![kit::MenuItem {
            label: &windows_default,
            enabled: default_hint.is_none(),
            disabled_hint: default_hint.as_deref(),
        }];
        if self.ve_original(id).is_some() {
            items.push(kit::MenuItem {
                label: &restore,
                enabled: busy.is_none(),
                disabled_hint: busy.as_deref(),
            });
        }
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&title]);
        // With the master pending on over a master live off, a gated
        // choice equal to its (maybe masked) live value stays pending and
        // counts (review T4-M1): its row shows it too.
        let pending = self.ve_pending.contains_key(&id);
        let not_applied = tr.t("ve.pill.not_applied");

        let mut action = None;
        kit::SettingRow {
            icon: None,
            title: &title,
            description: description.as_deref(),
            hover: None,
        }
        .show(ui, |ui| {
            if let Some(value) = self.ve_control(ui, id, shown, reason.as_deref()) {
                action = Some(RowAction::Set(value));
            }
            let salt = format!("ve_{}", id.key());
            match kit::overflow_menu(ui, &salt, &more, &more_for, &items) {
                Some(0) => action = Some(RowAction::Set(default)),
                Some(_) => action = Some(RowAction::Restore),
                None => {}
            }
            if pending {
                kit::pill(ui, &not_applied, PillKind::Accent);
            }
        });
        action
    }

    /// The control of `id`, showing `shown`; the value the user picked this
    /// frame, if any. A live value outside the page's range is shown as it
    /// is and never written unless the control is moved.
    fn ve_control(
        &self,
        ui: &mut egui::Ui,
        id: EffectId,
        shown: Option<SettingValue>,
        reason: Option<&str>,
    ) -> Option<SettingValue> {
        let tr = self.tr;
        match id.control() {
            VeControl::Toggle => {
                let mut on = shown == Some(SettingValue::Flag(true));
                kit::toggle_enabled(ui, &mut on, reason)
                    .changed()
                    .then_some(SettingValue::Flag(on))
            }
            VeControl::FadeSlide => {
                let (fade, slide) = (tr.t("ve.value.fade"), tr.t("ve.value.slide"));
                let mut value = match shown {
                    Some(SettingValue::Flag(fade)) => Some(fade),
                    _ => None,
                };
                let options = [(Some(true), fade.as_str()), (Some(false), slide.as_str())];
                if segmented_with_reason(ui, &mut value, &options, reason) {
                    value.map(SettingValue::Flag)
                } else {
                    None
                }
            }
            VeControl::SmoothingType => {
                let (standard, cleartype) = (tr.t("ve.value.standard"), tr.t("ve.value.cleartype"));
                let mut value = match shown {
                    Some(SettingValue::Number(n)) => Some(n),
                    _ => None,
                };
                let options = [
                    (Some(FE_STANDARD), standard.as_str()),
                    (Some(FE_CLEARTYPE), cleartype.as_str()),
                ];
                let changed = segmented_with_reason(ui, &mut value, &options, reason);
                // Neither choice is selected: say what Windows has.
                if let Some(v) = shown.filter(|v| !id.in_page_range(v)) {
                    secondary_line(ui, &messages::ve_value(tr, Some(id), &v));
                }
                if changed {
                    value.map(SettingValue::Number)
                } else {
                    None
                }
            }
            VeControl::Slider { min, max, step } => {
                let mut n = match shown {
                    Some(SettingValue::Number(n)) => n,
                    _ => min,
                };
                let label = shown
                    .map(|v| messages::ve_value(tr, Some(id), &v))
                    .unwrap_or_default();
                kit::slider(ui, &mut n, (min, max, step), &label, reason)
                    .changed()
                    .then_some(SettingValue::Number(n))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::{kit, polish, ActiveTab, WinTuneApp};
    use crate::i18n::messages;
    use crate::visual_effects::EffectId::{self, *};
    use crate::visual_effects::SettingValue::*;
    use crate::visual_effects::{AccountCheck, MasterMode, VeProblem, VeSection};
    use egui::{Event, Key, Modifiers};
    use std::collections::BTreeMap;

    /// Size of a toggle.
    const TOGGLE: egui::Vec2 = egui::vec2(40.0, 20.0);

    /// The Visual effects page, its first scan finished.
    fn page_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_ve_scan();
        run_until_ve_scanned(&mut app, ctx);
        app.active_tab = ActiveTab::VisualEffects;
        app
    }

    fn rescan(app: &mut WinTuneApp, ctx: &egui::Context) {
        app.request_ve_scan();
        run_until_ve_scanned(app, ctx);
    }

    /// Two frames, so the layout has settled; the output of the second.
    fn settled(app: &mut WinTuneApp, ctx: &egui::Context) -> egui::FullOutput {
        frame(app, ctx, tall_input(vec![]));
        frame(app, ctx, tall_input(vec![]))
    }

    fn texts(app: &mut WinTuneApp, ctx: &egui::Context) -> Vec<String> {
        drawn_texts(&settled(app, ctx))
    }

    /// Where `text` is drawn, after two frames.
    fn center_of(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) -> egui::Pos2 {
        let out = settled(app, ctx);
        text_center(&out, text)
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)))
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        let pos = center_of(app, ctx, text);
        click_at(app, ctx, pos);
    }

    fn title(app: &WinTuneApp, id: EffectId) -> String {
        messages::ve_title(app.tr, id)
    }

    /// The widget of `size` on the row of `id`.
    fn row_control(
        app: &mut WinTuneApp,
        ctx: &egui::Context,
        id: EffectId,
        size: egui::Vec2,
    ) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let title = title(app, id);
        widget_right_of(app, ctx, &title, size)
            .unwrap_or_else(|| panic!("no widget of {size:?} on row {title:?}"))
    }

    #[test]
    fn the_page_draws_six_sections_in_table_order_with_live_values() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let out = settled(&mut app, &ctx);

        let ys: Vec<f32> = VeSection::ALL
            .iter()
            .map(|s| {
                let text = messages::ve_section(app.tr, *s);
                text_center(&out, &text)
                    .unwrap_or_else(|| panic!("{text:?} not drawn"))
                    .y
            })
            .collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
        let shown = drawn_texts(&out);
        for want in ["1,200", "ClearType", "1 px"] {
            assert!(
                shown.iter().any(|t| t == want),
                "{want:?} not drawn: {shown:?}"
            );
        }
        for key in [
            "ve.subtitle",
            "ve.note",
            "ve.up_to_date",
            "ve.preset.appearance",
            "ve.preset.performance",
            "ve.text_note",
            "ve.drag_link",
        ] {
            assert!(shown.contains(&app.tr.t(key)), "{key} not drawn");
        }
        assert!(!shown.contains(&app.tr.t("tweaks.discard")));
        assert!(!shown.contains(&app.tr.t("ve.pill.not_applied")));
    }

    #[test]
    fn a_toggle_marks_not_applied_and_apply_sets_it() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let toggle = row_control(&mut app, &ctx, CursorShadow, TOGGLE);
        click_at(&mut app, &ctx, toggle);

        assert_eq!(app.ve_pending, BTreeMap::from([(CursorShadow, Flag(true))]));
        let shown = texts(&mut app, &ctx);
        for want in [
            app.tr.t("ve.pill.not_applied"),
            app.tr.t_args("ve.pending", &[&1]),
            app.tr.t("tweaks.discard"),
        ] {
            assert!(shown.contains(&want), "{want:?} not drawn: {shown:?}");
        }
        assert_eq!(app.tr.t_args("ve.pending", &[&1]), "1 changes not applied");

        let apply = app.tr.t("tweaks.apply");
        click_text(&mut app, &ctx, &apply);
        assert!(app.pending_confirm.is_none(), "Apply asked first");
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            fx.fake_effects().value(CursorShadow),
            Flag(true),
            "{}",
            app.last_notice()
        );
        assert!(app.ve_pending.is_empty());
        assert!(app.pending_confirm.is_none(), "a confirmation was shown");
    }

    #[test]
    fn an_out_of_range_live_value_is_shown_and_never_written_unless_moved() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        fx.fake_effects().put(FontSmoothingType, Number(3));
        fx.fake_effects().put(CaretWidth, Number(25));
        rescan(&mut app, &ctx);

        let shown = texts(&mut app, &ctx);
        for want in ["Unknown (3)", "25 px"] {
            assert!(shown.iter().any(|t| t == want), "{want:?}: {shown:?}");
        }
        assert_eq!(app.ve_pending_count(), 0);
        assert!(app.ve_pending.is_empty());

        let windows_default = app.tr.t("ve.windows_default");
        let more = row_control(&mut app, &ctx, FontSmoothingType, kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        click_text(&mut app, &ctx, &windows_default);
        assert_eq!(app.ve_pending.get(&FontSmoothingType), Some(&Number(2)));
        assert_eq!(fx.fake_effects().sets(), [], "nothing written before Apply");

        // Now shown: the item is disabled and says why.
        let more = row_control(&mut app, &ctx, FontSmoothingType, kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        let item = center_of(&mut app, &ctx, &windows_default);
        let hover = hover_texts(&mut app, &ctx, item);
        assert!(hover.contains(&app.tr.t("ve.default_shown")), "{hover:?}");
    }

    #[test]
    fn a_gated_row_is_disabled_with_the_reason() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        fx.fake_effects().put(UiEffects, Flag(false));
        rescan(&mut app, &ctx);
        let master = app.tr.t("ve.gate.master");

        assert_eq!(
            app.ve_control_disabled_reason(MenuAnimation),
            Some(master.clone())
        );
        assert_eq!(
            app.ve_control_disabled_reason(MenuFade),
            Some(master.clone())
        );
        assert_eq!(app.ve_control_disabled_reason(DropShadow), None);

        // The toggle ignores a click and says why on hover.
        let toggle = row_control(&mut app, &ctx, MenuAnimation, TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert!(app.ve_pending.is_empty(), "{:?}", app.ve_pending);
        let hover = hover_texts(&mut app, &ctx, toggle);
        assert!(hover.contains(&master), "{hover:?}");

        // So does the two-choice control (ruling F8): Menu fade's "Slide".
        let out = settled(&mut app, &ctx);
        let slide = text_centers(&out, &app.tr.t("ve.value.slide"))[0];
        click_at(&mut app, &ctx, slide);
        assert!(app.ve_pending.is_empty(), "{:?}", app.ve_pending);
        let hover = hover_texts(&mut app, &ctx, slide);
        assert!(hover.contains(&master), "{hover:?}");

        // "Windows default" is disabled with the gate's reason; "Restore
        // original" stays enabled once there is a snapshot.
        app.ve_store
            .record_first("TEST\\user", "menu_animation", "x", Flag(false));
        let windows_default = app.tr.t("ve.windows_default");
        let more = row_control(&mut app, &ctx, MenuAnimation, kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        let item = center_of(&mut app, &ctx, &windows_default);
        let hover = hover_texts(&mut app, &ctx, item);
        assert!(hover.contains(&master), "{hover:?}");
        click_at(&mut app, &ctx, item);
        assert!(app.ve_pending.is_empty(), "{:?}", app.ve_pending);

        let more = row_control(&mut app, &ctx, MenuAnimation, kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        let text = app.tr.t("tweaks.restore_original");
        click_text(&mut app, &ctx, &text);
        assert!(
            app.pending_confirm.is_some(),
            "Restore original was disabled"
        );
    }

    #[test]
    fn the_preset_buttons_only_fill_pending_values() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let text = app.tr.t("ve.preset.performance");
        click_text(&mut app, &ctx, &text);
        assert_eq!(app.ve_pending_count(), 8, "{:?}", app.ve_pending);
        assert_eq!(fx.fake_effects().sets(), []);

        let text = app.tr.t("tweaks.discard");
        click_text(&mut app, &ctx, &text);
        assert_eq!(app.ve_pending_count(), 0);
    }

    #[test]
    fn under_a_hidden_master_the_performance_preset_shows_every_choice_pending() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.fake_effects().master_mode(MasterMode::ReadsOff);
        fx.fake_effects().put(UiEffects, Flag(false));
        let mut app = page_app(&fx, &ctx);

        let text = app.tr.t("ve.preset.performance");
        click_text(&mut app, &ctx, &text);

        // T4-M1: the gated toggles that read off are counted, and shown.
        let count = app.ve_pending_count();
        assert_eq!(count, 10, "{:?}", app.ve_pending);
        assert_eq!(app.ve_pending.len(), count);
        let out = settled(&mut app, &ctx);
        let pills = text_centers(&out, &app.tr.t("ve.pill.not_applied"));
        assert_eq!(pills.len(), count);
        let status = app.tr.t_args("ve.pending", &[&count]);
        assert!(drawn_texts(&out).contains(&status), "{status:?}");
    }

    #[test]
    fn the_drag_link_opens_mouse_and_keyboard() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let text = app.tr.t("ve.drag_link");
        click_text(&mut app, &ctx, &text);

        assert_eq!(app.active_tab, ActiveTab::MouseKeyboard);
    }

    #[test]
    fn a_setting_windows_cannot_read_is_disabled_with_the_error() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.fake_effects().fail_get(CaretWidth);
        let mut app = page_app(&fx, &ctx);

        assert_eq!(
            app.ve_control_disabled_reason(CaretWidth),
            Some(messages::ve_problem(
                app.tr,
                &VeProblem::Unreadable {
                    id: CaretWidth,
                    code: 5
                }
            ))
        );
        assert_eq!(app.ve_control_disabled_reason(FocusBorderWidth), None);
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&title(&app, CaretWidth)), "{shown:?}");
    }

    #[test]
    fn the_wrong_account_shows_a_banner_instead_of_the_sections() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        fx.set_account(AccountCheck::Different {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        });
        rescan(&mut app, &ctx);
        let banner = messages::ve_problem(
            app.tr,
            &VeProblem::WrongAccount {
                process: "PC\\Admin".into(),
                session: "PC\\Ana".into(),
            },
        );
        assert!(banner.contains("PC\\Admin") && banner.contains("PC\\Ana"));

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&banner), "{shown:?}");
        for s in VeSection::ALL {
            let section = messages::ve_section(app.tr, s);
            assert!(!shown.contains(&section), "{section:?} drawn");
        }

        // The presets are disabled too.
        let text = app.tr.t("ve.preset.performance");
        click_text(&mut app, &ctx, &text);
        assert!(app.ve_pending.is_empty(), "{:?}", app.ve_pending);

        let apply = app.tr.t("tweaks.apply");
        let apply = center_of(&mut app, &ctx, &apply);
        click_at(&mut app, &ctx, apply);
        assert!(!app.is_busy());
        let hover = hover_texts(&mut app, &ctx, apply);
        assert!(hover.contains(&banner), "{hover:?}");
    }

    /// Final review: the bar's count and Discard come from one source, so
    /// choices kept under the wrong account never read "All changes
    /// applied" beside Discard.
    #[test]
    fn choices_kept_under_the_wrong_account_are_counted_beside_discard() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        app.set_ve_wanted(CursorShadow, Flag(true));
        app.set_ve_wanted(CaretWidth, Number(2));
        fx.set_account(AccountCheck::Different {
            process: "PC\\Admin".into(),
            session: "PC\\Ana".into(),
        });
        rescan(&mut app, &ctx);
        assert_eq!(app.ve_pending.len(), 2, "the choices are kept");

        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t_args("ve.pending", &[&2])),
            "{shown:?}"
        );
        assert!(shown.contains(&app.tr.t("tweaks.discard")), "{shown:?}");
        assert!(!shown.contains(&app.tr.t("ve.up_to_date")), "{shown:?}");

        let text = app.tr.t("tweaks.discard");
        click_text(&mut app, &ctx, &text);
        assert!(app.ve_pending.is_empty());
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("ve.up_to_date")), "{shown:?}");
        assert!(!shown.contains(&app.tr.t("tweaks.discard")), "{shown:?}");
    }

    #[test]
    fn ctrl_f_focuses_the_visual_effects_search() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        frame(&mut app, &ctx, tall_input(vec![]));
        let mut input = tall_input(vec![Event::Key {
            key: Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        input.modifiers = Modifiers::COMMAND;

        frame(&mut app, &ctx, input);
        frame(&mut app, &ctx, tall_input(vec![]));

        let id = polish::search_box_id(ActiveTab::VisualEffects);
        assert_eq!(id, Some(egui::Id::new("search_visual_effects")));
        assert!(ctx.memory(|m| m.has_focus(id.unwrap())));
    }

    #[test]
    fn search_filters_rows_by_title() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        // Typed into the box, upper case.
        let id = polish::search_box_id(ActiveTab::VisualEffects).unwrap();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, &ctx, tall_input(vec![]));
        frame(
            &mut app,
            &ctx,
            tall_input(vec![Event::Text("SHADOW".into())]),
        );

        let shown = texts(&mut app, &ctx);
        for id in [CursorShadow, DropShadow] {
            assert!(shown.contains(&title(&app, id)), "{shown:?}");
        }
        for hidden in [
            title(&app, MenuAnimation),
            messages::ve_section(app.tr, VeSection::Animations),
            messages::ve_section(app.tr, VeSection::Text),
        ] {
            assert!(!shown.contains(&hidden), "{hidden:?} drawn");
        }

        app.ve_search = "nothing like it".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("ve.no_match")), "{shown:?}");
    }

    #[test]
    fn the_page_shows_the_loading_line_until_the_first_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.ve_scan_needed = false;
        app.active_tab = ActiveTab::VisualEffects;
        let scanning = app.tr.t("ve.scanning");
        let shadow = title(&app, DropShadow);

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&scanning), "{shown:?}");
        assert!(!shown.contains(&shadow));

        app.request_ve_scan();
        run_until_ve_scanned(&mut app, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&scanning), "{shown:?}");
        assert!(shown.contains(&shadow), "{shown:?}");
    }

    #[test]
    fn opening_the_page_requests_a_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.active_tab = ActiveTab::Home;
        assert!(!app.ve_scan_needed);

        app.open_page(ActiveTab::VisualEffects);

        assert_eq!(app.active_tab, ActiveTab::VisualEffects);
        assert!(app.ve_scan_needed);
    }
}
