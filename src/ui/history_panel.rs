//! Operation history: every registry job is appended to history.jsonl and
//! listed (newest first) on the History page.

use super::{kit, WinTuneApp};
use crate::associations::OperationReport;
use crate::history::{append_entry, read_recent, HistoryEntry};
use crate::i18n::messages;
use eframe::egui::{self, RichText};

/// Entries kept in memory.
pub(super) const HISTORY_LIMIT: usize = 200;

/// Operations drawn before "Show all".
const RECENT_SHOWN: usize = 20;

/// Module filter of the operations list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum HistoryFilter {
    All,
    Associations,
    Explorer,
    Start,
    ContextMenu,
}

impl HistoryFilter {
    /// Whether an entry stored under `stored_module` (the English label
    /// written to history.jsonl) passes the filter.
    pub(super) fn matches(self, stored_module: &str) -> bool {
        match self {
            Self::All => true,
            Self::Associations => stored_module == "File associations",
            Self::Explorer => stored_module == "Explorer & Taskbar",
            Self::Start => stored_module == "Start & Privacy",
            Self::ContextMenu => stored_module == "Context menu",
        }
    }
}

impl WinTuneApp {
    pub(super) fn reload_history(&mut self) {
        self.history = read_recent(&self.env.history_path, HISTORY_LIMIT).unwrap_or_default();
    }

    /// Logs a finished operation. A failure to write the log never hides the
    /// operation's own outcome: it gets its own error toast.
    pub(super) fn record_history(&mut self, module: &str, action: &str, report: &OperationReport) {
        let entry = HistoryEntry::from_report(module, action, report);
        if let Err(e) = append_entry(&self.env.history_path, &entry) {
            let text = self.tr.t_args("notice.history_not_saved", &[&e]);
            self.notify(kit::ToastKind::Error, text);
        }
        self.history.insert(0, entry);
        self.history.truncate(HISTORY_LIMIT);
    }

    /// The History page: the saved originals, then the operations list.
    pub(super) fn show_history_page(&mut self, ctx: &egui::Context) {
        let title = self.tr.t("nav.history");
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &[], &title, None, |_| {});
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.show_originals(ui);
                self.show_operations(ui);
            });
        });
    }

    /// The "Operations" section: module filter and one expandable row per
    /// recorded operation, newest first.
    fn show_operations(&mut self, ui: &mut egui::Ui) {
        let p = kit::current_palette(ui.ctx());
        let tr = self.tr;
        let heading = tr.t("history.operations");
        let labels = [
            tr.t("history.filter.all"),
            tr.t("history.module.associations"),
            tr.t("history.module.explorer"),
            tr.t("history.module.start"),
            tr.t("history.module.context_menu"),
        ];
        let options = [
            (HistoryFilter::All, labels[0].as_str()),
            (HistoryFilter::Associations, labels[1].as_str()),
            (HistoryFilter::Explorer, labels[2].as_str()),
            (HistoryFilter::Start, labels[3].as_str()),
            (HistoryFilter::ContextMenu, labels[4].as_str()),
        ];
        kit::section(ui, &heading, |ui| {
            if self.history.is_empty() {
                kit::empty_state(ui, kit::Icon::History, &tr.t("history.empty"), None);
                return;
            }
            let before = self.history_filter;
            kit::segmented(ui, &mut self.history_filter, &options);
            if self.history_filter != before {
                self.history_show_all = false;
            }
            ui.add_space(kit::space::S);
            let filter = self.history_filter;
            let matching = self
                .history
                .iter()
                .filter(|e| filter.matches(&e.module))
                .count();
            let limit = if self.history_show_all {
                usize::MAX
            } else {
                RECENT_SHOWN
            };
            let len = self.history.len();
            let mut shown = 0;
            for (i, entry) in self.history.iter().enumerate() {
                if !filter.matches(&entry.module) {
                    continue;
                }
                if shown == limit {
                    break;
                }
                shown += 1;
                let summary = if entry.is_success() {
                    tr.t_args("history.ok", &[&entry.succeeded.len()])
                } else {
                    tr.t_args(
                        "history.ok_errors",
                        &[&entry.succeeded.len(), &entry.errors.len()],
                    )
                };
                let title = format!(
                    "{} \u{b7} {} \u{b7} {} \u{b7} {}",
                    entry.time,
                    messages::history_module(tr, &entry.module),
                    messages::history_action(tr, &entry.action),
                    summary
                );
                // Counted from the oldest entry, so recording a new one does
                // not move an opened row's id.
                let salt = format!(
                    "history_{}_{}_{}_{}",
                    len - i,
                    entry.time,
                    entry.module,
                    entry.action
                );
                kit::disclosure(ui, &salt, &title, false, |ui| {
                    if !entry.succeeded.is_empty() {
                        ui.label(tr.t_args("history.succeeded", &[&entry.succeeded.join(", ")]));
                    }
                    for e in &entry.errors {
                        ui.label(RichText::new(e).color(p.danger));
                    }
                    for w in &entry.warnings {
                        ui.label(RichText::new(w).color(p.warning));
                    }
                    if let Some(path) = &entry.reg_backup {
                        ui.label(RichText::new(path).color(p.text_secondary));
                        if ui.small_button(tr.t("history.copy_reg")).clicked() {
                            ui.ctx().copy_text(path.clone());
                        }
                    }
                });
            }
            if shown == 0 {
                ui.label(RichText::new(tr.t("history.no_match")).color(p.text_secondary));
            } else if matching > shown {
                let label = tr.t_args("history.show_all", &[&matching]);
                if ui.add(kit::secondary_button(&label)).clicked() {
                    self.history_show_all = true;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::ActiveTab;
    use super::*;
    use egui::RawInput;

    #[test]
    fn an_association_apply_is_logged_and_listed() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let rules = app.config.extensions.clone();

        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);

        assert_eq!(app.history.len(), 1);
        let e = &app.history[0];
        assert_eq!(
            (e.module.as_str(), e.action.as_str()),
            ("File associations", "Apply")
        );
        assert_eq!(e.succeeded, vec![".foo".to_string()]);
        assert!(e.reg_backup.is_some());
        let on_disk = read_recent(&fx.dir.join("history.jsonl"), 10).unwrap();
        assert_eq!(on_disk, app.history);
    }

    #[test]
    fn a_tweak_apply_is_logged_too() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.refresh_tweak_statuses();
        app.tweak_desired.insert("show_extensions", true);

        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);

        assert_eq!(app.history[0].module, "Explorer & Taskbar");
        assert_eq!(
            app.history[0].succeeded,
            vec!["show_extensions".to_string()]
        );
    }

    #[test]
    fn history_is_reloaded_newest_first() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let report = OperationReport {
            succeeded: vec![".a".into()],
            ..Default::default()
        };
        app.record_history("File associations", "Apply", &report);
        app.record_history("File associations", "Rollback", &report);

        app.history.clear();
        app.reload_history();

        let actions: Vec<&str> = app.history.iter().map(|e| e.action.as_str()).collect();
        assert_eq!(actions, ["Rollback", "Apply"]);
    }

    #[test]
    fn the_page_renders_with_entries_and_empty() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let report = OperationReport {
            succeeded: vec![".a".into()],
            errors: vec![".b: failed".into()],
            ..Default::default()
        };
        app.record_history("File associations", "Apply", &report);

        app.active_tab = ActiveTab::History;
        let out = frame(&mut app, &ctx, RawInput::default());
        let texts = drawn_texts(&out);
        assert!(
            texts.iter().any(|t| t.ends_with(" · 1 ok, 1 error(s)")),
            "{texts:?}"
        );

        app.history.clear();
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(drawn_texts(&out).contains(&"No operation recorded yet.".to_string()));
    }

    fn entry(module: &str, action: &str) -> HistoryEntry {
        let report = OperationReport {
            succeeded: vec!["a".into()],
            ..Default::default()
        };
        HistoryEntry::from_report(module, action, &report)
    }

    fn title_of(e: &HistoryEntry, module: &str) -> String {
        format!("{} · {} · Apply · 1 ok", e.time, module)
    }

    fn history_app(fx: &TempIcons, ctx: &egui::Context, entries: Vec<HistoryEntry>) -> WinTuneApp {
        let mut app = fx.app_with_rules(ctx, &[".foo"]);
        app.history = entries;
        app.active_tab = ActiveTab::History;
        app
    }

    #[test]
    fn the_filter_keeps_only_the_chosen_module() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let assoc = entry("File associations", "Apply");
        let tweak = entry("Explorer & Taskbar", "Apply");
        let mut app = history_app(&fx, &ctx, vec![assoc.clone(), tweak.clone()]);
        app.history_filter = HistoryFilter::Explorer;

        let texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
        assert!(
            texts.contains(&title_of(&tweak, "Explorer & Taskbar")),
            "{texts:?}"
        );
        assert!(
            !texts.contains(&title_of(&assoc, "File associations")),
            "{texts:?}"
        );
    }

    #[test]
    fn the_context_menu_chip_filters_operations() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let menu = entry("Context menu", "Apply");
        let assoc = entry("File associations", "Apply");
        let mut app = history_app(&fx, &ctx, vec![menu.clone(), assoc.clone()]);
        assert!(HistoryFilter::ContextMenu.matches("Context menu"));
        assert!(!HistoryFilter::ContextMenu.matches("Start & Privacy"));

        // The chip is drawn, right of the sidebar, and clicking it filters.
        let label = app.tr.t("history.module.context_menu");
        let out = settled(&mut app, &ctx);
        let at = text_centers(&out, &label)
            .into_iter()
            .filter(|c| c.x > 220.0)
            .next_back()
            .expect("context menu chip");
        click_at(&mut app, &ctx, at);
        let out = settled(&mut app, &ctx);
        assert_eq!(app.history_filter, HistoryFilter::ContextMenu);
        let texts = drawn_texts(&out);
        assert!(
            texts.contains(&title_of(&menu, "Context menu")),
            "{texts:?}"
        );
        assert!(
            !texts.contains(&title_of(&assoc, "File associations")),
            "{texts:?}"
        );
    }

    #[test]
    fn an_unknown_module_is_shown_as_stored_under_all() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let legacy = entry("Legacy thing", "Apply");
        let mut app = history_app(&fx, &ctx, vec![legacy.clone()]);

        let texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
        assert!(
            texts.contains(&title_of(&legacy, "Legacy thing")),
            "{texts:?}"
        );

        app.history_filter = HistoryFilter::Associations;
        let texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
        assert!(
            !texts.contains(&title_of(&legacy, "Legacy thing")),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"No operation for this module.".to_string()),
            "{texts:?}"
        );
    }

    #[test]
    fn opening_an_entry_shows_its_errors_and_reg_path() {
        use egui::{Event, Modifiers, PointerButton};
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut e = entry("File associations", "Apply");
        e.errors = vec![".b: failed".into()];
        e.reg_backup = Some("C:\\backups\\x.reg".into());
        let title = format!("{} · File associations · Apply · 1 ok, 1 error(s)", e.time);
        let mut app = history_app(&fx, &ctx, vec![e]);

        frame(&mut app, &ctx, RawInput::default());
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(!drawn_texts(&out).contains(&".b: failed".to_string()));
        let at = text_center(&out, &title).unwrap_or_else(|| panic!("{title:?} not drawn"));
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        for events in [
            vec![Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ] {
            frame(
                &mut app,
                &ctx,
                RawInput {
                    events,
                    ..Default::default()
                },
            );
        }
        let texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
        assert!(texts.contains(&".b: failed".to_string()), "{texts:?}");
        assert!(
            texts.contains(&"C:\\backups\\x.reg".to_string()),
            "{texts:?}"
        );
        assert!(texts.contains(&"Done: a".to_string()), "{texts:?}");
    }

    fn settled(app: &mut WinTuneApp, ctx: &egui::Context) -> egui::FullOutput {
        frame(app, ctx, tall_input(vec![]));
        frame(app, ctx, tall_input(vec![]))
    }

    fn operation_titles(out: &egui::FullOutput) -> usize {
        drawn_texts(out)
            .iter()
            .filter(|t| t.ends_with(" · Apply · 1 ok"))
            .count()
    }

    #[test]
    fn saved_originals_are_drawn_above_operations() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = history_app(&fx, &ctx, vec![entry("File associations", "Apply")]);

        let out = settled(&mut app, &ctx);
        let originals = text_center(&out, &app.tr.t("originals.title")).expect("originals");
        let operations = text_center(&out, &app.tr.t("history.operations")).expect("operations");
        assert!(originals.y < operations.y, "{originals:?} {operations:?}");
    }

    #[test]
    fn only_the_20_most_recent_entries_show_until_show_all() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let entries = (0..25)
            .map(|_| entry("File associations", "Apply"))
            .collect();
        let mut app = history_app(&fx, &ctx, entries);

        let out = settled(&mut app, &ctx);
        assert_eq!(operation_titles(&out), 20);
        let label = app.tr.t_args("history.show_all", &[&25]);
        let at = text_center(&out, &label).unwrap_or_else(|| panic!("{label:?} not drawn"));
        click_at(&mut app, &ctx, at);

        let out = settled(&mut app, &ctx);
        assert_eq!(operation_titles(&out), 25);
        assert!(text_center(&out, &label).is_none(), "button stays");

        // Changing the filter (a click on its chip, right of the sidebar)
        // folds the list back to 20.
        fn chip(shape: &egui::epaint::Shape, found: &mut Vec<egui::Pos2>) {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == "File associations" => {
                    found.push(t.pos + t.galley.rect.center().to_vec2());
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| chip(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        out.shapes.iter().for_each(|c| chip(&c.shape, &mut found));
        let at = *found.iter().find(|c| c.x > 220.0).expect("filter chip");
        click_at(&mut app, &ctx, at);
        let out = settled(&mut app, &ctx);
        assert_eq!(app.history_filter, HistoryFilter::Associations);
        assert!(!app.history_show_all);
        assert_eq!(operation_titles(&out), 20);
    }

    #[test]
    fn an_opened_entry_stays_open_when_a_new_one_is_recorded() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut e = entry("File associations", "Apply");
        e.errors = vec![".b: failed".into()];
        let title = format!("{} · File associations · Apply · 1 ok, 1 error(s)", e.time);
        let mut app = history_app(&fx, &ctx, vec![e]);

        let out = settled(&mut app, &ctx);
        let at = text_center(&out, &title).expect("title");
        click_at(&mut app, &ctx, at);
        let texts = drawn_texts(&settled(&mut app, &ctx));
        assert!(texts.contains(&".b: failed".to_string()), "{texts:?}");

        let report = OperationReport {
            succeeded: vec!["x".into()],
            ..Default::default()
        };
        app.record_history("Explorer & Taskbar", "Apply", &report);
        let texts = drawn_texts(&settled(&mut app, &ctx));
        assert!(texts.contains(&".b: failed".to_string()), "{texts:?}");
    }

    #[test]
    fn an_entry_with_no_success_has_no_done_line() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let report = OperationReport {
            errors: vec![".b: failed".into()],
            ..Default::default()
        };
        let e = HistoryEntry::from_report("File associations", "Apply", &report);
        let title = format!("{} · File associations · Apply · 0 ok, 1 error(s)", e.time);
        let mut app = history_app(&fx, &ctx, vec![e]);

        let out = settled(&mut app, &ctx);
        let at = text_center(&out, &title).unwrap_or_else(|| panic!("{title:?} not drawn"));
        click_at(&mut app, &ctx, at);
        let texts = drawn_texts(&settled(&mut app, &ctx));
        assert!(texts.contains(&".b: failed".to_string()), "{texts:?}");
        assert!(!texts.iter().any(|t| t.starts_with("Done:")), "{texts:?}");
    }
}
