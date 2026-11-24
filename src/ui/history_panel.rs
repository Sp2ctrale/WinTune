//! Operation history: every registry job is appended to history.jsonl and
//! listed (newest first) on the History page.

use super::{kit, WinTuneApp};
use crate::associations::OperationReport;
use crate::history::{append_entry, read_recent, HistoryEntry};
use crate::i18n::messages;
use eframe::egui::{self, RichText};

/// Entries kept in memory and shown.
pub(super) const HISTORY_LIMIT: usize = 200;

/// Module filter of the operations list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum HistoryFilter {
    All,
    Associations,
    Explorer,
    Start,
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

    /// The History page: the operations list (Task 3 adds saved originals).
    pub(super) fn show_history_page(&mut self, ctx: &egui::Context) {
        let title = self.tr.t("nav.history");
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &[], &title, None, |_| {});
            egui::ScrollArea::vertical().show(ui, |ui| {
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
        ];
        let options = [
            (HistoryFilter::All, labels[0].as_str()),
            (HistoryFilter::Associations, labels[1].as_str()),
            (HistoryFilter::Explorer, labels[2].as_str()),
            (HistoryFilter::Start, labels[3].as_str()),
        ];
        kit::section(ui, &heading, |ui| {
            if self.history.is_empty() {
                kit::empty_state(ui, kit::Icon::History, &tr.t("history.empty"), None);
                return;
            }
            kit::segmented(ui, &mut self.history_filter, &options);
            ui.add_space(kit::space::S);
            let filter = self.history_filter;
            let mut shown = 0;
            for (i, entry) in self.history.iter().enumerate() {
                if !filter.matches(&entry.module) {
                    continue;
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
                let salt = format!("history_{i}_{}", entry.time);
                kit::disclosure(ui, &salt, &title, false, |ui| {
                    ui.label(tr.t_args("history.succeeded", &[&entry.succeeded.join(", ")]));
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
}
