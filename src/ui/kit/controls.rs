//! Small interactive controls: toggle, pill, segmented choice, disclosure and
//! the action button. They return `Response`s or values and never act.

use egui::{
    pos2, vec2, Align2, CursorIcon, FontId, Galley, Id, Rect, Response, Rounding, Sense, Shape,
    Stroke, Ui, WidgetInfo, WidgetType,
};
use std::sync::Arc;

use super::{current_palette, radius, space, text_size};

/// Windows 11 style on/off switch, 40x20, accent when on.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    toggle_enabled(ui, on, None)
}

/// A toggle that, given a reason, is greyed out, ignores clicks, never
/// changes `on` and shows the reason on hover.
pub fn toggle_enabled(ui: &mut Ui, on: &mut bool, disabled_reason: Option<&str>) -> Response {
    let p = current_palette(ui.ctx());
    let enabled = disabled_reason.is_none();
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, mut response) = ui.allocate_exact_size(vec2(40.0, 20.0), sense);
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if let Some(reason) = disabled_reason {
        response = response.on_hover_text(reason);
    } else {
        response = response.on_hover_cursor(CursorIcon::PointingHand);
    }
    let value = *on;
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, value, ""));

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, value);
        let hovered = enabled && response.hovered();
        let (fill, stroke, knob_off, knob_on) = if !enabled {
            (
                if value { p.text_disabled } else { p.control },
                Stroke::new(1.0_f32, if value { p.text_disabled } else { p.border }),
                p.text_disabled,
                p.on_accent,
            )
        } else if value {
            let f = if hovered { p.accent_hover } else { p.accent };
            (f, Stroke::new(1.0_f32, f), p.on_accent, p.on_accent)
        } else {
            (
                if hovered { p.surface_hover } else { p.control },
                Stroke::new(1.0_f32, p.text_secondary),
                p.text_secondary,
                p.on_accent,
            )
        };
        let painter = ui.painter();
        painter.rect(rect, Rounding::same(rect.height() / 2.0), fill, stroke);
        // The knob grows slightly while on, as in Windows.
        let knob_r = 5.0 + t;
        let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
        painter.circle_filled(
            pos2(x, rect.center().y),
            knob_r,
            knob_off.lerp_to_gamma(knob_on, t),
        );
    }
    response
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillKind {
    Success,
    Warning,
    Accent,
    Neutral,
    Danger,
}

/// Small status label with a soft tinted background.
pub fn pill(ui: &mut Ui, text: &str, kind: PillKind) -> Response {
    let p = current_palette(ui.ctx());
    let color = match kind {
        PillKind::Success => p.success,
        PillKind::Warning => p.warning,
        PillKind::Accent => p.accent,
        PillKind::Neutral => p.neutral,
        PillKind::Danger => p.danger,
    };
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(text_size::SECONDARY),
        color,
    );
    let size = galley.size() + vec2(2.0 * space::S, 2.0 * space::XS - 2.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            Rounding::same(radius::CONTROL),
            color.gamma_multiply(0.14),
            Stroke::NONE,
        );
        painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    }
    response
}

/// An 8 px status dot in a 16x16 slot: filled with the kind's color, or a
/// 1 px ring when `kind` is `None` (not applied). `hover` is the hover text.
pub fn status_dot(ui: &mut Ui, kind: Option<PillKind>, hover: Option<&str>) -> Response {
    let p = current_palette(ui.ctx());
    let (rect, mut response) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        match kind {
            Some(kind) => {
                let color = match kind {
                    PillKind::Success => p.success,
                    PillKind::Warning => p.warning,
                    PillKind::Accent => p.accent,
                    PillKind::Neutral => p.neutral,
                    PillKind::Danger => p.danger,
                };
                painter.circle_filled(rect.center(), 4.0, color);
            }
            None => {
                painter.circle_stroke(rect.center(), 3.5, Stroke::new(1.0_f32, p.border));
            }
        }
    }
    if let Some(text) = hover {
        response = response.on_hover_text(text);
    }
    response
}

/// Joined buttons of which exactly one is selected. The response reports
/// `changed()` when the user picked another option.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &str)],
) -> Response {
    let p = current_palette(ui.ctx());
    let font = FontId::proportional(text_size::BODY);
    let galleys: Vec<_> = options
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), p.text)
        })
        .collect();
    let widths: Vec<f32> = galleys
        .iter()
        .map(|g| g.size().x + 2.0 * space::L)
        .collect();
    let height = 32.0;
    let total = widths.iter().sum::<f32>() + 2.0;
    let (rect, mut response) = ui.allocate_exact_size(vec2(total, height), Sense::hover());

    // Interact first (pointer input), then paint with the updated value.
    let mut x = rect.left() + 1.0;
    let mut cells = Vec::with_capacity(options.len());
    for (i, w) in widths.iter().enumerate() {
        let cell = egui::Rect::from_min_size(pos2(x, rect.top() + 1.0), vec2(*w, height - 2.0));
        let r = ui
            .interact(cell, response.id.with(i), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if r.clicked() && *value != options[i].0 {
            *value = options[i].0;
            response.mark_changed();
        }
        let selected = *value == options[i].0;
        let label = options[i].1;
        r.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, label));
        cells.push((cell, r.hovered(), r.has_focus()));
        x += w;
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            Rounding::same(radius::CONTROL),
            p.control,
            Stroke::new(1.0_f32, p.border),
        );
        for (i, ((cell, hovered, focused), galley)) in cells.into_iter().zip(galleys).enumerate() {
            let selected = *value == options[i].0;
            let (fill, text) = if selected {
                (Some(p.accent), p.on_accent)
            } else if hovered {
                (Some(p.surface_hover), p.text)
            } else {
                (None, p.text)
            };
            if let Some(fill) = fill {
                painter.rect_filled(cell, Rounding::same(radius::CONTROL - 1.0), fill);
            }
            painter.galley(cell.center() - galley.size() / 2.0, galley, text);
            if focused {
                painter.rect_stroke(
                    cell,
                    Rounding::same(radius::CONTROL - 1.0),
                    Stroke::new(2.0_f32, p.accent),
                );
            }
        }
    }
    response
}

/// Collapsible block: a clickable header with a chevron and, when open, the
/// body. Open state is remembered under `id_salt`.
pub fn disclosure<R>(
    ui: &mut Ui,
    id_salt: &str,
    title: &str,
    default_open: bool,
    body: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let p = current_palette(ui.ctx());
    let id: Id = ui.make_persistent_id(("kit_disclosure", id_salt));
    let mut open = ui
        .ctx()
        .data_mut(|d| d.get_persisted::<bool>(id))
        .unwrap_or(default_open);

    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    if response.clicked() {
        open = !open;
        ui.ctx().data_mut(|d| d.insert_persisted(id, open));
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::CollapsingHeader, true, title));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(id, open);
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, Rounding::same(radius::CONTROL), p.surface_hover);
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect,
                Rounding::same(radius::CONTROL),
                Stroke::new(2.0_f32, p.accent),
            );
        }
        // Chevron: points right when closed, down when open.
        let c = pos2(rect.left() + space::M + 4.0, rect.center().y);
        let a = egui::lerp(-std::f32::consts::FRAC_PI_2..=0.0, t);
        let rot = |dx: f32, dy: f32| {
            let (s, co) = a.sin_cos();
            pos2(c.x + dx * co - dy * s, c.y + dx * s + dy * co)
        };
        painter.add(Shape::line(
            vec![rot(-4.0, -2.0), rot(0.0, 2.0), rot(4.0, -2.0)],
            Stroke::new(1.5_f32, p.text_secondary),
        ));
        painter.text(
            pos2(rect.left() + space::XL + space::S, rect.center().y),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(text_size::BODY),
            p.text,
        );
    }
    if open {
        Some(
            ui.indent(id, |ui| {
                ui.add_space(space::XS);
                body(ui)
            })
            .inner,
        )
    } else {
        None
    }
}

/// Text button used by banners and empty states. `primary` draws it with the
/// accent fill.
pub(super) fn action_button(ui: &mut Ui, text: &str, primary: bool) -> Response {
    let galley = action_button_galley(ui, text, primary);
    let size = vec2(galley.size().x + 2.0 * space::L, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    paint_action_button(ui, rect, response, text, primary, galley)
}

/// `action_button` with a stable id, so callers can ask whether it has focus.
pub(super) fn action_button_with_id(ui: &mut Ui, id: Id, text: &str, primary: bool) -> Response {
    let galley = action_button_galley(ui, text, primary);
    let size = vec2(galley.size().x + 2.0 * space::L, 32.0);
    let (_, rect) = ui.allocate_space(size);
    let response = ui.interact(rect, id, Sense::click());
    paint_action_button(ui, rect, response, text, primary, galley)
}

fn action_button_galley(ui: &Ui, text: &str, primary: bool) -> Arc<Galley> {
    let p = current_palette(ui.ctx());
    let color = if primary { p.on_accent } else { p.text };
    ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(text_size::BODY),
        color,
    )
}

fn paint_action_button(
    ui: &Ui,
    rect: Rect,
    response: Response,
    text: &str,
    primary: bool,
    galley: Arc<Galley>,
) -> Response {
    let p = current_palette(ui.ctx());
    let (fill_rest, fill_hover, stroke, text_color) = if primary {
        (p.accent, p.accent_hover, Stroke::NONE, p.on_accent)
    } else {
        (
            p.control,
            p.surface_hover,
            Stroke::new(1.0_f32, p.border),
            p.text,
        )
    };
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, text));
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            fill_hover
        } else {
            fill_rest
        };
        let painter = ui.painter();
        painter.rect(rect, Rounding::same(radius::CONTROL), fill, stroke);
        if response.has_focus() {
            painter.rect_stroke(
                rect.expand(1.0),
                Rounding::same(radius::CONTROL),
                Stroke::new(2.0_f32, p.accent),
            );
        }
        painter.galley(rect.center() - galley.size() / 2.0, galley, text_color);
    }
    response
}
