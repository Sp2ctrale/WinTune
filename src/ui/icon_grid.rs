//! The grid of icon cards shared by the Icons page and the icon picker.

use super::kit::{self, radius, space, text_size};
use crate::gallery::IconGallery;
use eframe::egui::{self, RichText, Sense, Stroke};

/// Size of one grid slot; the card inside leaves a gap around it.
pub(super) const GALLERY_CELL: egui::Vec2 = egui::vec2(130.0, 98.0);
pub(super) const GALLERY_GAP: f32 = 8.0;

/// The card rect for column `col` of a row starting at `row_min`.
pub(super) fn gallery_cell_rect(row_min: egui::Pos2, col: usize) -> egui::Rect {
    egui::Rect::from_min_size(
        row_min + egui::vec2(col as f32 * GALLERY_CELL.x, 0.0),
        GALLERY_CELL - egui::vec2(GALLERY_GAP, GALLERY_GAP),
    )
}

/// Draws `names` as a scrolling grid of fixed-size cards and returns the name
/// of the icon clicked this frame. `current` gets an accent border. Only the
/// visible rows are laid out, so only visible icons get queued for
/// (background) decoding; every cell has the same rect, so long names or
/// loading states cannot shift the columns. `id_salt` keeps the scroll state
/// of two grids apart. With `interactive` false the cards only browse: no
/// hand cursor, hover fill or focus, and nothing is ever returned.
pub(super) fn icon_grid(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    gallery: &IconGallery,
    names: &[String],
    current: Option<&str>,
    id_salt: &str,
    interactive: bool,
) -> Option<String> {
    let cols = ((ui.available_width() / GALLERY_CELL.x).floor() as usize).max(1);
    let total_rows = names.len().div_ceil(cols);
    let mut chosen: Option<String> = None;

    egui::ScrollArea::vertical().id_salt(id_salt).show_rows(
        ui,
        GALLERY_CELL.y,
        total_rows,
        |ui, row_range| {
            for row in row_range {
                let (row_rect, _) = ui.allocate_exact_size(
                    egui::vec2(cols as f32 * GALLERY_CELL.x, GALLERY_CELL.y),
                    Sense::hover(),
                );
                let start = row * cols;
                let end = (start + cols).min(names.len());
                for (col, name) in names[start..end].iter().enumerate() {
                    let cell = gallery_cell_rect(row_rect.min, col);
                    let is_current = current.is_some_and(|c| c == name);
                    if icon_card(ui, ctx, gallery, cell, name, is_current, interactive) {
                        chosen = Some(name.clone());
                    }
                }
            }
        },
    );
    chosen
}

/// One card, filling `cell`. Returns true when clicked.
fn icon_card(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    gallery: &IconGallery,
    cell: egui::Rect,
    name: &str,
    is_current: bool,
    interactive: bool,
) -> bool {
    let p = kit::current_palette(ctx);
    let id = ui.id().with(("icon_card", name));
    let response = if interactive {
        let response = ui
            .interact(cell, id, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(name);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name)
        });
        response
    } else {
        ui.interact(cell, id, Sense::hover()).on_hover_text(name)
    };

    let fill = if interactive && response.hovered() {
        p.surface_hover
    } else {
        p.surface
    };
    let stroke = if is_current || (interactive && response.has_focus()) {
        Stroke::new(2.0_f32, p.accent)
    } else {
        Stroke::new(1.0_f32, p.border)
    };
    ui.painter()
        .rect(cell, egui::Rounding::same(radius::CARD), fill, stroke);

    let mut cell_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(cell.shrink(space::S))
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    match gallery.texture(ctx, name) {
        Some(t) => {
            cell_ui.image((t.id(), egui::vec2(42.0, 42.0)));
        }
        None if gallery.is_failed(name) => {
            cell_ui.add_sized(
                [42.0, 42.0],
                egui::Label::new(RichText::new("?").size(text_size::SECTION).color(p.danger))
                    .selectable(false),
            );
        }
        None => {
            cell_ui.add_sized([42.0, 42.0], egui::Spinner::new());
        }
    }
    cell_ui.add(
        egui::Label::new(
            RichText::new(name)
                .size(text_size::SECONDARY)
                .color(p.text_secondary),
        )
        .selectable(false)
        .truncate(),
    );
    interactive && response.clicked()
}
