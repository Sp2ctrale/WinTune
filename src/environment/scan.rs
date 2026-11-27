//! The read-only scan: the user's variables, the machine's variables and which
//! `Path` folders exist. Writes nothing and never opens `Volatile Environment`.

use std::collections::HashMap;
use std::path::Path;

use winreg::enums::KEY_READ;

use super::{
    checkable, env_slot, split_entries, unquote, EnvTarget, EnvVar, Resolver, REG_EXPAND_SZ_KIND,
    REG_SZ_KIND, SYSTEM_ENV_KEY, USER_ENV_KEY,
};
use crate::backups::{RegTarget, UserBackups};
use crate::context_menu::fileinfo::decode_utf16le;

/// The variables stored under `rel` of `t`, sorted by lowercase name. A
/// missing key has none. Text kinds are decoded raw (references unexpanded);
/// any other kind has no value.
pub fn read_vars(t: &RegTarget, rel: &str) -> Result<Vec<EnvVar>, String> {
    let key = match t.root.open_subkey_with_flags(t.path(rel), KEY_READ) {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut vars = Vec::new();
    for item in key.enum_values() {
        let (name, value) = item.map_err(|e| e.to_string())?;
        if name.is_empty() {
            continue;
        }
        let kind = value.vtype as u32;
        let text = kind == REG_SZ_KIND || kind == REG_EXPAND_SZ_KIND;
        vars.push(EnvVar {
            name,
            kind,
            value: text.then(|| decode_utf16le(&value.bytes)),
            has_snapshot: false,
        });
    }
    vars.sort_by_key(|v| v.name.to_lowercase());
    Ok(vars)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnvScan {
    pub user: Vec<EnvVar>,
    pub system: Vec<EnvVar>,
    pub user_error: Option<String>,
    pub system_error: Option<String>,
    /// Whether the folder exists, keyed by the expanded entry text. Only
    /// complete, local-drive entries are in the map.
    pub folders: HashMap<String, bool>,
}

fn path_entries(vars: &[EnvVar]) -> Vec<String> {
    vars.iter()
        .find(|v| v.is_path())
        .and_then(|v| v.value.as_deref())
        .map(split_entries)
        .unwrap_or_default()
}

/// The variables that feed `%name%` expansion, as `Resolver::new` takes
/// them: text variables only; expandable means `REG_EXPAND_SZ`.
pub fn resolver_vars(vars: &[EnvVar]) -> Vec<(String, String, bool)> {
    vars.iter()
        .filter_map(|v| {
            let value = v.value.clone()?;
            Some((v.name.clone(), value, v.kind == REG_EXPAND_SZ_KIND))
        })
        .collect()
}

impl EnvScan {
    pub fn user_var(&self, name: &str) -> Option<&EnvVar> {
        self.user.iter().find(|v| v.name.eq_ignore_ascii_case(name))
    }

    pub fn user_path(&self) -> Option<&EnvVar> {
        self.user.iter().find(|v| v.is_path())
    }

    pub fn user_path_entries(&self) -> Vec<String> {
        path_entries(&self.user)
    }

    pub fn system_path_entries(&self) -> Vec<String> {
        path_entries(&self.system)
    }

    pub fn resolver(&self, process: fn(&str) -> Option<String>) -> Resolver {
        Resolver::new(
            resolver_vars(&self.user),
            resolver_vars(&self.system),
            process,
        )
    }
}

pub fn scan_environment(
    target: &EnvTarget,
    snapshots: &UserBackups,
    user: &str,
    process: fn(&str) -> Option<String>,
) -> EnvScan {
    let mut scan = EnvScan::default();
    match read_vars(&target.user, USER_ENV_KEY) {
        Ok(vars) => scan.user = vars,
        Err(e) => scan.user_error = Some(e),
    }
    match read_vars(&target.machine, SYSTEM_ENV_KEY) {
        Ok(vars) => scan.system = vars,
        Err(e) => scan.system_error = Some(e),
    }
    for v in &mut scan.user {
        v.has_snapshot = snapshots.find_id(user, &env_slot(&v.name).id()).is_some();
    }

    let resolver = scan.resolver(process);
    let mut folders = HashMap::new();
    for entry in scan
        .user_path_entries()
        .into_iter()
        .chain(scan.system_path_entries())
    {
        let expanded = resolver.expand(&entry);
        if expanded.complete && checkable(&expanded.text) {
            let exists = Path::new(unquote(&expanded.text)).is_dir();
            folders.insert(expanded.text, exists);
        }
    }
    scan.folders = folders;
    scan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backups::{Backup, Snapshot};
    use crate::environment::{analyze_entries, encode_text, VOLATILE_ENV_KEY};
    use crate::regsnap::test_support::Sandbox;
    use crate::regsnap::ValueSnapshot;
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue};

    struct Env {
        _sb: Sandbox,
        tmp: PathBuf,
        target: EnvTarget,
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
        let tmp = std::env::temp_dir().join(format!("wintune-env-{leaf}"));
        std::fs::create_dir_all(&tmp).unwrap();
        let target = EnvTarget {
            user: reg_target(&sb, "HKCU"),
            machine: reg_target(&sb, "HKLM"),
        };
        Env {
            _sb: sb,
            tmp,
            target,
        }
    }

    fn put(t: &RegTarget, rel: &str, name: &str, vtype: RegType, bytes: Vec<u8>) {
        let (key, _) = t.root.create_subkey(t.path(rel)).unwrap();
        key.set_raw_value(name, &RegValue { bytes, vtype }).unwrap();
    }

    fn put_text(t: &RegTarget, rel: &str, name: &str, vtype: RegType, text: &str) {
        put(t, rel, name, vtype, encode_text(text));
    }

    fn no_process(_: &str) -> Option<String> {
        None
    }

    fn scan_of(e: &Env, store: &UserBackups) -> EnvScan {
        scan_environment(&e.target, store, "TEST\\user", no_process)
    }

    fn names(vars: &[EnvVar]) -> Vec<&str> {
        vars.iter().map(|v| v.name.as_str()).collect()
    }

    #[test]
    fn user_variables_are_read_raw_with_their_type() {
        let e = env();
        let u = &e.target.user;
        put_text(u, USER_ENV_KEY, "X", REG_EXPAND_SZ, r"%USERPROFILE%\x");
        put_text(u, USER_ENV_KEY, "b", REG_SZ, "y");
        put(u, USER_ENV_KEY, "D", REG_DWORD, vec![1, 0, 0, 0]);
        put_text(u, USER_ENV_KEY, "", REG_SZ, "default");
        put_text(u, USER_ENV_KEY, "PATH", REG_EXPAND_SZ, r"C:\a");
        let scan = scan_of(&e, &UserBackups::default());
        let x = scan.user_var("x").unwrap();
        assert_eq!(x.value.as_deref(), Some(r"%USERPROFILE%\x"));
        assert_eq!(x.kind, REG_EXPAND_SZ_KIND);
        let d = scan.user_var("D").unwrap();
        assert_eq!((d.value.clone(), d.kind), (None, 4));
        assert_eq!(names(&scan.user), ["b", "D", "PATH", "X"]);
        assert_eq!(scan.user_path().unwrap().name, "PATH");
        assert!(scan.user.iter().all(|v| !v.has_snapshot));
        assert_eq!(scan.user_error, None);
    }

    #[test]
    fn system_variables_are_read_from_the_machine_side() {
        let e = env();
        put_text(
            &e.target.machine,
            SYSTEM_ENV_KEY,
            "Path",
            REG_EXPAND_SZ,
            r"C:\s1;C:\s2",
        );
        let scan = scan_of(&e, &UserBackups::default());
        assert_eq!(scan.system_path_entries(), [r"C:\s1", r"C:\s2"]);
        assert!(scan.user.is_empty());
        assert!(scan.user_path_entries().is_empty());
    }

    #[test]
    fn a_missing_key_gives_no_variables_and_no_error() {
        let e = env();
        let scan = scan_of(&e, &UserBackups::default());
        assert!(scan.user.is_empty() && scan.system.is_empty());
        assert_eq!((scan.user_error, scan.system_error), (None, None));
        assert_eq!(
            read_vars(&e.target.user, USER_ENV_KEY),
            Ok(Vec::<EnvVar>::new())
        );
    }

    #[test]
    fn has_snapshot_follows_the_store() {
        let e = env();
        put_text(&e.target.user, USER_ENV_KEY, "FOO", REG_SZ, "1");
        put_text(&e.target.user, USER_ENV_KEY, "BAR", REG_SZ, "2");
        let mut store = UserBackups::default();
        store.users.entry("TEST\\user".into()).or_default().insert(
            env_slot("Foo").id(),
            Backup {
                timestamp: "t".into(),
                snapshot: Snapshot::Value(ValueSnapshot::default()),
            },
        );
        let scan = scan_of(&e, &store);
        assert!(scan.user_var("FOO").unwrap().has_snapshot);
        assert!(!scan.user_var("BAR").unwrap().has_snapshot);
    }

    #[test]
    fn folders_are_checked_for_expanded_path_entries() {
        let e = env();
        let tools = e.tmp.join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let tools = tools.display().to_string();
        let missing = e.tmp.join("missing").display().to_string();
        let u = &e.target.user;
        put_text(u, USER_ENV_KEY, "TOOLS", REG_SZ, &tools);
        put_text(
            u,
            USER_ENV_KEY,
            "Path",
            REG_EXPAND_SZ,
            &format!(r"%TOOLS%;{missing};\\server\share;%NOPE%\x"),
        );
        let scan = scan_of(&e, &UserBackups::default());
        let mut want = HashMap::new();
        want.insert(tools, true);
        want.insert(missing, false);
        assert_eq!(scan.folders, want);
    }

    #[test]
    fn a_quoted_existing_folder_is_not_reported_missing() {
        let e = env();
        let dir = e.tmp.join("quoted dir");
        std::fs::create_dir_all(&dir).unwrap();
        let quoted = format!("\"{}\"", dir.display());
        put_text(&e.target.user, USER_ENV_KEY, "Path", REG_EXPAND_SZ, &quoted);
        let scan = scan_of(&e, &UserBackups::default());
        assert_eq!(scan.folders.get(&quoted), Some(&true));
        let entries = scan.user_path_entries();
        let infos = analyze_entries(&entries, &[], &scan.resolver(no_process), &|p| {
            scan.folders.get(p).copied().unwrap_or(false)
        });
        assert!(!infos[0].missing);
    }

    #[test]
    fn volatile_environment_is_never_read() {
        let e = env();
        put_text(
            &e.target.user,
            VOLATILE_ENV_KEY,
            "USERPROFILE",
            REG_SZ,
            r"C:\Users\x",
        );
        put_text(&e.target.user, USER_ENV_KEY, "Real", REG_SZ, "1");
        let scan = scan_of(&e, &UserBackups::default());
        assert_eq!(names(&scan.user), ["Real"]);
    }
}
