use super::WinTuneApp;
use crate::theme::get_palette;
use eframe::egui::{self, Margin, RichText, Stroke};

impl WinTuneApp {
    pub(super) fn show_status_bar(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.ui_theme);

        egui::TopBottomPanel::bottom("fluent_status_bar")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_sidebar)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .inner_margin(Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(job) = &self.job {
                        let p = job.progress();
                        ui.spinner();
                        let text = if p.total > 0 {
                            format!("{} ({}/{})", p.label, p.done, p.total)
                        } else {
                            job.title().to_string()
                        };
                        ui.add(
                            egui::ProgressBar::new(p.fraction())
                                .desired_width(260.0)
                                .text(text),
                        );
                    } else {
                        let status = ui.label(
                            RichText::new(&self.status_message)
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                        if !self.status_details.is_empty() {
                            status.on_hover_text(&self.status_details);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = format!("History ({})", self.history.len());
                        if ui.selectable_label(self.show_history, label).clicked() {
                            self.show_history = !self.show_history;
                        }
                        ui.separator();
                        let enabled_count =
                            self.config.extensions.iter().filter(|e| e.enabled).count();
                        ui.label(
                            RichText::new(format!(
                                "{} / {} extensions enabled",
                                enabled_count,
                                self.config.extensions.len()
                            ))
                            .size(12.0)
                            .color(palette.text_secondary),
                        );
                    });
                });
            });
    }
}
