use super::*;
use crate::theme::ThemeMode;
use egui::{Color32, RawInput, Theme};

/// WCAG 2.x relative luminance of an opaque sRGB color.
fn luminance(c: Color32) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG 2.x contrast ratio, from 1 (same color) to 21 (black on white).
fn contrast(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn frame_with_system_theme(ctx: &egui::Context, system: Theme) {
    let input = RawInput {
        system_theme: Some(system),
        ..Default::default()
    };
    let _ = ctx.run(input, |_| {});
}

#[test]
fn contrast_formula_matches_known_values() {
    assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-9);
    assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-9);
}

#[test]
fn system_mode_follows_windows_between_frames() {
    let ctx = egui::Context::default();
    install_styles(&ctx);
    set_theme_mode(&ctx, ThemeMode::System);
    let mut input = RawInput {
        system_theme: Some(Theme::Light),
        ..Default::default()
    };
    let _ = ctx.run(input.clone(), |_| {});
    assert_eq!(current_palette(&ctx).page, palette(Theme::Light).page);
    input.system_theme = Some(Theme::Dark);
    let _ = ctx.run(input, |_| {});
    assert_eq!(current_palette(&ctx).page, palette(Theme::Dark).page);
}

#[test]
fn explicit_modes_ignore_windows() {
    let ctx = egui::Context::default();
    install_styles(&ctx);

    set_theme_mode(&ctx, ThemeMode::Light);
    frame_with_system_theme(&ctx, Theme::Dark);
    assert_eq!(current_palette(&ctx).page, palette(Theme::Light).page);

    set_theme_mode(&ctx, ThemeMode::Dark);
    frame_with_system_theme(&ctx, Theme::Light);
    assert_eq!(current_palette(&ctx).page, palette(Theme::Dark).page);
}

#[test]
fn resolved_never_returns_system() {
    let ctx = egui::Context::default();
    install_styles(&ctx);
    set_theme_mode(&ctx, ThemeMode::System);

    frame_with_system_theme(&ctx, Theme::Light);
    assert_eq!(resolved(ThemeMode::System, &ctx), ThemeMode::Light);
    frame_with_system_theme(&ctx, Theme::Dark);
    assert_eq!(resolved(ThemeMode::System, &ctx), ThemeMode::Dark);

    assert_eq!(resolved(ThemeMode::Light, &ctx), ThemeMode::Light);
    assert_eq!(resolved(ThemeMode::Dark, &ctx), ThemeMode::Dark);
}

#[test]
fn palettes_meet_contrast_minimums() {
    for theme in [Theme::Light, Theme::Dark] {
        let p = palette(theme);
        for (name, fg, bg) in [
            ("text on page", p.text, p.page),
            ("text on surface", p.text, p.surface),
            ("text on surface_hover", p.text, p.surface_hover),
            ("text on control", p.text, p.control),
            ("text_secondary on page", p.text_secondary, p.page),
            ("text_secondary on surface", p.text_secondary, p.surface),
            ("on_accent on accent", p.on_accent, p.accent),
        ] {
            let ratio = contrast(fg, bg);
            assert!(
                ratio >= 4.5,
                "{theme:?}: {name} is {ratio:.2}:1, below 4.5:1"
            );
        }
    }
}

#[test]
fn accent_is_windows_blue_in_light() {
    assert_eq!(
        palette(Theme::Light).accent,
        Color32::from_rgb(0x00, 0x78, 0xD4)
    );
}

#[test]
fn strong_text_uses_the_primary_text_color() {
    let ctx = egui::Context::default();
    install_styles(&ctx);
    for theme in [Theme::Light, Theme::Dark] {
        let style = ctx.style_of(theme);
        assert_eq!(
            style.visuals.strong_text_color(),
            palette(theme).text,
            "{theme:?}"
        );
    }
}

#[test]
fn installed_styles_use_the_tokens() {
    let ctx = egui::Context::default();
    install_styles(&ctx);
    for theme in [Theme::Light, Theme::Dark] {
        let style = ctx.style_of(theme);
        let p = palette(theme);
        let v = &style.visuals;
        assert_eq!(v.dark_mode, theme == Theme::Dark);
        assert_eq!(v.panel_fill, p.page);
        assert_eq!(v.window_fill, p.surface);
        assert_eq!(v.window_rounding, egui::Rounding::same(radius::CARD));
        assert_eq!(
            v.widgets.inactive.rounding,
            egui::Rounding::same(radius::CONTROL)
        );
        assert_eq!(v.widgets.inactive.bg_stroke.width, 1.0_f32);
        assert_eq!(
            v.widgets.noninteractive.bg_stroke,
            egui::Stroke::new(1.0_f32, p.border)
        );
        assert_eq!(v.selection.stroke.color, p.accent);
        assert_eq!(v.hyperlink_color, p.accent);
        assert_eq!(style.spacing.item_spacing, egui::vec2(space::S, space::S));
        let size = |ts: egui::TextStyle| style.text_styles[&ts].size;
        assert_eq!(size(egui::TextStyle::Heading), text_size::TITLE);
        assert_eq!(size(egui::TextStyle::Body), text_size::BODY);
        assert_eq!(size(egui::TextStyle::Button), text_size::BODY);
        assert_eq!(size(egui::TextStyle::Small), text_size::SECONDARY);
    }
}

#[test]
fn old_configs_keep_their_theme() {
    let c: ThemeMode = serde_json::from_str("\"Dark\"").unwrap();
    assert_eq!(c, ThemeMode::Dark);
    let c: ThemeMode = serde_json::from_str("\"Light\"").unwrap();
    assert_eq!(c, ThemeMode::Light);
    assert_eq!(ThemeMode::default(), ThemeMode::System);
}

#[test]
fn new_configs_follow_windows() {
    assert_eq!(crate::models::AppConfig::default().theme, ThemeMode::System);
}

// ---- fonts and icons ----

use std::path::{Path, PathBuf};

const WIN_FONTS: &str = r"C:\Windows\Fonts";

fn temp_dir_with(tag: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wintune-fonts-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    dir
}

/// Copies the named system fonts into a fresh temp dir; None if any is absent.
fn copy_system_fonts(tag: &str, names: &[&str]) -> Option<PathBuf> {
    let mut files = Vec::new();
    for n in names {
        match std::fs::read(Path::new(WIN_FONTS).join(n)) {
            Ok(b) => files.push((*n, b)),
            Err(_) => {
                eprintln!("skipping: {n} is absent from {WIN_FONTS}");
                return None;
            }
        }
    }
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, b)| (*n, b.as_slice())).collect();
    Some(temp_dir_with(tag, &refs))
}

fn run_label_frame(ctx: &egui::Context) {
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| ui.label("ok"));
    });
}

#[test]
fn missing_fonts_folder_falls_back_without_panicking() {
    let ctx = egui::Context::default();
    let r = fonts::install(&ctx, Path::new(r"C:\definitely\not\here"));
    assert_eq!(
        r,
        FontReport {
            text: None,
            semibold: false,
            icons: None
        }
    );
    run_label_frame(&ctx);
    assert!(!icons::icons_available(&ctx));
    assert!(icons::icon_text(&ctx, Icon::Home, 16.0).is_none());
    assert_eq!(
        fonts::semibold(&ctx, 14.0),
        egui::FontId::proportional(14.0)
    );
}

#[test]
fn corrupt_font_files_are_skipped() {
    let dir = temp_dir_with(
        "corrupt",
        &[
            ("SegUIVar.ttf", b"not a font"),
            ("seguisb.ttf", b"nope"),
            ("SegoeIcons.ttf", b"\0\0\0"),
        ],
    );
    let ctx = egui::Context::default();
    let r = fonts::install(&ctx, &dir);
    assert_eq!(r.text, None);
    assert!(!r.semibold);
    assert_eq!(r.icons, None);
    run_label_frame(&ctx);
    assert_eq!(
        fonts::semibold(&ctx, 14.0),
        egui::FontId::proportional(14.0)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn falls_back_to_the_next_candidate() {
    let Some(dir) = copy_system_fonts("fallback", &["segoeui.ttf", "segmdl2.ttf"]) else {
        return;
    };
    let ctx = egui::Context::default();
    let r = fonts::install(&ctx, &dir);
    assert_eq!(r.text, Some("segoeui.ttf"));
    assert!(!r.semibold);
    assert_eq!(r.icons, Some("segmdl2.ttf"));
    run_label_frame(&ctx);
    assert!(icons::icons_available(&ctx));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn corrupt_first_candidate_uses_the_second() {
    let Some(good) = std::fs::read(Path::new(WIN_FONTS).join("segoeui.ttf")).ok() else {
        eprintln!("skipping: segoeui.ttf is absent");
        return;
    };
    let dir = temp_dir_with(
        "corrupt-first",
        &[("SegUIVar.ttf", b"garbage"), ("segoeui.ttf", &good)],
    );
    let ctx = egui::Context::default();
    assert_eq!(fonts::install(&ctx, &dir).text, Some("segoeui.ttf"));
    run_label_frame(&ctx);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn real_windows_fonts_load_when_present() {
    let dir = Path::new(WIN_FONTS);
    if !dir.join("SegUIVar.ttf").exists() {
        eprintln!("skipping: SegUIVar.ttf is absent");
        return;
    }
    let ctx = egui::Context::default();
    let r = fonts::install(&ctx, dir);
    assert_eq!(r.text, Some("SegUIVar.ttf"));
    if dir.join("seguisb.ttf").exists() {
        assert!(r.semibold);
    }
    assert!(r.icons.is_some());
    run_label_frame(&ctx);
    assert!(icons::icons_available(&ctx));
    let t = icons::icon_text(&ctx, Icon::Settings, 16.0).expect("icon text");
    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(t.clone());
            ui.label(egui::RichText::new("Bold").font(fonts::semibold(ctx, 14.0)));
        });
    });
}

/// Exhaustive on purpose: adding an `Icon` variant fails to compile here until
/// it is also listed, and the length check below then forces `Icon::ALL` too.
fn icon_ordinal(i: Icon) -> usize {
    use Icon::*;
    match i {
        Home => 0,
        Associations => 1,
        Explorer => 2,
        Start => 3,
        ContextMenu => 4,
        Icons => 5,
        History => 6,
        Settings => 7,
        Search => 8,
        Add => 9,
        Delete => 10,
        Restore => 11,
        Warning => 12,
        Info => 13,
        Error => 14,
        Success => 15,
        ChevronRight => 16,
        ChevronDown => 17,
        More => 18,
        Folder => 19,
    }
}

#[test]
fn icon_all_lists_every_variant_once() {
    assert_eq!(Icon::ALL.len(), 20);
    for (n, i) in Icon::ALL.iter().enumerate() {
        assert_eq!(icon_ordinal(*i), n);
    }
}

#[test]
fn glyphs_are_private_use_and_unique_except_explorer_folder() {
    let mut seen = std::collections::HashMap::new();
    for i in Icon::ALL {
        let c = icons::glyph(i);
        assert!(('\u{E000}'..='\u{F8FF}').contains(&c), "{i:?} is {c:?}");
        if let Some(prev) = seen.insert(c, i) {
            let pair = [prev, i];
            assert!(
                pair.contains(&Icon::Explorer) && pair.contains(&Icon::Folder),
                "{prev:?} and {i:?} share {c:?}"
            );
        }
    }
    assert_eq!(icons::glyph(Icon::Home), '\u{E80F}');
    assert_eq!(icons::glyph(Icon::Folder), '\u{E8B7}');
}

#[test]
fn every_glyph_exists_in_both_icon_fonts() {
    use ab_glyph::Font;
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
    let dir = Path::new(&windir).join("Fonts");
    for file in ["SegoeIcons.ttf", "segmdl2.ttf"] {
        let Ok(bytes) = std::fs::read(dir.join(file)) else {
            eprintln!("skipping: {file} is absent");
            continue;
        };
        let font = ab_glyph::FontRef::try_from_slice(&bytes).unwrap();
        for i in Icon::ALL {
            let c = icons::glyph(i);
            assert_ne!(font.glyph_id(c).0, 0, "{i:?} ({c:?}) is missing in {file}");
        }
    }
}

// ---- controls and surfaces ----

mod controls {
    use super::super::test_input::{click_frames, frame_input};
    use super::super::*;
    use crate::theme::ThemeMode;
    use egui::{CentralPanel, Context, Rect};
    use std::cell::{Cell, RefCell};

    fn ctx_for(theme: ThemeMode) -> Context {
        let ctx = Context::default();
        install_styles(&ctx);
        set_theme_mode(&ctx, theme);
        ctx
    }

    /// Lay out once and return the rect `draw` recorded.
    fn layout(ctx: &Context, rect: &Cell<Rect>, mut draw: impl FnMut(&Context)) -> egui::Pos2 {
        let _ = ctx.run(frame_input(vec![]), |c| draw(c));
        rect.get().center()
    }

    #[test]
    fn clicking_a_toggle_flips_it() {
        let ctx = ctx_for(ThemeMode::Light);
        let on = Cell::new(false);
        let rect = Cell::new(Rect::NOTHING);
        let mut draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let mut v = on.get();
                rect.set(toggle(ui, &mut v).rect);
                on.set(v);
            });
        };
        let at = layout(&ctx, &rect, &mut draw);
        click_frames(&ctx, at, &mut draw);
        assert!(on.get());
    }

    #[test]
    fn a_disabled_toggle_never_flips() {
        let ctx = ctx_for(ThemeMode::Light);
        let on = Cell::new(false);
        let rect = Cell::new(Rect::NOTHING);
        let mut draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let mut v = on.get();
                let r = toggle_enabled(ui, &mut v, Some("reason"));
                rect.set(r.rect);
                assert!(!r.changed());
                on.set(v);
            });
        };
        let at = layout(&ctx, &rect, &mut draw);
        click_frames(&ctx, at, &mut draw);
        assert!(!on.get());
    }

    #[test]
    fn segmented_selects_the_clicked_option() {
        #[derive(Clone, Copy, PartialEq, Debug)]
        enum V {
            A,
            B,
        }
        let ctx = ctx_for(ThemeMode::Dark);
        let value = Cell::new(V::A);
        let changed = Cell::new(false);
        let rect = Cell::new(Rect::NOTHING);
        let mut draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let mut v = value.get();
                let r = segmented(ui, &mut v, &[(V::A, "Alpha"), (V::B, "Beta")]);
                rect.set(r.rect);
                changed.set(changed.get() || r.changed());
                value.set(v);
            });
        };
        let _ = layout(&ctx, &rect, &mut draw);
        // The second option is in the right half of the control.
        let r = rect.get();
        let at = egui::pos2(r.right() - 8.0, r.center().y);
        click_frames(&ctx, at, &mut draw);
        assert_eq!(value.get(), V::B);
        assert!(changed.get());
    }

    /// Centre of the first drawn text equal to `text` in `out`.
    fn text_center(out: &egui::FullOutput, text: &str) -> Option<egui::Pos2> {
        fn find(shape: &egui::Shape, text: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.visual_bounding_rect().center())
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        out.shapes.iter().find_map(|s| find(&s.shape, text))
    }

    /// Click `…`, run a frame so the popup appears, then click the item
    /// labelled `target`. Returns what the menu returned across all frames.
    fn click_menu_item(items: &[MenuItem<'_>], target: &str) -> Vec<usize> {
        let ctx = ctx_for(ThemeMode::Light);
        let picked = RefCell::new(Vec::new());
        let mut draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                if let Some(i) = overflow_menu(ui, "menu", "More actions", "More: row", items) {
                    picked.borrow_mut().push(i);
                }
            });
        };
        let out = ctx.run(frame_input(vec![]), |c| draw(c));
        let at = text_center(&out, "\u{2026}").expect("the … button is drawn");
        click_frames(&ctx, at, &mut draw);
        let out = ctx.run(frame_input(vec![]), |c| draw(c));
        let at = text_center(&out, target).expect("the menu item is drawn");
        click_frames(&ctx, at, &mut draw);
        let _ = ctx.run(frame_input(vec![]), |c| draw(c));
        picked.into_inner()
    }

    #[test]
    fn the_overflow_menu_returns_the_clicked_item() {
        let items = [
            MenuItem {
                label: "First",
                enabled: true,
                disabled_hint: None,
            },
            MenuItem {
                label: "Second",
                enabled: true,
                disabled_hint: None,
            },
        ];
        assert_eq!(click_menu_item(&items, "Second"), vec![1]);
    }

    #[test]
    fn a_disabled_menu_item_never_returns() {
        let items = [
            MenuItem {
                label: "First",
                enabled: false,
                disabled_hint: Some("not now"),
            },
            MenuItem {
                label: "Second",
                enabled: true,
                disabled_hint: None,
            },
        ];
        assert!(click_menu_item(&items, "First").is_empty());
    }

    #[test]
    fn a_setting_rows_note_shows_its_own_hover_text() {
        let ctx = ctx_for(ThemeMode::Light);
        let draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                SettingRow {
                    icon: None,
                    title: "Title",
                    description: Some("Description"),
                    hover: None,
                }
                .show_with_note(ui, "Note", "Why the note", |ui| {
                    let mut on = false;
                    toggle(ui, &mut on);
                });
            });
        };
        let texts = |out: &egui::FullOutput| {
            let mut found = Vec::new();
            for t in ["Description", "Note", "Why the note"] {
                if text_center(out, t).is_some() {
                    found.push(t);
                }
            }
            found
        };
        let out = ctx.run(frame_input(vec![]), draw);
        assert_eq!(texts(&out), ["Description", "Note"]);
        let note = text_center(&out, "Note").unwrap();
        let description = text_center(&out, "Description").unwrap();
        assert!(
            note.y > description.y,
            "the note sits under the description"
        );

        // Rest on the note long enough for the tooltip.
        let mut out = out;
        for i in 0..20 {
            let mut input = frame_input(vec![egui::Event::PointerMoved(note)]);
            if i >= 4 {
                input.time = Some(ctx.input(|i| i.time) + 0.1);
            }
            out = ctx.run(input, draw);
        }
        assert_eq!(texts(&out), ["Description", "Note", "Why the note"]);
    }

    #[test]
    fn banner_reports_its_action_click() {
        let ctx = ctx_for(ThemeMode::Light);
        let clicks = Cell::new(0);
        let at = RefCell::new(None);
        let mut draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                if banner(ui, BannerKind::Warning, "Careful", Some("Fix it")) {
                    clicks.set(clicks.get() + 1);
                }
                // Probe the action button: the right edge of the banner row.
                *at.borrow_mut() = Some(egui::pos2(
                    ui.max_rect().right() - 24.0,
                    ui.min_rect().top() + 20.0,
                ));
            });
        };
        let _ = ctx.run(frame_input(vec![]), |c| draw(c));
        let pos = at.borrow().unwrap();
        click_frames(&ctx, pos, &mut draw);
        assert_eq!(clicks.get(), 1);
    }

    #[test]
    fn a_long_banner_message_wraps_inside_the_banner() {
        let ctx = ctx_for(ThemeMode::Light);
        let height = Cell::new(0.0_f32);
        let right = Cell::new(0.0_f32);
        let long = "word ".repeat(60);
        let _ = ctx.run(frame_input(vec![]), |c| {
            CentralPanel::default().show(c, |ui| {
                let r = ui.allocate_ui(egui::vec2(360.0, 0.0), |ui| {
                    banner(ui, BannerKind::Error, &long, Some("Retry"));
                });
                height.set(r.response.rect.height());
                right.set(r.response.rect.right());
            });
        });
        // One row is about 48 px; 300 characters must take several rows.
        assert!(height.get() > 100.0, "banner height {}", height.get());
        assert!(
            right.get() <= 360.0 + 16.0 + 1.0,
            "banner overflows: {}",
            right.get()
        );
    }

    #[test]
    fn segmented_clicking_the_selected_option_does_not_change() {
        let ctx = ctx_for(ThemeMode::Light);
        let changed = Cell::new(false);
        let rect = Cell::new(Rect::NOTHING);
        let mut draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let mut v = 1;
                let r = segmented(ui, &mut v, &[(1, "One"), (2, "Two")]);
                rect.set(r.rect);
                changed.set(changed.get() || r.changed());
            });
        };
        let _ = layout(&ctx, &rect, &mut draw);
        let r = rect.get();
        click_frames(&ctx, egui::pos2(r.left() + 8.0, r.center().y), &mut draw);
        assert!(!changed.get());
    }

    #[test]
    fn every_control_renders_in_both_themes_without_fonts() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let ctx = ctx_for(mode);
            let _ = ctx.run(frame_input(vec![]), |c| {
                CentralPanel::default().show(c, |ui| {
                    let mut on = true;
                    toggle(ui, &mut on);
                    toggle_enabled(ui, &mut on, Some("why"));
                    for kind in [
                        PillKind::Success,
                        PillKind::Warning,
                        PillKind::Accent,
                        PillKind::Neutral,
                        PillKind::Danger,
                    ] {
                        pill(ui, "Applied", kind);
                    }
                    let mut v = 1;
                    segmented(ui, &mut v, &[(1, "One"), (2, "Two")]);
                    let opened = disclosure(ui, "d", "More", true, |ui| ui.label("body"));
                    assert!(opened.is_some());
                    assert!(disclosure(ui, "d2", "Less", false, |ui| ui.label("x")).is_none());
                    card(ui, |ui| ui.label("card"));
                    section(ui, "Section", |ui| {
                        SettingRow {
                            icon: Some(Icon::Home),
                            title: "Title",
                            description: Some("Description"),
                            hover: None,
                        }
                        .show(ui, |ui| toggle(ui, &mut on));
                    });
                    for kind in [BannerKind::Info, BannerKind::Warning, BannerKind::Error] {
                        banner(ui, kind, "Message", Some("Act"));
                        banner(ui, kind, "No action", None);
                    }
                    empty_state(ui, Icon::Search, "Nothing here", Some("Add"));
                    empty_state(ui, Icon::Search, "Nothing here", None);
                });
            });
        }
    }
}

mod layout {
    use super::super::test_input::{click_frames, frame_input};
    use super::super::*;
    use crate::theme::ThemeMode;
    use egui::{CentralPanel, Context};
    use std::cell::Cell;

    fn ctx_for(theme: ThemeMode) -> Context {
        let ctx = Context::default();
        install_styles(&ctx);
        set_theme_mode(&ctx, theme);
        ctx
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Page {
        Home,
        Explorer,
        Settings,
        Later,
    }

    const BRAND: Brand<'static> = Brand {
        logo: None,
        name: "WinTune",
        caption: "Customize Windows",
    };

    fn item(key: Page, label: &str, enabled: bool) -> NavItem<'_, Page> {
        NavItem {
            key,
            icon: Icon::Home,
            label,
            badge: None,
            enabled,
            disabled_hint: None,
        }
    }

    /// Draws the nav with `footer_item` as the only footer entry and returns
    /// the (selection, changed) pair after a click on that footer entry.
    fn click_footer(footer_item: NavItem<'_, Page>) -> (Page, bool) {
        let ctx = ctx_for(ThemeMode::Light);
        let selected = Cell::new(Page::Home);
        let changed = Cell::new(false);
        let groups = vec![
            NavGroup {
                title: None,
                items: vec![item(Page::Home, "Home", true)],
            },
            NavGroup {
                title: Some("Customize"),
                items: vec![item(Page::Explorer, "Explorer", true)],
            },
        ];
        let footer = [footer_item];
        let draw = |c: &Context| {
            let mut s = selected.get();
            if nav_sidebar(c, &BRAND, &mut s, &groups, &footer) {
                changed.set(true);
            }
            selected.set(s);
            CentralPanel::default().show(c, |_| {});
        };
        // Footer entries sit at the bottom of the 600 px high panel.
        click_frames(&ctx, egui::pos2(130.0, 570.0), draw);
        (selected.get(), changed.get())
    }

    #[test]
    fn clicking_a_nav_item_selects_it() {
        let (selected, changed) = click_footer(item(Page::Settings, "Settings", true));
        assert_eq!(selected, Page::Settings);
        assert!(changed);
    }

    #[test]
    fn a_disabled_nav_item_cannot_be_selected() {
        let (selected, changed) = click_footer(item(Page::Later, "Later", false));
        assert_eq!(selected, Page::Home);
        assert!(!changed);
    }

    #[test]
    fn info_toasts_expire_and_error_toasts_stay() {
        let ctx = ctx_for(ThemeMode::Light);
        let mut toasts = Toasts::default();
        toasts.push(ToastKind::Info, "Saved");
        toasts.push(ToastKind::Error, "Failed");
        let at = |t: f64| egui::RawInput {
            time: Some(t),
            ..frame_input(vec![])
        };
        let _ = ctx.run(at(0.0), |c| toasts.show(c, "Clear all"));
        assert_eq!(toasts.len(), 2);
        let _ = ctx.run(at(TOAST_SECONDS + 0.1), |c| toasts.show(c, "Clear all"));
        assert_eq!(toasts.len(), 1);
        assert!(!toasts.is_empty());
    }

    #[test]
    fn at_most_three_toasts_are_shown() {
        let ctx = ctx_for(ThemeMode::Dark);
        let mut toasts = Toasts::default();
        for i in 0..5 {
            toasts.push(ToastKind::Success, format!("Toast {i}"));
        }
        let _ = ctx.run(frame_input(vec![]), |c| toasts.show(c, "Clear all"));
        assert_eq!(toasts.len(), 5);
        // Five queued toasts take three rows plus one overflow row.
        let height = |ctx: &Context| {
            ctx.memory(|m| m.area_rect(egui::Id::new("wintune_toasts")))
                .unwrap()
                .height()
        };
        let five = height(&ctx);
        let mut three = Toasts::default();
        for i in 0..3 {
            three.push(ToastKind::Success, format!("Toast {i}"));
        }
        let ctx3 = ctx_for(ThemeMode::Dark);
        let _ = ctx3.run(frame_input(vec![]), |c| three.show(c, "Clear all"));
        let extra = five - height(&ctx3);
        assert!(extra > 0.0 && extra < 40.0, "overflow row height {extra}");
    }

    #[test]
    fn toasts_sit_above_a_bottom_panel() {
        const BAR_HEIGHT: f32 = 60.0;
        let ctx = ctx_for(ThemeMode::Light);
        let mut toasts = Toasts::default();
        for i in 0..5 {
            toasts.push(ToastKind::Error, format!("Toast {i}"));
        }
        let mut bar_top = f32::NAN;
        let mut out = None;
        // The first frames size the area; the last one is the settled layout.
        for _ in 0..3 {
            out = Some(ctx.run(frame_input(vec![]), |c| {
                let bar = egui::TopBottomPanel::bottom("test_bar")
                    .exact_height(BAR_HEIGHT)
                    .show(c, |_| {});
                bar_top = bar.response.rect.top();
                toasts.show(c, "Clear all");
            }));
        }
        fn text_bottom(shape: &egui::Shape, text: &str) -> Option<f32> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.pos.y + t.galley.rect.bottom())
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| text_bottom(s, text)),
                _ => None,
            }
        }
        let out = out.unwrap();
        // The oldest shown toast is the lowest card; the overflow row is below it.
        for text in ["Toast 2", "Toast 4", "Clear all"] {
            let bottom = out
                .shapes
                .iter()
                .find_map(|c| text_bottom(&c.shape, text))
                .unwrap_or_else(|| panic!("{text:?} not drawn"));
            assert!(
                bottom <= bar_top,
                "{text:?} ends at {bottom}, under the panel's top edge {bar_top}"
            );
        }
        let area = ctx
            .memory(|m| m.area_rect(egui::Id::new("wintune_toasts")))
            .unwrap();
        assert!(area.bottom() <= bar_top, "{area:?} overlaps the panel");
    }

    #[test]
    fn a_busy_action_bar_hides_its_actions() {
        let ctx = ctx_for(ThemeMode::Light);
        let ran = Cell::new(false);
        let _ = ctx.run(frame_input(vec![]), |c| {
            action_bar(
                c,
                "Working",
                ActionBarState::Busy {
                    label: "Applying",
                    fraction: 0.4,
                },
                |_| ran.set(true),
            );
        });
        assert!(!ran.get());
        let _ = ctx.run(frame_input(vec![]), |c| {
            action_bar(c, "Ready", ActionBarState::Idle, |_| ran.set(true));
        });
        assert!(ran.get());
    }

    #[test]
    fn layout_components_render_without_fonts_in_both_themes() {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let ctx = ctx_for(theme);
            let mut toasts = Toasts::default();
            toasts.push(ToastKind::Success, "Done");
            toasts.push(ToastKind::Info, "Note");
            toasts.push(ToastKind::Error, "Broke");
            for busy in [false, true] {
                let _ = ctx.run(frame_input(vec![]), |c| {
                    let mut sel = Page::Home;
                    let groups = [NavGroup {
                        title: Some("Group"),
                        items: vec![
                            NavItem {
                                badge: Some(3),
                                ..item(Page::Home, "Home", true)
                            },
                            item(Page::Later, "Later", false),
                        ],
                    }];
                    nav_sidebar(
                        c,
                        &BRAND,
                        &mut sel,
                        &groups,
                        &[item(Page::Settings, "Settings", true)],
                    );
                    let state = if busy {
                        ActionBarState::Busy {
                            label: "Applying",
                            fraction: 0.5,
                        }
                    } else {
                        ActionBarState::Idle
                    };
                    action_bar(c, "Status", state, |ui| {
                        ui.label("a");
                    });
                    CentralPanel::default().show(c, |ui| {
                        page_header(
                            ui,
                            &["Customize", "Explorer"],
                            "Explorer",
                            Some("Sub"),
                            |ui| {
                                ui.label("x");
                            },
                        );
                        page_header(ui, &[], "Plain", None, |_| {});
                    });
                    toasts.show(c, "Clear all");
                    dialog(c, "d", "Confirm", 360.0, |ui| {
                        ui.label("Sure?");
                    });
                });
            }
        }
    }

    struct ModalScene {
        bg_rect: Cell<egui::Rect>,
        ok_rect: Cell<egui::Rect>,
        bg_clicked: Cell<bool>,
        ok_clicked: Cell<bool>,
        open: Cell<bool>,
        focus_bg: Cell<bool>,
    }

    impl ModalScene {
        fn new() -> Self {
            Self {
                bg_rect: Cell::new(egui::Rect::NOTHING),
                ok_rect: Cell::new(egui::Rect::NOTHING),
                bg_clicked: Cell::new(false),
                ok_clicked: Cell::new(false),
                open: Cell::new(false),
                focus_bg: Cell::new(false),
            }
        }

        fn draw(&self, c: &Context) {
            CentralPanel::default().show(c, |ui| {
                let r = ui.button("Apply");
                if self.focus_bg.get() {
                    r.request_focus();
                    self.focus_bg.set(false);
                }
                self.bg_rect.set(r.rect);
                if r.clicked() {
                    self.bg_clicked.set(true);
                }
            });
            if self.open.get() {
                dialog(c, "confirm", "Confirm", 360.0, |ui| {
                    let r = ui.button("OK");
                    self.ok_rect.set(r.rect);
                    if r.clicked() {
                        self.ok_clicked.set(true);
                    }
                });
            }
        }
    }

    #[test]
    fn clicking_the_scrim_does_not_bury_the_dialog() {
        let ctx = ctx_for(ThemeMode::Light);
        let scene = ModalScene::new();
        scene.open.set(true);
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        // Click the dimmed area on top of the page's button, then the dialog.
        click_frames(&ctx, scene.bg_rect.get().center(), |c| scene.draw(c));
        click_frames(&ctx, egui::pos2(700.0, 550.0), |c| scene.draw(c));
        click_frames(&ctx, scene.ok_rect.get().center(), |c| scene.draw(c));
        assert!(scene.ok_clicked.get());
        assert!(!scene.bg_clicked.get());
    }

    #[test]
    fn keys_cannot_reach_a_focused_widget_behind_a_dialog() {
        use egui::{Event, Key, Modifiers};
        let ctx = ctx_for(ThemeMode::Light);
        let scene = ModalScene::new();
        scene.focus_bg.set(true);
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        // The dialog opens while the page button holds keyboard focus.
        scene.open.set(true);
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        for key in [Key::Enter, Key::Space] {
            for pressed in [true, false] {
                let ev = Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                };
                let _ = ctx.run(frame_input(vec![ev]), |c| scene.draw(c));
            }
        }
        assert!(!scene.bg_clicked.get());
    }

    #[test]
    fn dismissing_an_error_toast_with_its_cross() {
        let ctx = ctx_for(ThemeMode::Light);
        let mut toasts = Toasts::default();
        toasts.push(ToastKind::Error, "Failed");
        for _ in 0..2 {
            let _ = ctx.run(frame_input(vec![]), |c| toasts.show(c, "Clear all"));
        }
        let rect = ctx
            .memory(|m| m.area_rect(egui::Id::new("wintune_toasts")))
            .unwrap();
        // The cross sits at the right edge, inside the card padding.
        let at = egui::pos2(rect.right() - 20.0, rect.center().y);
        click_frames(&ctx, at, |c| toasts.show(c, "Clear all"));
        assert!(toasts.is_empty());
    }
}

mod shell_kit {
    use super::super::test_input::{click_frames, frame_input};
    use super::super::*;
    use crate::theme::ThemeMode;
    use egui::{CentralPanel, Context, Event, Key, Modifiers, RawInput, Shape};
    use std::cell::{Cell, RefCell};

    fn ctx_for(theme: ThemeMode) -> Context {
        let ctx = Context::default();
        install_styles(&ctx);
        set_theme_mode(&ctx, theme);
        ctx
    }

    fn at(t: f64) -> RawInput {
        RawInput {
            time: Some(t),
            ..frame_input(vec![])
        }
    }

    #[test]
    fn a_queued_toast_does_not_expire_before_it_is_drawn() {
        let ctx = ctx_for(ThemeMode::Light);
        let mut toasts = Toasts::default();
        for i in 0..5 {
            toasts.push(ToastKind::Info, format!("Toast {i}"));
        }
        let _ = ctx.run(at(0.0), |c| toasts.show(c, "Clear all"));
        let _ = ctx.run(at(10.0), |c| toasts.show(c, "Clear all"));
        // Only the three drawn toasts expired; the hidden two start now.
        assert_eq!(toasts.len(), 2);
        let _ = ctx.run(at(20.0), |c| toasts.show(c, "Clear all"));
        assert_eq!(toasts.len(), 0);
    }

    #[test]
    fn hidden_error_toasts_can_be_cleared() {
        let ctx = ctx_for(ThemeMode::Light);
        let mut toasts = Toasts::default();
        for i in 0..5 {
            toasts.push(ToastKind::Error, format!("Failure {i}"));
        }
        // The anchored area needs a few frames to settle on its place.
        for _ in 0..4 {
            let _ = ctx.run(frame_input(vec![]), |c| toasts.show(c, "Clear all"));
        }
        let link = ctx
            .read_response(egui::Id::new("wintune_toasts_clear_all"))
            .expect("the clear_all link is drawn when toasts are hidden");
        click_frames(&ctx, link.rect.center(), |c| toasts.show(c, "Clear all"));
        assert!(toasts.is_empty());
    }

    #[test]
    fn toast_details_are_kept_by_latest() {
        let mut toasts = Toasts::default();
        assert_eq!(toasts.latest(), None);
        toasts.push_details(ToastKind::Error, "Applied 1", "Error: x");
        assert_eq!(toasts.latest().as_deref(), Some("Applied 1\nError: x"));
        toasts.push(ToastKind::Info, "Plain");
        assert_eq!(toasts.latest().as_deref(), Some("Plain"));
    }

    struct DialogScene {
        page_focus: Cell<bool>,
        text: RefCell<String>,
        page_id: Cell<Option<egui::Id>>,
        result: Cell<Option<bool>>,
    }

    impl DialogScene {
        fn new() -> Self {
            Self {
                page_focus: Cell::new(false),
                text: RefCell::new(String::new()),
                page_id: Cell::new(None),
                result: Cell::new(None),
            }
        }

        fn draw(&self, c: &Context) {
            CentralPanel::default().show(c, |ui| {
                let r = ui.text_edit_singleline(&mut *self.text.borrow_mut());
                if self.page_focus.replace(false) {
                    r.request_focus();
                }
                self.page_id.set(Some(r.id));
            });
            dialog(c, "d", "Confirm", 360.0, |ui| {
                if let Some(answer) = dialog_buttons(ui, "d", "Apply", "Cancel") {
                    self.result.set(Some(answer));
                }
            });
        }
    }

    fn cancel_id() -> egui::Id {
        egui::Id::new(("wintune_dialog_cancel", "d"))
    }

    fn confirm_id() -> egui::Id {
        egui::Id::new(("wintune_dialog_confirm", "d"))
    }

    fn key(key: Key, modifiers: Modifiers) -> RawInput {
        frame_input(vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }])
    }

    #[test]
    fn dialog_focus_starts_on_cancel_and_tab_stays_inside() {
        let ctx = ctx_for(ThemeMode::Light);
        let scene = DialogScene::new();
        // The page has a focused text field when the dialog opens.
        scene.page_focus.set(true);
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        let has = |id| ctx.memory(|m| m.has_focus(id));
        assert!(has(cancel_id()), "focus starts on cancel");

        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), |c| scene.draw(c));
        assert!(has(confirm_id()), "Tab moves to confirm");
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), |c| scene.draw(c));
        assert!(has(cancel_id()), "Tab wraps to cancel");
        let _ = ctx.run(key(Key::Tab, Modifiers::SHIFT), |c| scene.draw(c));
        assert!(has(confirm_id()), "Shift+Tab moves to confirm");
        assert!(!has(scene.page_id.get().unwrap()), "the page never focuses");
        assert_eq!(scene.result.get(), None);
    }

    #[test]
    fn enter_on_the_initial_focus_cancels() {
        let ctx = ctx_for(ThemeMode::Light);
        let scene = DialogScene::new();
        let _ = ctx.run(frame_input(vec![]), |c| scene.draw(c));
        let _ = ctx.run(key(Key::Enter, Modifiers::NONE), |c| scene.draw(c));
        assert_eq!(scene.result.get(), Some(false));
    }

    struct TileScene {
        rect: Cell<egui::Rect>,
        clicked: Cell<bool>,
    }

    fn tile_clicked(enabled: bool) -> bool {
        let ctx = ctx_for(ThemeMode::Light);
        let scene = TileScene {
            rect: Cell::new(egui::Rect::NOTHING),
            clicked: Cell::new(false),
        };
        let draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let r = tile(
                    ui,
                    Icon::Explorer,
                    "Explorer",
                    "3 tweaks",
                    enabled,
                    Some("Not yet"),
                );
                scene.rect.set(r.rect);
                if r.clicked() {
                    scene.clicked.set(true);
                }
            });
        };
        let _ = ctx.run(frame_input(vec![]), draw);
        click_frames(&ctx, scene.rect.get().center(), draw);
        scene.clicked.get()
    }

    #[test]
    fn a_focused_tile_paints_a_focus_ring() {
        fn ring(shape: &Shape) -> bool {
            match shape {
                Shape::Rect(r) => r.stroke.width == 2.0,
                Shape::Vec(v) => v.iter().any(ring),
                _ => false,
            }
        }
        let ctx = ctx_for(ThemeMode::Light);
        let draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                tile(ui, Icon::Explorer, "Explorer", "3 tweaks", true, None);
            });
        };
        let has_ring = |out: &egui::FullOutput| out.shapes.iter().any(|c| ring(&c.shape));
        let _ = ctx.run(frame_input(vec![]), draw);
        let before = ctx.run(frame_input(vec![]), draw);
        assert!(!has_ring(&before), "no ring while unfocused");
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), draw);
        let after = ctx.run(frame_input(vec![]), draw);
        assert!(
            has_ring(&after),
            "Tab focuses the tile and it paints a ring"
        );
    }

    #[test]
    fn a_focused_nav_row_paints_a_focus_ring() {
        fn ring(shape: &Shape) -> bool {
            match shape {
                Shape::Rect(r) => r.stroke.width == 2.0,
                Shape::Vec(v) => v.iter().any(ring),
                _ => false,
            }
        }
        let ctx = ctx_for(ThemeMode::Light);
        let brand = Brand {
            logo: None,
            name: "WinTune",
            caption: "x",
        };
        let draw = |c: &Context| {
            let mut sel = 0_u8;
            let items = vec![NavItem {
                key: 1_u8,
                icon: Icon::Explorer,
                label: "Row",
                badge: None,
                enabled: true,
                disabled_hint: None,
            }];
            let groups = [NavGroup { title: None, items }];
            nav_sidebar(c, &brand, &mut sel, &groups, &[]);
        };
        let has_ring = |out: &egui::FullOutput| out.shapes.iter().any(|c| ring(&c.shape));
        let _ = ctx.run(frame_input(vec![]), draw);
        let before = ctx.run(frame_input(vec![]), draw);
        assert!(!has_ring(&before), "no ring while unfocused");
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), draw);
        let after = ctx.run(frame_input(vec![]), draw);
        assert!(has_ring(&after), "Tab focuses the row and it paints a ring");
    }

    fn has_two_px_ring(out: &egui::FullOutput) -> bool {
        fn ring(shape: &Shape) -> bool {
            match shape {
                Shape::Rect(r) => r.stroke.width == 2.0,
                Shape::Vec(v) => v.iter().any(ring),
                _ => false,
            }
        }
        out.shapes.iter().any(|c| ring(&c.shape))
    }

    #[test]
    fn a_focused_segmented_option_paints_a_focus_ring() {
        let ctx = ctx_for(ThemeMode::Light);
        let draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let mut v = 1_u8;
                segmented(ui, &mut v, &[(1, "A"), (2, "B")]);
            });
        };
        let _ = ctx.run(frame_input(vec![]), draw);
        let before = ctx.run(frame_input(vec![]), draw);
        assert!(!has_two_px_ring(&before), "no ring while unfocused");
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), draw);
        let after = ctx.run(frame_input(vec![]), draw);
        assert!(has_two_px_ring(&after), "Tab focuses an option: ring");
    }

    #[test]
    fn space_on_a_focused_segmented_option_selects_it() {
        let ctx = ctx_for(ThemeMode::Light);
        let value = Cell::new(1_u8);
        let changed = Cell::new(false);
        let draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                let mut v = value.get();
                let r = segmented(ui, &mut v, &[(1, "A"), (2, "B")]);
                value.set(v);
                changed.set(r.changed());
            });
        };
        let _ = ctx.run(frame_input(vec![]), draw);
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), draw);
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), draw);
        assert_eq!(value.get(), 1);
        let _ = ctx.run(key(Key::Space, Modifiers::NONE), draw);
        assert_eq!(value.get(), 2);
        assert!(changed.get(), "changed() on the selecting frame");
    }

    #[test]
    fn a_focused_disclosure_header_paints_a_focus_ring() {
        let ctx = ctx_for(ThemeMode::Light);
        let draw = |c: &Context| {
            CentralPanel::default().show(c, |ui| {
                disclosure(ui, "d", "Advanced", false, |_| {});
            });
        };
        let _ = ctx.run(frame_input(vec![]), draw);
        let before = ctx.run(frame_input(vec![]), draw);
        assert!(!has_two_px_ring(&before), "no ring while unfocused");
        let _ = ctx.run(key(Key::Tab, Modifiers::NONE), draw);
        let after = ctx.run(frame_input(vec![]), draw);
        assert!(has_two_px_ring(&after), "Tab focuses the header: ring");
    }

    #[test]
    fn status_dot_draws_a_ring_when_not_applied() {
        fn circles(shape: &Shape, out: &mut Vec<egui::epaint::CircleShape>) {
            match shape {
                Shape::Circle(c) => out.push(*c),
                Shape::Vec(v) => v.iter().for_each(|s| circles(s, out)),
                _ => {}
            }
        }
        let ctx = ctx_for(ThemeMode::Light);
        let dots = |kind: Option<PillKind>| {
            let draw = |c: &Context| {
                CentralPanel::default().show(c, |ui| {
                    status_dot(ui, kind, None);
                });
            };
            let _ = ctx.run(frame_input(vec![]), draw);
            let out = ctx.run(frame_input(vec![]), draw);
            let mut v = Vec::new();
            out.shapes.iter().for_each(|c| circles(&c.shape, &mut v));
            v
        };
        let none = dots(None);
        assert!(
            none.iter()
                .any(|c| c.fill == egui::Color32::TRANSPARENT && c.stroke.width > 0.0),
            "not applied draws a ring with no fill"
        );
        let ok = dots(Some(PillKind::Success));
        assert!(
            ok.iter().any(|c| c.fill != egui::Color32::TRANSPARENT),
            "a kind draws a filled circle"
        );
    }

    #[test]
    fn a_disabled_tile_does_not_click() {
        assert!(!tile_clicked(false));
    }

    #[test]
    fn an_enabled_tile_clicks() {
        assert!(tile_clicked(true));
    }

    fn texts(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(t) => out.push(t.galley.text().to_owned()),
            Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
            _ => {}
        }
    }

    #[test]
    fn the_nav_draws_the_brand_name() {
        #[derive(Clone, Copy, PartialEq)]
        struct Key0;
        let ctx = ctx_for(ThemeMode::Light);
        let brand = Brand {
            logo: None,
            name: "WinTune",
            caption: "Customize Windows",
        };
        let out = ctx.run(frame_input(vec![]), |c| {
            let mut sel = Key0;
            nav_sidebar(c, &brand, &mut sel, &[], &[]);
        });
        let mut found = Vec::new();
        for clipped in &out.shapes {
            texts(&clipped.shape, &mut found);
        }
        assert!(found.iter().any(|t| t == "WinTune"), "texts: {found:?}");
        assert!(found.iter().any(|t| t == "Customize Windows"));
    }
}
