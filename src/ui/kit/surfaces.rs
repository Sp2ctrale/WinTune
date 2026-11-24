//! Containers: card, section, setting row, banner and empty state.

use egui::{
    pos2, vec2, Align, CursorIcon, FontFamily, FontId, Frame, Layout, Margin, Rect, Response,
    RichText, Rounding, Sense, Stroke, Ui, WidgetInfo, WidgetType,
};

use super::controls::action_button;
use super::fonts::{semibold, ICONS};
use super::icons::{glyph, icon_text, icons_available};
use super::{current_palette, radius, space, text_size, Icon};

/// Runs `leading` on the left and `trailing` right-aligned on the same row,
/// at least `height` tall. Returns the trailing result.
pub(super) fn split_row<R>(
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
    /// A tooltip for the whole row, not only its control.
    pub hover: Option<&'a str>,
}

impl SettingRow<'_> {
    /// `trailing` is laid out right-to-left: the first widget it adds sits at
    /// the far right edge.
    pub fn show<R>(self, ui: &mut Ui, trailing: impl FnOnce(&mut Ui) -> R) -> R {
        let p = current_palette(ui.ctx());
        let row = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 48.0));
        let result = split_row(ui, 48.0, trailing, |ui| {
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
        });
        if let Some(hover) = self.hover {
            let id = ui
                .id()
                .with(("setting_row_hover", self.title, row.min.y as i32));
            ui.interact(row, id, Sense::hover()).on_hover_text(hover);
        }
        result
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
                    ui.add(
                        egui::Label::new(RichText::new(text).size(text_size::BODY).color(p.text))
                            .wrap(),
                    );
                },
            );
        });
    ui.add_space(space::S);
    clicked
}

const TILE_HEIGHT: f32 = 88.0;

/// A clickable card (Home's module cards): icon, title and a status line.
/// It only senses clicks while `enabled`; when disabled the text is dimmed
/// and `disabled_hint` shows on hover.
pub fn tile(
    ui: &mut Ui,
    icon: Icon,
    title: &str,
    status: &str,
    enabled: bool,
    disabled_hint: Option<&str>,
) -> Response {
    let p = current_palette(ui.ctx());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), TILE_HEIGHT), sense);
    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else if let Some(hint) = disabled_hint {
        response.on_hover_text(hint)
    } else {
        response
    };
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, title));
    if ui.is_rect_visible(rect) {
        let fill = if enabled && response.hovered() {
            p.surface_hover
        } else {
            p.surface
        };
        let (title_color, status_color) = if enabled {
            (p.text, p.text_secondary)
        } else {
            (p.text_disabled, p.text_disabled)
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            Rounding::same(radius::CARD),
            fill,
            Stroke::new(1.0_f32, p.border),
        );
        if enabled && response.has_focus() {
            painter.rect_stroke(
                rect.expand(1.0),
                Rounding::same(radius::CARD),
                Stroke::new(2.0_f32, p.accent),
            );
        }
        let mut x = rect.left() + space::L;
        if icons_available(ui.ctx()) {
            let galley = painter.layout_no_wrap(
                glyph(icon).to_string(),
                FontId::new(24.0, FontFamily::Name(ICONS.into())),
                title_color,
            );
            painter.galley(
                pos2(x, rect.center().y - galley.size().y / 2.0),
                galley,
                title_color,
            );
            x += 24.0 + space::M;
        }
        let title_galley = painter.layout_no_wrap(
            title.to_owned(),
            semibold(ui.ctx(), text_size::BODY),
            title_color,
        );
        let status_galley = painter.layout_no_wrap(
            status.to_owned(),
            FontId::proportional(text_size::SECONDARY),
            status_color,
        );
        let total = title_galley.size().y + 2.0 + status_galley.size().y;
        let top = rect.center().y - total / 2.0;
        let status_y = top + title_galley.size().y + 2.0;
        painter.galley(pos2(x, top), title_galley, title_color);
        painter.galley(pos2(x, status_y), status_galley, status_color);
    }
    response
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
