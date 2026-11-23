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

#[test]
fn glyphs_are_distinct_private_use_code_points() {
    use Icon::*;
    let all = [
        Home,
        Associations,
        Explorer,
        Start,
        ContextMenu,
        Icons,
        History,
        Settings,
        Search,
        Add,
        Delete,
        Restore,
        Warning,
        Info,
        Error,
        Success,
        ChevronRight,
        ChevronDown,
        More,
        Folder,
    ];
    for i in all {
        let c = icons::glyph(i) as u32;
        assert!((0xE000..=0xF8FF).contains(&c), "{i:?} is {c:X}");
    }
    assert_eq!(icons::glyph(Home), '\u{E80F}');
    assert_eq!(icons::glyph(Folder), '\u{E8B7}');
}
