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
