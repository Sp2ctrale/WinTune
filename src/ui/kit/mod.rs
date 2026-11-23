//! Design kit: tokens (colors, spacing, radii, type scale) and the light /
//! dark egui styles built from them. Every screen draws with these values,
//! never with hand-picked colors.

mod style;
mod tokens;

pub use style::{current_palette, install_styles, resolved, set_theme_mode};
pub use tokens::{palette, radius, space, text_size, Palette};

#[cfg(test)]
mod tests;
