//! Design tokens, in the spirit of Windows 11 Settings (Fluent / WinUI 3).
//!
//! Colors are opaque: the WinUI alpha brushes are flattened over their usual
//! background so egui never has to blend them.

use egui::{Color32, Theme};

/// Every color a screen may use, for one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Window background, behind everything (Mica base).
    pub page: Color32,
    /// Cards and dialogs.
    pub surface: Color32,
    /// A card or list row under the pointer.
    pub surface_hover: Color32,
    /// Resting fill of buttons, text boxes and other controls.
    pub control: Color32,
    /// 1 px hairline around cards and controls, and dividers.
    pub border: Color32,
    pub text: Color32,
    /// Descriptions and captions.
    pub text_secondary: Color32,
    pub text_disabled: Color32,
    /// Primary buttons, links, selection, focus.
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Text and glyphs drawn on an `accent` fill.
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    /// Status that is neither good nor bad (e.g. "not applied").
    pub neutral: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

static LIGHT: Palette = Palette {
    page: rgb(0xF3F3F3),
    surface: rgb(0xFFFFFF),
    surface_hover: rgb(0xF6F6F6),
    control: rgb(0xFBFBFB),
    border: rgb(0xE5E5E5),
    text: rgb(0x1A1A1A),
    text_secondary: rgb(0x5F5F5F),
    text_disabled: rgb(0xA0A0A0),
    accent: rgb(0x0078D4),
    accent_hover: rgb(0x106EBE),
    on_accent: rgb(0xFFFFFF),
    success: rgb(0x0F7B0F),
    warning: rgb(0x9D5D00),
    danger: rgb(0xC42B1C),
    neutral: rgb(0x6E6E6E),
};

static DARK: Palette = Palette {
    page: rgb(0x202020),
    surface: rgb(0x2B2B2B),
    surface_hover: rgb(0x323232),
    control: rgb(0x373737),
    border: rgb(0x1D1D1D),
    text: rgb(0xFFFFFF),
    text_secondary: rgb(0xC5C5C5),
    text_disabled: rgb(0x787878),
    accent: rgb(0x4CC2FF),
    accent_hover: rgb(0x47B1E8),
    on_accent: rgb(0x000000),
    success: rgb(0x6CCB5F),
    warning: rgb(0xFCE100),
    danger: rgb(0xFF99A4),
    neutral: rgb(0x9D9D9D),
};

/// The palette of `theme`.
pub fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Light => &LIGHT,
        Theme::Dark => &DARK,
    }
}

/// Spacing scale, in points.
/// Width of a page's search box.
pub const SEARCH_WIDTH: f32 = 280.0;

pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Corner radii, in points.
pub mod radius {
    /// Buttons, text boxes, checkboxes, badges.
    pub const CONTROL: f32 = 4.0;
    /// Cards, dialogs, menus.
    pub const CARD: f32 = 8.0;
}

/// Type scale, in points.
pub mod text_size {
    /// Page title.
    pub const TITLE: f32 = 24.0;
    /// Section header (semibold).
    pub const SECTION: f32 = 15.0;
    pub const BODY: f32 = 14.0;
    /// Descriptions, captions, badges.
    pub const SECONDARY: f32 = 12.0;
}
