//! The egui styles built from the tokens, and the theme switch.
//!
//! egui keeps one [`Style`] per theme and picks one every frame from the
//! theme preference (and, for `System`, from what Windows reports). So
//! [`install_styles`] runs once, and switching theme is only a preference
//! change: no style is rebuilt.

use super::tokens::{palette, radius, space, text_size, Palette};
use crate::models::ThemeMode;
use egui::{
    Color32, Context, FontId, Margin, Rounding, Stroke, Style, TextStyle, Theme, ThemePreference,
    Visuals,
};

/// Sets the light and the dark style. Call once, at start-up.
pub fn install_styles(ctx: &Context) {
    for theme in [Theme::Light, Theme::Dark] {
        let mut style = (*ctx.style_of(theme)).clone();
        apply_tokens(&mut style, theme);
        ctx.set_style_of(theme, style);
    }
}

/// Follows Windows for `System`, or forces light / dark.
pub fn set_theme_mode(ctx: &Context, mode: ThemeMode) {
    ctx.set_theme(match mode {
        ThemeMode::System => ThemePreference::System,
        ThemeMode::Light => ThemePreference::Light,
        ThemeMode::Dark => ThemePreference::Dark,
    });
}

/// The palette of the theme egui draws with this frame.
pub fn current_palette(ctx: &Context) -> &'static Palette {
    palette(ctx.theme())
}

/// `mode` with `System` replaced by the theme in use: never returns `System`.
pub fn resolved(mode: ThemeMode, ctx: &Context) -> ThemeMode {
    match mode {
        ThemeMode::System => match ctx.theme() {
            Theme::Light => ThemeMode::Light,
            Theme::Dark => ThemeMode::Dark,
        },
        explicit => explicit,
    }
}

/// `a` moved toward `b` by `t` (0 = `a`, 1 = `b`), on opaque colors.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

fn apply_tokens(style: &mut Style, theme: Theme) {
    let p = palette(theme);

    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(space::S, space::S);
    spacing.button_padding = egui::vec2(space::M, space::XS);
    spacing.menu_margin = Margin::same(space::S);
    spacing.window_margin = Margin::same(space::L);
    spacing.indent = space::XL;
    spacing.icon_spacing = space::XS;
    spacing.interact_size.y = space::XL;

    for (text_style, size) in [
        (TextStyle::Heading, text_size::TITLE),
        (TextStyle::Body, text_size::BODY),
        (TextStyle::Button, text_size::BODY),
        (TextStyle::Small, text_size::SECONDARY),
        (TextStyle::Monospace, text_size::SECONDARY),
    ] {
        let family = style
            .text_styles
            .get(&text_style)
            .map(|f| f.family.clone())
            .unwrap_or_default();
        style
            .text_styles
            .insert(text_style, FontId::new(size, family));
    }

    style.visuals = visuals(theme, p);
}

fn visuals(theme: Theme, p: &Palette) -> Visuals {
    let mut v = match theme {
        Theme::Light => Visuals::light(),
        Theme::Dark => Visuals::dark(),
    };
    let hairline = Stroke::new(1.0_f32, p.border);
    let control_rounding = Rounding::same(radius::CONTROL);
    // Windows lightens a control a little on hover and dims it toward the
    // page while pressed; neither shade is worth a token of its own.
    let control_hover = mix(p.control, p.text, 0.03);
    let control_pressed = mix(p.control, p.page, 0.5);

    v.override_text_color = None;
    v.panel_fill = p.page;
    v.window_fill = p.surface;
    v.window_stroke = hairline;
    v.window_rounding = Rounding::same(radius::CARD);
    v.menu_rounding = Rounding::same(radius::CARD);
    v.faint_bg_color = p.surface_hover;
    v.extreme_bg_color = p.control;
    v.code_bg_color = p.control;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.text_cursor.stroke.color = p.text;

    // Selected text, selected items and focused text boxes. The fill is a
    // translucent accent so the text drawn over it stays readable.
    v.selection.bg_fill = p.accent.gamma_multiply(match theme {
        Theme::Light => 0.2,
        Theme::Dark => 0.3,
    });
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = hairline;
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);

    w.inactive.bg_fill = p.control;
    w.inactive.weak_bg_fill = p.control;
    w.inactive.bg_stroke = hairline;
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);

    w.hovered.bg_fill = control_hover;
    w.hovered.weak_bg_fill = control_hover;
    w.hovered.bg_stroke = hairline;
    w.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);

    w.active.bg_fill = control_pressed;
    w.active.weak_bg_fill = control_pressed;
    w.active.bg_stroke = hairline;
    // egui draws strong text with this color, so it stays the primary text
    // color; the pressed state is carried by the fill alone.
    w.active.fg_stroke = Stroke::new(1.0_f32, p.text);

    w.open.bg_fill = control_hover;
    w.open.weak_bg_fill = control_hover;
    w.open.bg_stroke = hairline;
    w.open.fg_stroke = Stroke::new(1.0_f32, p.text);

    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.rounding = control_rounding;
        // Fluent controls do not grow under the pointer.
        state.expansion = 0.0;
    }

    v
}
