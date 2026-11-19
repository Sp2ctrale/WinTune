//! Live, read-only status of a rule compared with the registry.

use super::apply::script_launcher;
use super::target::AssocTarget;
use crate::models::{ExtensionBackup, ExtensionConfig};
use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

/// Live state of a rule in the registry, compared with what applying the
/// rule as currently configured would write.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleStatus {
    /// WinTune never applied it and the extension does not use its ProgID.
    NotApplied,
    /// The registry matches the rule.
    Applied,
    /// Applied, but something differs now (changed by Windows, another app,
    /// a per-user override, or the rule was edited since). Reasons included.
    Drifted(Vec<String>),
}

/// Rule-independent inputs of [`rule_status`].
pub struct StatusContext {
    pub editor_command: String,
    pub wsl_available: bool,
    /// Folder icons are installed into (`<folder>\<icon_name>.ico`).
    pub icon_dir: String,
}

/// Read-only: never writes to the registry.
pub fn rule_status(
    target: &AssocTarget,
    cfg: &ExtensionConfig,
    backup: Option<&ExtensionBackup>,
    ctx: &StatusContext,
) -> RuleStatus {
    let m = &target.machine_root;
    let ext = &cfg.extension;
    let current = read_string(m, &target.class_path(ext), "");
    if backup.is_none() && current.as_deref() != Some(cfg.prog_id.as_str()) {
        return RuleStatus::NotApplied;
    }

    let mut reasons = Vec::new();
    match current.as_deref() {
        Some(p) if p == cfg.prog_id => {}
        Some(p) if !p.is_empty() => reasons.push(format!(
            "{} now opens with {} instead of {}",
            ext, p, cfg.prog_id
        )),
        _ => reasons.push(format!(
            "{} has no default ProgID any more (expected {})",
            ext, cfg.prog_id
        )),
    }

    let prog_path = target.class_path(&cfg.prog_id);
    if m.open_subkey_with_flags(&prog_path, KEY_READ).is_err() {
        reasons.push(format!("ProgID {} is missing", cfg.prog_id));
    } else {
        let icon = Path::new(&ctx.icon_dir)
            .join(format!("{}.ico", cfg.icon_name))
            .to_string_lossy()
            .into_owned();
        let open_command = script_launcher(cfg, ctx.wsl_available).unwrap_or(&ctx.editor_command);
        let expected = [
            (prog_path.clone(), "description", cfg.description.as_str()),
            (format!("{}\\DefaultIcon", prog_path), "icon", icon.as_str()),
            (
                format!("{}\\shell\\open\\command", prog_path),
                "open command",
                open_command,
            ),
        ];
        for (path, label, want) in expected {
            let found = read_string(m, &path, "").unwrap_or_default();
            if found != want {
                reasons.push(format!(
                    "{} differs: expected \"{}\", found \"{}\"",
                    label, want, found
                ));
            }
        }
    }

    // Per-user settings win over HKLM in Explorer.
    let u = &target.users_root;
    for hive in &target.user_hives {
        let user = hive.rsplit('\\').next().unwrap_or(hive);
        for choice in ["UserChoice", "UserChoiceLatest"] {
            let path = format!("{}\\{}", target.user_file_exts_path(hive, ext), choice);
            if let Some(p) = read_string(u, &path, "ProgId").filter(|p| p != &cfg.prog_id) {
                reasons.push(format!(
                    "Explorer {} of user {} opens {} with {}",
                    choice, user, ext, p
                ));
            }
        }
        let classes = target.user_classes_path(hive, ext);
        if let Some(p) = read_string(u, &classes, "").filter(|p| !p.is_empty() && p != &cfg.prog_id)
        {
            reasons.push(format!(
                "a per-user override of user {} points {} to {}",
                user, ext, p
            ));
        }
    }

    if reasons.is_empty() {
        RuleStatus::Applied
    } else {
        RuleStatus::Drifted(reasons)
    }
}

fn read_string(root: &RegKey, path: &str, name: &str) -> Option<String> {
    root.open_subkey_with_flags(path, KEY_READ)
        .ok()?
        .get_value::<String, _>(name)
        .ok()
}
