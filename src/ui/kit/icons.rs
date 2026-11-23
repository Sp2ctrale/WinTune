//! Icons drawn from the Windows icon font (Segoe Fluent Icons, or Segoe MDL2
//! Assets on older Windows). Without an icon font the callers get `None` and
//! draw plain text instead.

use egui::{FontFamily, FontId, Id, RichText};

use super::fonts::{ICONS, ICONS_KEY};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Associations,
    Explorer,
    Start,
    ContextMenu,
    Icons,
    History,
    Settings,
    Search,
    Add,
    Delete,
    Restore,
    Warning,
    Info,
    Error,
    Success,
    ChevronRight,
    ChevronDown,
    More,
    Folder,
}

impl Icon {
    /// Every icon, for tests and galleries.
    pub const ALL: [Icon; 20] = [
        Icon::Home,
        Icon::Associations,
        Icon::Explorer,
        Icon::Start,
        Icon::ContextMenu,
        Icon::Icons,
        Icon::History,
        Icon::Settings,
        Icon::Search,
        Icon::Add,
        Icon::Delete,
        Icon::Restore,
        Icon::Warning,
        Icon::Info,
        Icon::Error,
        Icon::Success,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::More,
        Icon::Folder,
    ];
}

/// Code points shared by Segoe Fluent Icons and Segoe MDL2 Assets.
pub fn glyph(icon: Icon) -> char {
    match icon {
        Icon::Home => '\u{E80F}',
        Icon::Associations => '\u{E8A5}', // Document
        Icon::Explorer => '\u{E8B7}',     // Folder
        Icon::Start => '\u{E7C4}',        // TaskView
        Icon::ContextMenu => '\u{E8A9}',  // ViewAll
        Icon::Icons => '\u{E8B9}',        // Picture
        Icon::History => '\u{E81C}',
        Icon::Settings => '\u{E713}',
        Icon::Search => '\u{E721}',
        Icon::Add => '\u{E710}',
        Icon::Delete => '\u{E74D}',
        Icon::Restore => '\u{E777}', // Undo
        Icon::Warning => '\u{E7BA}',
        Icon::Info => '\u{E946}',
        Icon::Error => '\u{EA39}',   // ErrorBadge
        Icon::Success => '\u{E73E}', // CheckMark
        Icon::ChevronRight => '\u{E76C}',
        Icon::ChevronDown => '\u{E70D}',
        Icon::More => '\u{E712}',
        Icon::Folder => '\u{E8B7}',
    }
}

/// Whether an icon font was loaded into this context by `fonts::install`.
pub fn icons_available(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new(ICONS_KEY)))
        .unwrap_or(false)
}

/// The icon as styled text, or `None` when no icon font is loaded.
pub fn icon_text(ctx: &egui::Context, icon: Icon, size: f32) -> Option<RichText> {
    icons_available(ctx).then(|| {
        RichText::new(glyph(icon).to_string())
            .font(FontId::new(size, FontFamily::Name(ICONS.into())))
    })
}
