//! Visual effects: the animations, fades, shadows, font smoothing, caret and
//! focus-border settings Windows exposes through `SystemParametersInfoW`.
//!
//! Hard rule: WinTune never writes under `HKCU\Control Panel`. The values
//! live there (most effects are bits in `UserPreferencesMask`), but only the
//! system call changes them, and it also notifies running programs; the
//! registry is read for the `.reg` backup only.

mod backend;
mod catalog;
mod engine;
mod store;

pub use backend::*;
pub use catalog::*;
pub use engine::*;
pub use store::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference page must name every effect id, every GET and SET action
    /// in the table's uppercase hex, both Control Panel keys in their
    /// terminated form and the pre-merge checklist, so the table and the
    /// descriptors cannot drift apart.
    #[test]
    fn the_registry_reference_names_every_visual_effect() {
        let doc = include_str!("../../docs/registry-reference.md");
        let mut wanted: Vec<String> = Vec::new();
        for id in EffectId::ALL {
            let d = ve_descriptor(id);
            wanted.push(format!("`{}`", id.key()));
            wanted.push(format!("0x{:04X}", d.get_action));
            wanted.push(format!("0x{:04X}", d.set_action));
        }
        for key in DESKTOP_KEYS {
            wanted.push(format!("`HKCU\\{key}`"));
        }
        for text in [
            "`visual-effects-backups.json`",
            "`ANIMATIONINFO`",
            "`SPIF_UPDATEINIFILE`",
            "`UserPreferencesMask`",
            "Pre-merge checklist (Visual effects)",
        ] {
            wanted.push(text.to_string());
        }
        for text in &wanted {
            assert!(
                doc.contains(text.as_str()),
                "registry-reference.md lacks {text}"
            );
        }
    }
}
