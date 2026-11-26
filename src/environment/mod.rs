//! Environment variables: the per-user variables under `HKCU\Environment`
//! and the `Path` list, with the machine variables shown read-only.
//!
//! WinTune only ever writes values of `HKCU\Environment`. Nothing is written
//! under HKLM, and `HKCU\Volatile Environment` is never read or written.

use crate::backups::Slot;

mod path;
pub use path::*;

pub const USER_ENV_KEY: &str = "Environment";
pub const SYSTEM_ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
/// Never read, never written; named only for the guard test.
pub const VOLATILE_ENV_KEY: &str = "Volatile Environment";

pub const PATH_NAME: &str = "Path";
pub const PATH_LIMIT: usize = 8191;

pub const REG_SZ_KIND: u32 = 1;
pub const REG_EXPAND_SZ_KIND: u32 = 2;

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
