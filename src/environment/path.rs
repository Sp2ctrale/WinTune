//! The `Path` list: splitting, joining, expansion for display, and the
//! missing and duplicate folder checks. Pure: nothing here touches the
//! registry or the disk.

use std::collections::HashSet;

/// How deep an expandable value may reference other expandable values.
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Expanded {
    pub text: String,
    /// False when a reference could not be resolved.
    pub complete: bool,
}

/// Resolves `%name%` references the way Windows would: user variables, then
/// system variables, then the process environment, ignoring ASCII case.
pub struct Resolver {
    user: Vec<(String, String, bool)>,
    system: Vec<(String, String, bool)>,
    process: fn(&str) -> Option<String>,
}

impl Resolver {
    /// Each tuple is `(name, raw value, expandable)`; `expandable` means the
    /// value is `REG_EXPAND_SZ`.
    pub fn new(
        user: Vec<(String, String, bool)>,
        system: Vec<(String, String, bool)>,
        process: fn(&str) -> Option<String>,
    ) -> Self {
        Self {
            user,
            system,
            process,
        }
    }

    fn lookup(&self, name: &str) -> Option<(String, bool)> {
        [&self.user, &self.system]
            .into_iter()
            .flatten()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v, e)| (v.clone(), *e))
            .or_else(|| (self.process)(name).map(|v| (v, false)))
    }

    /// The value of a variable; an expandable value comes back expanded.
    pub fn get(&self, name: &str) -> Option<String> {
        let (value, expandable) = self.lookup(name)?;
        Some(if expandable {
            self.expand_at(&value, 1).text
        } else {
            value
        })
    }

    pub fn expand(&self, text: &str) -> Expanded {
        self.expand_at(text, 0)
    }

    fn expand_at(&self, text: &str, depth: usize) -> Expanded {
        let mut out = String::new();
        let mut complete = true;
        let mut rest = text;
        while let Some(at) = rest.find('%') {
            out.push_str(&rest[..at]);
            let after = &rest[at + 1..];
            match reference_len(after) {
                Some(len) => {
                    let name = &after[..len];
                    let written = &rest[at..at + len + 2];
                    match self.lookup(name) {
                        Some((value, true)) if depth < MAX_DEPTH => {
                            let inner = self.expand_at(&value, depth + 1);
                            complete &= inner.complete;
                            out.push_str(&inner.text);
                        }
                        Some((value, false)) => out.push_str(&value),
                        _ => {
                            complete = false;
                            out.push_str(written);
                        }
                    }
                    rest = &after[len + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        Expanded {
            text: out,
            complete,
        }
    }
}

/// Length of the name of a reference whose opening `%` was just consumed:
/// one or more characters other than `%` and `=`, then a closing `%`.
fn reference_len(after_percent: &str) -> Option<usize> {
    let len = after_percent
        .find(['%', '='])
        .unwrap_or(after_percent.len());
    (len > 0 && after_percent[len..].starts_with('%')).then_some(len)
}

/// True when the text holds a `%name%` reference (spec 5.2).
pub fn has_reference(text: &str) -> bool {
    text.match_indices('%')
        .any(|(i, _)| reference_len(&text[i + 1..]).is_some())
}

/// Split on `;`, dropping empty and whitespace-only entries. Every other
/// entry is kept verbatim.
pub fn split_entries(raw: &str) -> Vec<String> {
    raw.split(';')
        .filter(|e| !e.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// Join with `;`, dropping empty and whitespace-only entries; no trailing `;`.
pub fn join_entries(entries: &[String]) -> String {
    entries
        .iter()
        .filter(|e| !e.trim().is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(";")
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// Comparison key for a folder: one pair of quotes and trailing separators
/// removed, lowercased.
pub fn folder_key(expanded: &str) -> String {
    unquote(expanded)
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

/// An absolute local path: a drive letter, `:`, then a separator.
pub fn checkable(expanded: &str) -> bool {
    let b = unquote(expanded).as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryInfo {
    pub text: String,
    pub expanded: String,
    pub complete: bool,
    pub missing: bool,
    pub duplicate: bool,
}

/// Flag entries whose folder is missing and later copies of a folder already
/// listed (earlier user entry, or any system entry). `exists` is only asked
/// about complete, checkable entries.
pub fn analyze_entries(
    entries: &[String],
    system: &[String],
    resolver: &Resolver,
    exists: &dyn Fn(&str) -> bool,
) -> Vec<EntryInfo> {
    let mut seen: HashSet<String> = system
        .iter()
        .map(|s| folder_key(&resolver.expand(s).text))
        .collect();
    entries
        .iter()
        .map(|text| {
            let Expanded {
                text: expanded,
                complete,
            } = resolver.expand(text);
            let duplicate = !seen.insert(folder_key(&expanded));
            let missing = complete && checkable(&expanded) && !exists(&expanded);
            EntryInfo {
                text: text.clone(),
                expanded,
                complete,
                missing,
                duplicate,
            }
        })
        .collect()
}

pub fn without_missing_and_duplicates(infos: &[EntryInfo]) -> Vec<String> {
    infos
        .iter()
        .filter(|i| !i.missing && !i.duplicate)
        .map(|i| i.text.clone())
        .collect()
}

/// The multiset difference `from - to`, matched by exact text, in `from` order.
pub fn removed_entries(from: &[String], to: &[String]) -> Vec<String> {
    let mut left: Vec<&String> = to.iter().collect();
    from.iter()
        .filter(|f| match left.iter().position(|t| t == f) {
            Some(i) => {
                left.remove(i);
                false
            }
            None => true,
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryMark {
    Same,
    Added,
    Moved,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathEntry {
    pub text: String,
    /// Index in the list the edit started from; `None` for a new or edited entry.
    pub origin: Option<usize>,
    pub moved: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathEdit {
    pub started_from: Option<String>,
    pub entries: Vec<PathEntry>,
}

impl PathEdit {
    pub fn start(current_raw: Option<&str>) -> Self {
        let entries = split_entries(current_raw.unwrap_or(""))
            .into_iter()
            .enumerate()
            .map(|(i, text)| PathEntry {
                text,
                origin: Some(i),
                moved: false,
            })
            .collect();
        Self {
            started_from: current_raw.map(str::to_string),
            entries,
        }
    }

    pub fn texts(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.text.clone()).collect()
    }

    pub fn original(&self) -> Vec<String> {
        split_entries(self.started_from.as_deref().unwrap_or(""))
    }

    pub fn move_up(&mut self, i: usize) {
        if i > 0 && i < self.entries.len() {
            self.entries.swap(i, i - 1);
            self.entries[i - 1].moved = true;
        }
    }

    pub fn move_down(&mut self, i: usize) {
        if i + 1 < self.entries.len() {
            self.entries.swap(i, i + 1);
            self.entries[i + 1].moved = true;
        }
    }

    pub fn edit(&mut self, i: usize, text: String) {
        if let Some(e) = self.entries.get_mut(i) {
            *e = PathEntry {
                text,
                origin: None,
                moved: false,
            };
        }
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.entries.len() {
            self.entries.remove(i);
        }
    }

    pub fn add(&mut self, text: String) {
        self.entries.push(PathEntry {
            text,
            origin: None,
            moved: false,
        });
    }

    /// The original entries no current entry came from, in original order.
    pub fn removed(&self) -> Vec<String> {
        self.original()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !self.entries.iter().any(|e| e.origin == Some(*i)))
            .map(|(_, t)| t)
            .collect()
    }

    pub fn mark(&self, i: usize) -> EntryMark {
        match self.entries.get(i) {
            Some(e) if e.origin.is_none() => EntryMark::Added,
            Some(e) if e.moved => EntryMark::Moved,
            _ => EntryMark::Same,
        }
    }

    pub fn is_unchanged_from(&self, current_raw: Option<&str>) -> bool {
        self.texts() == split_entries(current_raw.unwrap_or(""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn tuples(v: &[(&str, &str, bool)]) -> Vec<(String, String, bool)> {
        v.iter()
            .map(|(n, r, e)| (n.to_string(), r.to_string(), *e))
            .collect()
    }

    fn proc_var(n: &str) -> Option<String> {
        (n == "PROC").then(|| "P".into())
    }

    fn resolver(user: &[(&str, &str, bool)], system: &[(&str, &str, bool)]) -> Resolver {
        Resolver::new(tuples(user), tuples(system), proc_var)
    }

    #[test]
    fn split_and_join_keep_entries_verbatim() {
        let entries = split_entries(r#"C:\a;;%X%\b\; ;"C:\q""#);
        assert_eq!(entries, s(&[r"C:\a", r"%X%\b\", r#""C:\q""#]));
        assert!(split_entries("").is_empty());
        assert_eq!(split_entries("A;"), s(&["A"]));
        assert_eq!(join_entries(&entries), r#"C:\a;%X%\b\;"C:\q""#);
        assert_eq!(join_entries(&[]), "");
        let raw = r"C:\a;%LOCALAPPDATA%\agy\bin;D:\x\";
        assert_eq!(join_entries(&split_entries(raw)), raw);
        let quoted = r#""C:\a;b";D:\c"#;
        assert_eq!(split_entries(quoted).len(), 3);
        assert_eq!(join_entries(&split_entries(quoted)), quoted);
    }

    #[test]
    fn references_follow_spec_5_2() {
        for t in ["%A%", r"%A%\b", "100% and 5%"] {
            assert!(has_reference(t), "{t}");
        }
        for t in ["50% off", "%%", "%a=b%", ""] {
            assert!(!has_reference(t), "{t}");
        }
    }

    #[test]
    fn expansion_uses_user_then_system_then_process() {
        let r = resolver(
            &[("A", "u", false)],
            &[("A", "s", false), ("B", "sb", false)],
        );
        assert_eq!(
            r.expand("%A%;%b%;%PROC%"),
            Expanded {
                text: "u;sb;P".into(),
                complete: true
            }
        );
    }

    #[test]
    fn expansion_keeps_unknown_names_and_marks_incomplete() {
        let r = resolver(&[], &[]);
        assert_eq!(
            r.expand(r"%NOPE%\x"),
            Expanded {
                text: r"%NOPE%\x".into(),
                complete: false
            }
        );
        assert_eq!(
            r.expand("50%"),
            Expanded {
                text: "50%".into(),
                complete: true
            }
        );
    }

    #[test]
    fn nested_expandable_values_are_expanded() {
        let r = resolver(
            &[
                ("BASE", r"C:\b", false),
                ("TOOLS", r"%BASE%\tools", true),
                ("Z", "%BASE%", false),
            ],
            &[],
        );
        assert_eq!(
            r.expand(r"%TOOLS%\bin"),
            Expanded {
                text: r"C:\b\tools\bin".into(),
                complete: true
            }
        );
        assert_eq!(r.get("Z"), Some("%BASE%".to_string()));
    }

    #[test]
    fn expansion_stops_on_cycles() {
        let r = resolver(&[("X", "%Y%", true), ("Y", "%X%", true)], &[]);
        assert!(!r.expand("%X%").complete);
    }

    #[test]
    fn folder_keys_ignore_case_quotes_and_trailing_backslash() {
        assert_eq!(folder_key(r"C:\Tools\"), folder_key(r#""c:\tools""#));
    }

    #[test]
    fn checkable_paths_are_absolute_local() {
        assert!(checkable(r"C:\x"));
        assert!(checkable("c:/x"));
        assert!(!checkable(r"\\srv\share"));
        assert!(!checkable("bin"));
        assert!(!checkable(r"%X%\y"));
    }

    fn analyzed() -> Vec<EntryInfo> {
        let r = resolver(&[("T", r"C:\tools", false)], &[]);
        let entries = s(&[r"C:\tools", r"%T%\", r"C:\gone", r"%NOPE%\x", r"C:\SYS\"]);
        let calls = RefCell::new(Vec::new());
        let exists = |p: &str| {
            calls.borrow_mut().push(p.to_string());
            let k = folder_key(p);
            k == r"c:\tools" || k == r"c:\sys"
        };
        let infos = analyze_entries(&entries, &s(&[r"c:\sys"]), &r, &exists);
        assert!(!calls.borrow().iter().any(|c| c.contains("NOPE")));
        infos
    }

    #[test]
    fn analyze_flags_missing_and_later_duplicates() {
        let infos = analyzed();
        let dup: Vec<bool> = infos.iter().map(|i| i.duplicate).collect();
        let miss: Vec<bool> = infos.iter().map(|i| i.missing).collect();
        assert_eq!(dup, [false, true, false, false, true]);
        assert_eq!(miss, [false, false, true, false, false]);
    }

    #[test]
    fn remove_missing_and_duplicates_keeps_first_copies() {
        assert_eq!(
            without_missing_and_duplicates(&analyzed()),
            s(&[r"C:\tools", r"%NOPE%\x"])
        );
    }

    #[test]
    fn removed_entries_is_a_multiset_difference_in_order() {
        assert_eq!(
            removed_entries(&s(&["A", "B", "A", "C"]), &s(&["A", "C"])),
            s(&["B", "A"])
        );
    }

    #[test]
    fn path_edit_moves_edits_removes_and_adds() {
        let mut e = PathEdit::start(Some("A;B;C"));
        e.move_up(1);
        assert_eq!(e.texts(), s(&["B", "A", "C"]));
        assert_eq!(e.mark(0), EntryMark::Moved);
        assert_eq!(e.mark(1), EntryMark::Same);
        e.remove(2);
        assert_eq!(e.removed(), s(&["C"]));
        e.edit(0, "B2".into());
        assert_eq!(e.mark(0), EntryMark::Added);
        assert!(e.removed().contains(&"B".to_string()));
        e.add("D".into());
        assert_eq!(e.mark(e.entries.len() - 1), EntryMark::Added);
        let before = e.clone();
        e.move_up(0);
        e.move_down(e.entries.len() - 1);
        assert_eq!(e, before);

        let none = PathEdit::start(None);
        assert!(none.entries.is_empty());
        assert!(none.original().is_empty());
        assert!(PathEdit::start(Some("A;B")).is_unchanged_from(Some("A;B;")));
    }
}
