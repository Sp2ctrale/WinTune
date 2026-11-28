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

    /// The `## Visual effects` section of `doc`, up to the next `## `
    /// heading: some actions and keys also appear under Mouse & keyboard.
    /// The checkout may end lines with CRLF.
    fn ve_section(doc: &str) -> &str {
        let start = doc
            .find("\n## Visual effects\n")
            .or_else(|| doc.find("\n## Visual effects\r\n"))
            .expect("no `## Visual effects` section");
        let body = &doc[start + 1..];
        let end = body[3..].find("\n## ").map_or(body.len(), |i| i + 4);
        &body[..end]
    }

    /// Whether the Visual effects section of `doc` names every effect id,
    /// every GET and SET action in the table's uppercase hex, both Control
    /// Panel keys in their terminated form and the pre-merge checklist. The
    /// first missing text, if any.
    fn first_missing(doc: &str) -> Option<String> {
        let section = ve_section(doc);
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
        wanted
            .into_iter()
            .find(|text| !section.contains(text.as_str()))
    }

    /// The reference page's Visual effects section must name everything
    /// `first_missing` checks, so the table and the descriptors cannot
    /// drift apart.
    #[test]
    fn the_registry_reference_names_every_visual_effect() {
        let doc = include_str!("../../docs/registry-reference.md");
        assert_eq!(first_missing(doc), None, "registry-reference.md lacks it");
    }

    /// The check reads only the Visual effects section, and it fails when
    /// an id is missing from the table there.
    #[test]
    fn the_drift_check_reads_only_the_visual_effects_section() {
        let doc = include_str!("../../docs/registry-reference.md");
        let section = ve_section(doc);
        assert!(section.starts_with("## Visual effects"));
        assert!(
            !section[3..].contains("\n## "),
            "runs into the next section"
        );
        assert!(section.contains("Pre-merge checklist (Visual effects)"));

        // An id gone from the table, even if another section names it.
        let key = format!("`{}`", EffectId::CaretWidth.key());
        let without = section.replace(&key, "`gone`");
        let doc = format!("# Doc\n\n## Mouse & keyboard\n\n{key}\n{without}");
        assert!(doc.contains(&key), "a whole-page check would miss it");
        assert_eq!(first_missing(&doc), Some(key));
    }
}
