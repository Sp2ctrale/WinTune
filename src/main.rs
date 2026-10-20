#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod detector;
mod gallery;
mod models;
mod registry;

use detector::{has_wsl_distribution, resolve_vscode_executable};
use eframe::egui;
use gallery::{build_compressed_pak, IconCategory, IconGallery};
use models::{AppConfig, BackupStore, CustomContextCommand, DoubleClickMode, ExtensionConfig};
use registry::{
    apply_extensions, load_backup_store, notify_shell_associations, rollback_selected_extensions,
};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(PartialEq, Eq)]
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
}

impl WinTuneApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut config = Self::load_or_init_config();
        if config.code_executable.is_empty() {
            if let Some(found) = resolve_vscode_executable(None) {
                config.code_executable = found;
            }
        }

        let mut gallery = IconGallery::default();
        gallery.reload(&config.icon_source_folder, &config.pak_file_path);

        let backup_store = load_backup_store();
        let wsl_detected = has_wsl_distribution();

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
            status_message: "Prêt.".to_string(),
            wsl_detected,
            new_ext_input: String::new(),
        }
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
                self.status_message = "Configuration sauvegardée.".to_string();
            }
        }
    }
}

impl eframe::App for WinTuneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_nav").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("WinTune");
                ui.separator();
                if ui
                    .selectable_label(
                        self.active_tab == ActiveTab::Associations,
                        "1. Extensions & Propriétés",
                    )
                    .clicked()
                {
                    self.active_tab = ActiveTab::Associations;
                }
                if ui
                    .selectable_label(
                        self.active_tab == ActiveTab::Gallery,
                        format!("2. Galerie d'icônes ({})", self.gallery.names.len()),
                    )
                    .clicked()
                {
                    self.active_tab = ActiveTab::Gallery;
                }
                if ui
                    .selectable_label(
                        self.active_tab == ActiveTab::Rollback,
                        format!("3. Rollback sélectif ({})", self.backup_store.backups.len()),
                    )
                    .clicked()
                {
                    self.backup_store = load_backup_store();
                    self.active_tab = ActiveTab::Rollback;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Rafraîchir l'Explorateur").clicked() {
                        notify_shell_associations();
                        self.status_message =
                            "Explorateur Windows notifié (SHChangeNotify).".to_string();
                    }
                    if ui.button("Sauvegarder Config").clicked() {
                        self.save_config();
                    }
                });
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("bottom_status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Statut : {}", self.status_message));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let wsl_txt = if self.wsl_detected {
                        "WSL : Détecté"
                    } else {
                        "WSL : Non détecté (.sh -> VS Code)"
                    };
                    ui.label(wsl_txt);
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
    fn render_associations_tab(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("global_paths_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Éditeur (Code.exe) :");
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.code_executable)
                        .desired_width(340.0),
                );
                if ui.button("Parcourir...").clicked() {
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("Exécutable", &["exe"])
                        .pick_file()
                    {
                        self.config.code_executable = file.to_string_lossy().to_string();
                    }
                }
                if ui.button("Auto-détecter").clicked() {
                    if let Some(found) = resolve_vscode_executable(None) {
                        self.config.code_executable = found;
                        self.status_message = "Code.exe détecté automatiquement.".to_string();
                    } else {
                        self.status_message = "Impossible de localiser Code.exe.".to_string();
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Appliquer les extensions cochées").clicked() {
                        let (ok_count, errs) = apply_extensions(
                            &self.config.extensions,
                            &self.config.code_executable,
                            &self.config.icon_destination_folder,
                            &self.gallery,
                        );
                        self.backup_store = load_backup_store();
                        self.save_config();
                        if errs.is_empty() {
                            self.status_message =
                                format!("{} extensions appliquées avec succès.", ok_count);
                        } else {
                            self.status_message = format!(
                                "{} appliquées, {} erreurs (ex: {}).",
                                ok_count,
                                errs.len(),
                                errs[0]
                            );
                        }
                    }
                });
            });
            ui.add_space(4.0);
        });

        egui::SidePanel::left("extensions_list_panel")
            .resizable(true)
            .default_width(310.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Filtrer :");
                    ui.text_edit_singleline(&mut self.ext_search_query);
                });

                ui.horizontal(|ui| {
                    if ui.button("Tout cocher").clicked() {
                        for e in &mut self.config.extensions {
                            e.enabled = true;
                        }
                    }
                    if ui.button("Tout décocher").clicked() {
                        for e in &mut self.config.extensions {
                            e.enabled = false;
                        }
                    }
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_ext_input)
                            .hint_text(".zig, .astro...")
                            .desired_width(140.0),
                    );
                    if ui.button("+ Ajouter extension").clicked() {
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
                        let tex = self.gallery.get_or_load_texture(ctx, &icon_name);

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.config.extensions[idx].enabled, "");
                            if let Some(t) = tex {
                                ui.image((t.id(), egui::vec2(20.0, 20.0)));
                            }
                            let label_text = format!(
                                "{}  —  {}",
                                self.config.extensions[idx].extension,
                                self.config.extensions[idx].description
                            );
                            if ui.selectable_label(is_selected, label_text).clicked() {
                                self.selected_ext_index = idx;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.selected_ext_index >= self.config.extensions.len() {
                return;
            }

            let current_icon_name = self.config.extensions[self.selected_ext_index].icon_name.clone();
            let current_tex = self.gallery.get_or_load_texture(ctx, &current_icon_name);

            let mut delete_current = false;
            let mut open_gallery_picker = false;
            let mut apply_single = false;

            egui::ScrollArea::vertical().show(ui, |ui| {
                let ext_cfg = &mut self.config.extensions[self.selected_ext_index];

                ui.horizontal(|ui| {
                    if let Some(t) = current_tex {
                        ui.image((t.id(), egui::vec2(48.0, 48.0)));
                    }
                    ui.vertical(|ui| {
                        ui.heading(format!("Configuration de {}", ext_cfg.extension));
                        ui.label(format!("ProgID dédié : {}", ext_cfg.prog_id));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Supprimer de la liste").clicked() {
                            delete_current = true;
                        }
                        if ui.button("Appliquer uniquement cette extension").clicked() {
                            apply_single = true;
                        }
                    });
                });

                ui.separator();

                egui::Grid::new("ext_props_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("Extension :");
                        ui.text_edit_singleline(&mut ext_cfg.extension);
                        ui.end_row();

                        ui.label("ProgId Windows :");
                        ui.text_edit_singleline(&mut ext_cfg.prog_id);
                        ui.end_row();

                        ui.label("Description du type :");
                        ui.text_edit_singleline(&mut ext_cfg.description);
                        ui.end_row();

                        ui.label("Icône associée (.ico) :");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut ext_cfg.icon_name);
                            if ui.button("Choisir dans la Galerie...").clicked() {
                                open_gallery_picker = true;
                            }
                        });
                        ui.end_row();

                        ui.label("PerceivedType (Aperçu Explorateur) :");
                        ui.text_edit_singleline(&mut ext_cfg.perceived_type);
                        ui.end_row();

                        ui.label("Content Type (MIME) :");
                        ui.text_edit_singleline(&mut ext_cfg.content_type);
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Comportement et Propriétés Registre");
                ui.add_space(4.0);

                ui.checkbox(
                    &mut ext_cfg.preserve_existing_properties,
                    "Conserver les propriétés natives de l'extension (OpenWithProgids, ShellNew, Shellex)",
                );
                ui.checkbox(
                    &mut ext_cfg.reset_existing,
                    "Nettoyer les remplacements HKCU\\Software\\Classes de cette extension",
                );
                ui.checkbox(
                    &mut ext_cfg.reset_user_choice,
                    "Réinitialiser le choix utilisateur (FileExts\\UserChoice)",
                );

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Action au double-clic :");
                    ui.radio_value(
                        &mut ext_cfg.mode,
                        DoubleClickMode::OpenInEditor,
                        "Ouvrir dans l'éditeur (VS Code)",
                    );
                    ui.radio_value(
                        &mut ext_cfg.mode,
                        DoubleClickMode::ExecuteScript,
                        "Exécuter le script (et placer VS Code dans 'Modifier')",
                    );
                });

                if ext_cfg.mode == DoubleClickMode::ExecuteScript {
                    let launcher_str = ext_cfg
                        .launcher_command
                        .get_or_insert_with(|| "\"%1\" %*".to_string());
                    ui.horizontal(|ui| {
                        ui.label("Commande d'exécution (open) :");
                        ui.text_edit_singleline(launcher_str);
                    });

                    let mut has_elevated = ext_cfg.elevated_command.is_some();
                    if ui
                        .checkbox(
                            &mut has_elevated,
                            "Ajouter 'Exécuter en tant qu'administrateur' (runas)",
                        )
                        .changed()
                    {
                        if has_elevated {
                            ext_cfg.elevated_command = Some("cmd.exe /C \"%1\" %*".to_string());
                        } else {
                            ext_cfg.elevated_command = None;
                        }
                    }
                    if let Some(elev_cmd) = &mut ext_cfg.elevated_command {
                        ui.horizontal(|ui| {
                            ui.label("Commande Administrateur (runas) :");
                            ui.text_edit_singleline(elev_cmd);
                        });
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading("Actions de menu contextuel supplémentaires (Clic droit)");
                    if ui.button("+ Ajouter une action").clicked() {
                        ext_cfg.extra_verbs.push(CustomContextCommand {
                            verb_key: "custom_action".to_string(),
                            menu_label: "Ouvrir dans le terminal".to_string(),
                            command: "wt.exe -d \"%V\"".to_string(),
                        });
                    }
                });

                let mut remove_verb_idx = None;
                for (v_idx, verb) in ext_cfg.extra_verbs.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Clé :");
                            ui.add(
                                egui::TextEdit::singleline(&mut verb.verb_key).desired_width(100.0),
                            );
                            ui.label("Libellé menu :");
                            ui.add(
                                egui::TextEdit::singleline(&mut verb.menu_label)
                                    .desired_width(160.0),
                            );
                            if ui.button("X").clicked() {
                                remove_verb_idx = Some(v_idx);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Commande :");
                            ui.text_edit_singleline(&mut verb.command);
                        });
                    });
                }
                if let Some(idx) = remove_verb_idx {
                    ext_cfg.extra_verbs.remove(idx);
                }
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
                        "Extension {} appliquée avec succès.",
                        self.config.extensions[self.selected_ext_index].extension
                    );
                } else if let Some(err) = errs.first() {
                    self.status_message = format!("Erreur : {}", err);
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
        egui::TopBottomPanel::top("gallery_controls").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Dossier source (.ico) :");
                ui.add(
                    egui::TextEdit::singleline(&mut self.config.icon_source_folder)
                        .desired_width(320.0),
                );
                if ui.button("Changer dossier...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.config.icon_source_folder = folder.to_string_lossy().to_string();
                        self.gallery
                            .reload(&self.config.icon_source_folder, &self.config.pak_file_path);
                    }
                }
                if ui.button("Importer des .ico...").clicked() {
                    if let Some(files) = rfd::FileDialog::new()
                        .add_filter("Icônes Windows", &["ico"])
                        .pick_files()
                    {
                        let _ = fs::create_dir_all(&self.config.icon_source_folder);
                        let mut imported = 0;
                        for f in files {
                            if let Some(name) = f.file_name() {
                                let dest = Path::new(&self.config.icon_source_folder).join(name);
                                if fs::copy(&f, &dest).is_ok() {
                                    imported += 1;
                                }
                            }
                        }
                        self.gallery
                            .reload(&self.config.icon_source_folder, &self.config.pak_file_path);
                        self.status_message = format!("{} fichiers .ico importés.", imported);
                    }
                }

                if ui
                    .button("Générer gallery.pak compressé (Optimiser 645 Mo)")
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
                                "Pack créé ({} icônes) : {:.1} Mo -> {:.1} Mo",
                                count,
                                before as f64 / 1_048_576.0,
                                after as f64 / 1_048_576.0
                            );
                        }
                        Err(e) => {
                            self.status_message = format!("Erreur compression pak : {}", e);
                        }
                    }
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Recherche :");
                ui.add(
                    egui::TextEdit::singleline(&mut self.gallery_search_query)
                        .hint_text("rust, react, folder-src, _light...")
                        .desired_width(240.0),
                );

                ui.separator();
                ui.selectable_value(&mut self.gallery_category, IconCategory::All, "Tout");
                ui.selectable_value(
                    &mut self.gallery_category,
                    IconCategory::FilesOnly,
                    "Fichiers",
                );
                ui.selectable_value(
                    &mut self.gallery_category,
                    IconCategory::FoldersOnly,
                    "Dossiers (folder-*)",
                );
                ui.selectable_value(
                    &mut self.gallery_category,
                    IconCategory::LightVariants,
                    "Thème Clair (*_light)",
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
                    ui.colored_label(
                        egui::Color32::LIGHT_GREEN,
                        format!(
                            "Clique sur une icône pour l'assigner à {}",
                            self.config.extensions[self.selected_ext_index].extension
                        ),
                    );
                }
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let filtered = self
                .gallery
                .filter_names(&self.gallery_search_query, self.gallery_category);

            let avail_width = ui.available_width();
            let item_width = 125.0;
            let item_height = 92.0;
            let cols = ((avail_width / item_width).floor() as usize).max(1);
            let total_rows = (filtered.len() + cols - 1) / cols;

            let mut chosen_icon: Option<String> = None;

            egui::ScrollArea::vertical().show_rows(ui, item_height, total_rows, |ui, row_range| {
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
                                        ui.group(|ui| {
                                            ui.set_min_size(egui::vec2(
                                                item_width - 16.0,
                                                item_height - 14.0,
                                            ));
                                            if let Some(t) = tex {
                                                if ui
                                                    .add(
                                                        egui::ImageButton::new((
                                                            t.id(),
                                                            egui::vec2(40.0, 40.0),
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
                                            ui.label(egui::RichText::new(name).size(11.0));
                                        });
                                    },
                                );
                            }
                        }
                    });
                }
            });

            if let Some(icon_name) = chosen_icon {
                if self.selected_ext_index < self.config.extensions.len() {
                    self.config.extensions[self.selected_ext_index].icon_name = icon_name.clone();
                    self.status_message = format!(
                        "Icône '{}' sélectionnée pour {}.",
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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Restauration sélective de l'état d'origine (Rollback)");
            ui.label("Sélectionne uniquement les extensions que tu souhaites remettre à leur état Windows initial sans toucher aux autres.");
            ui.add_space(8.0);

            if self.backup_store.backups.is_empty() {
                ui.label("Aucune sauvegarde active pour le moment. Les snapshots sont créés automatiquement avant la première modification d'une extension.");
                return;
            }

            ui.horizontal(|ui| {
                if ui.button("Tout sélectionner").clicked() {
                    self.rollback_selection =
                        self.backup_store.backups.keys().cloned().collect();
                }
                if ui.button("Tout désélectionner").clicked() {
                    self.rollback_selection.clear();
                }

                if ui
                    .button(format!(
                        "Restaurer les {} extensions cochées",
                        self.rollback_selection.len()
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
                            "{} extensions restaurées à leur état d'origine.",
                            restored
                        );
                    } else {
                        self.status_message = format!(
                            "{} restaurées, {} erreurs.",
                            restored,
                            errs.len()
                        );
                    }
                }
            });

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("rollback_grid")
                    .striped(true)
                    .num_columns(4)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        ui.strong("Restaurer");
                        ui.strong("Extension");
                        ui.strong("ProgID Custom Actif");
                        ui.strong("ProgID Windows Initial sauvegardé");
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
                            ui.label(ext);
                            ui.label(&bkp.custom_prog_id);
                            let orig = bkp
                                .hklm_extension_key
                                .default_value
                                .as_deref()
                                .unwrap_or("(Aucun / Clé vierge)");
                            ui.label(orig);
                            ui.end_row();
                        }
                    });
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let icon_bytes = include_bytes!("../assets/WinTune.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("Impossible de charger l'icône WinTune.png")
        .into_rgba8();

    let (width, height) = image.dimensions();
    let icon = egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([860.0, 540.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "WinTune",
        options,
        Box::new(|cc| Ok(Box::new(WinTuneApp::new(cc)))),
    )
}
