//! Window chrome for the borderless window: a caption strip (drag area and
//! minimize / maximize / close buttons, after Windows 11) and resize edges.
//! Like the rest of the kit they return what the user asked for and never
//! act; the shell turns the result into viewport commands.

use egui::viewport::ResizeDirection;
use egui::{
    pos2, vec2, Color32, Context, CursorIcon, Frame, Id, Pos2, Rect, Sense, Stroke, TopBottomPanel,
    WidgetInfo, WidgetType,
};

use super::current_palette;

/// Height of the caption strip, as in Windows 11.
pub const CAPTION_HEIGHT: f32 = 32.0;
const BUTTON_WIDTH: f32 = 46.0;
/// Thickness of the grab zone along each window edge.
const RESIZE_BORDER: f32 = 5.0;
/// Size of the grab zone at each corner.
const RESIZE_CORNER: f32 = 12.0;
/// Windows 11 close-button hover fill.
const CLOSE_HOVER: Color32 = Color32::from_rgb(0xC4, 0x2B, 0x1C);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionAction {
    Minimize,
    ToggleMaximize,
    Close,
    StartDrag,
}

/// Accessible names and tooltips of the caption buttons.
pub struct CaptionLabels<'a> {
    pub minimize: &'a str,
    pub maximize: &'a str,
    pub restore: &'a str,
    pub close: &'a str,
}

#[derive(Clone, Copy, PartialEq)]
enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Full-width strip at the top of the window. Drag it to move the window,
/// double-click it to maximize or restore.
pub fn caption_bar(
    ctx: &Context,
    maximized: bool,
    labels: &CaptionLabels<'_>,
) -> Option<CaptionAction> {
    let p = current_palette(ctx);
    let mut action = None;
    TopBottomPanel::top("wintune_caption")
        .exact_height(CAPTION_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::none().fill(p.page))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let buttons = [
                (Glyph::Close, labels.close, CaptionAction::Close),
                if maximized {
                    (
                        Glyph::Restore,
                        labels.restore,
                        CaptionAction::ToggleMaximize,
                    )
                } else {
                    (
                        Glyph::Maximize,
                        labels.maximize,
                        CaptionAction::ToggleMaximize,
                    )
                },
                (Glyph::Minimize, labels.minimize, CaptionAction::Minimize),
            ];
            let buttons_left = rect.right() - BUTTON_WIDTH * buttons.len() as f32;

            let drag_rect = Rect::from_min_max(rect.min, pos2(buttons_left, rect.bottom()));
            let drag = ui.interact(
                drag_rect,
                Id::new("wintune_caption_drag"),
                Sense::click_and_drag(),
            );
            if drag.double_clicked() {
                action = Some(CaptionAction::ToggleMaximize);
            } else if drag.drag_started() {
                action = Some(CaptionAction::StartDrag);
            }

            for (i, (glyph, label, on_click)) in buttons.into_iter().enumerate() {
                let right = rect.right() - BUTTON_WIDTH * i as f32;
                let button = Rect::from_min_max(
                    pos2(right - BUTTON_WIDTH, rect.top()),
                    pos2(right, rect.bottom()),
                );
                let response = ui
                    .interact(button, Id::new(("wintune_caption", i)), Sense::click())
                    .on_hover_text(label);
                response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
                let hot = response.hovered() || response.has_focus();
                let (fill, ink) = match (glyph, hot) {
                    (Glyph::Close, true) => (CLOSE_HOVER, Color32::WHITE),
                    (_, true) => (p.surface_hover, p.text),
                    (_, false) => (Color32::TRANSPARENT, p.text),
                };
                ui.painter().rect_filled(button, 0.0, fill);
                paint_glyph(ui.painter(), button.center(), glyph, ink);
                if response.clicked() {
                    action = Some(on_click);
                }
            }
        });
    action
}

/// Thin 10 px line glyphs, drawn rather than taken from the icon font so
/// they stay crisp and need no font.
fn paint_glyph(painter: &egui::Painter, c: Pos2, glyph: Glyph, ink: Color32) {
    let stroke = Stroke::new(1.0_f32, ink);
    let h = 5.0;
    match glyph {
        Glyph::Minimize => {
            painter.line_segment([pos2(c.x - h, c.y), pos2(c.x + h, c.y)], stroke);
        }
        Glyph::Maximize => {
            painter.rect_stroke(
                Rect::from_center_size(c, vec2(2.0 * h, 2.0 * h)),
                1.0,
                stroke,
            );
        }
        Glyph::Restore => {
            let front = Rect::from_min_size(pos2(c.x - h, c.y - h + 2.0), vec2(8.0, 8.0));
            painter.rect_stroke(front, 1.0, stroke);
            let (l, t, r, b) = (c.x - h + 2.0, c.y - h, c.x + h, c.y + h - 2.0);
            painter.line_segment([pos2(l, t), pos2(r, t)], stroke);
            painter.line_segment([pos2(r, t), pos2(r, b)], stroke);
        }
        Glyph::Close => {
            painter.line_segment([pos2(c.x - h, c.y - h), pos2(c.x + h, c.y + h)], stroke);
            painter.line_segment([pos2(c.x - h, c.y + h), pos2(c.x + h, c.y - h)], stroke);
        }
    }
}

/// The resize direction for a pointer at `pos` in a window of `screen`.
pub fn edge_at(screen: Rect, pos: Pos2) -> Option<ResizeDirection> {
    if !screen.contains(pos) {
        return None;
    }
    let near = |d: f32, zone: f32| d < zone;
    let (l, r) = (pos.x - screen.left(), screen.right() - pos.x);
    let (t, b) = (pos.y - screen.top(), screen.bottom() - pos.y);
    let corner = |a: f32, b: f32| {
        (near(a, RESIZE_BORDER) && near(b, RESIZE_CORNER))
            || (near(b, RESIZE_BORDER) && near(a, RESIZE_CORNER))
    };
    Some(if corner(t, l) {
        ResizeDirection::NorthWest
    } else if corner(t, r) {
        ResizeDirection::NorthEast
    } else if corner(b, l) {
        ResizeDirection::SouthWest
    } else if corner(b, r) {
        ResizeDirection::SouthEast
    } else if near(t, RESIZE_BORDER) {
        ResizeDirection::North
    } else if near(b, RESIZE_BORDER) {
        ResizeDirection::South
    } else if near(l, RESIZE_BORDER) {
        ResizeDirection::West
    } else if near(r, RESIZE_BORDER) {
        ResizeDirection::East
    } else {
        return None;
    })
}

fn cursor_for(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::ResizeNorth,
        ResizeDirection::South => CursorIcon::ResizeSouth,
        ResizeDirection::East => CursorIcon::ResizeEast,
        ResizeDirection::West => CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
    }
}

/// Resize edges of the borderless window. Call it last in the frame, so its
/// cursor wins over the widgets'. Shows the resize cursor along the edges and
/// returns the direction when the primary button goes down there. Does
/// nothing while the window is maximized or fullscreen.
pub fn resize_edge(ctx: &Context) -> Option<ResizeDirection> {
    let (screen, pos, pressed, fixed) = ctx.input(|i| {
        let v = i.viewport();
        (
            i.screen_rect(),
            i.pointer.hover_pos(),
            i.pointer.primary_pressed(),
            v.maximized.unwrap_or(false) || v.fullscreen.unwrap_or(false),
        )
    });
    if fixed {
        return None;
    }
    let direction = edge_at(screen, pos?)?;
    ctx.set_cursor_icon(cursor_for(direction));
    pressed.then_some(direction)
}

#[cfg(test)]
mod tests {
    use super::super::test_input::{click_frames, frame_input};
    use super::*;
    use egui::{Event, Modifiers, PointerButton};
    use std::cell::Cell;

    const LABELS: CaptionLabels<'static> = CaptionLabels {
        minimize: "Minimize",
        maximize: "Maximize",
        restore: "Restore",
        close: "Close",
    };

    fn clicked(at: Pos2, maximized: bool) -> Option<CaptionAction> {
        let ctx = Context::default();
        let got = Cell::new(None);
        click_frames(&ctx, at, |c| {
            if let Some(a) = caption_bar(c, maximized, &LABELS) {
                got.set(Some(a));
            }
        });
        got.get()
    }

    // The test screen is 800 px wide: buttons sit at 754-800, 708-754, 662-708.
    #[test]
    fn the_caption_buttons_report_their_action() {
        let y = CAPTION_HEIGHT / 2.0;
        assert_eq!(clicked(pos2(780.0, y), false), Some(CaptionAction::Close));
        assert_eq!(
            clicked(pos2(730.0, y), false),
            Some(CaptionAction::ToggleMaximize)
        );
        assert_eq!(
            clicked(pos2(730.0, y), true),
            Some(CaptionAction::ToggleMaximize)
        );
        assert_eq!(
            clicked(pos2(680.0, y), false),
            Some(CaptionAction::Minimize)
        );
    }

    #[test]
    fn dragging_the_caption_starts_a_window_drag() {
        let ctx = Context::default();
        let got = Cell::new(None);
        let draw = |c: &Context| {
            if let Some(a) = caption_bar(c, false, &LABELS) {
                got.set(Some(a));
            }
        };
        let at = pos2(300.0, 16.0);
        let press = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::default(),
        };
        for events in [
            vec![Event::PointerMoved(at)],
            vec![press],
            vec![Event::PointerMoved(pos2(340.0, 30.0))],
            vec![Event::PointerMoved(pos2(360.0, 40.0))],
        ] {
            let _ = ctx.run(frame_input(events), draw);
        }
        assert_eq!(got.get(), Some(CaptionAction::StartDrag));
    }

    #[test]
    fn a_plain_click_on_the_caption_does_nothing() {
        assert_eq!(clicked(pos2(300.0, 16.0), false), None);
    }

    #[test]
    fn edges_and_corners_map_to_resize_directions() {
        let s = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        assert_eq!(edge_at(s, pos2(2.0, 300.0)), Some(ResizeDirection::West));
        assert_eq!(edge_at(s, pos2(798.0, 300.0)), Some(ResizeDirection::East));
        assert_eq!(edge_at(s, pos2(400.0, 1.0)), Some(ResizeDirection::North));
        assert_eq!(edge_at(s, pos2(400.0, 598.0)), Some(ResizeDirection::South));
        assert_eq!(edge_at(s, pos2(2.0, 8.0)), Some(ResizeDirection::NorthWest));
        assert_eq!(
            edge_at(s, pos2(790.0, 2.0)),
            Some(ResizeDirection::NorthEast)
        );
        assert_eq!(
            edge_at(s, pos2(1.0, 595.0)),
            Some(ResizeDirection::SouthWest)
        );
        assert_eq!(
            edge_at(s, pos2(798.0, 598.0)),
            Some(ResizeDirection::SouthEast)
        );
        assert_eq!(edge_at(s, pos2(400.0, 300.0)), None);
        assert_eq!(edge_at(s, pos2(20.0, 300.0)), None);
    }

    fn press_at(maximized: bool, at: Pos2) -> Option<ResizeDirection> {
        let ctx = Context::default();
        let mut input = frame_input(vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ]);
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .maximized = Some(maximized);
        let mut got = None;
        let _ = ctx.run(input, |c| got = resize_edge(c));
        got
    }

    #[test]
    fn pressing_an_edge_starts_a_resize_unless_maximized() {
        let edge = pos2(2.0, 300.0);
        assert_eq!(press_at(false, edge), Some(ResizeDirection::West));
        assert_eq!(press_at(true, edge), None);
        assert_eq!(press_at(false, pos2(400.0, 300.0)), None);
    }
}
