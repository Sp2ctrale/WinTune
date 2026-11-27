//! Environment variables: the per-user variables under `HKCU\Environment`
//! and the `Path` list, with the machine variables shown read-only.
//!
//! WinTune only ever writes values of `HKCU\Environment`. Nothing is written
//! under HKLM, and `HKCU\Volatile Environment` is never read or written.

use crate::backups::{RegTarget, Slot};

mod engine;
pub use engine::*;
mod path;
pub use path::*;
mod rules;
pub use rules::*;
mod scan;
pub use scan::*;

pub const USER_ENV_KEY: &str = "Environment";
pub const SYSTEM_ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
/// Never read, never written; named only for the tests: the guards that it
/// stays untouched, and the drift test of the registry reference.
pub const VOLATILE_ENV_KEY: &str = "Volatile Environment";

pub const PATH_NAME: &str = "Path";
pub const PATH_LIMIT: usize = 8191;

pub const REG_SZ_KIND: u32 = 1;
pub const REG_EXPAND_SZ_KIND: u32 = 2;

/// Where variables live. Writes only ever use `user`; `machine` is read.
pub struct EnvTarget {
    pub user: RegTarget,
    pub machine: RegTarget,
}

impl EnvTarget {
    pub fn current_user() -> Self {
        Self {
            user: RegTarget::current_user(),
            machine: RegTarget::local_machine(),
        }
    }
}

pub fn is_path_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(PATH_NAME)
}

pub fn env_slot(name: &str) -> Slot {
    Slot::Value {
        path: USER_ENV_KEY.to_string(),
        name: name.to_string(),
    }
}

/// UTF-16LE with a NUL terminator, the registry's string layout.
pub fn encode_text(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Production fallback for per-sign-in variables such as `USERPROFILE`, which
/// live in `Volatile Environment`.
pub fn process_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference page must name each location in its terminated form (the
    /// key text followed by a closing backtick), so `HKCU\Environment` cannot
    /// be satisfied by a longer path or by the word "Environment" alone.
    #[test]
    fn the_registry_reference_names_every_environment_location() {
        let doc = include_str!("../../docs/registry-reference.md");
        let wanted = [
            format!("`HKCU\\{USER_ENV_KEY}`"),
            format!("`HKLM\\{SYSTEM_ENV_KEY}`"),
            format!("`HKCU\\{VOLATILE_ENV_KEY}`"),
            "`environment-backups.json`".to_string(),
            "`WM_SETTINGCHANGE`".to_string(),
            "Pre-merge checklist (Environment variables)".to_string(),
        ];
        for text in &wanted {
            assert!(
                doc.contains(text.as_str()),
                "registry-reference.md lacks {text}"
            );
        }
    }
}
