//! The icon picker: a modal dialog over the shared icon grid. Rules open it
//! from their detail; the Icons page only browses.

use super::icon_grid::icon_grid;
use super::{kit, WinTuneApp};
use crate::gallery::IconCategory;
use eframe::egui;

/// Dialog state. It remembers the extension, with the rule's index as a
/// hint, so a rule that is renamed or removed meanwhile is never confused
/// with another one, and duplicates of one extension stay apart.
pub(super) struct IconPicker {
    pub extension: String,
    /// Where the rule was when the picker opened.
    index: usize,
    pub query: String,
    pub category: IconCategory,
    /// Put the cursor in the search box on the first frame.
    focus_search: bool,
}

impl IconPicker {
    /// The rule this picker is for: the one at the remembered index while it
    /// still has the extension, else the first with that extension.
    fn rule_index(&self, rules: &[crate::models::ExtensionConfig]) -> Option<usize> {
        let same = |r: &crate::models::ExtensionConfig| r.extension == self.extension;
        if rules.get(self.index).is_some_and(same) {
            return Some(self.index);
        }
        rules
            .iter()
            .position(|r| r.extension.eq_ignore_ascii_case(&self.extension))
    }
}

impl WinTuneApp {
    pub(super) fn open_icon_picker(&mut self, index: usize) {
        let Some(rule) = self.config.extensions.get(index) else {
            return;
        };
        // An empty or unknown icon name opens on All.
        let category = if self.gallery.names().contains(&rule.icon_name) {
            IconCategory::of(&rule.icon_name)
        } else {
            IconCategory::All
        };
        self.icon_picker = Some(IconPicker {
            extension: rule.extension.clone(),
            index,
            query: String::new(),
            category,
            focus_search: true,
        });
    }

    /// Modal picker. Escape and Cancel close it; clicking an icon assigns it.
    pub(super) fn show_icon_picker(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.icon_picker.take() else {
            return;
        };
        let tr = self.tr;
        let names = self.gallery.filter_names(&picker.query, picker.category);
        let current = picker
            .rule_index(&self.config.extensions)
            .map(|i| self.config.extensions[i].icon_name.clone());
        let categories = [
            (IconCategory::All, tr.t("picker.cat.all")),
            (IconCategory::FilesOnly, tr.t("picker.cat.files")),
            (IconCategory::FoldersOnly, tr.t("picker.cat.folders")),
            (IconCategory::LightVariants, tr.t("picker.cat.light")),
            (IconCategory::Clones, tr.t("picker.cat.clones")),
        ];
        let options: Vec<(IconCategory, &str)> =
            categories.iter().map(|(c, l)| (*c, l.as_str())).collect();
        let folder = self.config.icon_source_folder.clone();

        let mut chosen: Option<String> = None;
        let mut cancelled = false;
        kit::dialog(ctx, "icon_picker", &tr.t("picker.title"), 720.0, |ui| {
            let search = ui.add(
                egui::TextEdit::singleline(&mut picker.query)
                    .id(egui::Id::new("icon_picker_search"))
                    .hint_text(tr.t("picker.search"))
                    .desired_width(ui.available_width()),
            );
            if picker.focus_search {
                search.request_focus();
                picker.focus_search = false;
            }
            ui.add_space(kit::space::S);
            kit::segmented(ui, &mut picker.category, &options);
            ui.add_space(kit::space::M);

            if names.is_empty() {
                let text = if self.gallery.names().is_empty() {
                    tr.t_args("picker.no_icons", &[&folder])
                } else {
                    tr.t("picker.empty")
                };
                kit::empty_state(ui, kit::Icon::Folder, &text, None);
            } else {
                ui.scope(|ui| {
                    ui.set_max_height(420.0);
                    chosen = icon_grid(
                        ui,
                        ctx,
                        &self.gallery,
                        &names,
                        current.as_deref(),
                        "icon_picker_grid",
                        true,
                    );
                });
            }

            ui.add_space(kit::space::M);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancelled = ui.button(tr.t("picker.cancel")).clicked();
            });
        });

        cancelled |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        self.icon_picker = Some(picker);
        if let Some(name) = chosen {
            self.choose_icon(&name);
        } else if cancelled {
            self.icon_picker = None;
        }
    }

    /// Assigns `name` to the rule the picker was opened for, then closes it.
    /// If that rule is gone (removed or renamed), nothing changes.
    fn choose_icon(&mut self, name: &str) {
        let Some(picker) = self.icon_picker.take() else {
            return;
        };
        let rule = picker
            .rule_index(&self.config.extensions)
            .map(|i| &mut self.config.extensions[i]);
        match rule {
            Some(rule) => {
                rule.icon_name = name.to_string();
                let text = self.tr.t_args("picker.assigned", &[&name, &rule.extension]);
                self.notify(kit::ToastKind::Success, text);
            }
            None => {
                let text = self.tr.t_args("notice.rule_gone", &[&picker.extension]);
                self.notify(kit::ToastKind::Info, text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};

    fn input(t: f64, events: Vec<Event>) -> RawInput {
        RawInput {
            time: Some(t),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 760.0))),
            events,
            ..Default::default()
        }
    }

    /// Draws the picker, then clicks the `bar` tile (move, press, release).
    fn click_bar(app: &mut WinTuneApp, ctx: &egui::Context) {
        frame(app, ctx, input(0.0, vec![]));
        let out = frame(app, ctx, input(0.01, vec![]));
        let bar = text_center(&out, "bar").expect("bar tile drawn");
        let button = |pressed| Event::PointerButton {
            pos: bar,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        frame(app, ctx, input(0.02, vec![Event::PointerMoved(bar)]));
        frame(app, ctx, input(0.03, vec![button(true)]));
        frame(app, ctx, input(0.04, vec![button(false)]));
    }

    #[test]
    fn choosing_an_icon_sets_it_on_the_rule_and_closes() {
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);

        click_bar(&mut app, &ctx);

        assert_eq!(app.config.extensions[0].icon_name, "bar");
        assert!(app.icon_picker.is_none());
    }

    #[test]
    fn the_picker_choice_is_autosaved() {
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);
        click_bar(&mut app, &ctx);

        frame(&mut app, &ctx, input(0.05, vec![]));
        frame(&mut app, &ctx, input(1.5, vec![]));

        let saved = std::fs::read_to_string(fx.dir.join("config.json")).unwrap_or_default();
        assert!(saved.contains("\"icon_name\": \"bar\""), "{saved}");
    }

    #[test]
    fn choosing_an_icon_for_a_removed_rule_changes_nothing() {
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);
        app.config.extensions[0].extension = ".baz".into();

        app.choose_icon("bar");

        assert_eq!(app.config.extensions[0].icon_name, "foo");
        assert!(app.icon_picker.is_none());
        assert!(app.last_notice().contains(".foo"), "{}", app.last_notice());
    }

    #[test]
    fn escape_closes_the_picker() {
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);
        frame(&mut app, &ctx, input(0.0, vec![]));
        assert!(app.icon_picker.is_some());

        let esc = Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&mut app, &ctx, input(0.1, vec![esc]));

        assert!(app.icon_picker.is_none());
        assert_eq!(app.config.extensions[0].icon_name, "foo");
    }

    #[test]
    fn an_empty_icon_folder_shows_the_empty_state() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);

        frame(&mut app, &ctx, input(0.0, vec![]));
        let out = frame(&mut app, &ctx, input(0.01, vec![]));

        let folder = app.config.icon_source_folder.clone();
        let text = app.tr.t_args("picker.no_icons", &[&folder]);
        assert!(drawn_texts(&out).contains(&text), "{:?}", drawn_texts(&out));
    }

    #[test]
    fn an_empty_folder_shows_the_no_icons_text_even_with_a_query() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);
        app.icon_picker.as_mut().unwrap().query = "zzz".into();

        frame(&mut app, &ctx, input(0.0, vec![]));
        let out = frame(&mut app, &ctx, input(0.01, vec![]));

        let folder = app.config.icon_source_folder.clone();
        let texts = drawn_texts(&out);
        assert!(texts.contains(&app.tr.t_args("picker.no_icons", &[&folder])));
        assert!(!texts.contains(&app.tr.t("picker.empty")));
    }

    #[test]
    fn a_duplicate_extension_does_not_redirect_the_picker() {
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".bar"]);
        app.active_tab = super::super::ActiveTab::Associations;
        app.config.extensions[1].extension = ".foo".into();
        app.selected_ext_index = 1;

        assert_eq!(
            app.rule_state(0),
            super::super::associations_tab::RuleState::Invalid
        );
        assert_eq!(
            app.rule_state(1),
            super::super::associations_tab::RuleState::Invalid
        );
        let reason = app.tr.t("assoc.blocked.this_rule");
        assert_eq!(
            app.apply_rule_disabled_reason(1, &app.rule_problems()),
            Some(reason)
        );

        app.open_icon_picker(1);
        click_bar(&mut app, &ctx);

        assert_eq!(app.config.extensions[1].icon_name, "bar");
        assert_eq!(app.config.extensions[0].icon_name, "foo");
    }

    #[test]
    fn the_picker_opens_on_the_current_icons_category() {
        let fx = TempIcons::new(&["foo", "folder-src", "rust_light", "rust.clone", "rust"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        for (icon, category) in [
            ("folder-src", IconCategory::FoldersOnly),
            ("rust_light", IconCategory::LightVariants),
            ("rust.clone", IconCategory::Clones),
            ("rust", IconCategory::FilesOnly),
            ("", IconCategory::All),
            ("not-in-pack", IconCategory::All),
        ] {
            app.config.extensions[0].icon_name = icon.into();
            app.open_icon_picker(0);
            assert_eq!(
                app.icon_picker.as_ref().unwrap().category,
                category,
                "icon {icon:?}"
            );
        }
    }

    fn press_ctrl_f(app: &mut WinTuneApp, ctx: &egui::Context) {
        let ctrl_f = Event::Key {
            key: Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        // Only the shortcut handler runs, so no search box resets the flag.
        let _ = ctx.run(input(0.0, vec![ctrl_f]), |ctx| app.handle_shortcuts(ctx));
    }

    #[test]
    fn ctrl_f_does_nothing_while_the_picker_is_open() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_page(super::super::ActiveTab::Associations);

        // Control: with no picker, Ctrl+F asks for the search box.
        press_ctrl_f(&mut app, &ctx);
        assert!(app.focus_search);
        app.focus_search = false;

        app.open_icon_picker(0);
        press_ctrl_f(&mut app, &ctx);
        assert!(!app.focus_search);
    }

    #[test]
    fn a_query_with_no_match_shows_the_no_match_text() {
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_icon_picker(0);
        app.icon_picker.as_mut().unwrap().query = "zzz".into();

        frame(&mut app, &ctx, input(0.0, vec![]));
        let out = frame(&mut app, &ctx, input(0.01, vec![]));

        let texts = drawn_texts(&out);
        assert!(texts.contains(&app.tr.t("picker.empty")), "{texts:?}");
        let folder = app.config.icon_source_folder.clone();
        assert!(!texts.contains(&app.tr.t_args("picker.no_icons", &[&folder])));
    }
}
