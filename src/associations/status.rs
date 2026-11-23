//! Live, read-only status of a rule compared with the registry.

use super::apply::script_launcher;
use super::target::AssocTarget;
use crate::models::{ExtensionBackup, ExtensionConfig};
use std::fmt;
use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

/// Live state of a rule in the registry, compared with what applying the
/// rule as currently configured would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleStatus {
    /// WinTune never applied it and the extension does not use its ProgID.
    NotApplied,
    /// The registry matches the rule.
    Applied,
    /// Applied, but something differs now (changed by Windows, another app,
    /// a per-user override, or the rule was edited since). Reasons included.
    Drifted(Vec<DriftReason>),
}

/// Which value of a ProgID differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftField {
    Description,
    Icon,
    OpenCommand,
}

impl fmt::Display for DriftField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DriftField::Description => "description",
            DriftField::Icon => "icon",
            DriftField::OpenCommand => "open command",
        })
    }
}

/// One difference between a rule and the registry. Carries the values: the
/// UI translates it (`i18n::messages::drift_reason`); `Display` is the
/// English text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftReason {
    DefaultChanged {
        ext: String,
        found: String,
        expected: String,
    },
    DefaultMissing {
        ext: String,
        expected: String,
    },
    ProgIdMissing {
        prog_id: String,
    },
    ValueDiffers {
        field: DriftField,
        expected: String,
        found: String,
    },
    /// `choice` is "UserChoice" or "UserChoiceLatest".
    UserChoice {
        choice: String,
        user: String,
        ext: String,
        prog_id: String,
    },
    UserOverride {
        user: String,
        ext: String,
        prog_id: String,
    },
}

impl fmt::Display for DriftReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriftReason::DefaultChanged {
                ext,
                found,
                expected,
            } => write!(
                f,
                "{} now opens with {} instead of {}",
                ext, found, expected
            ),
            DriftReason::DefaultMissing { ext, expected } => write!(
                f,
                "{} has no default ProgID any more (expected {})",
                ext, expected
            ),
            DriftReason::ProgIdMissing { prog_id } => write!(f, "ProgID {} is missing", prog_id),
            DriftReason::ValueDiffers {
                field,
                expected,
                found,
            } => write!(
                f,
                "{} differs: expected \"{}\", found \"{}\"",
                field, expected, found
            ),
            DriftReason::UserChoice {
                choice,
                user,
                ext,
                prog_id,
            } => write!(
                f,
                "Explorer {} of user {} opens {} with {}",
                choice, user, ext, prog_id
            ),
            DriftReason::UserOverride { user, ext, prog_id } => write!(
                f,
                "a per-user override of user {} points {} to {}",
                user, ext, prog_id
            ),
        }
    }
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
        Some(p) if !p.is_empty() => reasons.push(DriftReason::DefaultChanged {
            ext: ext.clone(),
            found: p.to_string(),
            expected: cfg.prog_id.clone(),
        }),
        _ => reasons.push(DriftReason::DefaultMissing {
            ext: ext.clone(),
            expected: cfg.prog_id.clone(),
        }),
    }

    let prog_path = target.class_path(&cfg.prog_id);
    if m.open_subkey_with_flags(&prog_path, KEY_READ).is_err() {
        reasons.push(DriftReason::ProgIdMissing {
            prog_id: cfg.prog_id.clone(),
        });
    } else {
        let icon = Path::new(&ctx.icon_dir)
            .join(format!("{}.ico", cfg.icon_name))
            .to_string_lossy()
            .into_owned();
        let open_command = script_launcher(cfg, ctx.wsl_available).unwrap_or(&ctx.editor_command);
        let expected = [
            (
                prog_path.clone(),
                DriftField::Description,
                cfg.description.as_str(),
            ),
            (
                format!("{}\\DefaultIcon", prog_path),
                DriftField::Icon,
                icon.as_str(),
            ),
            (
                format!("{}\\shell\\open\\command", prog_path),
                DriftField::OpenCommand,
                open_command,
            ),
        ];
        for (path, field, want) in expected {
            let found = read_string(m, &path, "").unwrap_or_default();
            if found != want {
                reasons.push(DriftReason::ValueDiffers {
                    field,
                    expected: want.to_string(),
                    found,
                });
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
                reasons.push(DriftReason::UserChoice {
                    choice: choice.to_string(),
                    user: user.to_string(),
                    ext: ext.clone(),
                    prog_id: p,
                });
            }
        }
        let classes = target.user_classes_path(hive, ext);
        if let Some(p) = read_string(u, &classes, "").filter(|p| !p.is_empty() && p != &cfg.prog_id)
        {
            reasons.push(DriftReason::UserOverride {
                user: user.to_string(),
                ext: ext.clone(),
                prog_id: p,
            });
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
