//! Curated list of well-known Windows right-click entries WinTune can hide.

use super::Place;
use crate::tweaks::OsRequirement;

/// How an entry is hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogMechanism {
    /// A shell extension, hidden by listing its CLSID under `Blocked`.
    Clsid(&'static str),
    /// Classic verbs: (class path, verb name).
    Verbs(&'static [(&'static str, &'static str)]),
}

/// What must exist on the machine for the entry to be offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    ClsidRegistered,
    TerminalInstalled,
    VerbExists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogItem {
    pub id: &'static str,
    pub name_key: &'static str,
    pub description_key: &'static str,
    pub also_affects_key: Option<&'static str>,
    pub mechanism: CatalogMechanism,
    pub places: &'static [Place],
    pub os: OsRequirement,
    pub presence: Presence,
}

static CATALOG: &[CatalogItem] = &[
    CatalogItem {
        id: "open_in_terminal",
        name_key: "menu.win.open_in_terminal",
        description_key: "menu.win.open_in_terminal.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Clsid("{9F156763-7844-4DC4-B2B1-901F640F5155}"),
        places: &[Place::Folder, Place::Background],
        os: OsRequirement::Any,
        presence: Presence::TerminalInstalled,
    },
    CatalogItem {
        id: "share",
        name_key: "menu.win.share",
        description_key: "menu.win.share.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Clsid("{E2BF9676-5F8F-435C-97EB-11607A5BEDF7}"),
        places: &[Place::Files, Place::Folder],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "give_access",
        name_key: "menu.win.give_access",
        description_key: "menu.win.give_access.desc",
        also_affects_key: Some("menu.also.sharing_tab"),
        mechanism: CatalogMechanism::Clsid("{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}"),
        places: &[Place::Files, Place::Folder, Place::Background, Place::Drive],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "previous_versions",
        name_key: "menu.win.previous_versions",
        description_key: "menu.win.previous_versions.desc",
        also_affects_key: Some("menu.also.previous_versions_tab"),
        mechanism: CatalogMechanism::Clsid("{596AB062-B4D2-4215-9F74-E9109B0A8153}"),
        places: &[Place::Files, Place::Folder, Place::Drive],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "include_in_library",
        name_key: "menu.win.include_in_library",
        description_key: "menu.win.include_in_library.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Clsid("{3DAD6C5D-2167-4CAE-9914-F99E41C12CFA}"),
        places: &[Place::Folder],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "pin_to_start",
        name_key: "menu.win.pin_to_start",
        description_key: "menu.win.pin_to_start.desc",
        also_affects_key: Some("menu.also.pin_to_start_apps"),
        mechanism: CatalogMechanism::Clsid("{470C0EBD-5D73-4D58-9CED-E91E22E23282}"),
        places: &[Place::Folder],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "send_to",
        name_key: "menu.win.send_to",
        description_key: "menu.win.send_to.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Clsid("{7BA4C740-9E81-11CF-99D3-00AA004AE837}"),
        places: &[Place::Files, Place::Folder],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "cast_to_device",
        name_key: "menu.win.cast_to_device",
        description_key: "menu.win.cast_to_device.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Clsid("{7AD84985-87B4-4A16-BE58-8B72A5B390F7}"),
        places: &[Place::Files],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "scan_with_defender",
        name_key: "menu.win.scan_with_defender",
        description_key: "menu.win.scan_with_defender.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Clsid("{09A47860-11B0-4DA5-AFA5-26D86198A780}"),
        places: &[Place::Files, Place::Folder, Place::Drive],
        os: OsRequirement::Any,
        presence: Presence::ClsidRegistered,
    },
    CatalogItem {
        id: "pin_to_quick_access",
        name_key: "menu.win.pin_to_quick_access",
        description_key: "menu.win.pin_to_quick_access.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Verbs(&[("Folder", "pintohome"), ("Drive", "pintohome")]),
        places: &[Place::Folder, Place::Drive],
        os: OsRequirement::Any,
        presence: Presence::VerbExists,
    },
    CatalogItem {
        id: "add_to_favorites",
        name_key: "menu.win.add_to_favorites",
        description_key: "menu.win.add_to_favorites.desc",
        also_affects_key: None,
        mechanism: CatalogMechanism::Verbs(&[("*", "pintohomefile")]),
        places: &[Place::Files],
        os: OsRequirement::Windows11,
        presence: Presence::VerbExists,
    },
];

pub fn catalog() -> &'static [CatalogItem] {
    CATALOG
}

pub fn find_catalog(id: &str) -> Option<&'static CatalogItem> {
    CATALOG.iter().find(|item| item.id == id)
}

/// The item hidden through this CLSID (case-insensitive).
pub fn catalog_clsid(clsid: &str) -> Option<&'static CatalogItem> {
    CATALOG.iter().find(|item| match item.mechanism {
        CatalogMechanism::Clsid(c) => c.eq_ignore_ascii_case(clsid),
        CatalogMechanism::Verbs(_) => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::keys_of;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_ids_are_unique_and_translated() {
        let mut seen = BTreeSet::new();
        for item in catalog() {
            assert!(seen.insert(item.id), "duplicate id {}", item.id);
            assert!(!item.places.is_empty(), "{} has no places", item.id);
            for code in ["en", "fr"] {
                let keys = keys_of(code).expect("catalog");
                let wanted = [item.name_key, item.description_key]
                    .into_iter()
                    .chain(item.also_affects_key);
                for key in wanted {
                    assert!(keys.contains_key(key), "{code}: missing {key}");
                }
            }
            assert_eq!(item.name_key, format!("menu.win.{}", item.id));
            assert_eq!(item.description_key, format!("menu.win.{}.desc", item.id));
        }
        assert_eq!(catalog().len(), 11);
        assert!(find_catalog("share").is_some());
        assert!(find_catalog("nope").is_none());
    }

    #[test]
    fn clsids_are_braced_uppercase_guids() {
        let re = regex::Regex::new(r"^\{[0-9A-F-]{36}\}$").unwrap();
        for item in catalog() {
            if let CatalogMechanism::Clsid(c) = item.mechanism {
                assert!(re.is_match(c), "{c}");
            }
        }
    }

    #[test]
    fn catalog_clsid_matches_case_insensitively() {
        let hit = catalog_clsid("{596ab062-b4d2-4215-9f74-e9109b0a8153}").unwrap();
        assert_eq!(hit.id, "previous_versions");
        assert!(catalog_clsid("{00000000-0000-0000-0000-000000000000}").is_none());
    }
}
