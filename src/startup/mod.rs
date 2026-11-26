//! Startup apps: what starts with Windows, and the per-user on/off switch
//! Windows itself keeps under `StartupApproved`.
//!
//! WinTune only ever writes the switch values under HKCU. It never edits a
//! `Run` value and never touches a file in a Startup folder.

use std::path::PathBuf;

mod engine;
mod scan;
pub use engine::*;
pub use scan::*;

use crate::backups::{RegTarget, Slot};
use crate::regsnap::ValueSnapshot;

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN32_KEY: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
pub const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// Where a startup item comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    UserRun,
    MachineRun,
    MachineRun32,
    UserFolder,
    CommonFolder,
}

impl Source {
    pub const ALL: [Source; 5] = [
        Source::UserRun,
        Source::MachineRun,
        Source::MachineRun32,
        Source::UserFolder,
        Source::CommonFolder,
    ];

    /// True when the item starts for every user of the machine.
    pub fn all_users(self) -> bool {
        matches!(
            self,
            Source::MachineRun | Source::MachineRun32 | Source::CommonFolder
        )
    }

    /// The `StartupApproved` subkey that holds this source's switch.
    pub fn approved(self) -> Approved {
        match self {
            Source::UserRun | Source::MachineRun => Approved::Run,
            Source::MachineRun32 => Approved::Run32,
            Source::UserFolder | Source::CommonFolder => Approved::StartupFolder,
        }
    }

    /// Whether a per-user switch in HKCU turns this source's items off.
    pub fn per_user_switch_works(self) -> bool {
        PER_USER_SWITCH_WORKS
            .iter()
            .find(|(s, _)| *s == self)
            .is_some_and(|(_, works)| *works)
    }
}

/// One flag per source: does the per-user switch in HKCU turn an item off?
/// The pre-merge checklist in `docs/registry-reference.md` decides each
/// all-users value; a failed check sets it to `false`.
pub const PER_USER_SWITCH_WORKS: [(Source, bool); 5] = [
    (Source::UserRun, true),
    (Source::MachineRun, true),
    (Source::MachineRun32, false),
    (Source::UserFolder, true),
    (Source::CommonFolder, true),
];

/// A `StartupApproved` subkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Approved {
    Run,
    Run32,
    StartupFolder,
}

impl Approved {
    /// Registry path of the subkey, under HKCU.
    pub fn key(self) -> String {
        let sub = match self {
            Approved::Run => "Run",
            Approved::Run32 => "Run32",
            Approved::StartupFolder => "StartupFolder",
        };
        format!(r"{APPROVED_KEY}\{sub}")
    }

    pub fn id_prefix(self) -> &'static str {
        match self {
            Approved::Run => "run",
            Approved::Run32 => "run32",
            Approved::StartupFolder => "folder",
        }
    }

    /// The switch value for `name`.
    pub fn slot(self, name: &str) -> Slot {
        Slot::Value {
            path: self.key(),
            name: name.to_string(),
        }
    }

    /// Stable row id: names compare case-insensitively.
    pub fn row_id(self, name: &str) -> String {
        format!("{}:{}", self.id_prefix(), name.to_lowercase())
    }
}

/// What a `StartupApproved` value says about an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Absent,
    On(u32),
    Off(u32),
    Unknown,
}

impl Switch {
    pub fn is_off(self) -> bool {
        matches!(self, Switch::Off(_))
    }
}

const REG_BINARY: u32 = 3;

/// Flag rules: absent = on; wrong kind or under 4 bytes = unknown; 2/6 on; 3/7 off; else unknown.
pub fn read_switch(v: &ValueSnapshot) -> Switch {
    if !v.exists {
        return Switch::Absent;
    }
    if v.kind != REG_BINARY || v.data.len() < 4 {
        return Switch::Unknown;
    }
    let flag = u32::from_le_bytes([v.data[0], v.data[1], v.data[2], v.data[3]]);
    match flag {
        2 | 6 => Switch::On(flag),
        3 | 7 => Switch::Off(flag),
        _ => Switch::Unknown,
    }
}

/// The value Windows writes when an item is turned off at `filetime`.
pub fn off_value(filetime: u64) -> ValueSnapshot {
    let mut data = vec![3, 0, 0, 0];
    data.extend_from_slice(&filetime.to_le_bytes());
    ValueSnapshot {
        exists: true,
        kind: REG_BINARY,
        data,
    }
}

/// The value Windows writes when an item is turned on.
pub fn on_value() -> ValueSnapshot {
    let mut data = vec![2, 0, 0, 0];
    data.extend_from_slice(&[0; 8]);
    ValueSnapshot {
        exists: true,
        kind: REG_BINARY,
        data,
    }
}

/// Current time as a FILETIME (100 ns ticks since 1601).
pub fn now_filetime() -> u64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    let mut ft = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: `ft` is a valid, writable FILETIME.
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// Where startup items live. Writes only ever use `user`.
pub struct StartupTarget {
    pub user: RegTarget,
    pub machine: RegTarget,
    pub user_folder: Option<PathBuf>,
    pub common_folder: Option<PathBuf>,
}

impl StartupTarget {
    pub fn current_user() -> Self {
        let folder = |var: &str, tail: &str| {
            std::env::var_os(var).map(|base| PathBuf::from(base).join(tail))
        };
        Self {
            user: RegTarget::current_user(),
            machine: RegTarget::local_machine(),
            user_folder: folder("APPDATA", r"Microsoft\Windows\Start Menu\Programs\Startup"),
            common_folder: folder(
                "ProgramData",
                r"Microsoft\Windows\Start Menu\Programs\StartUp",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(data: &[u8]) -> ValueSnapshot {
        ValueSnapshot {
            exists: true,
            kind: 3,
            data: data.to_vec(),
        }
    }

    #[test]
    fn switch_reads_every_known_shape() {
        let mut on2 = vec![2, 0, 0, 0];
        on2.extend([0; 8]);
        let mut on6 = vec![6, 0, 0, 0];
        on6.extend([0; 8]);
        let mut off3 = vec![3, 0, 0, 0];
        off3.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        let mut off7 = vec![7, 0, 0, 0];
        off7.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        let dword = ValueSnapshot {
            exists: true,
            kind: 4,
            data: vec![3, 0, 0, 0],
        };
        let cases: Vec<(ValueSnapshot, Switch)> = vec![
            (ValueSnapshot::default(), Switch::Absent),
            (bin(&on2), Switch::On(2)),
            (bin(&on6), Switch::On(6)),
            (bin(&off3), Switch::Off(3)),
            (bin(&off7), Switch::Off(7)),
            (bin(&[3, 0, 0, 0]), Switch::Off(3)),
            (bin(&[3, 0, 0]), Switch::Unknown),
            (dword, Switch::Unknown),
            (bin(&[0, 0, 0, 0]), Switch::Unknown),
            (bin(&[1, 0, 0, 0]), Switch::Unknown),
            (bin(&[4, 0, 0, 0]), Switch::Unknown),
        ];
        for (value, want) in cases {
            assert_eq!(read_switch(&value), want, "{value:?}");
        }
        assert!(Switch::Off(7).is_off());
        assert!(!Switch::On(2).is_off());
    }

    #[test]
    fn off_and_on_values_are_twelve_bytes() {
        let off = off_value(0x01DB_1234_5678_9ABC);
        assert!(off.exists);
        assert_eq!(off.kind, 3);
        assert_eq!(
            off.data,
            [3, 0, 0, 0, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12, 0xDB, 0x01]
        );
        assert_eq!(on_value().kind, 3);
        assert_eq!(on_value().data, [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn every_source_has_one_flag_and_per_user_sources_work() {
        for s in Source::ALL {
            let n = PER_USER_SWITCH_WORKS
                .iter()
                .filter(|(x, _)| *x == s)
                .count();
            assert_eq!(n, 1, "{s:?}");
        }
        assert!(Source::UserRun.per_user_switch_works());
        assert!(Source::UserFolder.per_user_switch_works());
    }

    #[test]
    fn sources_map_to_their_approved_value() {
        assert_eq!(Source::UserRun.approved(), Approved::Run);
        assert_eq!(Source::MachineRun.approved(), Approved::Run);
        assert_eq!(Source::MachineRun32.approved(), Approved::Run32);
        assert_eq!(Source::UserFolder.approved(), Approved::StartupFolder);
        assert_eq!(Source::CommonFolder.approved(), Approved::StartupFolder);
        assert!(!Source::UserRun.all_users());
        assert!(!Source::UserFolder.all_users());
        assert!(Source::MachineRun.all_users());
        assert!(Source::MachineRun32.all_users());
        assert!(Source::CommonFolder.all_users());
        assert_eq!(Approved::Run32.key(), format!(r"{APPROVED_KEY}\Run32"));
        assert_eq!(Approved::Run.id_prefix(), "run");
        assert_eq!(
            Approved::StartupFolder.row_id("Discord.LNK"),
            "folder:discord.lnk"
        );
        assert!(Approved::Run.slot("X").id().ends_with("\\\\X"));
    }

    #[test]
    fn now_filetime_is_after_2020() {
        assert!(now_filetime() > 132_223_104_000_000_000);
    }
}
