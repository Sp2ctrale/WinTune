//! Operation history: every registry job is appended to history.jsonl and
//! listed (newest first) on the History page.

use super::{kit, WinTuneApp};
use crate::associations::OperationReport;
use crate::history::{append_entry, read_recent, HistoryEntry};
use crate::i18n::messages;
use eframe::egui::{self, RichText};

/// Entries kept in memory and shown.
pub(super) const HISTORY_LIMIT: usize = 200;

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

    /// The History page: one row per recorded operation, newest first.
    pub(super) fn show_history_page(&mut self, ctx: &egui::Context) {
        let p = kit::current_palette(ctx);
        let tr = self.tr;
        let title = tr.t("nav.history");
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &[], &title, None, |_| {});
            if self.history.is_empty() {
                kit::empty_state(ui, kit::Icon::History, &tr.t("history.empty"), None);
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("history_grid")
                    .striped(true)
                    .num_columns(5)
                    .spacing([kit::space::XL, kit::space::XS])
                    .show(ui, |ui| {
                        for entry in &self.history {
                            ui.label(RichText::new(&entry.time).color(p.text_secondary));
                            ui.label(messages::history_module(tr, &entry.module));
                            ui.label(
                                RichText::new(messages::history_action(tr, &entry.action)).strong(),
                            );
                            let summary = if entry.is_success() {
                                RichText::new(tr.t_args("history.ok", &[&entry.succeeded.len()]))
                                    .color(p.success)
                            } else {
                                RichText::new(tr.t_args(
                                    "history.ok_errors",
                                    &[&entry.succeeded.len(), &entry.errors.len()],
                                ))
                                .color(p.warning)
                            };
                            let mut details = entry.succeeded.join(", ");
                            for e in entry.errors.iter().chain(&entry.warnings) {
                                details.push('\n');
                                details.push_str(e);
                            }
                            ui.label(summary).on_hover_text(details);
                            match &entry.reg_backup {
                                Some(path) => {
                                    if ui
                                        .small_button(tr.t("history.copy_reg"))
                                        .on_hover_text(path)
                                        .clicked()
                                    {
                                        ui.ctx().copy_text(path.clone());
                                    }
                                }
                                None => {
                                    ui.label("");
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
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
        assert!(texts.contains(&"1 ok, 1 error(s)".to_string()), "{texts:?}");

        app.history.clear();
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(drawn_texts(&out).contains(&"No operation recorded yet.".to_string()));
    }
}
