//! Context menu module: entries a user adds to the Explorer right-click menu.

pub mod catalog;
pub mod engine;
pub mod existing;
pub mod fileinfo;
mod origin;
pub mod programs;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub use catalog::{catalog, catalog_clsid, find_catalog, CatalogItem, CatalogMechanism, Presence};
pub use engine::{
    apply_menu, default_menu_store_path, entry_key, entry_status, key_place, load_menu_store,
    orphan_keys, restore_keys, save_menu_store, EntryStatus, MenuBackup, MenuBackupStore,
    MenuOptions, MenuTarget,
};
pub use existing::{
    class_places, rename_problem, scan, ExistingChange, ExistingEntry, ExistingState, Locked,
    Mechanism, RenameProblem, Scan, ScanOptions, VerbRef, BLOCKED_KEY, SCAN_CLASSES,
};
pub use programs::{command_line, icon_value, Programs};

/// Prefix of every shell key WinTune owns.
pub const KEY_PREFIX: &str = "WinTune.";

/// Name of the verb key for an entry id.
pub fn key_name(id: &str) -> String {
    format!("{KEY_PREFIX}{id}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Place {
    Folder,
    Background,
    Drive,
    Files,
}

impl Place {
    pub const ALL: [Place; 4] = [Place::Folder, Place::Background, Place::Drive, Place::Files];

    /// Class key under `HKCU\Software\Classes` that hosts the `shell` verbs.
    pub fn class_path(self) -> &'static str {
        match self {
            Place::Folder => "Directory",
            Place::Background => r"Directory\Background",
            Place::Drive => "Drive",
            Place::Files => "*",
        }
    }

    /// The place's name in reports and History records (always English).
    pub fn english_name(self) -> &'static str {
        match self {
            Place::Folder => "Folders",
            Place::Background => "Folder background",
            Place::Drive => "Drives",
            Place::Files => "Files",
        }
    }

    /// Explorer's substitution for the clicked item in a command line.
    pub fn placeholder(self) -> &'static str {
        match self {
            Place::Background => "%V",
            _ => "%1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path")]
pub enum ProgramChoice {
    VsCode,
    Terminal,
    PowerShell,
    Cmd,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuEntry {
    pub id: String,
    pub label: String,
    pub program: ProgramChoice,
    pub places: BTreeSet<Place>,
    /// `None` means the program's own icon.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextMenuConfig {
    pub users: BTreeMap<String, Vec<MenuEntry>>,
}

impl ContextMenuConfig {
    pub fn entries_of(&self, user: &str) -> &[MenuEntry] {
        self.users.get(user).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn entries_mut(&mut self, user: &str) -> &mut Vec<MenuEntry> {
        self.users.entry(user.to_string()).or_default()
    }
}

/// The lowest unused `e1`, `e2`, ...
pub fn new_entry_id(existing: &[MenuEntry]) -> String {
    (1..)
        .map(|n| format!("e{n}"))
        .find(|id| !existing.iter().any(|e| &e.id == id))
        .expect("an unused id exists")
}

pub const MAX_LABEL_CHARS: usize = 80;

/// A pasted program path without the whitespace and the quotes Explorer's
/// "Copy as path" puts around it.
pub fn clean_program_path(path: &str) -> String {
    path.trim().trim_matches('"').trim().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryProblem {
    EmptyLabel,
    LabelTooLong,
    NoPlace,
    ProgramNotFound,
    /// A custom program path that does not end in `.exe`.
    NotAnExe,
    /// The verb text reads a leading `@` as an indirect string (`@file,-id`).
    LeadingAt,
    IconNotFound,
    DuplicateLabel(Place),
    /// Terminal, PowerShell and cmd start in a folder; a file has none.
    ShellOnFiles,
}

/// Everything that would stop an entry from being applied, as (entry id, problem).
pub fn entry_problems(entries: &[MenuEntry], programs: &Programs) -> Vec<(String, EntryProblem)> {
    let mut problems = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let id = || entry.id.clone();
        let label = entry.label.trim();
        for problem in existing::label_problems(label) {
            let problem = match problem {
                RenameProblem::Empty => EntryProblem::EmptyLabel,
                RenameProblem::TooLong => EntryProblem::LabelTooLong,
                RenameProblem::LeadingAt => EntryProblem::LeadingAt,
            };
            problems.push((id(), problem));
        }
        if entry.places.is_empty() {
            problems.push((id(), EntryProblem::NoPlace));
        }
        let not_an_exe = match &entry.program {
            ProgramChoice::Custom(path) => {
                let path = clean_program_path(path);
                !path.is_empty()
                    && !std::path::Path::new(&path)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            }
            _ => false,
        };
        if not_an_exe {
            problems.push((id(), EntryProblem::NotAnExe));
        } else if programs.resolve(&entry.program).is_none() {
            problems.push((id(), EntryProblem::ProgramNotFound));
        }
        let is_shell = matches!(
            entry.program,
            ProgramChoice::Terminal | ProgramChoice::PowerShell | ProgramChoice::Cmd
        );
        if is_shell && entry.places.contains(&Place::Files) {
            problems.push((id(), EntryProblem::ShellOnFiles));
        }
        if let Some(icon) = &entry.icon {
            if !std::path::Path::new(icon).is_file() {
                problems.push((id(), EntryProblem::IconNotFound));
            }
        }
        if !label.is_empty() {
            for place in &entry.places {
                let duplicated = entries[..index].iter().any(|earlier| {
                    earlier.places.contains(place)
                        && earlier.label.trim().to_lowercase() == label.to_lowercase()
                });
                if duplicated {
                    problems.push((id(), EntryProblem::DuplicateLabel(*place)));
                }
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_file(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("wintune-cm-{name}"));
        std::fs::write(&path, b"x").unwrap();
        path
    }

    fn entry(id: &str, label: &str, places: &[Place], program: ProgramChoice) -> MenuEntry {
        MenuEntry {
            id: id.into(),
            label: label.into(),
            program,
            places: places.iter().copied().collect(),
            icon: None,
        }
    }

    fn problems_of(entries: &[MenuEntry], programs: &Programs) -> Vec<EntryProblem> {
        entry_problems(entries, programs)
            .into_iter()
            .map(|(_, p)| p)
            .collect()
    }

    #[test]
    fn entry_problems_cover_label_places_program_icon_and_duplicates() {
        let exe = temp_file("problems.exe");
        let programs = Programs {
            cmd: Some(exe.clone()),
            vscode: Some(exe.clone()),
            ..Programs::default()
        };
        let ok = |label: &str, places: &[Place]| entry("e1", label, places, ProgramChoice::Cmd);
        let check = |e: MenuEntry| problems_of(&[e], &programs);

        assert_eq!(
            check(ok("  ", &[Place::Folder])),
            [EntryProblem::EmptyLabel]
        );
        let long = "x".repeat(81);
        assert_eq!(
            check(ok(&long, &[Place::Folder])),
            [EntryProblem::LabelTooLong]
        );
        let max = "x".repeat(80);
        assert!(check(ok(&max, &[Place::Folder])).is_empty());
        assert_eq!(check(ok("Open", &[])), [EntryProblem::NoPlace]);

        let missing = entry(
            "e1",
            "Open",
            &[Place::Folder],
            ProgramChoice::Custom(r"C:\missing.exe".into()),
        );
        assert_eq!(check(missing), [EntryProblem::ProgramNotFound]);

        let mut bad_icon = ok("Open", &[Place::Folder]);
        bad_icon.icon = Some(r"C:\missing.ico".into());
        assert_eq!(check(bad_icon), [EntryProblem::IconNotFound]);

        let first = entry("e1", "Open", &[Place::Folder], ProgramChoice::VsCode);
        let mut second = entry("e2", " open ", &[Place::Folder], ProgramChoice::VsCode);
        assert_eq!(
            entry_problems(&[first.clone(), second.clone()], &programs),
            [(
                "e2".to_string(),
                EntryProblem::DuplicateLabel(Place::Folder)
            )]
        );
        second.places = BTreeSet::from([Place::Files]);
        assert!(entry_problems(&[first, second], &programs).is_empty());
        let _ = std::fs::remove_file(exe);
    }

    #[test]
    fn a_leading_at_or_a_non_exe_custom_program_is_a_problem() {
        let exe = temp_file("leading-at.exe");
        let txt = temp_file("not-an-exe.txt");
        let upper = temp_file("upper.EXE");
        let programs = Programs::default();
        let custom = |label: &str, path: &PathBuf| {
            let program = ProgramChoice::Custom(path.display().to_string());
            problems_of(&[entry("e1", label, &[Place::Folder], program)], &programs)
        };
        assert_eq!(custom("@shell32.dll,-1", &exe), [EntryProblem::LeadingAt]);
        assert_eq!(custom("  @x", &exe), [EntryProblem::LeadingAt]);
        assert!(custom("a@b", &exe).is_empty());
        assert_eq!(custom("Open", &txt), [EntryProblem::NotAnExe]);
        assert!(custom("Open", &upper).is_empty());
        let missing = ProgramChoice::Custom(r"C:\missing.exe".into());
        let e = entry("e1", "Open", &[Place::Folder], missing);
        assert_eq!(
            problems_of(&[e], &programs),
            [EntryProblem::ProgramNotFound]
        );
        for f in [exe, txt, upper] {
            let _ = std::fs::remove_file(f);
        }
    }

    #[test]
    fn a_pasted_path_loses_its_quotes_and_spaces() {
        assert_eq!(clean_program_path("  \"C:\\A b\\x.exe\" "), r"C:\A b\x.exe");
        assert_eq!(clean_program_path(r"C:\x.exe"), r"C:\x.exe");
        assert_eq!(clean_program_path(" \"\" "), "");
        let exe = temp_file("pasted path.exe");
        let pasted = format!("  \"{}\"  ", exe.display());
        let programs = Programs::default();
        let program = ProgramChoice::Custom(pasted);
        let e = entry("e1", "Open", &[Place::Folder], program);
        assert!(problems_of(&[e], &programs).is_empty());
        let _ = std::fs::remove_file(exe);
    }

    #[test]
    fn a_preset_that_vanished_is_a_program_problem() {
        let exe = temp_file("vanished-preset.exe");
        let programs = Programs {
            vscode: Some(exe.clone()),
            ..Programs::default()
        };
        let e = entry("e1", "Code", &[Place::Folder], ProgramChoice::VsCode);
        assert!(problems_of(std::slice::from_ref(&e), &programs).is_empty());
        std::fs::remove_file(&exe).unwrap();
        assert_eq!(
            problems_of(&[e], &programs),
            [EntryProblem::ProgramNotFound]
        );
    }

    #[test]
    fn shell_presets_on_files_are_a_problem_but_editors_are_not() {
        let exe = temp_file("shell-on-files.exe");
        let programs = Programs {
            vscode: Some(exe.clone()),
            terminal: Some(exe.clone()),
            ..Programs::default()
        };
        let places = [Place::Folder, Place::Files];
        let terminal = entry("e1", "Term", &places, ProgramChoice::Terminal);
        assert_eq!(
            problems_of(&[terminal], &programs),
            [EntryProblem::ShellOnFiles]
        );
        let code = entry("e1", "Code", &places, ProgramChoice::VsCode);
        assert!(problems_of(&[code], &programs).is_empty());
        let _ = std::fs::remove_file(exe);
    }

    #[test]
    fn menu_entries_survive_a_config_round_trip() {
        let mut config = crate::models::AppConfig::default();
        let list = config.context_menu.entries_mut("PC\\me");
        list.push(entry(
            "e1",
            "Mine",
            &[Place::Folder, Place::Drive],
            ProgramChoice::Custom(r"C:\Apps\My App\app.exe".into()),
        ));
        list.push(entry("e2", "Code", &[Place::Files], ProgramChoice::VsCode));
        let json = serde_json::to_string(&config).unwrap();
        let back: crate::models::AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn new_entry_ids_fill_the_lowest_gap() {
        assert_eq!(new_entry_id(&[]), "e1");
        let e = |id| entry(id, "x", &[Place::Folder], ProgramChoice::Cmd);
        assert_eq!(new_entry_id(&[e("e1"), e("e3")]), "e2");
    }

    #[test]
    fn an_old_config_without_context_menu_loads_with_an_empty_list() {
        let mut json = serde_json::to_value(crate::models::AppConfig::default()).unwrap();
        json.as_object_mut().unwrap().remove("context_menu");
        let config: crate::models::AppConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.context_menu, ContextMenuConfig::default());
    }

    #[test]
    fn entries_are_kept_per_user() {
        let mut config = ContextMenuConfig::default();
        config
            .entries_mut("A")
            .push(entry("e1", "Open", &[Place::Folder], ProgramChoice::Cmd));
        assert_eq!(config.entries_of("A").len(), 1);
        assert!(config.entries_of("B").is_empty());
    }

    #[test]
    fn key_names_carry_the_prefix_and_places_map_to_classes() {
        assert_eq!(key_name("e2"), "WinTune.e2");
        let classes: Vec<_> = Place::ALL.iter().map(|p| p.class_path()).collect();
        assert_eq!(
            classes,
            ["Directory", r"Directory\Background", "Drive", "*"]
        );
    }
}
