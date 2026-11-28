//! The snapshot store (`visual-effects-backups.json`) and the `.reg` net
//! taken before every Apply (spec 5.6).

use crate::backups::RegTarget;
use crate::models::app_data_dir;
use crate::persist::LoadError;
use crate::sysparams::{export_values, SpiBackupStore};
use std::path::{Path, PathBuf};

/// Original values, per user then per `EffectId::key`, with the master's
/// companions marked (spec 5.3).
pub type VeBackupStore = SpiBackupStore;

pub fn default_ve_store_path() -> PathBuf {
    app_data_dir().join("visual-effects-backups.json")
}

pub fn load_ve_store(path: &Path) -> Result<VeBackupStore, LoadError> {
    crate::persist::load_json(path)
}

/// Atomic save. The error is the bare cause (`VeProblem::NotSaved` and
/// `VeFailure::StoreNotSaved` give the sentence around it).
pub fn save_ve_store(store: &VeBackupStore, path: &Path) -> Result<(), String> {
    crate::persist::save_json(store, path)
}

/// The keys whose values the `.reg` net exports (spec 5.6).
pub const DESKTOP_KEYS: [&str; 2] = [
    r"Control Panel\Desktop",
    r"Control Panel\Desktop\WindowMetrics",
];

/// Exports every value directly in each of [`DESKTOP_KEYS`] (never another
/// subkey, never a key delete line) to
/// `<reg_dir>\<file_stamp>-visual-effects.reg`; a missing key is skipped.
/// Read only. The error is the bare cause.
pub fn export_desktop(source: &RegTarget, reg_dir: &Path) -> Result<PathBuf, String> {
    export_values(source, &DESKTOP_KEYS, reg_dir, "visual-effects")
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use crate::sysparams::test_support::Fixture;
    use std::path::PathBuf;
    use winreg::enums::REG_BINARY;
    use winreg::RegValue;

    pub(crate) const USER: &str = "TEST\\user";

    pub(crate) const MASK: [u8; 8] = [0x9E, 0x1E, 0x07, 0x80, 0x12, 0x00, 0x00, 0x00];

    /// The shared sandbox, seeded: `Control Panel\Desktop` holds
    /// `UserPreferencesMask` and `CaretWidth`, `Desktop\WindowMetrics` holds
    /// `MinAnimate` = "1", and `Desktop\MuiCached` (a subkey the net leaves
    /// out) holds `MachinePreferredUILanguages`.
    pub(crate) fn fixture() -> Fixture {
        let f = Fixture::new();
        let desktop = f.key(r"Control Panel\Desktop");
        desktop
            .set_raw_value(
                "UserPreferencesMask",
                &RegValue {
                    bytes: MASK.to_vec(),
                    vtype: REG_BINARY,
                },
            )
            .unwrap();
        desktop.set_value("CaretWidth", &1u32).unwrap();
        f.key(r"Control Panel\Desktop\WindowMetrics")
            .set_value("MinAnimate", &"1")
            .unwrap();
        f.key(r"Control Panel\Desktop\MuiCached")
            .set_value("MachinePreferredUILanguages", &"en-US")
            .unwrap();
        f
    }

    pub(crate) fn store_path(f: &Fixture) -> PathBuf {
        f.tmp.join("visual-effects-backups.json")
    }

    /// Everything under the sandbox `Control Panel`, subkeys included.
    pub(crate) fn control_panel(f: &Fixture) -> crate::regsnap::RegistryKeySnapshot {
        crate::regsnap::snapshot_key(&f.sb.root, &f.source.path("Control Panel")).unwrap()
    }

    /// The sandbox `UserPreferencesMask` bytes.
    pub(crate) fn mask(f: &Fixture) -> Vec<u8> {
        f.key(r"Control Panel\Desktop")
            .get_raw_value("UserPreferencesMask")
            .unwrap()
            .bytes
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{control_panel, fixture, mask, store_path, MASK};
    use super::*;
    use crate::visual_effects::SettingValue::*;

    fn utf16_file(path: &Path) -> String {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE], "UTF-16LE BOM");
        let (pairs, rest) = bytes[2..].as_chunks::<2>();
        assert!(rest.is_empty());
        let units: Vec<u16> = pairs.iter().map(|&p| u16::from_le_bytes(p)).collect();
        String::from_utf16(&units).unwrap()
    }

    #[test]
    fn the_store_loads_saves_and_has_its_file_name() {
        let f = fixture();
        assert_eq!(
            load_ve_store(&f.tmp.join("none.json")).unwrap(),
            VeBackupStore::default()
        );
        let mut store = VeBackupStore::default();
        store.record_first("U", "caret_width", "x", Number(2));
        store.record_companion("U", "cursor_shadow", "x", Flag(false), "ui_effects");
        save_ve_store(&store, &store_path(&f)).unwrap();
        assert_eq!(load_ve_store(&store_path(&f)).unwrap(), store);
        assert!(default_ve_store_path().ends_with("visual-effects-backups.json"));
    }

    #[test]
    fn the_reg_net_exports_desktop_and_window_metrics_only() {
        let f = fixture();
        let before = control_panel(&f);

        let file = export_desktop(&f.source, &f.reg_dir()).unwrap();

        assert_eq!(f.reg_files(), std::slice::from_ref(&file));
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with("-visual-effects.reg"), "{name}");
        let text = utf16_file(&file);
        assert!(
            text.contains(r#""UserPreferencesMask"=hex:9e,1e,07,80,12,00,00,00"#),
            "{text}"
        );
        assert!(text.contains(r#""MinAnimate"="1""#), "{text}");
        assert!(
            text.contains(r"Control Panel\Desktop\WindowMetrics]"),
            "{text}"
        );
        assert!(!text.contains("MuiCached"), "{text}");
        assert!(!text.contains("MachinePreferredUILanguages"), "{text}");
        assert!(!text.contains("[-"), "{text}");
        assert_eq!(control_panel(&f), before);
        assert_eq!(mask(&f), MASK);
    }
}
