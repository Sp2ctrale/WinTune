use super::icon_grid::icon_grid;
use super::{kit, ActiveTab, WinTuneApp};
use crate::gallery::IconCategory;
use eframe::egui::{self, RichText};
use kit::{primary_button, secondary_button, space, text_size};
use std::fs;
use std::path::Path;

impl WinTuneApp {
    pub(super) fn show_gallery_tab(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let p = kit::current_palette(ctx);
        let busy_reason = self.busy_or_blocked(&None);
        let folder = self.config.icon_source_folder.clone();
        let name = tr.t("nav.icons");
        let subtitle = tr.t("icons.subtitle");
        let build_label = tr.t("icons.build");
        let import_label = tr.t("icons.import");
        let categories = [
            (IconCategory::All, tr.t("picker.cat.all")),
            (IconCategory::FilesOnly, tr.t("picker.cat.files")),
            (IconCategory::FoldersOnly, tr.t("picker.cat.folders")),
            (IconCategory::LightVariants, tr.t("picker.cat.light")),
            (IconCategory::Clones, tr.t("picker.cat.clones")),
        ];
        let options: Vec<(IconCategory, &str)> =
            categories.iter().map(|(c, l)| (*c, l.as_str())).collect();

        let (mut build, mut import, mut to_settings) = (false, false, false);
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |ui| {
                let resp = ui.add_enabled(busy_reason.is_none(), primary_button(p, &build_label));
                build = match &busy_reason {
                    Some(reason) => resp.on_disabled_hover_text(reason),
                    None => resp,
                }
                .clicked();
                import = ui.add(secondary_button(&import_label)).clicked();
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr.t_args("icons.folder", &[&folder]))
                        .size(text_size::SECONDARY)
                        .color(p.text_secondary),
                );
                let link = ui
                    .add(
                        egui::Button::new(
                            RichText::new(tr.t("icons.change_in_settings"))
                                .size(text_size::SECONDARY)
                                .color(p.accent),
                        )
                        .frame(false),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                to_settings = link.clicked();
            });
            ui.add_space(space::M);

            ui.horizontal(|ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.gallery_search_query)
                        .id(super::polish::search_box_id(ActiveTab::Gallery).expect("search box"))
                        .hint_text(tr.t("icons.search"))
                        .desired_width(240.0),
                );
                self.focus_search_if_requested(&search);
                kit::segmented(ui, &mut self.gallery_category, &options);
            });
            ui.add_space(space::M);

            let filtered = self
                .gallery
                .filter_names(&self.gallery_search_query, self.gallery_category);
            if self.gallery.names().is_empty() {
                let text = tr.t_args("icons.empty", &[&folder]);
                if kit::empty_state(ui, kit::Icon::Icons, &text, Some(&import_label)) {
                    import = true;
                }
            } else if filtered.is_empty() {
                let text = tr.t_args("icons.no_match", &[&self.gallery_search_query]);
                ui.label(
                    RichText::new(text)
                        .size(text_size::BODY)
                        .color(p.text_secondary),
                );
            } else {
                // This page only browses: choosing an icon is done from a
                // rule, in the icon picker.
                let _ = icon_grid(
                    ui,
                    ctx,
                    &self.gallery,
                    &filtered,
                    None,
                    "gallery_page",
                    false,
                );
            }
        });

        if build {
            self.start_build_pak(ctx);
        }
        if import {
            self.import_icons();
        }
        if to_settings {
            self.open_page(ActiveTab::Settings);
        }
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
            let text = self.tr.t_args("icons.folder_error", &[&e.to_string()]);
            self.notify(kit::ToastKind::Error, text);
            return;
        }
        let dest_dir = Path::new(&self.config.icon_source_folder);
        let imported = files
            .iter()
            .filter_map(|f| f.file_name().map(|name| (f, dest_dir.join(name))))
            .filter(|(src, dest)| fs::copy(src, dest).is_ok())
            .count();
        self.reload_gallery();
        let text = self.tr.t_args(
            "icons.imported",
            &[&imported.to_string(), &files.len().to_string()],
        );
        self.notify(kit::ToastKind::Success, text);
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

    fn input(events: Vec<egui::Event>) -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 760.0))),
            events,
            ..Default::default()
        }
    }

    fn click_at(app: &mut WinTuneApp, ctx: &egui::Context, pos: Pos2) {
        use egui::{Event, Modifiers, PointerButton};
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        frame(app, ctx, input(vec![Event::PointerMoved(pos)]));
        frame(app, ctx, input(vec![button(true)]));
        frame(app, ctx, input(vec![button(false)]));
    }

    /// Rects of the size of an icon card that are drawn with a stroke
    /// at least `min_width` wide.
    fn stroked_cards(out: &egui::FullOutput, min_width: f32) -> usize {
        fn walk(shape: &egui::epaint::Shape, min_width: f32, n: &mut usize) {
            match shape {
                egui::epaint::Shape::Rect(r)
                    if (r.rect.size() - (GALLERY_CELL - egui::vec2(8.0, 8.0))).length() < 0.5
                        && r.stroke.width >= min_width =>
                {
                    *n += 1
                }
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, min_width, n)),
                _ => {}
            }
        }
        let mut n = 0;
        out.shapes
            .iter()
            .for_each(|c| walk(&c.shape, min_width, &mut n));
        n
    }

    #[test]
    fn icons_page_tiles_do_not_react() {
        use egui::{Event, Key, Modifiers};
        let fx = TempIcons::new(&["foo", "bar"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_page(ActiveTab::Gallery);
        frame(&mut app, &ctx, input(vec![]));
        let out = frame(&mut app, &ctx, input(vec![]));
        let bar = text_center(&out, "bar").expect("bar tile drawn");

        for events in [vec![Event::PointerMoved(bar)], vec![]] {
            let out = frame(&mut app, &ctx, input(events));
            assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Default);
        }

        // Tab through the whole page: no tile ever takes focus.
        for _ in 0..30 {
            let out = frame(
                &mut app,
                &ctx,
                input(vec![Event::Key {
                    key: Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::default(),
                }]),
            );
            assert_eq!(stroked_cards(&out, 2.0), 0, "a tile drew a focus ring");
        }

        click_at(&mut app, &ctx, bar);
        assert_eq!(app.config.extensions[0].icon_name, "foo");
        assert!(app.icon_picker.is_none());
        assert_eq!(app.active_tab, ActiveTab::Gallery);
    }

    #[test]
    fn change_in_settings_opens_settings() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Gallery);
        frame(&mut app, &ctx, input(vec![]));
        let out = frame(&mut app, &ctx, input(vec![]));
        let link = text_center(&out, "Change in Settings").expect("link drawn");
        click_at(&mut app, &ctx, link);
        assert_eq!(app.active_tab, ActiveTab::Settings);
    }

    #[test]
    fn an_empty_folder_offers_import() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Gallery);
        frame(&mut app, &ctx, input(vec![]));
        let out = frame(&mut app, &ctx, input(vec![]));
        let texts = drawn_texts(&out);
        let expected = format!("No icons in {}", app.config.icon_source_folder);
        assert!(texts.contains(&expected), "{texts:?}");
        // The empty state's action, besides the header's own button.
        assert!(
            texts.iter().filter(|t| *t == "Import .ico").count() >= 2,
            "{texts:?}"
        );
        assert!(texts.contains(&format!("Folder: {}", app.config.icon_source_folder)));
    }

    #[test]
    fn a_search_without_match_says_so() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Gallery);
        app.gallery_search_query = "zzz".into();
        frame(&mut app, &ctx, input(vec![]));
        let out = frame(&mut app, &ctx, input(vec![]));
        let texts = drawn_texts(&out);
        assert!(
            texts.contains(&"No icon matches \"zzz\".".to_string()),
            "{texts:?}"
        );
        assert!(!texts.iter().any(|t| t.starts_with("No icons in")));
    }
}
