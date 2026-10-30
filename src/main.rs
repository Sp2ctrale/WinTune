#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod detector;
mod gallery;
mod models;
mod registry;
mod theme;

use detector::{has_wsl_distribution, resolve_vscode_executable};
use eframe::egui::{
    self, ColorImage, Margin, RichText, Rounding, Stroke, TextureHandle, TextureOptions,
};
use gallery::{build_compressed_pak, IconCategory, IconGallery};
use models::{AppConfig, BackupStore, CustomContextCommand, DoubleClickMode, ExtensionConfig};
use registry::{
    apply_extensions, load_backup_store, notify_shell_associations, rollback_selected_extensions,
};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use theme::{
    accent_button, apply_fluent_theme, card_frame, draw_badge, get_palette, sidebar_frame,
    ThemeMode,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(PartialEq, Eq, Clone, Copy)]
enum ActiveTab {
    Associations,
    Gallery,
    Rollback,
}

struct WinTuneApp {
    config: AppConfig,
    gallery: IconGallery,
    backup_store: BackupStore,
    active_tab: ActiveTab,
    selected_ext_index: usize,
    ext_search_query: String,
    gallery_search_query: String,
    gallery_category: IconCategory,
    assign_icon_to_selected_ext: bool,
    rollback_selection: HashSet<String>,
    status_message: String,
    wsl_detected: bool,
    new_ext_input: String,
    logo_texture: Option<TextureHandle>,
}

impl WinTuneApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut config = Self::load_or_init_config();
        if config.code_executable.is_empty() {
            if let Some(found) = resolve_vscode_executable(None) {
                config.code_executable = found;
            }
        }

        apply_fluent_theme(&cc.egui_ctx, config.theme);

        let mut gallery = IconGallery::default();
        gallery.reload(&config.icon_source_folder, &config.pak_file_path);

        let backup_store = load_backup_store();
        let wsl_detected = has_wsl_distribution();
        let logo_texture = Self::load_app_logo(&cc.egui_ctx);

        Self {
            config,
            gallery,
            backup_store,
            active_tab: ActiveTab::Associations,
            selected_ext_index: 0,
            ext_search_query: String::new(),
            gallery_search_query: String::new(),
            gallery_category: IconCategory::FilesOnly,
            assign_icon_to_selected_ext: false,
            rollback_selection: HashSet::new(),
            status_message: "Ready.".to_string(),
            wsl_detected,
            new_ext_input: String::new(),
            logo_texture,
        }
    }

    fn load_app_logo(ctx: &egui::Context) -> Option<TextureHandle> {
        let png_bytes = include_bytes!("../assets/WinTune.png");
        if let Ok(img) = image::load_from_memory(png_bytes) {
            let rgba = img.thumbnail(64, 64).to_rgba8();
            let (w, h) = rgba.dimensions();
            let color_img =
                ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            return Some(ctx.load_texture("wintune_logo", color_img, TextureOptions::LINEAR));
        }
        None
    }

    fn config_path() -> String {
        let program_data =
            std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
        format!("{}\\Wintune\\config.json", program_data)
    }

    fn load_or_init_config() -> AppConfig {
        let path = Self::config_path();
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<AppConfig>(&content) {
                return cfg;
            }
        }
        AppConfig::default()
    }

    fn save_config(&mut self) {
        let path = Self::config_path();
        if let Some(parent) = Path::new(&path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.config) {
            if fs::write(&path, json).is_ok() {
                self.status_message = "Configuration saved.".to_string();
            }
        }
    }
}

impl eframe::App for WinTuneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

                self.sidebar_nav_button(
                    ui,
                    ActiveTab::Associations,
                    "File Associations",
                    &format!("{} rules", self.config.extensions.len()),
                );
                self.sidebar_nav_button(
                    ui,
                    ActiveTab::Gallery,
                    "Icon Gallery",
                    &format!("{} icons", self.gallery.names.len()),
                );
                self.sidebar_nav_button(
                    ui,
                    ActiveTab::Rollback,
                    "Selective Rollback",
                    &format!("{} backups", self.backup_store.backups.len()),
                );

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(4.0);
                    let wsl_badge_color = if self.wsl_detected {
                        palette.success
                    } else {
                        palette.warning
                    };
                    let wsl_label = if self.wsl_detected {
                        "WSL2 Active"
                    } else {
                        "No WSL (.sh -> Editor)"
                    };
                    draw_badge(ui, wsl_label, wsl_badge_color);

                    ui.add_space(8.0);
                    if ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("Refresh Windows Explorer"),
                        )
                        .clicked()
                    {
                        notify_shell_associations();
                        self.status_message =
                            "Windows Explorer refreshed via SHChangeNotify.".to_string();
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
                                self.config.theme = ThemeMode::Light;
                                apply_fluent_theme(ctx, ThemeMode::Light);
                                self.save_config();
                            }
                            if ui.selectable_label(is_dark, "Dark").clicked() {
                                self.config.theme = ThemeMode::Dark;
                                apply_fluent_theme(ctx, ThemeMode::Dark);
                                self.save_config();
                            }
                        });
                    });
                });
            });

        egui::TopBottomPanel::bottom("fluent_status_bar")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_sidebar)
                    .stroke(Stroke::new(1.0_f32, palette.border))
                    .inner_margin(Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.status_message)
                            .size(12.0)
                            .color(palette.text_secondary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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

        match self.active_tab {
            ActiveTab::Associations => self.render_associations_tab(ctx),
            ActiveTab::Gallery => self.render_gallery_tab(ctx),
            ActiveTab::Rollback => self.render_rollback_tab(ctx),
        }
    }
}

impl WinTuneApp {
    fn sidebar_nav_button(
        &mut self,
        ui: &mut egui::Ui,
        tab: ActiveTab,
        title: &str,
        subtitle: &str,
    ) {
        let palette = get_palette(self.config.theme);
        let is_active = self.active_tab == tab;
        let fill = if is_active {
            palette.accent.gamma_multiply(0.22)
        } else {
            egui::Color32::TRANSPARENT
        };
        let stroke = if is_active {
            Stroke::new(1.0_f32, palette.accent)
        } else {
            Stroke::NONE
        };

        let resp = egui::Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title)
                            .size(13.5)
                            .strong()
                            .color(if is_active {
                                palette.accent_hover
                            } else {
                                palette.text_primary
                            }),
                    );
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
            if tab == ActiveTab::Rollback {
                self.backup_store = load_backup_store();
            }
            self.active_tab = tab;
        }
        ui.add_space(4.0);
    }

    fn render_associations_tab(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);

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
                                self.status_message =
                                    "VS Code path resolved automatically.".to_string();
                            } else {
                                self.status_message =
                                    "Could not locate Code.exe automatically.".to_string();
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(accent_button(
                                    "Apply Checked Extensions",
                                    self.config.theme,
                                ))
                                .clicked()
                            {
                                let (ok_count, errs) = apply_extensions(
                                    &self.config.extensions,
                                    &self.config.code_executable,
                                    &self.config.icon_destination_folder,
                                    &self.gallery,
                                );
                                self.backup_store = load_backup_store();
                                self.save_config();
                                if errs.is_empty() {
                                    self.status_message = format!(
                                        "Successfully applied {} file associations.",
                                        ok_count
                                    );
                                } else {
                                    self.status_message = format!(
                                        "Applied {} extensions, {} errors (first: {}).",
                                        ok_count,
                                        errs.len(),
                                        errs[0]
                                    );
                                }
                            }
                        });
                    });
                });
            });

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
                            let mut ext = self.new_ext_input.trim().to_lowercase();
                            if !ext.is_empty() {
                                if !ext.starts_with('.') {
                                    ext = format!(".{}", ext);
                                }
                                if !self.config.extensions.iter().any(|e| e.extension == ext) {
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
                            }
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
                            let icon_name = self.config.extensions[idx].icon_name.clone();
                            let is_script =
                                self.config.extensions[idx].mode == DoubleClickMode::ExecuteScript;
                            let tex = self.gallery.get_or_load_texture(ctx, &icon_name);

                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.config.extensions[idx].enabled, "");
                                if let Some(t) = tex {
                                    ui.image((t.id(), egui::vec2(20.0, 20.0)));
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

                let current_icon_name =
                    self.config.extensions[self.selected_ext_index].icon_name.clone();
                let current_tex = self.gallery.get_or_load_texture(ctx, &current_icon_name);
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
                                        .add(accent_button("Apply This Extension", self.config.theme))
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

                    card_frame(self.config.theme).show(ui, |ui| {
                        ui.label(RichText::new("Identity & File Properties").size(15.0).strong());
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
                                        open_gallery_picker = true;
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

                    ui.add_space(10.0);

                    card_frame(self.config.theme).show(ui, |ui| {
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
                                if has_elevated {
                                    ext_cfg.elevated_command =
                                        Some("cmd.exe /C \"%1\" %*".to_string());
                                } else {
                                    ext_cfg.elevated_command = None;
                                }
                            }
                            if let Some(elev_cmd) = &mut ext_cfg.elevated_command {
                                ui.horizontal(|ui| {
                                    ui.label("Administrator Command (runas):");
                                    ui.text_edit_singleline(elev_cmd);
                                });
                            }
                        }
                    });

                    ui.add_space(10.0);

                    card_frame(self.config.theme).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Custom Context Menu Verbs (Right-Click)")
                                    .size(15.0)
                                    .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("+ Add Context Verb").clicked() {
                                        ext_cfg.extra_verbs.push(CustomContextCommand {
                                            verb_key: "open_terminal".to_string(),
                                            menu_label: "Open in Windows Terminal".to_string(),
                                            command: "wt.exe -d \"%V\"".to_string(),
                                        });
                                    }
                                },
                            );
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
                                    ui.add(
                                        egui::TextEdit::singleline(&mut verb.verb_key)
                                            .desired_width(110.0),
                                    );
                                    ui.label("Menu Label:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut verb.menu_label)
                                            .desired_width(180.0),
                                    );
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
                });

                if open_gallery_picker {
                    self.assign_icon_to_selected_ext = true;
                    self.active_tab = ActiveTab::Gallery;
                }

                if apply_single {
                    let mut single = self.config.extensions[self.selected_ext_index].clone();
                    single.enabled = true;
                    let (ok, errs) = apply_extensions(
                        &[single],
                        &self.config.code_executable,
                        &self.config.icon_destination_folder,
                        &self.gallery,
                    );
                    self.backup_store = load_backup_store();
                    self.save_config();
                    if ok == 1 {
                        self.status_message = format!(
                            "Applied {} successfully.",
                            self.config.extensions[self.selected_ext_index].extension
                        );
                    } else if let Some(err) = errs.first() {
                        self.status_message = format!("Error: {}", err);
                    }
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

    fn render_gallery_tab(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);

        egui::TopBottomPanel::top("gallery_header")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.config.theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Icon Source Folder:").strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut self.config.icon_source_folder)
                                .desired_width(280.0),
                        );
                        if ui.button("Change Folder...").clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.config.icon_source_folder =
                                    folder.to_string_lossy().to_string();
                                self.gallery.reload(
                                    &self.config.icon_source_folder,
                                    &self.config.pak_file_path,
                                );
                            }
                        }
                        if ui.button("Import .ico Files...").clicked() {
                            if let Some(files) = rfd::FileDialog::new()
                                .add_filter("Windows Icons", &["ico"])
                                .pick_files()
                            {
                                let _ = fs::create_dir_all(&self.config.icon_source_folder);
                                let mut imported = 0;
                                for f in files {
                                    if let Some(name) = f.file_name() {
                                        let dest =
                                            Path::new(&self.config.icon_source_folder).join(name);
                                        if fs::copy(&f, &dest).is_ok() {
                                            imported += 1;
                                        }
                                    }
                                }
                                self.gallery.reload(
                                    &self.config.icon_source_folder,
                                    &self.config.pak_file_path,
                                );
                                self.status_message =
                                    format!("Imported {} .ico files into gallery.", imported);
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(accent_button(
                                    "Build Compressed gallery.pak",
                                    self.config.theme,
                                ))
                                .clicked()
                            {
                                match build_compressed_pak(
                                    &self.config.icon_source_folder,
                                    &self.config.pak_file_path,
                                ) {
                                    Ok((count, before, after)) => {
                                        self.gallery.reload(
                                            &self.config.icon_source_folder,
                                            &self.config.pak_file_path,
                                        );
                                        self.status_message = format!(
                                            "Packed {} icons: {:.1} MB -> {:.1} MB",
                                            count,
                                            before as f64 / 1_048_576.0,
                                            after as f64 / 1_048_576.0
                                        );
                                    }
                                    Err(e) => {
                                        self.status_message = format!("Pack error: {}", e);
                                    }
                                }
                            }
                        });
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Search:").strong());
                        ui.add(
                            egui::TextEdit::singleline(&mut self.gallery_search_query)
                                .hint_text("Filter icons (rust, react, folder-src, _light...)")
                                .desired_width(240.0),
                        );

                        ui.separator();
                        ui.selectable_value(&mut self.gallery_category, IconCategory::All, "All");
                        ui.selectable_value(
                            &mut self.gallery_category,
                            IconCategory::FilesOnly,
                            "Files",
                        );
                        ui.selectable_value(
                            &mut self.gallery_category,
                            IconCategory::FoldersOnly,
                            "Folders (folder-*)",
                        );
                        ui.selectable_value(
                            &mut self.gallery_category,
                            IconCategory::LightVariants,
                            "Light (*_light)",
                        );
                        ui.selectable_value(
                            &mut self.gallery_category,
                            IconCategory::Clones,
                            "Clones (*.clone)",
                        );

                        if self.assign_icon_to_selected_ext
                            && self.selected_ext_index < self.config.extensions.len()
                        {
                            ui.separator();
                            draw_badge(
                                ui,
                                &format!(
                                    "Selecting icon for {}",
                                    self.config.extensions[self.selected_ext_index].extension
                                ),
                                palette.success,
                            );
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 4.0)),
            )
            .show(ctx, |ui| {
                let filtered = self
                    .gallery
                    .filter_names(&self.gallery_search_query, self.gallery_category);

                let avail_width = ui.available_width();
                let item_width = 130.0;
                let item_height = 98.0;
                let cols = ((avail_width / item_width).floor() as usize).max(1);
                let total_rows = (filtered.len() + cols - 1) / cols;

                let mut chosen_icon: Option<String> = None;

                egui::ScrollArea::vertical().show_rows(
                    ui,
                    item_height,
                    total_rows,
                    |ui, row_range| {
                        for row in row_range {
                            ui.horizontal(|ui| {
                                for col in 0..cols {
                                    let idx = row * cols + col;
                                    if idx < filtered.len() {
                                        let name = &filtered[idx];
                                        let tex = self.gallery.get_or_load_texture(ctx, name);

                                        ui.allocate_ui_with_layout(
                                            egui::vec2(item_width - 8.0, item_height - 6.0),
                                            egui::Layout::top_down(egui::Align::Center),
                                            |ui| {
                                                card_frame(self.config.theme).show(ui, |ui| {
                                                    ui.set_min_size(egui::vec2(
                                                        item_width - 28.0,
                                                        item_height - 28.0,
                                                    ));
                                                    ui.vertical_centered(|ui| {
                                                        if let Some(t) = tex {
                                                            if ui
                                                                .add(
                                                                    egui::ImageButton::new((
                                                                        t.id(),
                                                                        egui::vec2(42.0, 42.0),
                                                                    ))
                                                                    .frame(false),
                                                                )
                                                                .clicked()
                                                            {
                                                                chosen_icon = Some(name.clone());
                                                            }
                                                        } else if ui.button("?").clicked() {
                                                            chosen_icon = Some(name.clone());
                                                        }
                                                        ui.label(
                                                            RichText::new(name)
                                                                .size(11.0)
                                                                .color(palette.text_secondary),
                                                        );
                                                    });
                                                });
                                            },
                                        );
                                    }
                                }
                            });
                        }
                    },
                );

                if let Some(icon_name) = chosen_icon {
                    if self.selected_ext_index < self.config.extensions.len() {
                        self.config.extensions[self.selected_ext_index].icon_name =
                            icon_name.clone();
                        self.status_message = format!(
                            "Assigned icon '{}' to {}.",
                            icon_name, self.config.extensions[self.selected_ext_index].extension
                        );
                        if self.assign_icon_to_selected_ext {
                            self.assign_icon_to_selected_ext = false;
                            self.active_tab = ActiveTab::Associations;
                        }
                    }
                }
            });
    }

    fn render_rollback_tab(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);

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
                            if ui
                                .add(accent_button(
                                    format!("Restore Selected ({})", self.rollback_selection.len()),
                                    self.config.theme,
                                ))
                                .clicked()
                                && !self.rollback_selection.is_empty()
                            {
                                let to_restore: Vec<String> =
                                    self.rollback_selection.iter().cloned().collect();
                                let (restored, errs) = rollback_selected_extensions(&to_restore);
                                self.backup_store = load_backup_store();
                                self.rollback_selection.clear();
                                if errs.is_empty() {
                                    self.status_message = format!(
                                        "Restored {} extensions to their initial state.",
                                        restored
                                    );
                                } else {
                                    self.status_message = format!(
                                        "Restored {} extensions, {} errors.",
                                        restored,
                                        errs.len()
                                    );
                                }
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
                            .num_columns(4)
                            .spacing([28.0, 10.0])
                            .show(ui, |ui| {
                                ui.strong("Restore");
                                ui.strong("Extension");
                                ui.strong("Active Custom ProgID");
                                ui.strong("Original Windows ProgID");
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
                                    ui.label(
                                        RichText::new(orig).color(palette.text_secondary),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let icon_bytes = include_bytes!("../assets/WinTune.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load WinTune.png icon")
        .into_rgba8();

    let (width, height) = image.dimensions();
    let icon = egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("WinTune v{}", APP_VERSION))
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([920.0, 580.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "WinTune",
        options,
        Box::new(|cc| Ok(Box::new(WinTuneApp::new(cc)))),
    )
}
