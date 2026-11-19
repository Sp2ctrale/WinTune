use super::WinTuneApp;
use crate::theme::{accent_button, card_frame, get_palette};
use eframe::egui::{self, Margin, RichText};

impl WinTuneApp {
    pub(super) fn show_rollback_tab(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);
        let busy = self.is_busy();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.config.theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.heading("Selective Registry Rollback");
                            ui.label(
                                RichText::new(
                                    "Select specific extensions to restore to their original Windows state without affecting other customized extensions.",
                                )
                                .color(palette.text_secondary),
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label =
                                format!("Restore Selected ({})", self.rollback_selection.len());
                            let enabled = !busy && !self.rollback_selection.is_empty();
                            if ui
                                .add_enabled(enabled, accent_button(label, self.config.theme))
                                .clicked()
                            {
                                self.request_rollback();
                            }
                        });
                    });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Select All").clicked() {
                            self.rollback_selection =
                                self.backup_store.backups.keys().cloned().collect();
                        }
                        if ui.button("Deselect All").clicked() {
                            self.rollback_selection.clear();
                        }
                    });
                });

                ui.add_space(10.0);

                card_frame(self.config.theme).show(ui, |ui| {
                    if self.backup_store.backups.is_empty() {
                        ui.label(
                            RichText::new(
                                "No registry snapshots recorded yet. Snapshots are automatically created before an extension is modified for the first time.",
                            )
                            .color(palette.text_secondary),
                        );
                        return;
                    }

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("rollback_fluent_grid")
                            .striped(true)
                            .num_columns(5)
                            .spacing([28.0, 10.0])
                            .show(ui, |ui| {
                                ui.strong("Restore");
                                ui.strong("Extension");
                                ui.strong("Active Custom ProgID");
                                ui.strong("Original Windows ProgID");
                                ui.strong("Snapshot taken");
                                ui.end_row();

                                for (ext, bkp) in &self.backup_store.backups {
                                    let mut checked = self.rollback_selection.contains(ext);
                                    if ui.checkbox(&mut checked, "").changed() {
                                        if checked {
                                            self.rollback_selection.insert(ext.clone());
                                        } else {
                                            self.rollback_selection.remove(ext);
                                        }
                                    }
                                    ui.label(RichText::new(ext).strong());
                                    ui.label(&bkp.custom_prog_id);
                                    let orig = bkp
                                        .hklm_extension_key
                                        .default_value
                                        .as_deref()
                                        .unwrap_or("(None / Unassigned)");
                                    ui.label(RichText::new(orig).color(palette.text_secondary));
                                    ui.label(
                                        RichText::new(&bkp.timestamp).color(palette.text_secondary),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                });
            });
    }
}
