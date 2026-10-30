use eframe::egui::{self, Color32, Context, Frame, Margin, Rounding, Stroke, Style, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

pub struct FluentColors {
    pub bg_base: Color32,
    pub bg_sidebar: Color32,
    pub bg_card: Color32,
    pub bg_input: Color32,
    pub bg_hover: Color32,
    pub border: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub success: Color32,
    pub warning: Color32,
}

pub fn get_palette(mode: ThemeMode) -> FluentColors {
    match mode {
        ThemeMode::Dark => FluentColors {
            bg_base: Color32::from_rgb(20, 22, 27),
            bg_sidebar: Color32::from_rgb(26, 28, 35),
            bg_card: Color32::from_rgb(31, 34, 42),
            bg_input: Color32::from_rgb(22, 24, 30),
            bg_hover: Color32::from_rgb(42, 46, 57),
            border: Color32::from_rgb(48, 53, 66),
            text_primary: Color32::from_rgb(243, 244, 246),
            text_secondary: Color32::from_rgb(156, 163, 175),
            accent: Color32::from_rgb(0, 120, 212),
            accent_hover: Color32::from_rgb(26, 140, 230),
            success: Color32::from_rgb(16, 185, 129),
            warning: Color32::from_rgb(245, 158, 11),
        },
        ThemeMode::Light => FluentColors {
            bg_base: Color32::from_rgb(243, 244, 248),
            bg_sidebar: Color32::from_rgb(235, 237, 242),
            bg_card: Color32::from_rgb(255, 255, 255),
            bg_input: Color32::from_rgb(249, 250, 252),
            bg_hover: Color32::from_rgb(226, 230, 238),
            border: Color32::from_rgb(212, 216, 226),
            text_primary: Color32::from_rgb(24, 26, 32),
            text_secondary: Color32::from_rgb(100, 110, 125),
            accent: Color32::from_rgb(0, 103, 192),
            accent_hover: Color32::from_rgb(0, 120, 212),
            success: Color32::from_rgb(5, 150, 105),
            warning: Color32::from_rgb(217, 119, 6),
        },
    }
}

pub fn apply_fluent_theme(ctx: &Context, mode: ThemeMode) {
    let p = get_palette(mode);
    let mut style: Style = (*ctx.style()).clone();

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = Margin::same(14.0);

    let mut visuals = match mode {
        ThemeMode::Dark => Visuals::dark(),
        ThemeMode::Light => Visuals::light(),
    };

    visuals.panel_fill = p.bg_base;
    visuals.window_fill = p.bg_card;
    visuals.extreme_bg_color = p.bg_input;
    visuals.faint_bg_color = p.bg_card;
    visuals.override_text_color = Some(p.text_primary);

    let rounding = Rounding::same(6.0);
    visuals.window_rounding = Rounding::same(10.0);

    visuals.widgets.noninteractive.bg_fill = p.bg_card;
    visuals.widgets.noninteractive.weak_bg_fill = p.bg_card;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text_primary);
    visuals.widgets.noninteractive.rounding = rounding;

    visuals.widgets.inactive.bg_fill = p.bg_input;
    visuals.widgets.inactive.weak_bg_fill = p.bg_card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text_primary);
    visuals.widgets.inactive.rounding = rounding;

    visuals.widgets.hovered.bg_fill = p.bg_hover;
    visuals.widgets.hovered.weak_bg_fill = p.bg_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, p.text_primary);
    visuals.widgets.hovered.rounding = rounding;

    visuals.widgets.active.bg_fill = p.accent;
    visuals.widgets.active.weak_bg_fill = p.accent;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent_hover);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.active.rounding = rounding;

    visuals.selection.bg_fill = p.accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);

    style.visuals = visuals;
    ctx.set_style(style);
}

pub fn card_frame(mode: ThemeMode) -> Frame {
    let p = get_palette(mode);
    Frame::none()
        .fill(p.bg_card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(14.0))
}

pub fn sidebar_frame(mode: ThemeMode) -> Frame {
    let p = get_palette(mode);
    Frame::none()
        .fill(p.bg_sidebar)
        .stroke(Stroke::new(1.0_f32, p.border))
        .inner_margin(Margin::symmetric(12.0, 14.0))
}

pub fn accent_button<'a>(text: impl Into<egui::WidgetText>, mode: ThemeMode) -> egui::Button<'a> {
    let p = get_palette(mode);
    egui::Button::new(
        egui::RichText::new(text.into().text())
            .color(Color32::WHITE)
            .strong(),
    )
    .fill(p.accent)
    .rounding(Rounding::same(6.0))
}

pub fn draw_badge(ui: &mut egui::Ui, label: &str, color: Color32) {
    let bg = color.gamma_multiply(0.18);
    Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.5)))
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(11.0).color(color).strong());
        });
}
