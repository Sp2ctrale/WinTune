use super::{ActiveTab, WinTuneApp};
use crate::gallery::IconCategory;
use crate::theme::{accent_button, card_frame, draw_badge, get_palette};
use eframe::egui::{self, Margin, RichText};
use std::fs;
use std::path::Path;

impl WinTuneApp {
    pub(super) fn show_gallery_tab(&mut self, ctx: &egui::Context) {
        self.show_gallery_header(ctx);
        self.show_gallery_grid(ctx);
    }

    fn reload_gallery(&mut self) {
        self.gallery
            .reload(&self.config.icon_source_folder, &self.config.pak_file_path);
    }

    fn import_icons(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .add_filter("Windows Icons", &["ico"])
            .pick_files()
        else {
            return;
        };
        if let Err(e) = fs::create_dir_all(&self.config.icon_source_folder) {
            self.set_status(format!("Cannot create the icon folder: {}", e));
            return;
        }
        let dest_dir = Path::new(&self.config.icon_source_folder);
        let imported = files
            .iter()
            .filter_map(|f| f.file_name().map(|name| (f, dest_dir.join(name))))
            .filter(|(src, dest)| fs::copy(src, dest).is_ok())
            .count();
        self.reload_gallery();
        self.set_status(format!(
            "Imported {} of {} .ico files into the gallery.",
            imported,
            files.len()
        ));
    }

    fn show_gallery_header(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);
        let busy = self.is_busy();

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
                                self.reload_gallery();
                            }
                        }
                        if ui.button("Import .ico Files...").clicked() {
                            self.import_icons();
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !busy,
                                    accent_button(
                                        "Build Compressed gallery.pak",
                                        self.config.theme,
                                    ),
                                )
                                .clicked()
                            {
                                self.start_build_pak(ctx);
                            }
                        });
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Search:").strong());
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.gallery_search_query)
                                .id(super::polish::search_box_id(ActiveTab::Gallery)
                                    .expect("search box"))
                                .hint_text("Filter icons (rust, react, folder-src, _light...)")
                                .desired_width(240.0),
                        );
                        self.focus_search_if_requested(&search);

                        ui.separator();
                        for (category, label) in [
                            (IconCategory::All, "All"),
                            (IconCategory::FilesOnly, "Files"),
                            (IconCategory::FoldersOnly, "Folders (folder-*)"),
                            (IconCategory::LightVariants, "Light (*_light)"),
                            (IconCategory::Clones, "Clones (*.clone)"),
                        ] {
                            ui.selectable_value(&mut self.gallery_category, category, label);
                        }

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
    }

    fn show_gallery_grid(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);

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
                let total_rows = filtered.len().div_ceil(cols);

                let mut chosen_icon: Option<String> = None;

                // Only visible rows are laid out, so only visible icons get
                // queued for (background) decoding.
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    item_height,
                    total_rows,
                    |ui, row_range| {
                        for row in row_range {
                            ui.horizontal(|ui| {
                                let start = row * cols;
                                let end = (start + cols).min(filtered.len());
                                for name in &filtered[start..end] {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(item_width - 8.0, item_height - 6.0),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            if self.gallery_tile(
                                                ui,
                                                ctx,
                                                name,
                                                item_width,
                                                item_height,
                                            ) {
                                                chosen_icon = Some(name.clone());
                                            }
                                        },
                                    );
                                }
                            });
                        }
                    },
                );

                if let Some(icon_name) = chosen_icon {
                    self.assign_icon(icon_name);
                }
            });
    }

    /// One gallery cell. Returns true when clicked.
    fn gallery_tile(
        &self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        name: &str,
        item_width: f32,
        item_height: f32,
    ) -> bool {
        let palette = get_palette(self.config.theme);
        let mut clicked = false;
        card_frame(self.config.theme).show(ui, |ui| {
            ui.set_min_size(egui::vec2(item_width - 28.0, item_height - 28.0));
            ui.vertical_centered(|ui| {
                match self.gallery.texture(ctx, name) {
                    Some(t) => {
                        clicked |= ui
                            .add(
                                egui::ImageButton::new((t.id(), egui::vec2(42.0, 42.0)))
                                    .frame(false),
                            )
                            .clicked();
                    }
                    None if self.gallery.is_failed(name) => {
                        clicked |= ui.button("?").clicked();
                    }
                    None => {
                        ui.add_sized([42.0, 42.0], egui::Spinner::new());
                    }
                }
                ui.label(RichText::new(name).size(11.0).color(palette.text_secondary));
            });
        });
        clicked
    }

    fn assign_icon(&mut self, icon_name: String) {
        if self.selected_ext_index >= self.config.extensions.len() {
            return;
        }
        let ext = &mut self.config.extensions[self.selected_ext_index];
        ext.icon_name = icon_name.clone();
        let msg = format!("Assigned icon '{}' to {}.", icon_name, ext.extension);
        self.set_status(msg);
        if self.assign_icon_to_selected_ext {
            self.assign_icon_to_selected_ext = false;
            self.active_tab = ActiveTab::Associations;
        }
    }
}
