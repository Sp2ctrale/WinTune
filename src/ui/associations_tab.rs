use super::{ActiveTab, WinTuneApp};
use crate::detector::resolve_vscode_executable;
use crate::models::{CustomContextCommand, DoubleClickMode, ExtensionConfig};
use crate::theme::{accent_button, card_frame, draw_badge, get_palette};
use eframe::egui::{self, Margin, RichText};

impl WinTuneApp {
    pub(super) fn show_associations_tab(&mut self, ctx: &egui::Context) {
        self.show_editor_bar(ctx);
        self.show_extension_list(ctx);
        self.show_extension_details(ctx);
    }

    fn show_editor_bar(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);
        let busy = self.is_busy();

        egui::TopBottomPanel::top("associations_top_bar")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.config.theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Default Code Editor:").strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut self.config.code_executable)
                                .hint_text("C:\\...\\Microsoft VS Code\\Code.exe")
                                .desired_width(320.0),
                        );
                        if ui.button("Browse...").clicked() {
                            if let Some(file) = rfd::FileDialog::new()
                                .add_filter("Executable", &["exe"])
                                .pick_file()
                            {
                                self.config.code_executable = file.to_string_lossy().to_string();
                            }
                        }
                        if ui.button("Auto-Detect").clicked() {
                            if let Some(found) = resolve_vscode_executable(None) {
                                self.config.code_executable = found;
                                self.set_status("VS Code path resolved automatically.");
                            } else {
                                self.set_status("Could not locate Code.exe automatically.");
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !busy,
                                    accent_button("Apply Checked Extensions", self.config.theme),
                                )
                                .clicked()
                            {
                                let rules = self.config.extensions.clone();
                                self.start_apply(ctx, rules);
                            }
                        });
                    });
                });
            });
    }

    fn add_extension_from_input(&mut self) {
        let mut ext = self.new_ext_input.trim().to_lowercase();
        if ext.is_empty() {
            return;
        }
        if !ext.starts_with('.') {
            ext = format!(".{}", ext);
        }
        if self.config.extensions.iter().any(|e| e.extension == ext) {
            self.set_status(format!("{} is already in the list.", ext));
            return;
        }
        let clean = ext.trim_start_matches('.').to_uppercase();
        let default_icon = ext.trim_start_matches('.').to_string();
        self.config.extensions.push(ExtensionConfig {
            enabled: true,
            extension: ext.clone(),
            prog_id: format!("Custom{}File", clean),
            description: format!("{} Source File", clean),
            icon_name: default_icon,
            reset_existing: false,
            reset_user_choice: true,
            mode: DoubleClickMode::OpenInEditor,
            launcher_command: None,
            elevated_command: None,
            perceived_type: "text".to_string(),
            content_type: "text/plain".to_string(),
            preserve_existing_properties: true,
            extra_verbs: Vec::new(),
        });
        self.selected_ext_index = self.config.extensions.len() - 1;
        self.new_ext_input.clear();
    }

    fn show_extension_list(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);

        egui::SidePanel::left("extensions_master_list")
            .resizable(true)
            .default_width(330.0)
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 4.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.config.theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ext_search_query)
                                .hint_text("Search extension (.rs, python, react...)")
                                .desired_width(ui.available_width()),
                        );
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("Check All").clicked() {
                            for e in &mut self.config.extensions {
                                e.enabled = true;
                            }
                        }
                        if ui.button("Uncheck All").clicked() {
                            for e in &mut self.config.extensions {
                                e.enabled = false;
                            }
                        }
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_ext_input)
                                .hint_text(".zig, .astro...")
                                .desired_width(150.0),
                        );
                        if ui.button("+ Add Extension").clicked() {
                            self.add_extension_from_input();
                        }
                    });

                    ui.separator();

                    let q = self.ext_search_query.trim().to_lowercase();
                    let filtered_indices: Vec<usize> = self
                        .config
                        .extensions
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| {
                            q.is_empty()
                                || e.extension.to_lowercase().contains(&q)
                                || e.description.to_lowercase().contains(&q)
                                || e.icon_name.to_lowercase().contains(&q)
                        })
                        .map(|(i, _)| i)
                        .collect();

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for idx in filtered_indices {
                            let is_selected = self.selected_ext_index == idx;
                            let is_script =
                                self.config.extensions[idx].mode == DoubleClickMode::ExecuteScript;
                            let tex = self
                                .gallery
                                .texture(ctx, &self.config.extensions[idx].icon_name);

                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.config.extensions[idx].enabled, "");
                                match tex {
                                    Some(t) => {
                                        ui.image((t.id(), egui::vec2(20.0, 20.0)));
                                    }
                                    None => {
                                        // Keep rows aligned while the icon decodes.
                                        ui.allocate_exact_size(
                                            egui::vec2(20.0, 20.0),
                                            egui::Sense::hover(),
                                        );
                                    }
                                }
                                let label_text = format!(
                                    "{}  •  {}",
                                    self.config.extensions[idx].extension,
                                    self.config.extensions[idx].description
                                );
                                if ui.selectable_label(is_selected, label_text).clicked() {
                                    self.selected_ext_index = idx;
                                }
                                if is_script {
                                    draw_badge(ui, "Script", palette.warning);
                                }
                            });
                        }
                    });
                });
            });
    }

    fn show_extension_details(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);
        let busy = self.is_busy();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(12.0, 4.0)),
            )
            .show(ctx, |ui| {
                if self.selected_ext_index >= self.config.extensions.len() {
                    return;
                }

                let current_tex = self.gallery.texture(
                    ctx,
                    &self.config.extensions[self.selected_ext_index].icon_name,
                );
                let has_backup = self
                    .backup_store
                    .backups
                    .contains_key(&self.config.extensions[self.selected_ext_index].extension);

                let mut delete_current = false;
                let mut open_gallery_picker = false;
                let mut apply_single = false;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let ext_cfg = &mut self.config.extensions[self.selected_ext_index];

                    card_frame(self.config.theme).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(t) = current_tex {
                                ui.image((t.id(), egui::vec2(52.0, 52.0)));
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.heading(
                                        RichText::new(format!(
                                            "{} — {}",
                                            ext_cfg.extension, ext_cfg.description
                                        ))
                                        .size(20.0),
                                    );
                                    if has_backup {
                                        draw_badge(ui, "Snapshot Saved", palette.success);
                                    }
                                });
                                ui.label(
                                    RichText::new(format!(
                                        "ProgID: {}  |  Icon: {}.ico",
                                        ext_cfg.prog_id, ext_cfg.icon_name
                                    ))
                                    .color(palette.text_secondary),
                                );
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            !busy,
                                            accent_button(
                                                "Apply This Extension",
                                                self.config.theme,
                                            ),
                                        )
                                        .clicked()
                                    {
                                        apply_single = true;
                                    }
                                    if ui.button("Remove").clicked() {
                                        delete_current = true;
                                    }
                                },
                            );
                        });
                    });

                    ui.add_space(10.0);
                    identity_card(ui, ext_cfg, self.config.theme, &mut open_gallery_picker);
                    ui.add_space(10.0);
                    behavior_card(ui, ext_cfg, self.config.theme);
                    ui.add_space(10.0);
                    verbs_card(ui, ext_cfg, self.config.theme);
                });

                if open_gallery_picker {
                    self.assign_icon_to_selected_ext = true;
                    self.active_tab = ActiveTab::Gallery;
                }

                if apply_single {
                    let mut single = self.config.extensions[self.selected_ext_index].clone();
                    single.enabled = true;
                    self.start_apply(ctx, vec![single]);
                }

                if delete_current && !self.config.extensions.is_empty() {
                    self.config.extensions.remove(self.selected_ext_index);
                    if self.selected_ext_index >= self.config.extensions.len()
                        && !self.config.extensions.is_empty()
                    {
                        self.selected_ext_index = self.config.extensions.len() - 1;
                    }
                }
            });
    }
}

fn identity_card(
    ui: &mut egui::Ui,
    ext_cfg: &mut ExtensionConfig,
    theme: crate::theme::ThemeMode,
    open_gallery_picker: &mut bool,
) {
    card_frame(theme).show(ui, |ui| {
        ui.label(
            RichText::new("Identity & File Properties")
                .size(15.0)
                .strong(),
        );
        ui.add_space(6.0);

        egui::Grid::new("ext_identity_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                ui.label("File Extension:");
                ui.text_edit_singleline(&mut ext_cfg.extension);
                ui.end_row();

                ui.label("Custom ProgID:");
                ui.text_edit_singleline(&mut ext_cfg.prog_id);
                ui.end_row();

                ui.label("Type Description:");
                ui.text_edit_singleline(&mut ext_cfg.description);
                ui.end_row();

                ui.label("Assigned Icon:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut ext_cfg.icon_name);
                    if ui.button("Pick from Gallery...").clicked() {
                        *open_gallery_picker = true;
                    }
                });
                ui.end_row();

                ui.label("PerceivedType (Preview Pane):");
                ui.text_edit_singleline(&mut ext_cfg.perceived_type);
                ui.end_row();

                ui.label("MIME Content Type:");
                ui.text_edit_singleline(&mut ext_cfg.content_type);
                ui.end_row();
            });
    });
}

fn behavior_card(ui: &mut egui::Ui, ext_cfg: &mut ExtensionConfig, theme: crate::theme::ThemeMode) {
    card_frame(theme).show(ui, |ui| {
        ui.label(
            RichText::new("Registry Behavior & Execution")
                .size(15.0)
                .strong(),
        );
        ui.add_space(6.0);

        ui.checkbox(
            &mut ext_cfg.preserve_existing_properties,
            "Preserve native extension properties (OpenWithProgids, ShellNew, Shellex)",
        );
        ui.checkbox(
            &mut ext_cfg.reset_existing,
            "Clean per-user HKCU\\Software\\Classes overrides",
        );
        ui.checkbox(
            &mut ext_cfg.reset_user_choice,
            "Reset Explorer UserChoice override (FileExts)",
        );

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Double-Click Action:").strong());
            ui.radio_value(
                &mut ext_cfg.mode,
                DoubleClickMode::OpenInEditor,
                "Open in Code Editor",
            );
            ui.radio_value(
                &mut ext_cfg.mode,
                DoubleClickMode::ExecuteScript,
                "Execute Script (Put Editor under Right-Click 'Edit')",
            );
        });

        if ext_cfg.mode == DoubleClickMode::ExecuteScript {
            ui.add_space(6.0);
            let launcher_str = ext_cfg
                .launcher_command
                .get_or_insert_with(|| "\"%1\" %*".to_string());
            ui.horizontal(|ui| {
                ui.label("Launch Command (open):");
                ui.text_edit_singleline(launcher_str);
            });

            let mut has_elevated = ext_cfg.elevated_command.is_some();
            if ui
                .checkbox(
                    &mut has_elevated,
                    "Enable 'Run as Administrator' context menu verb (runas)",
                )
                .changed()
            {
                ext_cfg.elevated_command = has_elevated.then(|| "cmd.exe /C \"%1\" %*".to_string());
            }
            if let Some(elev_cmd) = &mut ext_cfg.elevated_command {
                ui.horizontal(|ui| {
                    ui.label("Administrator Command (runas):");
                    ui.text_edit_singleline(elev_cmd);
                });
            }
        }
    });
}

fn verbs_card(ui: &mut egui::Ui, ext_cfg: &mut ExtensionConfig, theme: crate::theme::ThemeMode) {
    let palette = get_palette(theme);
    card_frame(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Custom Context Menu Verbs (Right-Click)")
                    .size(15.0)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ Add Context Verb").clicked() {
                    ext_cfg.extra_verbs.push(CustomContextCommand {
                        verb_key: "open_terminal".to_string(),
                        menu_label: "Open in Windows Terminal".to_string(),
                        command: "wt.exe -d \"%V\"".to_string(),
                    });
                }
            });
        });

        if ext_cfg.extra_verbs.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("No extra context menu actions configured.")
                    .color(palette.text_secondary),
            );
        }

        let mut remove_verb_idx = None;
        for (v_idx, verb) in ext_cfg.extra_verbs.iter_mut().enumerate() {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Registry Key:");
                    ui.add(egui::TextEdit::singleline(&mut verb.verb_key).desired_width(110.0));
                    ui.label("Menu Label:");
                    ui.add(egui::TextEdit::singleline(&mut verb.menu_label).desired_width(180.0));
                    if ui.button("Remove").clicked() {
                        remove_verb_idx = Some(v_idx);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Command:");
                    ui.text_edit_singleline(&mut verb.command);
                });
            });
        }
        if let Some(idx) = remove_verb_idx {
            ext_cfg.extra_verbs.remove(idx);
        }
    });
}
