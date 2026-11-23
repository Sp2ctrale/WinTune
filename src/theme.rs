use eframe::egui::{self, Color32, Frame, Margin, Rounding, Stroke};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    /// Follow the Windows light / dark setting.
    #[default]
    System,
    Dark,
    Light,
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
        // `System` is never passed once the UI gives these helpers
        // `ui::kit::resolved(...)`; until then it draws dark, as before.
        ThemeMode::Dark | ThemeMode::System => FluentColors {
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

/// Small colored pill. The returned response accepts a hover text.
pub fn draw_badge(ui: &mut egui::Ui, label: &str, color: Color32) -> egui::Response {
    let bg = color.gamma_multiply(0.18);
    Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.5)))
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(11.0).color(color).strong());
        })
        .response
}
