//! The "Startup apps" page: programs that start when the user signs in, each
//! with an on/off switch. Choices stay pending until Apply. The page body is
//! built on top of the app wiring in `mod.rs`, `startup_state.rs` and `jobs.rs`.

use super::startup_state::startup_change;
use super::{kit, polish, ActiveTab, WinTuneApp};
use crate::i18n::messages;
use crate::startup::{StartupItem, StartupState};
use eframe::egui::{self, RichText};
use kit::{primary_button, secondary_button, space, text_size, PillKind};

/// What the user did on a row this frame.
enum RowAction {
    /// The toggle: the wanted on state.
    Switch(bool),
    Restore,
}

fn is_on(item: &StartupItem) -> bool {
    item.state == StartupState::On
}

impl WinTuneApp {
    pub(super) fn show_startup_tab(&mut self, ctx: &egui::Context) {
        // Before the panel, so the bar spans the whole page width and the
        // progress of a running job shows.
        self.startup_action_bar(ctx);
        let tr = self.tr;
        let name = tr.t("nav.startup");
        let subtitle = tr.t("startup.subtitle");
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |ui| {
                kit::pill(ui, &tr.t("tweaks.pill.unofficial"), PillKind::Warning)
                    .on_hover_text(tr.t("startup.unofficial_hint"));
            });
            let p = kit::current_palette(ui.ctx());
            ui.label(
                RichText::new(tr.t("startup.next_sign_in"))
                    .size(text_size::SECONDARY)
                    .color(p.text_secondary),
            );
            ui.add_space(space::S);
            self.startup_search_box(ui);
            ui.add_space(space::S);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.startup_list(ui));
        });
    }

    /// Why Apply is disabled: an unreadable snapshot store, a running job, or
    /// no change to write.
    fn startup_apply_disabled_reason(&self) -> Option<String> {
        self.startup_blocked_reason()
            .or_else(|| self.busy_or_blocked(&None))
            .or_else(|| {
                (self.startup_pending_count() == 0).then(|| self.tr.t("startup.nothing_to_apply"))
            })
    }

    /// The page's bar: how much is not applied, Discard and Apply; or the
    /// job's progress while one runs.
    fn startup_action_bar(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let pending = self.startup_pending_count();
        let status = if pending > 0 {
            tr.t_args("startup.pending", &[&pending])
        } else {
            tr.t("startup.up_to_date")
        };
        let disabled_reason = self.startup_apply_disabled_reason();
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("tweaks.apply");
        let discard_label = tr.t("tweaks.discard");
        let can_discard = !self.startup_pending.is_empty();

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
            self.start_startup_apply(ctx);
        }
        if discard {
            self.discard_startup_changes();
        }
    }

    fn startup_search_box(&mut self, ui: &mut egui::Ui) {
        let id = polish::search_box_id(ActiveTab::Startup).expect("search box");
        let mut search = std::mem::take(&mut self.startup_search);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut search)
                .id(id)
                .hint_text(self.tr.t("startup.search"))
                .desired_width(kit::SEARCH_WIDTH),
        );
        self.startup_search = search;
        self.focus_search_if_requested(&resp);
    }

    /// The scanned items whose name or program contains the search text
    /// (case-insensitive), in scan order.
    fn visible_startup_items(&self) -> Vec<StartupItem> {
        let query = self.startup_search.trim().to_lowercase();
        let Some(scan) = &self.startup_scan else {
            return Vec::new();
        };
        scan.items
            .iter()
            .filter(|i| {
                query.is_empty()
                    || i.display_name.to_lowercase().contains(&query)
                    || i.program.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    /// The rows, or the loading line, or an empty state.
    fn startup_list(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let p = kit::current_palette(ui.ctx());
        let none_at_all = self
            .startup_scan
            .as_ref()
            .is_some_and(|s| s.items.is_empty());
        let rows = self.visible_startup_items();
        let scanning = self.startup_scan.is_none();
        let mut actions = Vec::new();
        kit::card(ui, |ui| {
            let line = |ui: &mut egui::Ui, text: String| {
                ui.label(
                    RichText::new(text)
                        .size(text_size::BODY)
                        .color(p.text_secondary),
                );
            };
            if scanning {
                line(ui, tr.t("startup.scanning"));
            } else if none_at_all {
                kit::empty_state(ui, kit::Icon::Startup, &tr.t("startup.empty"), None);
            } else if rows.is_empty() {
                line(ui, tr.t("startup.no_match"));
            } else {
                for item in &rows {
                    if let Some(action) = self.startup_row(ui, item) {
                        actions.push((item.clone(), action));
                    }
                }
            }
        });
        for (item, action) in actions {
            self.apply_startup_row_action(&item, action);
        }
    }

    /// One item: name and program on the left; then, right to left, the
    /// toggle, the `…` menu and the pills.
    fn startup_row(&self, ui: &mut egui::Ui, item: &StartupItem) -> Option<RowAction> {
        let tr = self.tr;
        let pending = self.startup_pending.get(&item.id).copied();
        let mut on = pending.unwrap_or_else(|| is_on(item));
        // Locked first: waiting for a job would not unlock it.
        let change_reason = item
            .locked
            .as_ref()
            .map(|l| messages::startup_lock(tr, l))
            .or_else(|| self.busy_or_blocked(&self.startup_store_error));
        let busy = self.busy_or_blocked(&self.startup_store_error);

        let hover = item
            .origins
            .iter()
            .map(|o| format!("{}: {}", messages::startup_source(tr, o.source), o.detail))
            .collect::<Vec<_>>()
            .join("\n");
        let restore = tr.t("tweaks.restore_original");
        let items = [kit::MenuItem {
            label: &restore,
            enabled: busy.is_none(),
            disabled_hint: busy.as_deref(),
        }];
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&item.display_name]);

        let mut action = None;
        kit::SettingRow {
            icon: None,
            title: &item.display_name,
            description: Some(&item.program),
            hover: (!hover.is_empty()).then_some(hover.as_str()),
        }
        .show(ui, |ui| {
            if kit::toggle_enabled(ui, &mut on, change_reason.as_deref()).changed() {
                action = Some(RowAction::Switch(on));
            }
            if item.has_snapshot {
                let salt = format!("startup_{}", item.id);
                if kit::overflow_menu(ui, &salt, &more, &more_for, &items) == Some(0) {
                    action = Some(RowAction::Restore);
                }
            }
            if item.origins.len() > 1 {
                kit::pill(ui, &tr.t("startup.pill.both"), PillKind::Neutral);
            } else if item.all_users() {
                kit::pill(ui, &tr.t("startup.pill.all_users"), PillKind::Neutral);
            }
            if item.program_missing {
                kit::pill(ui, &tr.t("startup.pill.not_found"), PillKind::Warning);
            }
            if item.state != StartupState::On {
                kit::pill(
                    ui,
                    &messages::startup_state(tr, item.state),
                    PillKind::Neutral,
                );
            }
            // After the toggle, so the pill follows this frame's click.
            if startup_change(item, on).is_some() {
                let key = if on {
                    "startup.pill.will_turn_on"
                } else {
                    "startup.pill.will_turn_off"
                };
                kit::pill(ui, &tr.t(key), PillKind::Accent);
            }
        });
        action
    }

    fn apply_startup_row_action(&mut self, item: &StartupItem, action: RowAction) {
        match action {
            RowAction::Switch(on) => {
                // Back to the current state is no change at all.
                if on == is_on(item) {
                    self.startup_pending.remove(&item.id);
                } else {
                    self.startup_pending.insert(item.id.clone(), on);
                }
            }
            RowAction::Restore => self.request_restore_startup(vec![item.slot().id()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::{ActiveTab, WinTuneApp};
    use crate::i18n::messages;
    use crate::startup::{StartupLock, RUN32_KEY};
    use egui::{Event, Key, Modifiers};
    use std::collections::BTreeMap;

    /// Size of a toggle.
    const TOGGLE: egui::Vec2 = egui::vec2(40.0, 20.0);
    const APP: &str = r#""C:\Apps\app.exe""#;

    /// The Startup page, its first scan finished.
    fn page_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_startup_scan();
        run_until_startup_scanned(&mut app, ctx);
        app.active_tab = ActiveTab::Startup;
        app
    }

    /// Two frames, so the layout has settled; the texts of the second.
    fn texts(app: &mut WinTuneApp, ctx: &egui::Context) -> Vec<String> {
        frame(app, ctx, tall_input(vec![]));
        drawn_texts(&frame(app, ctx, tall_input(vec![])))
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        let pos = text_center(&out, text)
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)));
        click_at(app, ctx, pos);
    }

    fn click_key(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) {
        let text = app.tr.t(key);
        click_text(app, ctx, &text);
    }

    /// Center of the widget of exactly `size` on the row titled `title`: to
    /// its right, at most one row height below the title line.
    fn row_widget(
        app: &mut WinTuneApp,
        ctx: &egui::Context,
        title: &str,
        size: egui::Vec2,
    ) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        let at = text_center(&out, title)
            .unwrap_or_else(|| panic!("{title:?} not drawn: {:?}", drawn_texts(&out)));
        ctx.viewport_for(egui::ViewportId::ROOT, |v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, widgets)| widgets.iter())
                .filter(|w| w.rect.size() == size && w.rect.center().x > at.x)
                .map(|w| w.rect.center())
                .filter(|c| c.y > at.y - 8.0 && c.y - at.y < 48.0)
                .min_by(|a, b| (a.y - at.y).total_cmp(&(b.y - at.y)))
        })
        .unwrap_or_else(|| panic!("no widget of {size:?} on row {title:?}"))
    }

    #[test]
    fn the_page_lists_items_with_their_pills() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "Mine", r#""C:\Nowhere\mine.exe""#);
        fx.seed_run(true, "Shared", r#""C:\Nowhere\shared.exe""#);
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);

        for want in [
            "Mine".to_string(),
            "Shared".to_string(),
            app.tr.t("startup.pill.all_users"),
            app.tr.t("startup.pill.not_found"),
            app.tr.t("startup.subtitle"),
            app.tr.t("startup.next_sign_in"),
            app.tr.t("tweaks.pill.unofficial"),
            app.tr.t("startup.up_to_date"),
        ] {
            assert!(shown.contains(&want), "{want:?} not drawn: {shown:?}");
        }
        let discard = app.tr.t("tweaks.discard");
        assert!(!shown.contains(&discard), "Discard with nothing pending");
    }

    #[test]
    fn toggling_marks_pending_and_discard_clears_it() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "App", APP);
        let mut app = page_app(&fx, &ctx);

        let toggle = row_widget(&mut app, &ctx, "App", TOGGLE);
        click_at(&mut app, &ctx, toggle);

        assert_eq!(
            app.startup_pending,
            BTreeMap::from([("run:app".to_string(), false)])
        );
        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t_args("startup.pending", &[&1])),
            "{shown:?}"
        );
        assert!(shown.contains(&app.tr.t("startup.pill.will_turn_off")));

        // Clicking back to the current state is no change.
        let toggle = row_widget(&mut app, &ctx, "App", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert!(app.startup_pending.is_empty(), "{:?}", app.startup_pending);

        let toggle = row_widget(&mut app, &ctx, "App", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert_eq!(app.startup_pending.len(), 1);
        click_key(&mut app, &ctx, "tweaks.discard");

        assert!(app.startup_pending.is_empty());
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("startup.up_to_date")), "{shown:?}");
        assert!(!shown.contains(&app.tr.t("startup.pill.will_turn_off")));
    }

    #[test]
    fn apply_from_the_page_turns_the_app_off() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "App", APP);
        let mut app = page_app(&fx, &ctx);

        let toggle = row_widget(&mut app, &ctx, "App", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        click_key(&mut app, &ctx, "tweaks.apply");
        run_until_idle(&mut app, &ctx);

        let switch = fx.user_switch("App").expect("a switch value");
        assert_eq!(switch.first(), Some(&3), "{}", app.last_notice());
        run_until_startup_scanned(&mut app, &ctx);
        assert!(app.startup_pending.is_empty());
        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&messages::startup_state(
                app.tr,
                crate::startup::StartupState::Off
            )),
            "{shown:?}"
        );
        // The toggle now reads off: clicking it asks to turn the app on.
        let toggle = row_widget(&mut app, &ctx, "App", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert_eq!(app.startup_pending.get("run:app"), Some(&true));
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("startup.pill.will_turn_on")));
    }

    #[test]
    fn a_locked_row_has_a_disabled_toggle_with_the_reason() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run_in(true, RUN32_KEY, "Wow", r#""C:\Nowhere\wow.exe""#);
        let mut app = page_app(&fx, &ctx);

        let toggle = row_widget(&mut app, &ctx, "Wow", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert!(app.startup_pending.is_empty(), "a locked toggle changed");
        let hover = hover_texts(&mut app, &ctx, toggle);

        let reason = messages::startup_lock(app.tr, &StartupLock::AllUsersOnly);
        assert!(hover.contains(&reason), "{hover:?}");
    }

    #[test]
    fn the_overflow_menu_restores_an_original() {
        use super::super::confirm::ConfirmAction;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "App", APP);
        let mut app = page_app(&fx, &ctx);
        app.startup_pending.insert("run:app".into(), false);
        app.start_startup_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        run_until_startup_scanned(&mut app, &ctx);
        assert!(app.startup_scan.as_ref().unwrap().items[0].has_snapshot);

        let more = row_widget(&mut app, &ctx, "App", super::kit::OVERFLOW_BUTTON);
        click_at(&mut app, &ctx, more);
        click_key(&mut app, &ctx, "tweaks.restore_original");

        let slot = crate::startup::Approved::Run.slot("App").id();
        assert_eq!(
            app.pending_confirm.as_ref().map(|c| c.action.clone()),
            Some(ConfirmAction::RestoreStartup(vec![slot]))
        );
    }

    #[test]
    fn apply_is_disabled_when_nothing_is_pending() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        frame(&mut app, &ctx, tall_input(vec![]));
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let apply = text_center(&out, &app.tr.t("tweaks.apply")).expect("Apply drawn");
        click_at(&mut app, &ctx, apply);
        assert!(!app.is_busy(), "apply started with nothing to apply");
        let hover = hover_texts(&mut app, &ctx, apply);

        assert!(
            hover.contains(&app.tr.t("startup.nothing_to_apply")),
            "{hover:?}"
        );
    }

    #[test]
    fn ctrl_f_focuses_the_startup_search() {
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

        let id = super::polish::search_box_id(ActiveTab::Startup);
        assert_eq!(id, Some(egui::Id::new("search_startup")));
        assert!(ctx.memory(|m| m.has_focus(id.unwrap())));
    }

    #[test]
    fn search_filters_by_program() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "Alpha", r#""C:\Tools\sync.exe""#);
        fx.seed_run(false, "Beta", r#""C:\Tools\mail.exe""#);
        let mut app = page_app(&fx, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"Alpha".to_string()), "{shown:?}");
        assert!(shown.contains(&"Beta".to_string()), "{shown:?}");

        // Typed into the box, upper case: the program matches, not the name.
        let id = super::polish::search_box_id(ActiveTab::Startup).unwrap();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, &ctx, tall_input(vec![]));
        frame(&mut app, &ctx, tall_input(vec![Event::Text("MAIL".into())]));

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"Beta".to_string()), "{shown:?}");
        assert!(!shown.contains(&"Alpha".to_string()), "{shown:?}");

        app.startup_search = "nothing like it".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("startup.no_match")), "{shown:?}");
    }

    #[test]
    fn an_empty_scan_shows_the_empty_state() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);

        assert!(shown.contains(&app.tr.t("startup.empty")), "{shown:?}");
        assert!(!shown.contains(&app.tr.t("startup.scanning")));
    }

    #[test]
    fn the_page_shows_the_loading_line_until_the_first_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "App", APP);
        let mut app = fx.app(&ctx);
        app.startup_scan_needed = false;
        app.active_tab = ActiveTab::Startup;
        let scanning = app.tr.t("startup.scanning");

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&scanning), "{shown:?}");
        assert!(!shown.contains(&"App".to_string()));

        app.request_startup_scan();
        run_until_startup_scanned(&mut app, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&scanning), "{shown:?}");
        assert!(shown.contains(&"App".to_string()), "{shown:?}");
    }

    #[test]
    fn opening_the_page_requests_a_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        run_until_startup_scanned(&mut app, &ctx);
        app.active_tab = ActiveTab::Home;
        assert!(!app.startup_scan_needed);

        app.open_page(ActiveTab::Startup);

        assert_eq!(app.active_tab, ActiveTab::Startup);
        assert!(app.startup_scan_needed);
    }
}
