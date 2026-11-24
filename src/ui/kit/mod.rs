//! Design kit: tokens (colors, spacing, radii, type scale) and the light /
//! dark egui styles built from them. Every screen draws with these values,
//! never with hand-picked colors.

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

pub use controls::{disclosure, pill, segmented, toggle, toggle_enabled, PillKind};
pub use feedback::{
    action_bar, dialog, dialog_buttons, ActionBarState, ToastKind, Toasts, TOAST_SECONDS,
};
pub use fonts::FontReport;
pub use icons::Icon;
pub use nav::{nav_sidebar, page_header, NavGroup, NavItem};
pub use style::{current_palette, install_styles, resolved, set_theme_mode};
pub use surfaces::{banner, card, empty_state, section, tile, BannerKind, SettingRow};
pub use tokens::{palette, radius, space, text_size, Palette};

#[cfg(test)]
mod tests;
