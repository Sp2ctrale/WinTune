//! Rule validation: values that must never reach the registry.

use crate::models::ExtensionConfig;
use std::collections::BTreeMap;
use std::fmt;

const FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Structural `Classes` keys and launch types of executables: writing a
/// ProgID over them (or claiming these extensions) would break Windows.
const RESERVED_PROGIDS: &[&str] = &[
    "*",
    "AllFilesystemObjects",
    "AppID",
    "Applications",
    "CLSID",
    "DesktopBackground",
    "Directory",
    "Drive",
    "Folder",
    "Installer",
    "Interface",
    "MIME",
    "SystemFileAssociations",
    "TypeLib",
    "Unknown",
    "Wow6432Node",
    "batfile",
    "cmdfile",
    "comfile",
    "cplfile",
    "exefile",
    "lnkfile",
    "Msi.Package",
    "Msi.Patch",
    "piffile",
    "scrfile",
];

const RESERVED_EXTENSIONS: &[&str] = &[
    ".com", ".cpl", ".dll", ".exe", ".lnk", ".msi", ".msp", ".pif", ".scr", ".sys", ".url",
];

/// Why a rule cannot be applied. Carries the values, not a sentence: the UI
/// translates it (`i18n::messages::rule_problem`); `Display` is the English
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleProblem {
    InvalidExtension {
        ext: String,
    },
    ReservedExtension {
        ext: String,
    },
    InvalidProgId {
        prog_id: String,
    },
    /// `suggestion` is "Custom<EXT>File".
    ReservedProgId {
        prog_id: String,
        suggestion: String,
    },
    DuplicateExtension {
        ext: String,
    },
    SharedProgId {
        prog_id: String,
    },
}

impl fmt::Display for RuleProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleProblem::InvalidExtension { ext } => write!(
                f,
                "invalid extension '{}' (expected e.g. '.rs', without spaces or path characters)",
                ext
            ),
            RuleProblem::ReservedExtension { ext } => write!(
                f,
                "{} is reserved: changing how Windows runs it could break the system",
                ext
            ),
            RuleProblem::InvalidProgId { prog_id } => write!(
                f,
                "invalid ProgID '{}' (expected e.g. 'CustomRustFile', without spaces or path characters)",
                prog_id
            ),
            RuleProblem::ReservedProgId {
                prog_id,
                suggestion,
            } => write!(
                f,
                "ProgID '{}' is a Windows system key: choose another name (e.g. '{}')",
                prog_id, suggestion
            ),
            RuleProblem::DuplicateExtension { ext } => {
                write!(f, "{} is listed more than once", ext)
            }
            RuleProblem::SharedProgId { prog_id } => write!(
                f,
                "ProgID {} is used by several rules: give each rule its own",
                prog_id
            ),
        }
    }
}

pub(super) fn has_forbidden_chars(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_whitespace() || c.is_control() || FORBIDDEN_CHARS.contains(&c))
}

/// Rejects values that would write to an unexpected registry location.
pub fn validate_extension(cfg: &ExtensionConfig) -> Result<(), RuleProblem> {
    let ext = &cfg.extension;
    if !ext.starts_with('.') || ext.trim_matches('.').is_empty() || has_forbidden_chars(ext) {
        return Err(RuleProblem::InvalidExtension { ext: ext.clone() });
    }
    if RESERVED_EXTENSIONS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(ext))
    {
        return Err(RuleProblem::ReservedExtension { ext: ext.clone() });
    }
    let pid = &cfg.prog_id;
    if pid.trim().is_empty() || pid.starts_with('.') || has_forbidden_chars(pid) {
        return Err(RuleProblem::InvalidProgId {
            prog_id: pid.clone(),
        });
    }
    if RESERVED_PROGIDS.iter().any(|r| r.eq_ignore_ascii_case(pid)) {
        return Err(RuleProblem::ReservedProgId {
            prog_id: pid.clone(),
            suggestion: format!("Custom{}File", ext.trim_start_matches('.').to_uppercase()),
        });
    }
    Ok(())
}

/// Everything that must be fixed before applying `rules`: invalid values
/// (see [`validate_extension`]), extensions listed more than once and
/// ProgIDs shared by several rules (case-insensitive). Returns
/// `(rule index, problem)`, in index order.
pub fn find_rule_problems(rules: &[ExtensionConfig]) -> Vec<(usize, RuleProblem)> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut prog_ids: BTreeMap<String, usize> = BTreeMap::new();
    for rule in rules {
        *seen.entry(rule.extension.to_lowercase()).or_default() += 1;
        *prog_ids.entry(rule.prog_id.to_lowercase()).or_default() += 1;
    }
    rules
        .iter()
        .enumerate()
        .filter_map(|(i, rule)| {
            if let Err(e) = validate_extension(rule) {
                return Some((i, e));
            }
            let count = seen
                .get(&rule.extension.to_lowercase())
                .copied()
                .unwrap_or(0);
            if count > 1 {
                return Some((
                    i,
                    RuleProblem::DuplicateExtension {
                        ext: rule.extension.clone(),
                    },
                ));
            }
            // One ProgID per rule: rolling back one extension would otherwise
            // delete the key the other one still uses.
            let shared = prog_ids
                .get(&rule.prog_id.to_lowercase())
                .copied()
                .unwrap_or(0);
            (shared > 1).then(|| {
                (
                    i,
                    RuleProblem::SharedProgId {
                        prog_id: rule.prog_id.clone(),
                    },
                )
            })
        })
        .collect()
}
