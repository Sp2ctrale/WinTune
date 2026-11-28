//! The "Mouse & keyboard" page: pointer, button, wheel, keyboard and window
//! timing settings in five sections, each a toggle or a slider. Choices stay
//! pending until Apply. The page is built on top of the app wiring in
//! `mouse_keyboard_state.rs` and `jobs.rs`.

use super::{kit, polish, ActiveTab, WinTuneApp};
use crate::i18n::messages;
use crate::mouse_keyboard::{
    is_on, trails_start_length, Control, MkProblem, Section, SettingId, SettingValue, SliderSpec,
    Wanted, WHEEL_PAGESCROLL,
};
use eframe::egui::{self, RichText};
use kit::{primary_button, secondary_button, space, text_size, PillKind};

/// Lines per notch when "Lines" is picked and the live value is not a
/// number of lines (one screen, or 0): the Windows default.
const FALLBACK_LINES: u32 = 3;

/// The double-click test target: lit after two clicks close enough.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct ClickTest {
    last_click: Option<f64>,
    lit: bool,
}

/// One click on the target at `now` (seconds). A click within `limit_ms` of
/// the previous one lights it; any other click starts a new pair.
pub(super) fn click_test_step(t: &mut ClickTest, now: f64, limit_ms: u32) {
    match t.last_click {
        Some(previous) if (now - previous) * 1000.0 <= f64::from(limit_ms) => {
            t.lit = true;
            t.last_click = None;
        }
        _ => {
            t.lit = false;
            t.last_click = Some(now);
        }
    }
}

/// What the user did on a row this frame.
enum RowAction {
    Set(Wanted),
    Restore,
}

/// The two choices of the wheel lines row.
#[derive(Clone, Copy, PartialEq)]
enum WheelMode {
    Lines,
    Screen,
}

fn number(v: Option<SettingValue>) -> Option<u32> {
    match v {
        Some(SettingValue::Number(n)) => Some(n),
        _ => None,
    }
}

fn span(s: SliderSpec) -> (u32, u32, u32) {
    (s.min, s.max, s.step)
}

/// A toggle click: precision and inactive scrolling are resolved from the
/// live value at Apply; the others are plain flags.
fn toggle_wanted(id: SettingId, on: bool) -> Wanted {
    match id {
        SettingId::PointerPrecision | SettingId::ScrollInactive => Wanted::Toggle(on),
        _ => Wanted::Value(SettingValue::Flag(on)),
    }
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

impl WinTuneApp {
    pub(super) fn show_mouse_keyboard_tab(&mut self, ctx: &egui::Context) {
        // Before the panel, so the bar spans the whole page width and the
        // progress of a running job shows.
        self.mk_action_bar(ctx);
        let tr = self.tr;
        let name = tr.t("nav.mouse_keyboard");
        let subtitle = tr.t("mk.subtitle");
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |_| {});
            secondary_line(ui, &tr.t("mk.note"));
            ui.add_space(space::S);
            self.mk_search_box(ui);
            ui.add_space(space::S);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.mk_sections(ui));
        });
    }

    /// The read error of `id` in the last scan, as the page words it.
    fn mk_unreadable(&self, id: SettingId) -> Option<String> {
        let error = self.mk_scan.as_ref()?.read_error(id)?;
        let problem = MkProblem::Unreadable {
            id,
            code: error.code,
        };
        Some(messages::mk_problem(self.tr, &problem))
    }

    /// Why the control of `id` is disabled: Windows could not read it, a job
    /// runs or the store is unreadable, or (ClickLock delay) ClickLock shows
    /// off.
    pub(super) fn mk_control_disabled_reason(&self, id: SettingId) -> Option<String> {
        self.mk_unreadable(id)
            .or_else(|| self.busy_or_blocked(&self.mk_store_error))
            .or_else(|| {
                let lock_off =
                    self.mk_display(SettingId::ClickLock) == Some(SettingValue::Flag(false));
                (id == SettingId::ClickLockTime && lock_off).then(|| self.tr.t("mk.click_lock_off"))
            })
    }

    /// Why the trail length slider is disabled: the trails toggle's reason,
    /// or trails showing off. The toggle itself never gets the second one.
    pub(super) fn mk_trails_length_reason(&self) -> Option<String> {
        let id = SettingId::PointerTrails;
        self.mk_control_disabled_reason(id).or_else(|| {
            let on = self
                .mk_display(id)
                .and_then(|v| is_on(id, &v))
                .unwrap_or(false);
            (!on).then(|| self.tr.t("mk.trails_off"))
        })
    }

    /// Why Apply is disabled: an unreadable store, the wrong account, a
    /// running job, or no change to write.
    fn mk_apply_disabled_reason(&self) -> Option<String> {
        let tr = self.tr;
        self.mk_blocked_reason()
            .or_else(|| {
                let problem = self.mk_scan.as_ref()?.account.problem()?;
                Some(messages::mk_problem(tr, &problem))
            })
            .or_else(|| self.busy_or_blocked(&None))
            .or_else(|| (self.mk_pending_count() == 0).then(|| tr.t("mk.nothing_to_apply")))
    }

    /// The page's bar: how much is not applied, Discard and Apply; or the
    /// job's progress while one runs.
    fn mk_action_bar(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let pending = self.mk_pending_count();
        let status = if pending > 0 {
            tr.t_args("mk.pending", &[&pending])
        } else {
            tr.t("mk.up_to_date")
        };
        let disabled_reason = self.mk_apply_disabled_reason();
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("tweaks.apply");
        let discard_label = tr.t("tweaks.discard");
        let can_discard = !self.mk_pending.is_empty();

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
            self.start_mk_apply(ctx);
        }
        if discard {
            self.discard_mk_changes();
        }
    }

    fn mk_search_box(&mut self, ui: &mut egui::Ui) {
        let id = polish::search_box_id(ActiveTab::MouseKeyboard).expect("search box");
        let mut search = std::mem::take(&mut self.mk_search);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut search)
                .id(id)
                .hint_text(self.tr.t("mk.search"))
                .desired_width(kit::SEARCH_WIDTH),
        );
        self.mk_search = search;
        self.focus_search_if_requested(&resp);
    }

    /// The five sections, or the loading line, or the wrong-account banner.
    fn mk_sections(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let Some(scan) = &self.mk_scan else {
            kit::card(ui, |ui| note_line(ui, &tr.t("mk.scanning")));
            return;
        };
        if let Some(problem) = scan.account.problem() {
            let text = messages::mk_problem(tr, &problem);
            kit::banner(ui, kit::BannerKind::Error, &text, None);
            return;
        }
        let filter_keys = scan.filter_keys_on;
        let query = self.mk_search.trim().to_lowercase();
        let tests = query.is_empty();
        let mut key_test = std::mem::take(&mut self.mk_key_test);
        let mut actions = Vec::new();
        let mut target_clicked = false;
        let mut shown = 0;
        for section in Section::ALL {
            let ids: Vec<SettingId> = SettingId::ALL
                .into_iter()
                .filter(|id| id.section() == section)
                .filter(|id| title_matches(&query, &messages::mk_title(tr, *id)))
                .collect();
            if ids.is_empty() {
                continue;
            }
            shown += ids.len();
            kit::section(ui, &messages::mk_section(tr, section), |ui| {
                if section == Section::Keyboard && filter_keys {
                    kit::banner(ui, kit::BannerKind::Info, &tr.t("mk.filter_keys"), None);
                }
                for id in ids {
                    if let Some(action) = self.mk_row(ui, id) {
                        actions.push((id, action));
                    }
                    if id == SettingId::DoubleClickTime && tests {
                        target_clicked |= self.mk_click_test_row(ui);
                    }
                }
                if section == Section::Keyboard && tests {
                    self.mk_key_test_row(ui, &mut key_test);
                }
            });
        }
        if shown == 0 {
            note_line(ui, &tr.t("mk.no_match"));
        }
        self.mk_key_test = key_test;

        if target_clicked {
            // The speed on the slider, applied or not.
            let id = SettingId::DoubleClickTime;
            let speed = self.mk_display(id).unwrap_or_else(|| id.windows_default());
            let now = ui.input(|i| i.time);
            click_test_step(
                &mut self.mk_click_test,
                now,
                number(Some(speed)).unwrap_or_default(),
            );
        }
        // After drawing, as on the Startup rows.
        for (id, action) in actions {
            match action {
                RowAction::Set(wanted) => self.set_mk_wanted(id, wanted),
                RowAction::Restore => self.request_restore_mouse_keyboard(vec![id.key().into()]),
            }
        }
    }

    /// One setting: title and description on the left; then, right to left,
    /// the control, the `…` menu and the "Not applied" pill.
    fn mk_row(&self, ui: &mut egui::Ui, id: SettingId) -> Option<RowAction> {
        let tr = self.tr;
        let title = messages::mk_title(tr, id);
        let description = messages::mk_description(tr, id);
        let shown = self.mk_display(id);
        let reason = self.mk_control_disabled_reason(id);
        let busy = self.busy_or_blocked(&self.mk_store_error);
        let default = id.windows_default();
        let default_hint = busy
            .clone()
            .or_else(|| self.mk_unreadable(id))
            .or_else(|| (shown == Some(default)).then(|| tr.t("mk.default_shown")));

        let windows_default = tr.t("mk.windows_default");
        let restore = tr.t("tweaks.restore_original");
        let mut items = vec![kit::MenuItem {
            label: &windows_default,
            enabled: default_hint.is_none(),
            disabled_hint: default_hint.as_deref(),
        }];
        if self.mk_original(id).is_some() {
            items.push(kit::MenuItem {
                label: &restore,
                enabled: busy.is_none(),
                disabled_hint: busy.as_deref(),
            });
        }
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&title]);
        let pending = self.mk_pending.contains_key(&id);
        let not_applied = tr.t("mk.pill.not_applied");

        let mut action = None;
        kit::SettingRow {
            icon: None,
            title: &title,
            description: description.as_deref(),
            hover: None,
        }
        .show(ui, |ui| {
            if let Some(wanted) = self.mk_control(ui, id, shown, reason.as_deref()) {
                action = Some(RowAction::Set(wanted));
            }
            let salt = format!("mk_{}", id.key());
            match kit::overflow_menu(ui, &salt, &more, &more_for, &items) {
                Some(0) => action = Some(RowAction::Set(Wanted::Value(default))),
                Some(_) => action = Some(RowAction::Restore),
                None => {}
            }
            if pending {
                kit::pill(ui, &not_applied, PillKind::Accent);
            }
        });
        action
    }

    /// The control of `id`, showing `shown`; the choice the user made this
    /// frame, if any.
    fn mk_control(
        &self,
        ui: &mut egui::Ui,
        id: SettingId,
        shown: Option<SettingValue>,
        reason: Option<&str>,
    ) -> Option<Wanted> {
        match id.control() {
            Control::Toggle => {
                let mut on = shown.and_then(|v| is_on(id, &v)).unwrap_or(false);
                kit::toggle_enabled(ui, &mut on, reason)
                    .changed()
                    .then(|| toggle_wanted(id, on))
            }
            Control::Slider(spec) => {
                let mut n = number(shown).unwrap_or(spec.min);
                let label = shown
                    .map(|v| messages::mk_value(self.tr, Some(id), &v))
                    .unwrap_or_default();
                kit::slider(ui, &mut n, span(spec), &label, reason)
                    .changed()
                    .then_some(Wanted::Value(SettingValue::Number(n)))
            }
            Control::TrailsLength(spec) => self.mk_trails_control(ui, spec, shown, reason),
            Control::LinesOrScreen(spec) => self.mk_wheel_control(ui, spec, shown, reason),
        }
    }

    /// Pointer trails: a toggle, then the length slider (disabled while the
    /// trails show off). The length starts from the original or live length.
    fn mk_trails_control(
        &self,
        ui: &mut egui::Ui,
        spec: SliderSpec,
        shown: Option<SettingValue>,
        reason: Option<&str>,
    ) -> Option<Wanted> {
        let id = SettingId::PointerTrails;
        let on = shown.and_then(|v| is_on(id, &v)).unwrap_or(false);
        let mut length = match number(shown) {
            Some(n) if on => n,
            _ => {
                let live = self.mk_scan.as_ref().and_then(|s| s.live(id));
                let start = self.mk_original(id).or(live);
                trails_start_length(&start.unwrap_or(SettingValue::Number(0)))
            }
        };
        let mut wanted = None;
        let mut toggled = on;
        if kit::toggle_enabled(ui, &mut toggled, reason).changed() {
            let n = if toggled { length } else { 0 };
            wanted = Some(Wanted::Value(SettingValue::Number(n)));
        }
        let label = messages::mk_value(self.tr, Some(id), &SettingValue::Number(length));
        let length_reason = self.mk_trails_length_reason();
        if kit::slider(
            ui,
            &mut length,
            span(spec),
            &label,
            length_reason.as_deref(),
        )
        .changed()
        {
            wanted = Some(Wanted::Value(SettingValue::Number(length)));
        }
        wanted
    }

    /// Wheel lines: "Lines" or "One screen at a time", then the lines slider
    /// while Lines is shown.
    fn mk_wheel_control(
        &self,
        ui: &mut egui::Ui,
        spec: SliderSpec,
        shown: Option<SettingValue>,
        reason: Option<&str>,
    ) -> Option<Wanted> {
        let tr = self.tr;
        let id = SettingId::WheelLines;
        let screen = shown == Some(SettingValue::Number(WHEEL_PAGESCROLL));
        let mut mode = if screen {
            WheelMode::Screen
        } else {
            WheelMode::Lines
        };
        let lines = tr.t("mk.wheel.lines");
        let one_screen = tr.t("mk.value.one_screen");
        let options = [
            (WheelMode::Lines, lines.as_str()),
            (WheelMode::Screen, one_screen.as_str()),
        ];
        let resp = ui
            .add_enabled_ui(reason.is_none(), |ui| {
                kit::segmented(ui, &mut mode, &options)
            })
            .inner;
        let resp = match reason {
            Some(reason) => resp.on_disabled_hover_text(reason),
            None => resp,
        };
        let mut wanted = None;
        if resp.changed() {
            let n = match mode {
                WheelMode::Screen => WHEEL_PAGESCROLL,
                WheelMode::Lines => {
                    let live = number(self.mk_scan.as_ref().and_then(|s| s.live(id)));
                    live.filter(|n| (spec.min..=spec.max).contains(n))
                        .unwrap_or(FALLBACK_LINES)
                }
            };
            wanted = Some(Wanted::Value(SettingValue::Number(n)));
        }
        if !screen {
            let mut n = number(shown).unwrap_or(spec.min);
            let label = shown
                .map(|v| messages::mk_value(tr, Some(id), &v))
                .unwrap_or_default();
            if kit::slider(ui, &mut n, span(spec), &label, reason).changed() {
                wanted = Some(Wanted::Value(SettingValue::Number(n)));
            }
        }
        wanted
    }

    /// The double-click test row. Returns whether its target was clicked;
    /// the click is timed by the caller, not by egui's own double-click.
    fn mk_click_test_row(&self, ui: &mut egui::Ui) -> bool {
        let tr = self.tr;
        let title = tr.t("mk.click_test.title");
        let description = tr.t("mk.click_test.desc");
        let label = if self.mk_click_test.lit {
            tr.t("mk.click_test.ok")
        } else {
            tr.t("mk.click_test.target")
        };
        kit::SettingRow {
            icon: None,
            title: &title,
            description: Some(&description),
            hover: None,
        }
        .show(ui, |ui| ui.add(secondary_button(&label)).clicked())
    }

    /// The key repeat test row: a field to type in (Windows repeats keys
    /// with the applied settings).
    fn mk_key_test_row(&self, ui: &mut egui::Ui, text: &mut String) {
        let tr = self.tr;
        let title = tr.t("mk.key_test.title");
        let note = tr.t("mk.key_test.note");
        let hint = tr.t("mk.key_test.hint");
        kit::SettingRow {
            icon: None,
            title: &title,
            description: Some(&note),
            hover: None,
        }
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .desired_width(kit::SEARCH_WIDTH),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::{kit, polish, ActiveTab, WinTuneApp};
    use super::{click_test_step, ClickTest};
    use crate::i18n::messages;
    use crate::mouse_keyboard::SettingId::{self, *};
    use crate::mouse_keyboard::SettingValue::*;
    use crate::mouse_keyboard::{AccountCheck, MkProblem, Section, Wanted, WHEEL_PAGESCROLL};
    use egui::{Event, Key, Modifiers};
    use std::collections::BTreeMap;

    /// Size of a toggle.
    const TOGGLE: egui::Vec2 = egui::vec2(40.0, 20.0);

    /// The Mouse & keyboard page, its first scan finished.
    fn page_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_mk_scan();
        run_until_mk_scanned(&mut app, ctx);
        app.active_tab = ActiveTab::MouseKeyboard;
        app
    }

    fn rescan(app: &mut WinTuneApp, ctx: &egui::Context) {
        app.request_mk_scan();
        run_until_mk_scanned(app, ctx);
    }

    /// Two frames, so the layout has settled; the texts of the second.
    fn texts(app: &mut WinTuneApp, ctx: &egui::Context) -> Vec<String> {
        frame(app, ctx, tall_input(vec![]));
        drawn_texts(&frame(app, ctx, tall_input(vec![])))
    }

    /// Where `text` is drawn, after two frames.
    fn center_of(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        text_center(&out, text)
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)))
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        let pos = center_of(app, ctx, text);
        click_at(app, ctx, pos);
    }

    fn title(app: &WinTuneApp, id: SettingId) -> String {
        messages::mk_title(app.tr, id)
    }

    /// The widget of `size` on the row of `id`.
    fn row_control(
        app: &mut WinTuneApp,
        ctx: &egui::Context,
        id: SettingId,
        size: egui::Vec2,
    ) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let title = title(app, id);
        widget_right_of(app, ctx, &title, size)
            .unwrap_or_else(|| panic!("no widget of {size:?} on row {title:?}"))
    }

    /// Hover, press, release on the double-click target, every frame at
    /// `time` seconds.
    fn click_target_at(app: &mut WinTuneApp, ctx: &egui::Context, time: f64) {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        let pos = ["mk.click_test.target", "mk.click_test.ok"]
            .iter()
            .find_map(|key| text_center(&out, &app.tr.t(key)))
            .expect("the double-click target is drawn");
        let button = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
        ] {
            let mut input = tall_input(events);
            input.time = Some(time);
            frame(app, ctx, input);
        }
    }

    #[test]
    fn click_test_lights_within_the_slider_time() {
        let mut t = ClickTest::default();
        click_test_step(&mut t, 0.0, 500);
        assert!(!t.lit, "one click");
        click_test_step(&mut t, 0.3, 500);
        assert!(t.lit, "two clicks 300 ms apart");
        click_test_step(&mut t, 0.5, 500);
        assert!(!t.lit, "a third click starts a new pair");

        let mut t = ClickTest::default();
        click_test_step(&mut t, 0.0, 500);
        click_test_step(&mut t, 0.6, 500);
        assert!(!t.lit, "two clicks 600 ms apart");
    }

    #[test]
    fn the_page_draws_five_sections_in_table_order_with_live_values() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        frame(&mut app, &ctx, tall_input(vec![]));
        let out = frame(&mut app, &ctx, tall_input(vec![]));

        let ys: Vec<f32> = Section::ALL
            .iter()
            .map(|s| {
                let text = messages::mk_section(app.tr, *s);
                text_center(&out, &text)
                    .unwrap_or_else(|| panic!("{text:?} not drawn"))
                    .y
            })
            .collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
        let shown = drawn_texts(&out);
        for want in ["10", "500 ms", "1200 ms"] {
            assert!(
                shown.iter().any(|t| t == want),
                "{want:?} not drawn: {shown:?}"
            );
        }
        for key in ["mk.subtitle", "mk.note", "mk.up_to_date"] {
            assert!(shown.contains(&app.tr.t(key)), "{key} not drawn");
        }
        assert!(!shown.contains(&app.tr.t("tweaks.discard")));
    }

    #[test]
    fn a_toggle_marks_not_applied_and_apply_sets_it() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let toggle = row_control(&mut app, &ctx, PointerVanish, TOGGLE);
        click_at(&mut app, &ctx, toggle);

        assert_eq!(
            app.mk_pending,
            BTreeMap::from([(PointerVanish, Wanted::Value(Flag(false)))])
        );
        let shown = texts(&mut app, &ctx);
        for want in [
            app.tr.t("mk.pill.not_applied"),
            app.tr.t_args("mk.pending", &[&1]),
            app.tr.t("tweaks.discard"),
        ] {
            assert!(shown.contains(&want), "{want:?} not drawn: {shown:?}");
        }

        let apply = app.tr.t("tweaks.apply");
        click_text(&mut app, &ctx, &apply);
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            fx.fake_input().value(PointerVanish),
            Flag(false),
            "{}",
            app.last_notice()
        );
        assert!(app.mk_pending.is_empty());
    }

    #[test]
    fn an_out_of_range_live_value_is_shown_and_never_written_unless_moved() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        fx.fake_input().put(DoubleClickTime, Number(1300));
        rescan(&mut app, &ctx);

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"1300 ms".to_string()), "{shown:?}");
        assert_eq!(app.mk_pending_count(), 0);
        assert!(app.mk_pending.is_empty());

        let windows_default = app.tr.t("mk.windows_default");
        let more = row_control(&mut app, &ctx, DoubleClickTime, kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        click_text(&mut app, &ctx, &windows_default);
        assert_eq!(
            app.mk_pending.get(&DoubleClickTime),
            Some(&Wanted::Value(Number(500)))
        );
        assert_eq!(fx.fake_input().sets(), [], "nothing written before Apply");

        // Now shown: the item is disabled and says why.
        let more = row_control(&mut app, &ctx, DoubleClickTime, kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        let item = center_of(&mut app, &ctx, &windows_default);
        let hover = hover_texts(&mut app, &ctx, item);
        assert!(hover.contains(&app.tr.t("mk.default_shown")), "{hover:?}");
    }

    #[test]
    fn one_screen_at_a_time_sets_the_page_scroll_value() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let (one_screen, lines) = (app.tr.t("mk.value.one_screen"), app.tr.t("mk.wheel.lines"));
        click_text(&mut app, &ctx, &one_screen);
        assert_eq!(
            app.mk_pending.get(&WheelLines),
            Some(&Wanted::Value(Number(WHEEL_PAGESCROLL)))
        );

        click_text(&mut app, &ctx, &lines);
        assert!(
            !app.mk_pending.contains_key(&WheelLines),
            "back to the live 3: {:?}",
            app.mk_pending
        );
    }

    #[test]
    fn clicklock_delay_and_trails_length_follow_the_shown_toggle() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        let tr = app.tr;

        assert_eq!(
            app.mk_control_disabled_reason(ClickLockTime),
            Some(tr.t("mk.click_lock_off"))
        );
        app.set_mk_wanted(ClickLock, Wanted::Value(Flag(true)));
        assert_eq!(app.mk_control_disabled_reason(ClickLockTime), None);

        assert_eq!(app.mk_trails_length_reason(), Some(tr.t("mk.trails_off")));
        assert_eq!(app.mk_control_disabled_reason(PointerTrails), None);
        let toggle = row_control(&mut app, &ctx, PointerTrails, TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert_eq!(
            app.mk_pending.get(&PointerTrails),
            Some(&Wanted::Value(Number(7)))
        );
        assert_eq!(app.mk_trails_length_reason(), None);
    }

    #[test]
    fn a_setting_windows_cannot_read_is_disabled_with_the_error() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.fake_input().fail_get(HoverTime);
        let mut app = page_app(&fx, &ctx);

        assert_eq!(
            app.mk_control_disabled_reason(HoverTime),
            Some(messages::mk_problem(
                app.tr,
                &MkProblem::Unreadable {
                    id: HoverTime,
                    code: 5
                }
            ))
        );
        assert_eq!(app.mk_control_disabled_reason(MenuShowDelay), None);
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&title(&app, HoverTime)), "{shown:?}");
    }

    #[test]
    fn the_double_click_target_uses_the_slider_speed() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        let detected = app.tr.t("mk.click_test.ok");
        let base = ctx.input(|i| i.time) + 1.0;

        click_target_at(&mut app, &ctx, base);
        click_target_at(&mut app, &ctx, base + 0.3);
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&detected), "{shown:?}");

        // 200 ms on the slider, not applied: 300 ms is too slow.
        app.set_mk_wanted(DoubleClickTime, Wanted::Value(Number(200)));
        let base = base + 2.0;
        click_target_at(&mut app, &ctx, base);
        click_target_at(&mut app, &ctx, base + 0.3);
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&detected), "{shown:?}");
        assert!(shown.contains(&app.tr.t("mk.click_test.target")));
    }

    #[test]
    fn the_filter_keys_note_shows_only_when_on() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        let note = app.tr.t("mk.filter_keys");
        assert!(!texts(&mut app, &ctx).contains(&note));

        fx.fake_input().set_filter_keys(true);
        rescan(&mut app, &ctx);

        let out = {
            frame(&mut app, &ctx, tall_input(vec![]));
            frame(&mut app, &ctx, tall_input(vec![]))
        };
        let at = text_center(&out, &note).expect("the Filter Keys note");
        let keyboard = text_center(&out, &app.tr.t("mk.section.keyboard")).unwrap();
        let delay = text_center(&out, &title(&app, KeyRepeatDelay)).unwrap();
        assert!(keyboard.y < at.y && at.y < delay.y, "first in Keyboard");
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
        let banner = messages::mk_problem(
            app.tr,
            &MkProblem::WrongAccount {
                process: "PC\\Admin".into(),
                session: "PC\\Ana".into(),
            },
        );
        assert!(banner.contains("PC\\Admin") && banner.contains("PC\\Ana"));

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&banner), "{shown:?}");
        for s in Section::ALL {
            let section = messages::mk_section(app.tr, s);
            assert!(!shown.contains(&section), "{section:?} drawn");
        }

        let apply = app.tr.t("tweaks.apply");
        let apply = center_of(&mut app, &ctx, &apply);
        click_at(&mut app, &ctx, apply);
        assert!(!app.is_busy());
        let hover = hover_texts(&mut app, &ctx, apply);
        assert!(hover.contains(&banner), "{hover:?}");
    }

    #[test]
    fn ctrl_f_focuses_the_mouse_keyboard_search() {
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

        let id = polish::search_box_id(ActiveTab::MouseKeyboard);
        assert_eq!(id, Some(egui::Id::new("search_mouse_keyboard")));
        assert!(ctx.memory(|m| m.has_focus(id.unwrap())));
    }

    #[test]
    fn search_filters_rows_by_title() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        // Typed into the box, upper case.
        let id = polish::search_box_id(ActiveTab::MouseKeyboard).unwrap();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, &ctx, tall_input(vec![]));
        frame(
            &mut app,
            &ctx,
            tall_input(vec![Event::Text("SCROLL".into())]),
        );

        let shown = texts(&mut app, &ctx);
        for id in [WheelLines, WheelChars, ScrollInactive] {
            assert!(shown.contains(&title(&app, id)), "{shown:?}");
        }
        for hidden in [
            title(&app, PointerSpeed),
            messages::mk_section(app.tr, Section::Pointer),
            app.tr.t("mk.click_test.title"),
            app.tr.t("mk.key_test.title"),
        ] {
            assert!(!shown.contains(&hidden), "{hidden:?} drawn");
        }

        app.mk_search = "nothing like it".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("mk.no_match")), "{shown:?}");
    }

    #[test]
    fn the_page_shows_the_loading_line_until_the_first_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.mk_scan_needed = false;
        app.active_tab = ActiveTab::MouseKeyboard;
        let scanning = app.tr.t("mk.scanning");
        let speed = title(&app, PointerSpeed);

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&scanning), "{shown:?}");
        assert!(!shown.contains(&speed));

        app.request_mk_scan();
        run_until_mk_scanned(&mut app, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&scanning), "{shown:?}");
        assert!(shown.contains(&speed), "{shown:?}");
    }

    #[test]
    fn opening_the_page_requests_a_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.active_tab = ActiveTab::Home;
        assert!(!app.mk_scan_needed);

        app.open_page(ActiveTab::MouseKeyboard);

        assert_eq!(app.active_tab, ActiveTab::MouseKeyboard);
        assert!(app.mk_scan_needed);
    }
}
