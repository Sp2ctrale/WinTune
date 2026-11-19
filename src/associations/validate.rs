//! Rule validation: values that must never reach the registry.

use crate::models::ExtensionConfig;
use std::collections::BTreeMap;

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

pub(super) fn has_forbidden_chars(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_whitespace() || c.is_control() || FORBIDDEN_CHARS.contains(&c))
}

/// Rejects values that would write to an unexpected registry location.
pub fn validate_extension(cfg: &ExtensionConfig) -> Result<(), String> {
    let ext = &cfg.extension;
    if !ext.starts_with('.') || ext.trim_matches('.').is_empty() || has_forbidden_chars(ext) {
        return Err(format!(
            "invalid extension '{}' (expected e.g. '.rs', without spaces or path characters)",
            ext
        ));
    }
    if RESERVED_EXTENSIONS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(ext))
    {
        return Err(format!(
            "{} is reserved: changing how Windows runs it could break the system",
            ext
        ));
    }
    let pid = &cfg.prog_id;
    if pid.trim().is_empty() || pid.starts_with('.') || has_forbidden_chars(pid) {
        return Err(format!(
            "invalid ProgID '{}' (expected e.g. 'CustomRustFile', without spaces or path characters)",
            pid
        ));
    }
    if RESERVED_PROGIDS.iter().any(|r| r.eq_ignore_ascii_case(pid)) {
        return Err(format!(
            "ProgID '{}' is a Windows system key: choose another name (e.g. 'Custom{}File')",
            pid,
            ext.trim_start_matches('.').to_uppercase()
        ));
    }
    Ok(())
}

/// Everything that must be fixed before applying `rules`: invalid values
/// (see [`validate_extension`]), extensions listed more than once and
/// ProgIDs shared by several rules (case-insensitive). Returns
/// `(rule index, message)`, in index order.
pub fn find_rule_problems(rules: &[ExtensionConfig]) -> Vec<(usize, String)> {
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
                return Some((i, format!("{} is listed more than once", rule.extension)));
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
                    format!(
                        "ProgID {} is used by several rules: give each rule its own",
                        rule.prog_id
                    ),
                )
            })
        })
        .collect()
}
