//! Mouse & keyboard: pointer, button, wheel, keyboard and window-timing
//! settings, read and written through `SystemParametersInfo`.
//!
//! Hard rule: WinTune never writes under `HKCU\Control Panel`. The values
//! live there, but only the system call changes them (it also notifies
//! running programs); the registry is read for the `.reg` backup only.

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

    /// The reference page must name every setting id, every Control Panel key
    /// in its terminated form (so `HKCU\Control Panel\Mouse` cannot be met by
    /// a longer path) and every SET action in the table's uppercase hex, so
    /// the table and the descriptors cannot drift apart.
    #[test]
    fn the_registry_reference_names_every_mouse_keyboard_setting() {
        let doc = include_str!("../../docs/registry-reference.md");
        let mut wanted: Vec<String> = Vec::new();
        for id in SettingId::ALL {
            wanted.push(format!("`{}`", id.key()));
            wanted.push(format!("0x{:04X}", descriptor(id).set_action));
        }
        for key in CONTROL_PANEL_KEYS {
            wanted.push(format!("`HKCU\\{key}`"));
        }
        for text in [
            "`mouse-keyboard-backups.json`",
            "`SystemParametersInfoW`",
            "`SPIF_UPDATEINIFILE`",
            "Pre-merge checklist (Mouse & keyboard)",
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
