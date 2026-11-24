use super::{ActiveTab, WinTuneApp};
use crate::gallery::IconCategory;
use crate::theme::{accent_button, card_frame, draw_badge, get_palette};
use eframe::egui::{self, Margin, RichText};
use std::fs;
use std::path::Path;

/// Size of one gallery grid slot; the card inside leaves a gap around it.
const GALLERY_CELL: egui::Vec2 = egui::vec2(130.0, 98.0);
const GALLERY_GAP: f32 = 8.0;

/// The card rect for column `col` of a row starting at `row_min`.
fn gallery_cell_rect(row_min: egui::Pos2, col: usize) -> egui::Rect {
    egui::Rect::from_min_size(
        row_min + egui::vec2(col as f32 * GALLERY_CELL.x, 0.0),
        GALLERY_CELL - egui::vec2(GALLERY_GAP, GALLERY_GAP),
    )
}

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
            self.notify(
                super::kit::ToastKind::Error,
                format!("Cannot create the icon folder: {}", e),
            );
            return;
        }
        let dest_dir = Path::new(&self.config.icon_source_folder);
        let imported = files
            .iter()
            .filter_map(|f| f.file_name().map(|name| (f, dest_dir.join(name))))
            .filter(|(src, dest)| fs::copy(src, dest).is_ok())
            .count();
        self.reload_gallery();
        self.notify(
            super::kit::ToastKind::Success,
            format!(
                "Imported {} of {} .ico files into the gallery.",
                imported,
                files.len()
            ),
        );
    }

    fn show_gallery_header(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.ui_theme);
        let busy = self.is_busy();

        egui::TopBottomPanel::top("gallery_header")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.ui_theme).show(ui, |ui| {
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
                                    accent_button("Build Compressed gallery.pak", self.ui_theme),
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
        let palette = get_palette(self.ui_theme);

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
                let cols = ((avail_width / GALLERY_CELL.x).floor() as usize).max(1);
                let total_rows = filtered.len().div_ceil(cols);

                let mut chosen_icon: Option<String> = None;

                // Only visible rows are laid out, so only visible icons get
                // queued for (background) decoding. Every cell gets the same
                // fixed rect, so long names or loading states can't shift the
                // columns.
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    GALLERY_CELL.y,
                    total_rows,
                    |ui, row_range| {
                        for row in row_range {
                            let (row_rect, _) = ui.allocate_exact_size(
                                egui::vec2(cols as f32 * GALLERY_CELL.x, GALLERY_CELL.y),
                                egui::Sense::hover(),
                            );
                            let start = row * cols;
                            let end = (start + cols).min(filtered.len());
                            for (col, name) in filtered[start..end].iter().enumerate() {
                                let cell = gallery_cell_rect(row_rect.min, col);
                                let mut cell_ui = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(cell)
                                        .layout(egui::Layout::top_down(egui::Align::Center)),
                                );
                                if self.gallery_tile(&mut cell_ui, ctx, name) {
                                    chosen_icon = Some(name.clone());
                                }
                            }
                        }
                    },
                );

                if let Some(icon_name) = chosen_icon {
                    self.assign_icon(icon_name);
                }
            });
    }

    /// One gallery cell, filling `ui`'s fixed rect. Returns true when clicked.
    fn gallery_tile(&self, ui: &mut egui::Ui, ctx: &egui::Context, name: &str) -> bool {
        let palette = get_palette(self.ui_theme);
        let mut clicked = false;
        card_frame(self.ui_theme)
            .inner_margin(Margin::same(8.0))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
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
                            clicked |= ui.add_sized([42.0, 42.0], egui::Button::new("?")).clicked();
                        }
                        None => {
                            ui.add_sized([42.0, 42.0], egui::Spinner::new());
                        }
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(name).size(11.0).color(palette.text_secondary),
                        )
                        .truncate(),
                    );
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
        self.notify(super::kit::ToastKind::Success, msg);
        if self.assign_icon_to_selected_ext {
            self.assign_icon_to_selected_ext = false;
            self.open_page(ActiveTab::Associations);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use egui::{Pos2, RawInput, Rect};

    #[test]
    fn gallery_columns_stay_aligned_with_a_long_name() {
        let long = "a-very-long-icon-name-that-is-much-wider-than-a-tile";
        let short = [
            "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o",
        ];
        let mut names = vec![long];
        names.extend(short);
        let fx = TempIcons::new(&names);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Gallery);
        let input = || RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 760.0))),
            ..Default::default()
        };
        frame(&mut app, &ctx, input());
        let out = frame(&mut app, &ctx, input());

        let centers: Vec<Pos2> = short
            .iter()
            .map(|n| text_center(&out, n).unwrap_or_else(|| panic!("{n} not drawn")))
            .collect();
        let rows: std::collections::BTreeSet<i32> =
            centers.iter().map(|c| c.y.round() as i32).collect();
        assert!(rows.len() >= 2, "the test needs two rows: {centers:?}");
        let origin = centers[0].x;
        for c in &centers {
            let offset = (c.x - origin) / GALLERY_CELL.x;
            assert!(
                (offset - offset.round()).abs() < 0.01,
                "label off the column grid: {centers:?}"
            );
        }
        // The long name is cut to its tile instead of widening it.
        fn width_of(shape: &egui::epaint::Shape, text: &str) -> Option<f32> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.galley.rect.width())
                }
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| width_of(s, text)),
                _ => None,
            }
        }
        let width = out
            .shapes
            .iter()
            .find_map(|c| width_of(&c.shape, long))
            .expect("long name drawn");
        assert!(width < GALLERY_CELL.x, "long name is {width} px wide");
    }
}
