//! What the modules that change settings through `SystemParametersInfoW`
//! share: the value type, the SET/GET executor, the check that WinTune runs
//! as the session's user, the snapshot store and the `.reg` export.

mod account;
mod spi;
mod store;

pub use account::{compare_accounts, session_account_check, AccountCheck};
pub use spi::ApiError;
pub(crate) use spi::{last_error, spi_get, spi_get_animation, spi_set, SetCall, WRONG_SHAPE};
pub use store::{export_values, SpiBackup, SpiBackupStore};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Number,
    Flag,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum SettingValue {
    Number(u32),
    Flag(bool),
    Mouse([i32; 3]),
}

impl SettingValue {
    pub fn shape(&self) -> Shape {
        match self {
            SettingValue::Number(_) => Shape::Number,
            SettingValue::Flag(_) => Shape::Flag,
            SettingValue::Mouse(_) => Shape::Mouse,
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::backups::RegTarget;
    use crate::regsnap::test_support::Sandbox;
    use std::path::PathBuf;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    /// A sandbox `{sandbox}\HKCU` plus a temp folder for the store and `reg`.
    pub(crate) struct Fixture {
        pub(crate) sb: Sandbox,
        pub(crate) tmp: PathBuf,
        pub(crate) source: RegTarget,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.tmp);
        }
    }

    impl Fixture {
        pub(crate) fn new() -> Self {
            let sb = Sandbox::new();
            let leaf = sb.path.rsplit('\\').next().unwrap().to_string();
            let tmp = std::env::temp_dir().join(format!("wintune-sp-{leaf}"));
            std::fs::create_dir_all(&tmp).unwrap();
            let source = RegTarget {
                root: RegKey::predef(HKEY_CURRENT_USER),
                root_name: "HKEY_CURRENT_USER".into(),
                base: sb.key("HKCU"),
            };
            Fixture { sb, tmp, source }
        }

        /// Creates (or opens for writing) `rel` under the sandbox HKCU.
        pub(crate) fn key(&self, rel: &str) -> RegKey {
            self.sb.root.create_subkey(self.source.path(rel)).unwrap().0
        }

        pub(crate) fn reg_dir(&self) -> PathBuf {
            self.tmp.join("reg")
        }

        pub(crate) fn reg_files(&self) -> Vec<PathBuf> {
            std::fs::read_dir(self.reg_dir())
                .map(|d| d.map(|e| e.unwrap().path()).collect())
                .unwrap_or_default()
        }
    }
}
