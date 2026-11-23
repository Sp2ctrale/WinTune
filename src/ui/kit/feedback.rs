//! Feedback surfaces: the bottom action bar, toasts and the modal dialog.
//! They return values or draw state they were given and never act.

use egui::{
    vec2, Align, Align2, Area, Color32, CursorIcon, Frame, Id, Key, LayerId, Layout, Margin,
    Modifiers, Order, RichText, Rounding, Sense, Shadow, Stroke, Ui,
};

use super::controls::action_button_with_id;
use super::fonts::semibold;
use super::icons::icon_text;
use super::surfaces::split_row;
use super::{current_palette, radius, space, text_size, Icon};

/// Success and Info toasts disappear after this many seconds.
pub const TOAST_SECONDS: f64 = 4.0;
const MAX_TOASTS: usize = 3;
const TOAST_WIDTH: f32 = 320.0;
const CLEAR_ALL_ID: &str = "wintune_toasts_clear_all";

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
                            egui::ProgressBar::new(if fraction.is_nan() {
                                0.0
                            } else {
                                fraction.clamp(0.0, 1.0)
                            })
                            .desired_width(240.0)
                            .desired_height(8.0)
                            .rounding(Rounding::same(radius::CONTROL)),
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
    details: String,
    /// Input time of the first frame that drew it. Toasts waiting in the
    /// queue behind the visible ones have not started their clock.
    born: Option<f64>,
}

/// Queue of transient messages, drawn bottom-right.
#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
    /// Text of the most recently pushed toast, kept after it is gone.
    latest: Option<String>,
}

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.push_details(kind, text, String::new());
    }

    /// Like `push`, with a longer text shown on hover when not empty.
    pub fn push_details(
        &mut self,
        kind: ToastKind,
        text: impl Into<String>,
        details: impl Into<String>,
    ) {
        let (text, details) = (text.into(), details.into());
        self.latest = Some(if details.is_empty() {
            text.clone()
        } else {
            format!("{text}\n{details}")
        });
        self.items.push(Toast {
            id: self.next_id,
            kind,
            text,
            details,
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

    /// The most recently pushed toast: its text, then a newline and its
    /// details when it has some. Still answers after it expired.
    pub fn latest(&self) -> Option<String> {
        self.latest.clone()
    }

    /// Expires old toasts and draws the newest three, newest on top. When
    /// more are queued a last row counts the hidden ones and offers
    /// `clear_all` to empty the queue.
    pub fn show(&mut self, ctx: &egui::Context, clear_all: &str) {
        let now = ctx.input(|i| i.time);
        // Only toasts that were drawn can expire.
        self.items.retain(|t| {
            t.kind == ToastKind::Error || t.born.is_none_or(|born| now - born < TOAST_SECONDS)
        });
        if self.items.is_empty() {
            return;
        }
        let hidden = self.items.len().saturating_sub(MAX_TOASTS);
        for t in &mut self.items[hidden..] {
            t.born.get_or_insert(now);
        }
        // Wake up when the next timed toast is due.
        let next_due = self
            .items
            .iter()
            .filter(|t| t.kind != ToastKind::Error)
            .filter_map(|t| t.born.map(|born| born + TOAST_SECONDS - now))
            .fold(f64::INFINITY, f64::min);
        if next_due.is_finite() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(next_due.max(0.0) + 0.05));
        }

        let p = current_palette(ctx);
        let mut dismissed = None;
        let mut cleared = false;
        Area::new(Id::new("wintune_toasts"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-space::L, -space::L))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = space::S;
                for toast in self.items[hidden..].iter().rev() {
                    let (color, icon) = match toast.kind {
                        ToastKind::Success => (p.success, Icon::Success),
                        ToastKind::Info => (p.accent, Icon::Info),
                        ToastKind::Error => (p.danger, Icon::Error),
                    };
                    ui.push_id(("wintune_toast", toast.id), |ui| {
                        let card = Frame::none()
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
                                // A fixed row height: a centered layout in an anchored
                                // area would grow with the space it is offered.
                                split_row(
                                    ui,
                                    24.0,
                                    |ui| {
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
                                    },
                                    |ui| {
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
                                    },
                                );
                            });
                        if !toast.details.is_empty() {
                            card.response.on_hover_text(&toast.details);
                        }
                    });
                }
                if hidden > 0 {
                    ui.allocate_ui_with_layout(
                        vec2(TOAST_WIDTH, 20.0),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            let label = ui.label(
                                RichText::new(clear_all)
                                    .size(text_size::SECONDARY)
                                    .color(p.accent),
                            );
                            let link = ui
                                .interact(label.rect, Id::new(CLEAR_ALL_ID), Sense::click())
                                .on_hover_cursor(CursorIcon::PointingHand);
                            if link.clicked() {
                                cleared = true;
                            }
                            ui.label(
                                RichText::new(format!("+{hidden}"))
                                    .size(text_size::SECONDARY)
                                    .color(p.text_secondary),
                            );
                        },
                    );
                }
            });
        if cleared {
            self.items.clear();
        } else if let Some(id) = dismissed {
            self.items.retain(|t| t.id != id);
        }
    }
}

/// The two buttons of a confirmation dialog: confirm (primary) then cancel.
/// Returns `Some(true)` for confirm and `Some(false)` for cancel. Focus
/// starts on cancel and Tab / Shift+Tab only toggle between the two, so the
/// page behind the dialog is unreachable. Escape is left to the caller.
pub fn dialog_buttons(ui: &mut Ui, id: &str, confirm: &str, cancel: &str) -> Option<bool> {
    let confirm_id = Id::new(("wintune_dialog_confirm", id));
    let cancel_id = Id::new(("wintune_dialog_cancel", id));
    let cancel_had = ui.memory(|m| m.has_focus(cancel_id));
    // Tab is not a focus move for these buttons: egui leaves it to us.
    let lock = egui::EventFilter {
        tab: true,
        horizontal_arrows: false,
        vertical_arrows: false,
        escape: false,
    };
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(confirm_id, lock);
        m.set_focus_lock_filter(cancel_id, lock);
    });
    let toggle = ui.ctx().input_mut(|i| {
        let tab = i.consume_key(Modifiers::NONE, Key::Tab);
        let back = i.consume_key(Modifiers::SHIFT, Key::Tab);
        tab || back
    });

    let (confirm_r, cancel_r) = ui
        .horizontal(|ui| {
            let confirm_r = action_button_with_id(ui, confirm_id, confirm, true);
            let cancel_r = action_button_with_id(ui, cancel_id, cancel, false);
            (confirm_r, cancel_r)
        })
        .inner;
    let result = if confirm_r.clicked() {
        Some(true)
    } else if cancel_r.clicked() {
        Some(false)
    } else {
        None
    };

    if toggle && cancel_had {
        // Two buttons: next and previous both mean "the other one".
        confirm_r.request_focus();
    } else if toggle || !ui.memory(|m| m.has_focus(confirm_id) || m.has_focus(cancel_id)) {
        cancel_r.request_focus();
    }
    result
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
    // A strictly higher order than the scrim: clicking the scrim raises the
    // scrim within its own order, which must never put it above the dialog.
    let dialog_id = Id::new(("wintune_dialog", id));
    let dialog_layer = LayerId::new(Order::Tooltip, dialog_id);
    // Modal: focus held by anything outside the dialog is dropped so Enter
    // and Space cannot reach the page behind it.
    if let Some(focused) = ctx.memory(|m| m.focused()) {
        let outside = ctx
            .read_response(focused)
            .is_some_and(|r| r.layer_id != dialog_layer);
        if outside {
            ctx.memory_mut(|m| m.surrender_focus(focused));
        }
    }
    Area::new(dialog_id)
        .order(Order::Tooltip)
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
