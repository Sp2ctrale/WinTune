//! Headless pointer input for tests.

use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

/// A deterministic 800x600 frame carrying `events`.
pub fn frame_input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
        events,
        ..Default::default()
    }
}

/// Click at `at` over three frames: hover, press, release. egui only reports
/// a click once the pointer was seen over the widget before the press.
pub fn click_frames(ctx: &Context, at: Pos2, mut draw: impl FnMut(&Context)) {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    };
    for events in [
        vec![Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
    ] {
        let _ = ctx.run(frame_input(events), |c| draw(c));
    }
}
