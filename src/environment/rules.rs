//! Rules for variables: which registry type to write, what is refused, what
//! deserves a warning, and how pending edits become changes. Pure.

use super::{
    env_slot, has_reference, is_path_name, join_entries, PathEdit, PATH_LIMIT, PATH_NAME,
    REG_EXPAND_SZ_KIND, REG_SZ_KIND,
};
use crate::backups::Slot;

const NAME_LIMIT: usize = 255;
const VALUE_LIMIT: usize = 32_767;

#[derive(Debug, Clone, PartialEq)]
pub struct EnvVar {
    pub name: String,
    pub kind: u32,
    /// `None` when the kind is not `REG_SZ` or `REG_EXPAND_SZ`.
    pub value: Option<String>,
    pub has_snapshot: bool,
}

impl EnvVar {
    pub fn is_text(&self) -> bool {
        self.kind == REG_SZ_KIND || self.kind == REG_EXPAND_SZ_KIND
    }

    pub fn is_path(&self) -> bool {
        is_path_name(&self.name)
    }

    pub fn uses_references(&self) -> bool {
        self.kind == REG_EXPAND_SZ_KIND
    }

    pub fn slot(&self) -> Slot {
        env_slot(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvChange {
    Add {
        name: String,
        value: String,
    },
    Edit {
        name: String,
        value: String,
    },
    Delete {
        name: String,
    },
    Rename {
        from: String,
        to: String,
        value: String,
    },
}

impl EnvChange {
    /// The registry value names this change touches, once per name ignoring case.
    pub fn slot_names(&self) -> Vec<&str> {
        match self {
            Self::Add { name, .. } | Self::Edit { name, .. } | Self::Delete { name } => {
                vec![name]
            }
            Self::Rename { from, to, .. } if from.eq_ignore_ascii_case(to) => vec![from],
            Self::Rename { from, to, .. } => vec![from, to],
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Add { name, .. } | Self::Edit { name, .. } | Self::Delete { name } => {
                name.clone()
            }
            Self::Rename { from, to, .. } => format!("{from} \u{2192} {to}"),
        }
    }
}

/// The registry type to write for a value (spec 5.2).
pub fn kind_to_write(existing: Option<u32>, name: &str, value: &str) -> u32 {
    match existing {
        Some(REG_SZ_KIND) if has_reference(value) => REG_EXPAND_SZ_KIND,
        Some(kind) => kind,
        None if is_path_name(name) || has_reference(value) => REG_EXPAND_SZ_KIND,
        None => REG_SZ_KIND,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvProblem {
    EmptyName,
    NameHasEquals,
    NameHasNul,
    NameTooLong,
    NameTaken(String),
    PathName,
    EmptyValue,
    ValueTooLong,
    ValueHasNul,
    EmptyEntry,
    EntryHasSemicolon,
    Gone(String),
    NotText(String),
    Twice(String),
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

pub fn name_problem(name: &str) -> Option<EnvProblem> {
    let name = name.trim();
    if name.is_empty() {
        Some(EnvProblem::EmptyName)
    } else if name.contains('=') {
        Some(EnvProblem::NameHasEquals)
    } else if name.contains('\0') {
        Some(EnvProblem::NameHasNul)
    } else if utf16_len(name) > NAME_LIMIT {
        Some(EnvProblem::NameTooLong)
    } else {
        None
    }
}

pub fn value_problem(value: &str) -> Option<EnvProblem> {
    if value.is_empty() {
        Some(EnvProblem::EmptyValue)
    } else if value.contains('\0') {
        Some(EnvProblem::ValueHasNul)
    } else if utf16_len(value) > VALUE_LIMIT {
        Some(EnvProblem::ValueTooLong)
    } else {
        None
    }
}

pub fn entry_problem(entry: &str) -> Option<EnvProblem> {
    if entry.trim().is_empty() {
        Some(EnvProblem::EmptyEntry)
    } else if entry.contains(';') {
        Some(EnvProblem::EntryHasSemicolon)
    } else if entry.contains('\0') {
        Some(EnvProblem::ValueHasNul)
    } else {
        None
    }
}

/// The variable names that will exist once every change is applied.
pub fn names_after(current: &[EnvVar], changes: &[EnvChange]) -> Vec<String> {
    let gone = |name: &str| {
        changes.iter().any(|c| match c {
            EnvChange::Delete { name: n } => n.eq_ignore_ascii_case(name),
            EnvChange::Rename { from, .. } => from.eq_ignore_ascii_case(name),
            _ => false,
        })
    };
    let mut names: Vec<String> = current
        .iter()
        .filter(|v| !gone(&v.name))
        .map(|v| v.name.clone())
        .collect();
    for change in changes {
        match change {
            EnvChange::Add { name, .. } => names.push(name.clone()),
            EnvChange::Rename { to, .. } => names.push(to.clone()),
            _ => {}
        }
    }
    names
}

fn find<'a>(current: &'a [EnvVar], name: &str) -> Option<&'a EnvVar> {
    current.iter().find(|v| v.name.eq_ignore_ascii_case(name))
}

/// `Gone` or `NotText` for a variable a change reads from.
fn source_problem(current: &[EnvVar], name: &str) -> Option<EnvProblem> {
    match find(current, name) {
        None => Some(EnvProblem::Gone(name.to_string())),
        Some(v) if !v.is_text() => Some(EnvProblem::NotText(name.to_string())),
        Some(_) => None,
    }
}

/// Everything that makes the batch unacceptable, each with the variable it is
/// about. Empty means the batch may be applied.
pub fn validate_changes(current: &[EnvVar], changes: &[EnvChange]) -> Vec<(String, EnvProblem)> {
    let mut found: Vec<(String, EnvProblem)> = Vec::new();
    let mut push = |name: &str, problem: Option<EnvProblem>| {
        if let Some(p) = problem {
            let entry = (name.to_string(), p);
            if !found.contains(&entry) {
                found.push(entry);
            }
        }
    };
    for change in changes {
        match change {
            EnvChange::Add { name, value } => {
                push(name, name_problem(name));
                push(name, value_problem(value));
                if find(current, name).is_some() {
                    push(name, Some(EnvProblem::NameTaken(name.clone())));
                }
            }
            EnvChange::Edit { name, value } => {
                push(name, value_problem(value));
                push(name, source_problem(current, name));
            }
            EnvChange::Delete { name } => push(name, source_problem(current, name)),
            EnvChange::Rename { from, to, value } => {
                push(to, name_problem(to));
                push(to, value_problem(value));
                push(from, source_problem(current, from));
                if !to.eq_ignore_ascii_case(from) && find(current, to).is_some() {
                    push(to, Some(EnvProblem::NameTaken(to.clone())));
                }
            }
        }
    }
    for (i, a) in changes.iter().enumerate() {
        for b in &changes[i + 1..] {
            for name in a.slot_names() {
                if b.slot_names().iter().any(|n| n.eq_ignore_ascii_case(name)) {
                    push(name, Some(EnvProblem::Twice(name.to_string())));
                }
            }
        }
    }
    let after = names_after(current, changes);
    for (i, name) in after.iter().enumerate() {
        if after[..i].iter().any(|n| n.eq_ignore_ascii_case(name)) {
            push(name, Some(EnvProblem::NameTaken(name.clone())));
        }
    }
    found
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvWarning {
    Removes(String),
    PathTooLong(usize),
    PathChangedElsewhere,
}

/// Variables the system and common programs rely on.
pub fn is_protected(name: &str) -> bool {
    ["Path", "TEMP", "TMP"]
        .iter()
        .any(|p| name.eq_ignore_ascii_case(p))
        || name
            .get(..8)
            .is_some_and(|p| p.eq_ignore_ascii_case("OneDrive"))
}

/// `PathChangedElsewhere` is added by the UI.
pub fn change_warnings(changes: &[EnvChange], merged_path_len: Option<usize>) -> Vec<EnvWarning> {
    let mut warnings: Vec<EnvWarning> = changes
        .iter()
        .filter_map(|c| match c {
            EnvChange::Delete { name } if is_protected(name) => {
                Some(EnvWarning::Removes(name.clone()))
            }
            EnvChange::Rename { from, .. } if is_protected(from) => {
                Some(EnvWarning::Removes(from.clone()))
            }
            _ => None,
        })
        .collect();
    if let Some(n) = merged_path_len.filter(|n| *n > PATH_LIMIT) {
        warnings.push(EnvWarning::PathTooLong(n));
    }
    warnings
}

/// A pending edit of one variable. `original` is its current name, `None` for
/// a new variable; `wanted` is `(name, value)`, `None` for a delete.
#[derive(Debug, Clone, PartialEq)]
pub struct VarEdit {
    pub original: Option<String>,
    pub wanted: Option<(String, String)>,
}

pub fn var_change(edit: &VarEdit, current: Option<&EnvVar>) -> Option<EnvChange> {
    if edit.original.is_none() {
        let (name, value) = edit.wanted.clone()?;
        return Some(EnvChange::Add { name, value });
    }
    let current = current.filter(|c| c.is_text())?;
    let Some((name, value)) = edit.wanted.clone() else {
        return Some(EnvChange::Delete {
            name: current.name.clone(),
        });
    };
    if name == current.name {
        (current.value.as_deref() != Some(value.as_str())).then(|| EnvChange::Edit {
            name: current.name.clone(),
            value,
        })
    } else {
        Some(EnvChange::Rename {
            from: current.name.clone(),
            to: name,
            value,
        })
    }
}

pub fn path_change(edit: &PathEdit, current: Option<&EnvVar>) -> Option<EnvChange> {
    if current.is_some_and(|c| !c.is_text()) {
        return None;
    }
    let raw = current.and_then(|c| c.value.as_deref());
    if edit.is_unchanged_from(raw) {
        return None;
    }
    let texts = edit.texts();
    if texts.iter().all(|t| t.trim().is_empty()) {
        return current.map(|c| EnvChange::Delete {
            name: c.name.clone(),
        });
    }
    let value = join_entries(&texts);
    Some(match current {
        Some(c) => EnvChange::Edit {
            name: c.name.clone(),
            value,
        },
        None => EnvChange::Add {
            name: PATH_NAME.to_string(),
            value,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str, kind: u32, value: Option<&str>) -> EnvVar {
        EnvVar {
            name: name.into(),
            kind,
            value: value.map(str::to_string),
            has_snapshot: false,
        }
    }

    fn current() -> Vec<EnvVar> {
        vec![
            var("Path", 2, Some(r"C:\a")),
            var("TEMP", 2, Some(r"%USERPROFILE%\t")),
            var("x", 1, Some("1")),
            var("D", 4, None),
        ]
    }

    fn add(n: &str, v: &str) -> EnvChange {
        EnvChange::Add {
            name: n.into(),
            value: v.into(),
        }
    }
    fn edit(n: &str, v: &str) -> EnvChange {
        EnvChange::Edit {
            name: n.into(),
            value: v.into(),
        }
    }
    fn del(n: &str) -> EnvChange {
        EnvChange::Delete { name: n.into() }
    }
    fn rename(f: &str, t: &str) -> EnvChange {
        EnvChange::Rename {
            from: f.into(),
            to: t.into(),
            value: "v".into(),
        }
    }

    fn problems(changes: &[EnvChange]) -> Vec<EnvProblem> {
        validate_changes(&current(), changes)
            .into_iter()
            .map(|(_, p)| p)
            .collect()
    }

    #[test]
    fn kind_rules_follow_spec_5_2() {
        assert_eq!(kind_to_write(Some(1), "X", "50% done"), 1);
        assert_eq!(kind_to_write(Some(1), "X", r"%TEMP%\x"), 2);
        assert_eq!(kind_to_write(Some(2), "X", "plain"), 2);
        assert_eq!(kind_to_write(Some(1), "Path", r"C:\a;%X%\b"), 2);
        assert_eq!(kind_to_write(Some(1), "Path", r"C:\a"), 1);
        assert_eq!(kind_to_write(None, "path", r"C:\a"), 2);
        assert_eq!(kind_to_write(None, "X", "%Y%"), 2);
        assert_eq!(kind_to_write(None, "X", "y"), 1);
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(name_problem("  "), Some(EnvProblem::EmptyName));
        assert_eq!(name_problem("A=B"), Some(EnvProblem::NameHasEquals));
        assert_eq!(name_problem("A\0"), Some(EnvProblem::NameHasNul));
        assert_eq!(
            name_problem(&"a".repeat(256)),
            Some(EnvProblem::NameTooLong)
        );
        assert_eq!(name_problem(&"é".repeat(255)), None);
        assert_eq!(name_problem(" Foo "), None);
    }

    #[test]
    fn values_are_checked() {
        assert_eq!(value_problem(""), Some(EnvProblem::EmptyValue));
        assert_eq!(value_problem(&"a".repeat(32_767)), None);
        assert_eq!(
            value_problem(&"a".repeat(32_768)),
            Some(EnvProblem::ValueTooLong)
        );
        assert_eq!(
            value_problem(&"\u{1D11E}".repeat(16_384)),
            Some(EnvProblem::ValueTooLong)
        );
        assert_eq!(value_problem("a\0b"), Some(EnvProblem::ValueHasNul));
    }

    #[test]
    fn entries_are_checked() {
        assert_eq!(entry_problem(""), Some(EnvProblem::EmptyEntry));
        assert_eq!(entry_problem("  "), Some(EnvProblem::EmptyEntry));
        assert_eq!(
            entry_problem(r"C:\a;b"),
            Some(EnvProblem::EntryHasSemicolon)
        );
        assert_eq!(entry_problem("a\0"), Some(EnvProblem::ValueHasNul));
        assert_eq!(entry_problem(r"C:\a"), None);
    }

    #[test]
    fn change_names_and_slots() {
        assert_eq!(rename("a", "b").slot_names(), ["a", "b"]);
        assert_eq!(rename("foo", "Foo").slot_names(), ["foo"]);
        assert_eq!(rename("a", "b").display_name(), "a \u{2192} b");
        assert_eq!(del("a").display_name(), "a");
        assert_eq!(
            names_after(&current(), &[del("x"), add("N", "1"), rename("TEMP", "T")]),
            ["Path", "D", "N", "T"]
        );
    }

    fn ve(original: Option<&str>, wanted: Option<(&str, &str)>) -> VarEdit {
        VarEdit {
            original: original.map(str::to_string),
            wanted: wanted.map(|(n, v)| (n.to_string(), v.to_string())),
        }
    }

    #[test]
    fn var_edits_become_changes() {
        let foo = var("FOO", 1, Some("1"));
        assert_eq!(
            var_change(&ve(None, Some(("FOO", "1"))), None),
            Some(add("FOO", "1"))
        );
        assert_eq!(var_change(&ve(None, None), None), None);
        assert_eq!(
            var_change(&ve(Some("FOO"), Some(("FOO", "2"))), Some(&foo)),
            Some(edit("FOO", "2"))
        );
        assert_eq!(
            var_change(&ve(Some("FOO"), Some(("FOO", "1"))), Some(&foo)),
            None
        );
        assert_eq!(
            var_change(&ve(Some("FOO"), Some(("BAR", "1"))), Some(&foo)),
            Some(EnvChange::Rename {
                from: "FOO".into(),
                to: "BAR".into(),
                value: "1".into()
            })
        );
        let lower = var("foo", 1, Some("1"));
        let case_only = var_change(&ve(Some("foo"), Some(("Foo", "1"))), Some(&lower));
        assert_eq!(
            case_only,
            Some(EnvChange::Rename {
                from: "foo".into(),
                to: "Foo".into(),
                value: "1".into()
            })
        );
        assert_eq!(case_only.unwrap().slot_names(), ["foo"]);
        assert_eq!(
            var_change(&ve(Some("FOO"), None), Some(&foo)),
            Some(del("FOO"))
        );
        let binary = var("FOO", 4, None);
        assert_eq!(var_change(&ve(Some("FOO"), None), Some(&binary)), None);
        assert_eq!(var_change(&ve(Some("FOO"), Some(("FOO", "2"))), None), None);
    }

    #[test]
    fn path_edits_become_changes() {
        let path = var("PATH", 2, Some("A;B"));
        let unchanged = PathEdit::start(Some("A;B"));
        assert_eq!(path_change(&unchanged, Some(&path)), None);

        let mut moved = PathEdit::start(Some("A;B"));
        moved.move_up(1);
        assert_eq!(path_change(&moved, Some(&path)), Some(edit("PATH", "B;A")));

        let mut emptied = PathEdit::start(Some("A;B"));
        emptied.remove(0);
        emptied.remove(0);
        assert_eq!(path_change(&emptied, Some(&path)), Some(del("PATH")));
        assert_eq!(path_change(&emptied, None), None);

        let mut fresh = PathEdit::start(None);
        fresh.add(r"C:\x".into());
        assert_eq!(path_change(&fresh, None), Some(add("Path", r"C:\x")));

        let binary = var("Path", 4, None);
        assert_eq!(path_change(&moved, Some(&binary)), None);
    }

    #[test]
    fn validation_refuses_what_spec_5_3_refuses() {
        assert!(problems(&[add("X", "1")]).contains(&EnvProblem::NameTaken("X".into())));
        let pair = problems(&[add("Y", "1"), add("y", "1")]);
        assert!(pair.contains(&EnvProblem::NameTaken("y".into())));
        assert!(problems(&[rename("x", "TEMP")]).contains(&EnvProblem::NameTaken("TEMP".into())));
        assert_eq!(problems(&[rename("x", "X")]), []);
        assert_eq!(
            problems(&[edit("GONE", "1")]),
            [EnvProblem::Gone("GONE".into())]
        );
        assert_eq!(
            problems(&[edit("D", "1")]),
            [EnvProblem::NotText("D".into())]
        );
        assert_eq!(problems(&[del("D")]), [EnvProblem::NotText("D".into())]);
        assert!(problems(&[add("A=B", "1")]).contains(&EnvProblem::NameHasEquals));
        assert_eq!(problems(&[edit("x", "")]), [EnvProblem::EmptyValue]);
        assert!(problems(&[edit("x", "2"), del("X")]).contains(&EnvProblem::Twice("x".into())));
        // Ruling P2: a name freed by a delete in the same batch stays refused.
        let freed = problems(&[del("x"), add("X", "1")]);
        assert!(freed.contains(&EnvProblem::Twice("x".into())));
        assert_eq!(problems(&[edit("x", "2"), add("N", "1")]), []);
        let about = validate_changes(&current(), &[edit("GONE", "1")]);
        assert_eq!(about[0].0, "GONE");
    }

    #[test]
    fn warnings_name_protected_variables_and_long_paths() {
        assert_eq!(
            change_warnings(&[del("TEMP")], None),
            [EnvWarning::Removes("TEMP".into())]
        );
        assert_eq!(
            change_warnings(&[rename("OneDriveCommercial", "X")], None),
            [EnvWarning::Removes("OneDriveCommercial".into())]
        );
        assert_eq!(change_warnings(&[edit("TEMP", "1")], None), []);
        assert_eq!(change_warnings(&[del("FOO")], None), []);
        assert_eq!(change_warnings(&[], Some(8191)), []);
        assert_eq!(
            change_warnings(&[], Some(8192)),
            [EnvWarning::PathTooLong(8192)]
        );
        assert!(is_protected("tmp"));
        assert!(is_protected("onedrive"));
        assert!(!is_protected("PATHEXT"));
    }
}
