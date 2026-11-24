use super::icon_grid::icon_grid;
use super::{ActiveTab, WinTuneApp};
use crate::gallery::IconCategory;
use crate::theme::{accent_button, card_frame, get_palette};
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
                // This page only browses: choosing an icon is done from a
                // rule, in the icon picker.
                let _ = icon_grid(ui, ctx, &self.gallery, &filtered, None, "gallery_page");
            });
    }
}

#[cfg(test)]
mod tests {
    use super::super::icon_grid::GALLERY_CELL;
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

    #[test]
    fn clicking_an_icon_on_the_icons_page_changes_no_rule() {
        use egui::{Event, Modifiers, PointerButton};
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_page(ActiveTab::Gallery);
        let input = |events: Vec<Event>| RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 760.0))),
            events,
            ..Default::default()
        };
        frame(&mut app, &ctx, input(vec![]));
        let out = frame(&mut app, &ctx, input(vec![]));
        let bar = text_center(&out, "bar").expect("bar tile drawn");
        let button = |pressed| Event::PointerButton {
            pos: bar,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        frame(&mut app, &ctx, input(vec![Event::PointerMoved(bar)]));
        frame(&mut app, &ctx, input(vec![button(true)]));
        frame(&mut app, &ctx, input(vec![button(false)]));

        assert_eq!(app.config.extensions[0].icon_name, "foo");
        assert!(app.icon_picker.is_none());
        assert_eq!(app.active_tab, ActiveTab::Gallery);
    }
}
