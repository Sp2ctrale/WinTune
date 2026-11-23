//! Containers: card, section, setting row, banner and empty state.

use egui::{vec2, Align, Frame, Layout, Margin, RichText, Rounding, Stroke, Ui};

use super::controls::action_button;
use super::fonts::semibold;
use super::icons::icon_text;
use super::{current_palette, radius, space, text_size, Icon};

/// Runs `leading` on the left and `trailing` right-aligned on the same row,
/// at least `height` tall. Returns the trailing result.
fn split_row<R>(
    ui: &mut Ui,
    height: f32,
    trailing: impl FnOnce(&mut Ui) -> R,
    leading: impl FnOnce(&mut Ui),
) -> R {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(width, height),
        Layout::right_to_left(Align::Center),
        |ui| {
            let result = trailing(ui);
            ui.with_layout(Layout::left_to_right(Align::Center), leading);
            result
        },
    )
    .inner
}

/// White rounded panel with a hairline border.
pub fn card<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let p = current_palette(ui.ctx());
    let inner_width = ui.available_width() - 2.0 * space::L - 2.0;
    Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(radius::CARD))
        .inner_margin(Margin::symmetric(space::L, space::S))
        .show(ui, |ui| {
            ui.set_width(inner_width.max(0.0));
            body(ui)
        })
        .inner
}

/// A semibold title above a card.
pub fn section<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let p = current_palette(ui.ctx());
    ui.label(
        RichText::new(title)
            .font(semibold(ui.ctx(), text_size::SECTION))
            .color(p.text),
    );
    ui.add_space(space::S);
    let result = card(ui, body);
    ui.add_space(space::L);
    result
}

/// One line of a settings card: icon, title and description on the left, a
/// control on the right.
pub struct SettingRow<'a> {
    pub icon: Option<Icon>,
    pub title: &'a str,
    pub description: Option<&'a str>,
}

impl SettingRow<'_> {
    pub fn show<R>(self, ui: &mut Ui, trailing: impl FnOnce(&mut Ui) -> R) -> R {
        let p = current_palette(ui.ctx());
        split_row(ui, 48.0, trailing, |ui| {
            if let Some(icon) = self.icon.and_then(|i| icon_text(ui.ctx(), i, 20.0)) {
                ui.label(icon.color(p.text));
                ui.add_space(space::S);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    RichText::new(self.title)
                        .size(text_size::BODY)
                        .color(p.text),
                );
                if let Some(d) = self.description {
                    ui.label(
                        RichText::new(d)
                            .size(text_size::SECONDARY)
                            .color(p.text_secondary),
                    );
                }
            });
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Warning,
    Error,
}

/// Full-width message with a tinted background. Returns true when the
/// optional action button was clicked.
pub fn banner(ui: &mut Ui, kind: BannerKind, text: &str, action: Option<&str>) -> bool {
    let p = current_palette(ui.ctx());
    let (color, icon) = match kind {
        BannerKind::Info => (p.accent, Icon::Info),
        BannerKind::Warning => (p.warning, Icon::Warning),
        BannerKind::Error => (p.danger, Icon::Error),
    };
    let inner_width = ui.available_width() - 2.0 * space::M - 2.0;
    let mut clicked = false;
    Frame::none()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.4)))
        .rounding(Rounding::same(radius::CARD))
        .inner_margin(Margin::symmetric(space::M, space::S))
        .show(ui, |ui| {
            ui.set_width(inner_width.max(0.0));
            clicked = split_row(
                ui,
                32.0,
                |ui| action.is_some_and(|a| action_button(ui, a, false).clicked()),
                |ui| {
                    if let Some(i) = icon_text(ui.ctx(), icon, 18.0) {
                        ui.label(i.color(color));
                        ui.add_space(space::S);
                    }
                    ui.label(RichText::new(text).size(text_size::BODY).color(p.text));
                },
            );
        });
    ui.add_space(space::S);
    clicked
}

/// Centered placeholder for an empty list. Returns true when the optional
/// action button was clicked.
pub fn empty_state(ui: &mut Ui, icon: Icon, text: &str, action: Option<&str>) -> bool {
    let p = current_palette(ui.ctx());
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space(space::XXL);
        if let Some(i) = icon_text(ui.ctx(), icon, 36.0) {
            ui.label(i.color(p.text_secondary));
            ui.add_space(space::S);
        }
        ui.label(
            RichText::new(text)
                .size(text_size::BODY)
                .color(p.text_secondary),
        );
        if let Some(a) = action {
            ui.add_space(space::M);
            clicked = action_button(ui, a, true).clicked();
        }
        ui.add_space(space::XXL);
    });
    clicked
}
