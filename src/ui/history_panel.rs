//! Operation history: every registry job is appended to history.jsonl and
//! listed (newest first) in a collapsible panel above the status bar.

use super::WinTuneApp;
use crate::associations::OperationReport;
use crate::history::{append_entry, read_recent, HistoryEntry};
use crate::theme::get_palette;
use eframe::egui::{self, Margin, RichText, Stroke};

/// Entries kept in memory and shown.
pub(super) const HISTORY_LIMIT: usize = 200;

impl WinTuneApp {
    pub(super) fn reload_history(&mut self) {
        self.history = read_recent(&self.env.history_path, HISTORY_LIMIT).unwrap_or_default();
    }

    /// Logs a finished operation. A failure to write the log never hides the
    /// operation's own outcome: it is only added to the status details.
    pub(super) fn record_history(&mut self, module: &str, action: &str, report: &OperationReport) {
        let entry = HistoryEntry::from_report(module, action, report);
        if let Err(e) = append_entry(&self.env.history_path, &entry) {
            if !self.status_details.is_empty() {
                self.status_details.push('\n');
            }
            self.status_details
                .push_str(&format!("History not saved: {}", e));
        }
        self.history.insert(0, entry);
        self.history.truncate(HISTORY_LIMIT);
    }

    pub(super) fn show_history_panel(&mut self, ctx: &egui::Context) {
        if !self.show_history {
            return;
        }
        let palette = get_palette(self.ui_theme);

        egui::TopBottomPanel::bottom("history_panel")
            .resizable(true)
            .default_height(190.0)
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .inner_margin(Margin::symmetric(14.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Operation history").strong());
                    ui.label(
                        RichText::new(format!("{} most recent first", self.history.len()))
                            .size(11.0)
                            .color(palette.text_secondary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            self.show_history = false;
                        }
                    });
                });
                ui.separator();

                if self.history.is_empty() {
                    ui.label(
                        RichText::new("No operation recorded yet.").color(palette.text_secondary),
                    );
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("history_grid")
                        .striped(true)
                        .num_columns(5)
                        .spacing([18.0, 4.0])
                        .show(ui, |ui| {
                            for entry in &self.history {
                                ui.label(RichText::new(&entry.time).color(palette.text_secondary));
                                ui.label(&entry.module);
                                ui.label(RichText::new(&entry.action).strong());
                                let summary = if entry.is_success() {
                                    RichText::new(format!("{} ok", entry.succeeded.len()))
                                        .color(palette.success)
                                } else {
                                    RichText::new(format!(
                                        "{} ok, {} error(s)",
                                        entry.succeeded.len(),
                                        entry.errors.len()
                                    ))
                                    .color(palette.warning)
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
                                            .small_button("Copy .reg path")
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
    fn the_panel_renders_with_entries_on_every_tab() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let report = OperationReport {
            succeeded: vec![".a".into()],
            errors: vec![".b: failed".into()],
            ..Default::default()
        };
        app.record_history("File associations", "Apply", &report);
        app.show_history = true;

        for tab in ActiveTab::ALL {
            app.active_tab = tab;
            frame(&mut app, &ctx, RawInput::default());
        }
    }
}
