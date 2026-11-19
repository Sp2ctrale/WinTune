use super::{ActiveTab, WinTuneApp, APP_VERSION};
use crate::associations::notify_shell_associations;
use crate::theme::{apply_fluent_theme, draw_badge, get_palette, sidebar_frame, ThemeMode};
use eframe::egui::{self, Margin, RichText, Rounding, Stroke};

impl WinTuneApp {
    fn tab_subtitle(&self, tab: ActiveTab) -> String {
        match tab {
            ActiveTab::Associations => format!("{} rules", self.config.extensions.len()),
            ActiveTab::Tweaks | ActiveTab::StartPrivacy => {
                let groups = WinTuneApp::tweak_groups_of(tab);
                let on = crate::tweaks::catalog()
                    .iter()
                    .filter(|t| groups.contains(&t.group))
                    .filter(|t| {
                        self.tweak_statuses.get(t.id) == Some(&crate::tweaks::TweakStatus::On)
                    })
                    .count();
                format!("{} on", on)
            }
            ActiveTab::Gallery => format!("{} icons", self.gallery.names().len()),
            ActiveTab::Rollback => format!("{} backups", self.backup_store.backups.len()),
        }
    }

    pub(super) fn show_sidebar(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);

        egui::SidePanel::left("fluent_sidebar")
            .resizable(false)
            .exact_width(235.0)
            .frame(sidebar_frame(self.config.theme))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(logo) = &self.logo_texture {
                        ui.image((logo.id(), egui::vec2(34.0, 34.0)));
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new("WinTune").size(19.0).strong());
                        ui.label(
                            RichText::new(format!("v{} • System Tuner", APP_VERSION))
                                .size(11.0)
                                .color(palette.text_secondary),
                        );
                    });
                });

                ui.add_space(16.0);
                ui.label(
                    RichText::new("MODULES")
                        .size(11.0)
                        .color(palette.text_secondary)
                        .strong(),
                );
                ui.add_space(4.0);

                for tab in ActiveTab::ALL {
                    let subtitle = self.tab_subtitle(tab);
                    self.sidebar_nav_button(ui, tab, &subtitle);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(4.0);
                    let (wsl_label, wsl_color) = if self.wsl_detected {
                        ("WSL2 Active", palette.success)
                    } else {
                        ("No WSL (.sh -> Editor)", palette.warning)
                    };
                    draw_badge(ui, wsl_label, wsl_color);

                    ui.add_space(8.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("Refresh Windows Explorer"),
                        )
                        .clicked()
                    {
                        notify_shell_associations();
                        self.set_status("Windows Explorer refreshed via SHChangeNotify.");
                    }

                    ui.add_space(6.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("Save Configuration"),
                        )
                        .clicked()
                    {
                        self.save_config();
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Theme")
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_dark = self.config.theme == ThemeMode::Dark;
                            if ui.selectable_label(!is_dark, "Light").clicked() {
                                self.set_theme(ctx, ThemeMode::Light);
                            }
                            if ui.selectable_label(is_dark, "Dark").clicked() {
                                self.set_theme(ctx, ThemeMode::Dark);
                            }
                        });
                    });
                });
            });
    }

    fn set_theme(&mut self, ctx: &egui::Context, mode: ThemeMode) {
        self.config.theme = mode;
        apply_fluent_theme(ctx, mode);
        self.save_config();
    }

    fn sidebar_nav_button(&mut self, ui: &mut egui::Ui, tab: ActiveTab, subtitle: &str) {
        let palette = get_palette(self.config.theme);
        let is_active = self.active_tab == tab;
        let (fill, stroke) = if is_active {
            (
                palette.accent.gamma_multiply(0.22),
                Stroke::new(1.0_f32, palette.accent),
            )
        } else {
            (egui::Color32::TRANSPARENT, Stroke::NONE)
        };

        let resp =
            egui::Frame::none()
                .fill(fill)
                .stroke(stroke)
                .rounding(Rounding::same(6.0))
                .inner_margin(Margin::symmetric(10.0, 8.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tab.title()).size(13.5).strong().color(
                            if is_active {
                                palette.accent_hover
                            } else {
                                palette.text_primary
                            },
                        ));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(subtitle)
                                    .size(11.0)
                                    .color(palette.text_secondary),
                            );
                        });
                    });
                })
                .response
                .interact(egui::Sense::click());

        if resp.clicked() {
            // Don't swap the store under a running job: it will replace it.
            if tab == ActiveTab::Rollback && !self.is_busy() {
                self.reload_backup_store();
            }
            if tab == ActiveTab::Associations && self.active_tab != tab {
                self.request_status_refresh();
            }
            if matches!(tab, ActiveTab::Tweaks | ActiveTab::StartPrivacy) && !self.is_busy() {
                self.reload_tweak_store();
                self.refresh_tweak_statuses();
            }
            self.active_tab = tab;
        }
        ui.add_space(4.0);
    }
}
