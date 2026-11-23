//! Windows fonts, loaded from the Windows fonts folder at runtime. They are
//! never bundled (licensing). Every candidate is validated before egui sees
//! it, because egui panics on bytes it cannot parse.

use std::path::Path;

use egui::{FontData, FontDefinitions, FontFamily, FontId, Id};

/// Family name of the semibold weight.
pub const SEMIBOLD: &str = "wintune-semibold";
/// Family name of the icon font.
pub const ICONS: &str = "wintune-icons";

const TEXT_CANDIDATES: [&str; 2] = ["SegUIVar.ttf", "segoeui.ttf"];
const SEMIBOLD_FILE: &str = "seguisb.ttf";
const ICON_CANDIDATES: [&str; 2] = ["SegoeIcons.ttf", "segmdl2.ttf"];

const SEMIBOLD_KEY: &str = "wintune.semibold";
pub(super) const ICONS_KEY: &str = "wintune.icons";

/// Which font files were loaded.
#[derive(Debug, PartialEq, Eq)]
pub struct FontReport {
    pub text: Option<&'static str>,
    pub semibold: bool,
    pub icons: Option<&'static str>,
}

/// Reads `dir/name` and returns its bytes only if they parse as a font.
fn load_valid(dir: &Path, name: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(dir.join(name)).ok()?;
    ab_glyph::FontRef::try_from_slice(&bytes).ok()?;
    Some(bytes)
}

fn first_valid(dir: &Path, candidates: &[&'static str]) -> Option<(&'static str, Vec<u8>)> {
    candidates
        .iter()
        .find_map(|name| load_valid(dir, name).map(|b| (*name, b)))
}

/// Installs the Windows fonts found in `fonts_dir`. Anything missing or
/// corrupt is skipped and egui's built-in fonts stay in use. Takes effect at
/// the next frame.
pub fn install(ctx: &egui::Context, fonts_dir: &Path) -> FontReport {
    let mut defs = FontDefinitions::default();

    let text = first_valid(fonts_dir, &TEXT_CANDIDATES).map(|(name, bytes)| {
        let key = "wintune-text".to_owned();
        defs.font_data
            .insert(key.clone(), FontData::from_owned(bytes));
        defs.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, key);
        name
    });

    let semibold = load_valid(fonts_dir, SEMIBOLD_FILE)
        .map(|bytes| {
            let key = "wintune-semibold-data".to_owned();
            defs.font_data
                .insert(key.clone(), FontData::from_owned(bytes));
            let mut chain = vec![key];
            // Fall back to the other fonts for glyphs the weight lacks.
            chain.extend(
                defs.families
                    .get(&FontFamily::Proportional)
                    .cloned()
                    .unwrap_or_default(),
            );
            defs.families
                .insert(FontFamily::Name(SEMIBOLD.into()), chain);
        })
        .is_some();

    let icons = first_valid(fonts_dir, &ICON_CANDIDATES).map(|(name, bytes)| {
        let key = "wintune-icons-data".to_owned();
        defs.font_data
            .insert(key.clone(), FontData::from_owned(bytes));
        defs.families
            .insert(FontFamily::Name(ICONS.into()), vec![key]);
        name
    });

    ctx.set_fonts(defs);
    ctx.data_mut(|d| {
        d.insert_temp(Id::new(SEMIBOLD_KEY), semibold);
        d.insert_temp(Id::new(ICONS_KEY), icons.is_some());
    });
    FontReport {
        text,
        semibold,
        icons,
    }
}

/// Semibold font of `size`; plain proportional when the weight is missing.
pub fn semibold(ctx: &egui::Context, size: f32) -> FontId {
    let loaded = ctx
        .data(|d| d.get_temp::<bool>(Id::new(SEMIBOLD_KEY)))
        .unwrap_or(false);
    if loaded {
        FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
    } else {
        FontId::proportional(size)
    }
}
