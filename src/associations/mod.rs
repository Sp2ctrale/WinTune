//! File-association engine: writes custom ProgIDs under `HKLM\SOFTWARE\Classes`,
//! cleans per-user overrides, and undoes everything from lossless snapshots.
//!
//! Safety model of [`apply_extensions`]:
//! 1. validate every rule and install its icon — nothing written yet;
//! 2. snapshot every key about to change, persist the snapshots to
//!    `backups.json` and export a `.reg` safety net — abort if either fails;
//! 3. only then write to the registry, reporting every failure.
//!
//! Layout: `target` (where keys live), `validate` (rules that must never
//! reach the registry), `store` (`backups.json`), `apply`, `rollback` and
//! `status` (live state of a rule).

mod apply;
mod rollback;
mod status;
mod store;
mod target;
mod validate;

#[cfg(test)]
mod tests;

pub use apply::{apply_extensions, notify_shell_associations, ApplyOptions};
pub use rollback::rollback_extensions;
pub use status::{rule_status, DriftField, DriftReason, RuleStatus, StatusContext};
pub use store::{
    default_backup_store_path, default_reg_backup_dir, load_backup_store, save_backup_store,
};
pub use target::AssocTarget;
pub use validate::{find_rule_problems, validate_extension, RuleProblem};

use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct OperationReport {
    pub succeeded: Vec<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub reg_backup_file: Option<PathBuf>,
}
