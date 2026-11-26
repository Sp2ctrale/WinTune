//! The read-only scan: which items start with Windows, and whether each is
//! switched off. Reads the Run keys, the two Startup folders and the
//! `StartupApproved` switches; writes nothing.

use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

use super::{read_switch, Approved, Source, StartupTarget, Switch, RUN32_KEY, RUN_KEY};
use crate::backups::{RegTarget, Slot, UserBackups};
use crate::context_menu::fileinfo::decode_utf16le;
use crate::context_menu::origin::command_program;
use crate::regsnap::snapshot_value;

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// One place an item starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub source: Source,
    /// The Run command as stored (unexpanded), or the file's full path.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupState {
    On,
    /// Off, and WinTune holds the original switch.
    Off,
    /// Off, but turned off by Windows or another tool: no snapshot of ours.
    OffElsewhere,
}

/// Why WinTune cannot change an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupLock {
    Unreadable(String),
    UnknownShape,
    OffForEveryone,
    AllUsersOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartupItem {
    pub id: String,
    pub approved: Approved,
    /// The approved value name, spelled as in the first origin.
    pub name: String,
    pub display_name: String,
    pub program: String,
    /// In `Source` order.
    pub origins: Vec<Origin>,
    pub program_missing: bool,
    pub state: StartupState,
    pub locked: Option<StartupLock>,
    pub has_snapshot: bool,
}

impl StartupItem {
    pub fn slot(&self) -> Slot {
        self.approved.slot(&self.name)
    }

    pub fn all_users(&self) -> bool {
        self.origins.iter().any(|o| o.source.all_users())
    }

    pub fn sources(&self) -> Vec<Source> {
        self.origins.iter().map(|o| o.source).collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StartupScan {
    pub items: Vec<StartupItem>,
}

impl StartupScan {
    pub fn find(&self, id: &str) -> Option<&StartupItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The item whose switch slot has this id, ignoring case.
    pub fn by_slot(&self, slot_id: &str) -> Option<&StartupItem> {
        self.items
            .iter()
            .find(|i| i.slot().id().eq_ignore_ascii_case(slot_id))
    }
}

pub struct ScanOptions<'a> {
    /// The account name the snapshot store is keyed by.
    pub user: &'a str,
    /// The product name in a program's version info, if it has one.
    pub product_name: &'a dyn Fn(&Path) -> Option<String>,
    /// Whether a per-user switch turns items of this source off.
    pub switch_works: fn(Source) -> bool,
}

/// One value or file found in one source.
struct Found {
    source: Source,
    name: String,
    display_name: String,
    detail: String,
    program: String,
    missing: bool,
}

pub fn scan(target: &StartupTarget, store: &UserBackups, opts: &ScanOptions) -> StartupScan {
    let mut found = Vec::new();
    run_values(&target.user, RUN_KEY, Source::UserRun, opts, &mut found);
    run_values(
        &target.machine,
        RUN_KEY,
        Source::MachineRun,
        opts,
        &mut found,
    );
    run_values(
        &target.machine,
        RUN32_KEY,
        Source::MachineRun32,
        opts,
        &mut found,
    );
    folder_files(
        target.user_folder.as_deref(),
        Source::UserFolder,
        opts,
        &mut found,
    );
    folder_files(
        target.common_folder.as_deref(),
        Source::CommonFolder,
        opts,
        &mut found,
    );

    // Rows share an approved value: same subkey, same name ignoring case.
    // `found` is in `Source` order, so each group's origins are too.
    let mut order: Vec<(Approved, String)> = Vec::new();
    let mut groups: HashMap<(Approved, String), Vec<Found>> = HashMap::new();
    for f in found {
        let key = (f.source.approved(), f.name.to_lowercase());
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(f);
    }

    let mut items: Vec<StartupItem> = order
        .into_iter()
        .map(|key| {
            let group = groups.remove(&key).unwrap_or_default();
            build_item(key.0, group, target, store, opts)
        })
        .collect();
    items.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    StartupScan { items }
}

fn build_item(
    approved: Approved,
    group: Vec<Found>,
    target: &StartupTarget,
    store: &UserBackups,
    opts: &ScanOptions,
) -> StartupItem {
    let first = &group[0];
    let name = first.name.clone();
    let slot_id = approved.slot(&name).id();
    let has_snapshot = store.find_id(opts.user, &slot_id).is_some();
    let sources: Vec<Source> = group.iter().map(|f| f.source).collect();

    let read = |t: &RegTarget| snapshot_value(&t.root, &t.path(&approved.key()), &name);
    let user_read = read(&target.user);
    let machine_read = read(&target.machine);
    let (user_sw, machine_sw, read_error) = match (&user_read, &machine_read) {
        (Ok(u), Ok(m)) => (read_switch(u), read_switch(m), None),
        (Err(e), _) | (_, Err(e)) => (Switch::Unknown, Switch::Unknown, Some(e.clone())),
    };

    let off = user_sw.is_off() || machine_sw.is_off();
    let state = match (off, has_snapshot) {
        (true, true) => StartupState::Off,
        (true, false) => StartupState::OffElsewhere,
        (false, _) => StartupState::On,
    };
    let locked = if let Some(e) = read_error {
        Some(StartupLock::Unreadable(e))
    } else if user_sw == Switch::Unknown || machine_sw == Switch::Unknown {
        Some(StartupLock::UnknownShape)
    } else if machine_sw.is_off() {
        Some(StartupLock::OffForEveryone)
    } else if sources.iter().any(|s| !(opts.switch_works)(*s)) {
        Some(StartupLock::AllUsersOnly)
    } else {
        None
    };

    StartupItem {
        id: approved.row_id(&name),
        approved,
        display_name: first.display_name.clone(),
        program: first.program.clone(),
        program_missing: group.iter().any(|f| f.missing),
        origins: group
            .iter()
            .map(|f| Origin {
                source: f.source,
                detail: f.detail.clone(),
            })
            .collect(),
        name,
        state,
        locked,
        has_snapshot,
    }
}

fn run_values(
    target: &RegTarget,
    rel: &str,
    source: Source,
    opts: &ScanOptions,
    out: &mut Vec<Found>,
) {
    let Ok(key) = open_read(target, rel) else {
        return;
    };
    for (name, value) in key.enum_values().flatten() {
        if name.is_empty() || !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) {
            continue;
        }
        let detail = decode_utf16le(&value.bytes);
        let path = command_program(&detail);
        let program = path
            .as_deref()
            .map(|p| (opts.product_name)(Path::new(p)).unwrap_or_else(|| stem(Path::new(p))))
            .unwrap_or_else(|| name.clone());
        let missing = path
            .as_deref()
            .is_some_and(|p| p.contains(['\\', '/']) && !Path::new(p).exists());
        out.push(Found {
            source,
            display_name: name.clone(),
            name,
            detail,
            program,
            missing,
        });
    }
}

fn folder_files(folder: Option<&Path>, source: Source, opts: &ScanOptions, out: &mut Vec<Found>) {
    let Some(entries) = folder.and_then(|f| std::fs::read_dir(f).ok()) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let hidden = meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0;
        if !meta.is_file() || hidden || name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let path = entry.path();
        let file_stem = stem(&path);
        let is_link = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
        let program = if is_link {
            file_stem.clone()
        } else {
            (opts.product_name)(&path).unwrap_or_else(|| file_stem.clone())
        };
        out.push(Found {
            source,
            display_name: file_stem,
            name,
            detail: path.display().to_string(),
            program,
            missing: false,
        });
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn open_read(target: &RegTarget, rel: &str) -> std::io::Result<RegKey> {
    target
        .root
        .open_subkey_with_flags(target.path(rel), KEY_READ)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups::{Backup, Snapshot};
    use crate::regsnap::test_support::Sandbox;
    use crate::regsnap::ValueSnapshot;
    use std::path::PathBuf;
    use winreg::RegValue;

    struct Env {
        sb: Sandbox,
        tmp: PathBuf,
        target: StartupTarget,
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.tmp);
        }
    }

    fn reg_target(sb: &Sandbox, side: &str) -> RegTarget {
        RegTarget {
            root: RegKey::predef(HKEY_CURRENT_USER),
            root_name: "HKEY_CURRENT_USER".into(),
            base: format!("{}\\{side}", sb.path),
        }
    }

    fn env() -> Env {
        let sb = Sandbox::new();
        let leaf = sb.path.rsplit('\\').next().unwrap().to_string();
        let tmp = std::env::temp_dir().join(format!("wintune-startup-{leaf}"));
        let (user, common) = (tmp.join("user"), tmp.join("common"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&common).unwrap();
        let target = StartupTarget {
            user: reg_target(&sb, "HKCU"),
            machine: reg_target(&sb, "HKLM"),
            user_folder: Some(user),
            common_folder: Some(common),
        };
        Env { sb, tmp, target }
    }

    fn put(t: &RegTarget, rel: &str, name: &str, vtype: RegType, bytes: Vec<u8>) {
        let (key, _) = t.root.create_subkey(t.path(rel)).unwrap();
        key.set_raw_value(name, &RegValue { bytes, vtype }).unwrap();
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain(Some(0))
            .flat_map(|w| w.to_le_bytes())
            .collect()
    }

    fn run(t: &RegTarget, rel: &str, name: &str, command: &str) {
        put(t, rel, name, REG_SZ, utf16(command));
    }

    fn flag(flag: u8) -> Vec<u8> {
        let mut v = vec![flag, 0, 0, 0];
        v.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        v
    }

    fn switch(t: &RegTarget, approved: Approved, name: &str, bytes: Vec<u8>) {
        put(t, &approved.key(), name, REG_BINARY, bytes);
    }

    fn works(s: Source) -> bool {
        s != Source::MachineRun32
    }

    fn scan_with(
        e: &Env,
        store: &UserBackups,
        name: &dyn Fn(&Path) -> Option<String>,
    ) -> StartupScan {
        scan(
            &e.target,
            store,
            &ScanOptions {
                user: "TEST\\user",
                product_name: name,
                switch_works: works,
            },
        )
    }

    fn scan_plain(e: &Env) -> StartupScan {
        scan_with(e, &UserBackups::default(), &|_| None)
    }

    fn make_file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    fn only(scan: &StartupScan) -> &StartupItem {
        assert_eq!(scan.items.len(), 1, "{:?}", scan.items);
        &scan.items[0]
    }

    #[test]
    fn run_values_become_rows_with_their_command_and_program() {
        let e = env();
        let exe = e.tmp.join("Tool").join("tool.exe");
        make_file(&exe);
        let command = format!("\"{}\" --min", exe.display());
        run(&e.target.user, RUN_KEY, "Tool", &command);
        let want = exe.clone();
        let result = scan_with(&e, &UserBackups::default(), &move |p| {
            (p == want).then(|| "Tool Pro".to_string())
        });
        let item = only(&result);
        assert_eq!(item.id, "run:tool");
        assert_eq!(item.display_name, "Tool");
        assert_eq!(item.program, "Tool Pro");
        assert_eq!(
            item.origins,
            vec![Origin {
                source: Source::UserRun,
                detail: command
            }]
        );
        assert_eq!(item.state, StartupState::On);
        assert_eq!(item.locked, None);
        assert!(!item.program_missing);
        assert!(!item.all_users());
    }

    #[test]
    fn commands_windows_accepts_are_parsed_without_false_alarms() {
        let e = env();
        let parsec = e.tmp.join("Par sec").join("parsecd.exe");
        make_file(&parsec);
        let u = &e.target.user;
        run(
            u,
            RUN_KEY,
            "Parsec",
            &format!("{} app_silent=1", parsec.display()),
        );
        put(
            u,
            RUN_KEY,
            "Sec",
            REG_EXPAND_SZ,
            utf16(r"%SystemRoot%\System32\cmd.exe"),
        );
        run(u, RUN_KEY, "Teams", "ms-teams.exe msteams:system-initiated");
        run(
            u,
            RUN_KEY,
            "Gone",
            &format!("\"{}\" -x", e.tmp.join("gone").join("app.exe").display()),
        );
        let result = scan_plain(&e);
        let get = |id: &str| result.find(id).unwrap();
        assert_eq!(get("run:parsec").program, "parsecd");
        assert!(!get("run:parsec").program_missing);
        let sec = get("run:sec");
        assert!(sec.origins[0].detail.contains("%SystemRoot%"));
        assert_eq!(sec.program, "cmd");
        assert!(!sec.program_missing);
        assert_eq!(get("run:teams").program, "ms-teams");
        assert!(!get("run:teams").program_missing);
        assert!(get("run:gone").program_missing);
    }

    #[test]
    fn a_value_in_both_run_keys_is_one_row_for_everyone() {
        let e = env();
        run(&e.target.user, RUN_KEY, "X", "x.exe");
        run(&e.target.machine, RUN_KEY, "X", "x.exe");
        let result = scan_plain(&e);
        let item = only(&result);
        assert_eq!(item.sources(), vec![Source::UserRun, Source::MachineRun]);
        assert!(item.all_users());
        assert_eq!(item.locked, None);
    }

    #[test]
    fn names_match_case_insensitively() {
        let e = env();
        run(&e.target.user, RUN_KEY, "OneDrive", "onedrive.exe");
        switch(&e.target.user, Approved::Run, "onedrive", flag(3));
        make_file(&e.target.user_folder.as_ref().unwrap().join("Discord.lnk"));
        make_file(&e.target.common_folder.as_ref().unwrap().join("discord.LNK"));
        let result = scan_plain(&e);
        assert_eq!(result.items.len(), 2);
        let od = result.find("run:onedrive").unwrap();
        assert_eq!(od.state, StartupState::OffElsewhere);
        assert_eq!(od.name, "OneDrive");
        let d = result.find("folder:discord.lnk").unwrap();
        assert_eq!(d.sources(), vec![Source::UserFolder, Source::CommonFolder]);
    }

    fn backup(slot_id: &str) -> UserBackups {
        let mut store = UserBackups::default();
        store.users.entry("TEST\\user".into()).or_default().insert(
            slot_id.to_string(),
            Backup {
                timestamp: "t".into(),
                snapshot: Snapshot::Value(ValueSnapshot::default()),
            },
        );
        store
    }

    #[test]
    fn states_follow_the_switch_and_the_store() {
        let e = env();
        let u = &e.target.user;
        for name in ["Abs", "Two", "Six", "Elsewhere", "Mine", "OnWithSnap"] {
            run(u, RUN_KEY, name, "a.exe");
        }
        switch(u, Approved::Run, "Two", flag(2));
        switch(u, Approved::Run, "Six", flag(6));
        switch(u, Approved::Run, "Elsewhere", flag(3));
        switch(u, Approved::Run, "Mine", flag(3));
        switch(u, Approved::Run, "OnWithSnap", flag(2));
        let mut store = backup(&Approved::Run.slot("Mine").id());
        store.users.get_mut("TEST\\user").unwrap().extend(
            backup(&Approved::Run.slot("OnWithSnap").id())
                .users
                .remove("TEST\\user")
                .unwrap(),
        );
        let result = scan_with(&e, &store, &|_| None);
        let state = |id: &str| result.find(id).unwrap().state;
        assert_eq!(state("run:abs"), StartupState::On);
        assert_eq!(state("run:two"), StartupState::On);
        assert_eq!(state("run:six"), StartupState::On);
        assert_eq!(state("run:elsewhere"), StartupState::OffElsewhere);
        assert!(!result.find("run:elsewhere").unwrap().has_snapshot);
        assert_eq!(state("run:mine"), StartupState::Off);
        assert!(result.find("run:mine").unwrap().has_snapshot);
        assert_eq!(state("run:onwithsnap"), StartupState::On);
        assert!(result.find("run:onwithsnap").unwrap().has_snapshot);
    }

    #[test]
    fn locks_say_why() {
        let e = env();
        let (u, m) = (&e.target.user, &e.target.machine);
        run(u, RUN_KEY, "Dword", "a.exe");
        put(
            u,
            &Approved::Run.key(),
            "Dword",
            REG_DWORD,
            vec![3, 0, 0, 0],
        );
        run(u, RUN_KEY, "One", "a.exe");
        switch(u, Approved::Run, "One", flag(1));
        run(m, RUN_KEY, "M", "a.exe");
        switch(m, Approved::Run, "M", flag(3));
        run(m, RUN32_KEY, "W", "a.exe");
        run(m, RUN_KEY, "Free", "a.exe");
        let result = scan_plain(&e);
        let lock = |id: &str| result.find(id).unwrap().locked.clone();
        assert_eq!(lock("run:dword"), Some(StartupLock::UnknownShape));
        assert_eq!(lock("run:one"), Some(StartupLock::UnknownShape));
        assert_eq!(lock("run:m"), Some(StartupLock::OffForEveryone));
        assert_ne!(result.find("run:m").unwrap().state, StartupState::On);
        assert_eq!(lock("run32:w"), Some(StartupLock::AllUsersOnly));
        assert_eq!(lock("run:free"), None);
    }

    #[test]
    fn run32_items_use_the_run32_switch() {
        let e = env();
        run(&e.target.machine, RUN32_KEY, "Old", "old.exe");
        switch(&e.target.user, Approved::Run, "Old", flag(3));
        let result = scan_plain(&e);
        let item = only(&result);
        assert_eq!(item.id, "run32:old");
        assert_eq!(item.approved, Approved::Run32);
        assert_eq!(item.state, StartupState::On);

        switch(&e.target.user, Approved::Run32, "Old", flag(3));
        let result = scan_plain(&e);
        assert_eq!(only(&result).state, StartupState::OffElsewhere);
    }

    #[cfg(windows)]
    fn hide(path: &Path) {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};
        let w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `w` is a NUL-terminated wide path that outlives the call.
        assert_ne!(
            unsafe { SetFileAttributesW(w.as_ptr(), FILE_ATTRIBUTE_HIDDEN) },
            0
        );
    }

    #[cfg(windows)]
    fn mark_system(path: &Path) {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_SYSTEM};
        let w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `w` is a NUL-terminated wide path that outlives the call.
        assert_ne!(
            unsafe { SetFileAttributesW(w.as_ptr(), FILE_ATTRIBUTE_SYSTEM) },
            0
        );
    }

    #[test]
    fn startup_folders_skip_hidden_system_and_desktop_ini() {
        let mut e = env();
        let user = e.target.user_folder.clone().unwrap();
        make_file(&user.join("desktop.ini"));
        make_file(&user.join("hidden.lnk"));
        hide(&user.join("hidden.lnk"));
        make_file(&user.join("system.lnk"));
        mark_system(&user.join("system.lnk"));
        std::fs::create_dir_all(user.join("sub")).unwrap();
        make_file(&user.join("App.lnk"));
        make_file(&user.join("script.bat"));
        let result = scan_plain(&e);
        let names: Vec<&str> = result
            .items
            .iter()
            .map(|i| i.display_name.as_str())
            .collect();
        assert_eq!(names, ["App", "script"]);
        let app = result.find("folder:app.lnk").unwrap();
        assert_eq!(
            app.origins[0].detail,
            user.join("App.lnk").display().to_string()
        );
        assert_eq!(app.program, "App");
        assert!(!app.program_missing);

        e.target.user_folder = None;
        e.target.common_folder = Some(e.tmp.join("does-not-exist"));
        assert!(scan_plain(&e).items.is_empty());
    }

    #[test]
    fn orphan_switches_and_odd_run_values_are_ignored() {
        let e = env();
        switch(&e.target.user, Approved::Run, "Ghost", flag(2));
        put(&e.target.user, RUN_KEY, "Num", REG_DWORD, vec![1, 0, 0, 0]);
        run(&e.target.user, RUN_KEY, "", "");
        assert!(scan_plain(&e).items.is_empty());
        let _ = &e.sb;
    }

    #[test]
    fn rows_are_sorted_by_name() {
        let e = env();
        for n in ["b", "A", "c"] {
            run(&e.target.user, RUN_KEY, n, "a.exe");
        }
        let result = scan_plain(&e);
        let names: Vec<&str> = result
            .items
            .iter()
            .map(|i| i.display_name.as_str())
            .collect();
        assert_eq!(names, ["A", "b", "c"]);
    }

    #[test]
    fn by_slot_ignores_case() {
        let e = env();
        run(&e.target.user, RUN_KEY, "Tool", "a.exe");
        let result = scan_plain(&e);
        let id = Approved::Run.slot("TOOL").id();
        assert_eq!(result.by_slot(&id).unwrap().id, "run:tool");
        assert!(result.by_slot("nope").is_none());
    }
}
