//! Design kit: tokens (colors, spacing, radii, type scale) and the light /
//! dark egui styles built from them. Every screen draws with these values,
//! never with hand-picked colors.

mod chrome;
mod controls;
mod feedback;
pub mod fonts;
pub mod icons;
mod nav;
mod style;
mod surfaces;
#[cfg(test)]
mod test_input;
mod tokens;

pub use chrome::{caption_bar, resize_edge, CaptionAction, CaptionLabels, CAPTION_HEIGHT};
pub use controls::{
    disclosure, overflow_menu, pill, primary_button, secondary_button, segmented, status_dot,
    toggle, toggle_enabled, MenuItem, PillKind, BUTTON_HEIGHT, OVERFLOW_BUTTON,
};
pub use feedback::{
    action_bar, dialog, dialog_buttons, ActionBarState, ToastKind, Toasts, TOAST_SECONDS,
};
pub use fonts::FontReport;
pub use icons::Icon;
pub use nav::{nav_sidebar, page_header, Brand, NavGroup, NavItem};
pub use style::{current_palette, install_styles, resolved, set_theme_mode};
pub use surfaces::{
    banner, card, empty_state, section, section_with_action, tile, BannerKind, SettingRow,
};
pub use tokens::{palette, radius, space, text_size, Palette};

#[cfg(test)]
mod tests;
