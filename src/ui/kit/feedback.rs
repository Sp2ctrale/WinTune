//! Feedback surfaces: the bottom action bar, toasts and the modal dialog.
//! They return values or draw state they were given and never act.

use egui::{
    vec2, Align, Align2, Area, Color32, Frame, Id, Layout, Margin, Order, RichText, Rounding,
    Sense, Shadow, Stroke, Ui,
};

use super::fonts::semibold;
use super::icons::icon_text;
use super::surfaces::split_row;
use super::{current_palette, radius, space, text_size, Icon};

/// Success and Info toasts disappear after this many seconds.
pub const TOAST_SECONDS: f64 = 4.0;
const MAX_TOASTS: usize = 3;
const TOAST_WIDTH: f32 = 320.0;

pub enum ActionBarState<'a> {
    Idle,
    Busy { label: &'a str, fraction: f32 },
}

/// Bottom bar with a status text and right-aligned actions. While busy the
/// actions are not drawn at all and a progress bar takes their place, so no
/// action can be triggered during a job.
pub fn action_bar(
    ctx: &egui::Context,
    status: &str,
    state: ActionBarState<'_>,
    actions: impl FnOnce(&mut Ui),
) {
    let p = current_palette(ctx);
    egui::TopBottomPanel::bottom("wintune_action_bar")
        .frame(
            Frame::none()
                .fill(p.surface)
                .stroke(Stroke::new(1.0_f32, p.border))
                .inner_margin(Margin::symmetric(space::L, space::S)),
        )
        .show(ctx, |ui| {
            split_row(
                ui,
                32.0,
                |ui| match state {
                    ActionBarState::Idle => actions(ui),
                    ActionBarState::Busy { label, fraction } => {
                        ui.add(
                            egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                                .desired_width(240.0)
                                .desired_height(8.0)
                                .rounding(Rounding::same(4.0)),
                        );
                        ui.label(
                            RichText::new(label)
                                .size(text_size::SECONDARY)
                                .color(p.text_secondary),
                        );
                    }
                },
                |ui| {
                    ui.label(RichText::new(status).size(text_size::BODY).color(p.text));
                },
            );
        });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
}

struct Toast {
    id: u64,
    kind: ToastKind,
    text: String,
    /// Input time of the first frame that saw it.
    born: Option<f64>,
}

/// Queue of transient messages, drawn bottom-right.
#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.items.push(Toast {
            id: self.next_id,
            kind,
            text: text.into(),
            born: None,
        });
        self.next_id += 1;
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Expires old toasts and draws the newest three, newest on top.
    pub fn show(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        for t in &mut self.items {
            t.born.get_or_insert(now);
        }
        self.items
            .retain(|t| t.kind == ToastKind::Error || now - t.born.unwrap_or(now) < TOAST_SECONDS);
        if self.items.is_empty() {
            return;
        }
        // Wake up when the next timed toast is due.
        let next_due = self
            .items
            .iter()
            .filter(|t| t.kind != ToastKind::Error)
            .map(|t| t.born.unwrap_or(now) + TOAST_SECONDS - now)
            .fold(f64::INFINITY, f64::min);
        if next_due.is_finite() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(next_due.max(0.0) + 0.05));
        }

        let p = current_palette(ctx);
        let mut dismissed = None;
        Area::new(Id::new("wintune_toasts"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-space::L, -space::L))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = space::S;
                for toast in self.items.iter().rev().take(MAX_TOASTS) {
                    let (color, icon) = match toast.kind {
                        ToastKind::Success => (p.success, Icon::Success),
                        ToastKind::Info => (p.accent, Icon::Info),
                        ToastKind::Error => (p.danger, Icon::Error),
                    };
                    ui.push_id(("wintune_toast", toast.id), |ui| {
                        Frame::none()
                            .fill(p.surface)
                            .stroke(Stroke::new(1.0_f32, p.border))
                            .rounding(Rounding::same(radius::CARD))
                            .shadow(Shadow {
                                offset: vec2(0.0, 2.0),
                                blur: 8.0,
                                spread: 0.0,
                                color: color.gamma_multiply(0.25),
                            })
                            .inner_margin(Margin::symmetric(space::M, space::S))
                            .show(ui, |ui| {
                                ui.set_width(TOAST_WIDTH - 2.0 * space::M - 2.0);
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if toast.kind == ToastKind::Error {
                                        let close = ui.add(
                                            egui::Label::new(
                                                RichText::new("\u{00D7}")
                                                    .size(text_size::SECTION)
                                                    .color(p.text_secondary),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if close.clicked() {
                                            dismissed = Some(toast.id);
                                        }
                                    }
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        if let Some(i) = icon_text(ctx, icon, 18.0) {
                                            ui.label(i.color(color));
                                            ui.add_space(space::S);
                                        }
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&toast.text)
                                                    .size(text_size::BODY)
                                                    .color(p.text),
                                            )
                                            .wrap(),
                                        );
                                    });
                                });
                            });
                    });
                }
            });
        if let Some(id) = dismissed {
            self.items.retain(|t| t.id != id);
        }
    }
}

/// Centered modal over a dimmed layer that swallows clicks outside it.
pub fn dialog(ctx: &egui::Context, id: &str, title: &str, width: f32, body: impl FnOnce(&mut Ui)) {
    let p = current_palette(ctx);
    let screen = ctx.screen_rect();
    // Scrim: the page color pushed towards black, in both themes.
    let [r, g, b, _] = p.page.to_array();
    let scrim = Color32::from_rgba_unmultiplied(r / 4, g / 4, b / 4, 150);
    Area::new(Id::new(("wintune_dialog_scrim", id)))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
            ui.painter().rect_filled(rect, Rounding::ZERO, scrim);
        });
    Area::new(Id::new(("wintune_dialog", id)))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .show(ctx, |ui| {
            Frame::none()
                .fill(p.surface)
                .stroke(Stroke::new(1.0_f32, p.border))
                .rounding(Rounding::same(radius::CARD))
                .shadow(Shadow {
                    offset: vec2(0.0, 4.0),
                    blur: 16.0,
                    spread: 0.0,
                    color: scrim,
                })
                .inner_margin(Margin::same(space::XL))
                .show(ui, |ui| {
                    ui.set_width((width - 2.0 * space::XL - 2.0).max(0.0));
                    ui.label(
                        RichText::new(title)
                            .font(semibold(ctx, text_size::SECTION))
                            .color(p.text),
                    );
                    ui.add_space(space::M);
                    body(ui);
                });
        });
}
