//! Generic, lossless registry snapshot / restore primitives.
//!
//! Every WinTune module that modifies the registry must snapshot the keys it
//! touches with [`snapshot_key`] before writing, so the change can be undone
//! later with [`restore_key`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RegistryKeySnapshot {
    pub exists: bool,
    pub default_value: Option<String>,
    pub values: BTreeMap<String, (u32, Vec<u8>)>,
    pub subkeys: BTreeMap<String, RegistryKeySnapshot>,
}

fn is_not_found(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
}

/// Maps a raw registry type id back to its `RegType`. Unknown ids are an
/// error: silently rewriting them as REG_BINARY would corrupt the data.
fn reg_type_from_u32(id: u32) -> Option<RegType> {
    Some(match id {
        0 => REG_NONE,
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        3 => REG_BINARY,
        4 => REG_DWORD,
        5 => REG_DWORD_BIG_ENDIAN,
        6 => REG_LINK,
        7 => REG_MULTI_SZ,
        8 => REG_RESOURCE_LIST,
        9 => REG_FULL_RESOURCE_DESCRIPTOR,
        10 => REG_RESOURCE_REQUIREMENTS_LIST,
        11 => REG_QWORD,
        _ => return None,
    })
}

/// Decodes a REG_SZ / REG_EXPAND_SZ payload (UTF-16LE, trailing NULs dropped).
fn decode_utf16_text(bytes: &[u8]) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let words: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let trimmed: &[u16] = match words.iter().rposition(|&w| w != 0) {
        Some(last) => &words[..=last],
        None => &[],
    };
    String::from_utf16(trimmed).ok()
}

/// Recursively captures a key: every value (default value included, with its
/// exact type) and every subkey. A missing key yields `exists: false`; any
/// other failure (e.g. access denied) is an error, because recording it as
/// "missing" would make a later restore delete the real key.
pub fn snapshot_key(root: &RegKey, path: &str) -> Result<RegistryKeySnapshot, String> {
    let key = match root.open_subkey_with_flags(path, KEY_READ) {
        Ok(k) => k,
        Err(e) if is_not_found(&e) => return Ok(RegistryKeySnapshot::default()),
        Err(e) => return Err(format!("Cannot read {}: {}", path, e)),
    };

    let mut values = BTreeMap::new();
    for item in key.enum_values() {
        let (name, value) =
            item.map_err(|e| format!("Cannot enumerate values of {}: {}", path, e))?;
        values.insert(name, (value.vtype as u32, value.bytes));
    }

    let default_value = values.get("").and_then(|(kind, bytes)| match *kind {
        1 | 2 => decode_utf16_text(bytes),
        _ => None,
    });

    let mut subkeys = BTreeMap::new();
    for item in key.enum_keys() {
        let name = item.map_err(|e| format!("Cannot enumerate subkeys of {}: {}", path, e))?;
        let child = format!("{}\\{}", path, name);
        subkeys.insert(name, snapshot_key(root, &child)?);
    }

    Ok(RegistryKeySnapshot {
        exists: true,
        default_value,
        values,
        subkeys,
    })
}

/// Deletes a key and its whole subtree. A missing key is not an error.
pub fn delete_tree(root: &RegKey, path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Refusing to delete the root of a registry hive".to_string());
    }
    match root.delete_subkey_all(path) {
        Ok(()) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(format!("Cannot delete {}: {}", path, e)),
    }
}

/// Puts `path` back exactly as captured: the current subtree is removed, then
/// the snapshot is rewritten (or nothing, if the key did not exist). Every
/// failure is reported; nothing is silently skipped.
pub fn restore_key(
    root: &RegKey,
    path: &str,
    snapshot: &RegistryKeySnapshot,
) -> Result<(), String> {
    let deleted = delete_tree(root, path);
    restore_after_delete(root, path, snapshot, deleted)
}

/// Second half of [`restore_key`], given the outcome of the delete.
fn restore_after_delete(
    root: &RegKey,
    path: &str,
    snapshot: &RegistryKeySnapshot,
    deleted: Result<(), String>,
) -> Result<(), String> {
    // RegDeleteTree is not atomic: after a partial delete, what it removed
    // must still be written back, so keep going and report both problems.
    let mut errors = Vec::new();
    if let Err(e) = deleted {
        errors.push(e);
    }
    if snapshot.exists {
        write_snapshot(root, path, snapshot, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn write_snapshot(
    root: &RegKey,
    path: &str,
    snapshot: &RegistryKeySnapshot,
    errors: &mut Vec<String>,
) {
    let key = match root.create_subkey(path) {
        Ok((k, _)) => k,
        Err(e) => {
            errors.push(format!("Cannot create {}: {}", path, e));
            return;
        }
    };

    // Snapshots written by WinTune <= 0.2.0 kept the default value only as text.
    if !snapshot.values.contains_key("") {
        if let Some(text) = &snapshot.default_value {
            if let Err(e) = key.set_value("", text) {
                errors.push(format!("{}: cannot write default value: {}", path, e));
            }
        }
    }

    for (name, (kind, bytes)) in &snapshot.values {
        let Some(vtype) = reg_type_from_u32(*kind) else {
            errors.push(format!(
                "{}: unsupported type {} for value '{}'",
                path, kind, name
            ));
            continue;
        };
        let value = RegValue {
            bytes: bytes.clone(),
            vtype,
        };
        if let Err(e) = key.set_raw_value(name, &value) {
            errors.push(format!("{}: cannot write value '{}': {}", path, name, e));
        }
    }

    for (name, child) in snapshot.subkeys.iter().filter(|(_, c)| c.exists) {
        write_snapshot(root, &format!("{}\\{}", path, name), child, errors);
    }
}

/// One key to include in a `.reg` safety-net export.
pub struct RegExportEntry<'a> {
    /// Full hive name as regedit expects it, e.g. `HKEY_LOCAL_MACHINE`.
    pub root_name: &'a str,
    pub path: String,
    pub snapshot: &'a RegistryKeySnapshot,
}

/// Renders snapshots as a regedit-compatible `.reg` file. Each key is first
/// deleted then recreated, so importing the file restores the exact state.
pub fn export_reg(entries: &[RegExportEntry]) -> String {
    export_reg_full(entries, &[])
}

/// One registry value (not its whole key) captured before a change.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ValueSnapshot {
    pub exists: bool,
    /// Raw registry type id (REG_DWORD = 4…), meaningful when `exists`.
    pub kind: u32,
    pub data: Vec<u8>,
}

/// Captures a single value. A missing key or value yields `exists: false`;
/// any other failure is an error.
pub fn snapshot_value(root: &RegKey, path: &str, name: &str) -> Result<ValueSnapshot, String> {
    let key = match root.open_subkey_with_flags(path, KEY_READ) {
        Ok(k) => k,
        Err(e) if is_not_found(&e) => return Ok(ValueSnapshot::default()),
        Err(e) => return Err(format!("Cannot read {}: {}", path, e)),
    };
    match key.get_raw_value(name) {
        Ok(value) => Ok(ValueSnapshot {
            exists: true,
            kind: value.vtype as u32,
            data: value.bytes,
        }),
        Err(e) if is_not_found(&e) => Ok(ValueSnapshot::default()),
        Err(e) => Err(format!("Cannot read {}\\{}: {}", path, name, e)),
    }
}

/// Puts a value back as captured: rewritten with its exact type, or deleted
/// when it did not exist (the key itself is left in place).
pub fn restore_value(
    root: &RegKey,
    path: &str,
    name: &str,
    snapshot: &ValueSnapshot,
) -> Result<(), String> {
    if snapshot.exists {
        let vtype = reg_type_from_u32(snapshot.kind).ok_or_else(|| {
            format!(
                "{}: unsupported type {} for '{}'",
                path, snapshot.kind, name
            )
        })?;
        let (key, _) = root
            .create_subkey(path)
            .map_err(|e| format!("Cannot create {}: {}", path, e))?;
        return key
            .set_raw_value(
                name,
                &RegValue {
                    bytes: snapshot.data.clone(),
                    vtype,
                },
            )
            .map_err(|e| format!("Cannot write {}\\{}: {}", path, name, e));
    }

    let key = match root.open_subkey_with_flags(path, KEY_SET_VALUE) {
        Ok(k) => k,
        Err(e) if is_not_found(&e) => return Ok(()),
        Err(e) => return Err(format!("Cannot open {}: {}", path, e)),
    };
    match key.delete_value(name) {
        Ok(()) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(e) => Err(format!("Cannot delete {}\\{}: {}", path, name, e)),
    }
}

/// One value to include in a `.reg` safety-net export.
pub struct RegValueExport<'a> {
    pub root_name: &'a str,
    pub path: String,
    pub name: String,
    pub snapshot: &'a ValueSnapshot,
}

/// Like [`export_reg`], plus single values: a value that did not exist is
/// written as `"name"=-` so importing the file deletes it again.
pub fn export_reg_full(keys: &[RegExportEntry], values: &[RegValueExport]) -> String {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    for entry in keys {
        let full = format!("{}\\{}", entry.root_name, entry.path);
        out.push_str(&format!("[-{}]\r\n\r\n", full));
        if entry.snapshot.exists {
            write_reg_key(&mut out, &full, entry.snapshot);
        }
    }

    // Group values by key, keeping the order in which keys first appear.
    let mut groups: Vec<(String, Vec<&RegValueExport>)> = Vec::new();
    for value in values {
        let full = format!("{}\\{}", value.root_name, value.path);
        match groups.iter_mut().find(|(k, _)| *k == full) {
            Some((_, list)) => list.push(value),
            None => groups.push((full, vec![value])),
        }
    }
    for (full, list) in groups {
        out.push_str(&format!("[{}]\r\n", full));
        for value in list {
            let lhs = if value.name.is_empty() {
                "@".to_string()
            } else {
                format!("\"{}\"", escape_reg_string(&value.name))
            };
            if value.snapshot.exists {
                out.push_str(&format_reg_value(
                    &lhs,
                    value.snapshot.kind,
                    &value.snapshot.data,
                ));
            } else {
                out.push_str(&format!("{}=-", lhs));
            }
            out.push_str("\r\n");
        }
        out.push_str("\r\n");
    }
    out
}

fn escape_reg_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_reg_key(out: &mut String, full: &str, snapshot: &RegistryKeySnapshot) {
    out.push_str(&format!("[{}]\r\n", full));
    if !snapshot.values.contains_key("") {
        if let Some(text) = &snapshot.default_value {
            out.push_str(&format!("@=\"{}\"\r\n", escape_reg_string(text)));
        }
    }
    for (name, (kind, bytes)) in &snapshot.values {
        let lhs = if name.is_empty() {
            "@".to_string()
        } else {
            format!("\"{}\"", escape_reg_string(name))
        };
        out.push_str(&format_reg_value(&lhs, *kind, bytes));
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    for (name, child) in snapshot.subkeys.iter().filter(|(_, c)| c.exists) {
        write_reg_key(out, &format!("{}\\{}", full, name), child);
    }
}

fn format_reg_value(lhs: &str, kind: u32, bytes: &[u8]) -> String {
    if kind == 1 {
        if let Some(text) = decode_utf16_text(bytes) {
            if !text.contains(['\r', '\n', '\0']) {
                return format!("{}=\"{}\"", lhs, escape_reg_string(&text));
            }
        }
    }
    if kind == 4 && bytes.len() == 4 {
        let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        return format!("{}=dword:{:08x}", lhs, v);
    }
    let prefix = if kind == 3 {
        "hex:".to_string()
    } else {
        format!("hex({:x}):", kind)
    };

    // regedit wraps hex data at 80 columns with a trailing backslash.
    let mut out = String::new();
    let mut line = format!("{}={}", lhs, prefix);
    for (i, b) in bytes.iter().enumerate() {
        if line.len() + 3 > 76 {
            out.push_str(&line);
            out.push_str("\\\r\n");
            line = "  ".to_string();
        }
        line.push_str(&format!("{:02x}", b));
        if i + 1 < bytes.len() {
            line.push(',');
        }
    }
    out.push_str(&line);
    out
}

/// `<dir>\<stem>.reg`, or `<stem>-2.reg`, `-3`… so an export never
/// overwrites an earlier one taken in the same second.
pub fn unique_reg_file(dir: &std::path::Path, stem: &str) -> std::path::PathBuf {
    let mut candidate = dir.join(format!("{}.reg", stem));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{}-{}.reg", stem, n));
        n += 1;
    }
    candidate
}

/// Writes a `.reg` file in the encoding regedit itself uses (UTF-16LE + BOM).
pub fn write_reg_file(path: &std::path::Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(content.encode_utf16().flat_map(|w| w.to_le_bytes()));
    std::fs::write(path, bytes).map_err(|e| format!("Cannot write {}: {}", path.display(), e))
}

// Test helpers shared by every module that touches the registry.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use winreg::enums::*;
    use winreg::RegKey;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    const SANDBOX_PARENT: &str = "Software\\WinTuneTest";

    /// A throw-away key under `HKCU\Software\WinTuneTest`, deleted on drop.
    pub struct Sandbox {
        pub root: RegKey,
        pub path: String,
    }

    impl Sandbox {
        pub fn new() -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = format!(
                "{}\\{}-{}-{}",
                SANDBOX_PARENT,
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst),
                nanos
            );
            let root = RegKey::predef(HKEY_CURRENT_USER);
            root.create_subkey(&path).expect("create sandbox");
            Self { root, path }
        }

        pub fn key(&self, rel: &str) -> String {
            format!("{}\\{}", self.path, rel)
        }

        pub fn exists(&self, rel: &str) -> bool {
            self.root.open_subkey(self.key(rel)).is_ok()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = self.root.delete_subkey_all(&self.path);
            let _ = self.root.delete_subkey(SANDBOX_PARENT); // only succeeds when empty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::Sandbox;
    use super::*;

    fn utf16z(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(Some(0))
            .flat_map(|w| w.to_le_bytes())
            .collect()
    }

    fn set_raw(key: &RegKey, name: &str, vtype: RegType, bytes: Vec<u8>) {
        key.set_raw_value(name, &RegValue { bytes, vtype }).unwrap();
    }

    /// Reads (name -> (type, bytes)) for every value of a key, default included.
    fn raw_values(root: &RegKey, path: &str) -> BTreeMap<String, (u32, Vec<u8>)> {
        let key = root.open_subkey(path).unwrap();
        key.enum_values()
            .map(|r| r.unwrap())
            .map(|(n, v)| (n, (v.vtype as u32, v.bytes)))
            .collect()
    }

    fn populate_every_value_type(sb: &Sandbox, rel: &str) {
        let (k, _) = sb.root.create_subkey(sb.key(rel)).unwrap();
        set_raw(&k, "", REG_EXPAND_SZ, utf16z("%SystemRoot%\\icon.ico,0"));
        set_raw(&k, "None", REG_NONE, Vec::new());
        set_raw(&k, "Sz", REG_SZ, utf16z("hello"));
        set_raw(&k, "Dword", REG_DWORD, 42u32.to_le_bytes().to_vec());
        set_raw(&k, "Qword", REG_QWORD, 7u64.to_le_bytes().to_vec());
        set_raw(&k, "Multi", REG_MULTI_SZ, utf16z("a\0b\0"));
        set_raw(&k, "Bin", REG_BINARY, vec![1, 2, 3]);
        let (child, _) = k.create_subkey("Child\\Grandchild").unwrap();
        set_raw(&child, "", REG_SZ, utf16z("deep"));
    }

    #[test]
    fn roundtrip_preserves_every_value_type_and_default_value_type() {
        let sb = Sandbox::new();
        populate_every_value_type(&sb, "Target");
        let before = raw_values(&sb.root, &sb.key("Target"));
        let before_child = raw_values(&sb.root, &sb.key("Target\\Child\\Grandchild"));

        let snap = snapshot_key(&sb.root, &sb.key("Target")).unwrap();
        sb.root.delete_subkey_all(sb.key("Target")).unwrap();
        restore_key(&sb.root, &sb.key("Target"), &snap).unwrap();

        assert_eq!(raw_values(&sb.root, &sb.key("Target")), before);
        assert_eq!(
            raw_values(&sb.root, &sb.key("Target\\Child\\Grandchild")),
            before_child
        );
    }

    #[test]
    fn snapshot_survives_json_roundtrip() {
        let sb = Sandbox::new();
        populate_every_value_type(&sb, "Target");
        let snap = snapshot_key(&sb.root, &sb.key("Target")).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        let back: RegistryKeySnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
    }

    #[test]
    fn snapshot_exposes_default_value_as_text_for_display() {
        let sb = Sandbox::new();
        let (k, _) = sb.root.create_subkey(sb.key(".foo")).unwrap();
        k.set_value("", &"Old.File").unwrap();
        let snap = snapshot_key(&sb.root, &sb.key(".foo")).unwrap();
        assert_eq!(snap.default_value.as_deref(), Some("Old.File"));
    }

    #[test]
    fn restoring_a_missing_snapshot_deletes_the_key() {
        let sb = Sandbox::new();
        let snap = snapshot_key(&sb.root, &sb.key("Ghost")).unwrap();
        assert!(!snap.exists);
        sb.root.create_subkey(sb.key("Ghost\\Sub")).unwrap();

        restore_key(&sb.root, &sb.key("Ghost"), &snap).unwrap();

        assert!(!sb.exists("Ghost"));
    }

    #[test]
    fn restore_removes_values_and_subkeys_added_after_snapshot() {
        let sb = Sandbox::new();
        let (k, _) = sb.root.create_subkey(sb.key("K")).unwrap();
        k.set_value("Keep", &"1").unwrap();
        let snap = snapshot_key(&sb.root, &sb.key("K")).unwrap();
        k.set_value("Added", &"2").unwrap();
        k.create_subkey("AddedSub").unwrap();

        restore_key(&sb.root, &sb.key("K"), &snap).unwrap();

        let names: Vec<String> = raw_values(&sb.root, &sb.key("K")).into_keys().collect();
        assert_eq!(names, vec!["Keep".to_string()]);
        assert!(!sb.exists("K\\AddedSub"));
    }

    #[test]
    fn legacy_snapshot_with_text_default_only_restores_it_as_reg_sz() {
        // Format written by WinTune <= 0.2.0: default value kept as a string
        // outside of `values`.
        let sb = Sandbox::new();
        let legacy = r#"{"exists":true,"default_value":"Old.File","values":{},"subkeys":{}}"#;
        let snap: RegistryKeySnapshot = serde_json::from_str(legacy).unwrap();

        restore_key(&sb.root, &sb.key(".foo"), &snap).unwrap();

        let vals = raw_values(&sb.root, &sb.key(".foo"));
        assert_eq!(vals.get(""), Some(&(REG_SZ as u32, utf16z("Old.File"))));
    }

    #[test]
    fn snapshot_of_unreadable_key_is_an_error_not_a_missing_key() {
        // HKLM\SECURITY is readable by SYSTEM only. Treating "access denied" as
        // "does not exist" would make a later restore DELETE the real key.
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        assert!(snapshot_key(&hklm, "SECURITY\\Policy").is_err());
    }

    #[test]
    fn delete_tree_on_missing_key_is_ok() {
        let sb = Sandbox::new();
        assert!(delete_tree(&sb.root, &sb.key("Nope")).is_ok());
    }

    #[test]
    fn delete_tree_removes_nested_keys() {
        let sb = Sandbox::new();
        sb.root.create_subkey(sb.key("A\\B\\C")).unwrap();
        delete_tree(&sb.root, &sb.key("A")).unwrap();
        assert!(!sb.exists("A"));
    }

    #[test]
    fn restore_reports_errors_from_nested_subkeys() {
        // Key names are limited to 255 chars: creating this child always fails,
        // whatever the privileges. The error must not be swallowed.
        let sb = Sandbox::new();
        let mut snap = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        snap.subkeys.insert(
            "x".repeat(300),
            RegistryKeySnapshot {
                exists: true,
                ..Default::default()
            },
        );
        assert!(restore_key(&sb.root, &sb.key("Parent"), &snap).is_err());
    }

    #[test]
    fn export_reg_deletes_then_recreates_existing_key() {
        let mut values = BTreeMap::new();
        values.insert(String::new(), (REG_SZ as u32, utf16z("Old.File")));
        values.insert(
            "Content Type".into(),
            (REG_SZ as u32, utf16z("text/\"x\"\\y")),
        );
        values.insert(
            "Flags".into(),
            (REG_DWORD as u32, 0x1Fu32.to_le_bytes().to_vec()),
        );
        values.insert("Old.Progid".into(), (REG_NONE as u32, Vec::new()));
        values.insert("Icon".into(), (REG_EXPAND_SZ as u32, utf16z("%X%")));
        let mut sub = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        sub.values
            .insert("Bin".into(), (REG_BINARY as u32, vec![0xAB, 0x01]));
        let mut snap = RegistryKeySnapshot {
            exists: true,
            values,
            ..Default::default()
        };
        snap.subkeys.insert("Sub".into(), sub);

        let text = export_reg(&[RegExportEntry {
            root_name: "HKEY_LOCAL_MACHINE",
            path: "SOFTWARE\\Classes\\.foo".into(),
            snapshot: &snap,
        }]);

        let expected = concat!(
            "Windows Registry Editor Version 5.00\r\n\r\n",
            "[-HKEY_LOCAL_MACHINE\\SOFTWARE\\Classes\\.foo]\r\n\r\n",
            "[HKEY_LOCAL_MACHINE\\SOFTWARE\\Classes\\.foo]\r\n",
            "@=\"Old.File\"\r\n",
            "\"Content Type\"=\"text/\\\"x\\\"\\\\y\"\r\n",
            "\"Flags\"=dword:0000001f\r\n",
            "\"Icon\"=hex(2):25,00,58,00,25,00,00,00\r\n",
            "\"Old.Progid\"=hex(0):\r\n\r\n",
            "[HKEY_LOCAL_MACHINE\\SOFTWARE\\Classes\\.foo\\Sub]\r\n",
            "\"Bin\"=hex:ab,01\r\n\r\n",
        );
        assert_eq!(text, expected);
    }

    #[test]
    fn export_reg_only_deletes_a_key_that_did_not_exist() {
        let snap = RegistryKeySnapshot::default();
        let text = export_reg(&[RegExportEntry {
            root_name: "HKEY_USERS",
            path: "S-1\\Software\\Classes\\.foo".into(),
            snapshot: &snap,
        }]);
        assert_eq!(
            text,
            "Windows Registry Editor Version 5.00\r\n\r\n[-HKEY_USERS\\S-1\\Software\\Classes\\.foo]\r\n\r\n"
        );
    }

    #[test]
    fn export_reg_wraps_long_hex_lines() {
        let mut snap = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        snap.values
            .insert("B".into(), (REG_BINARY as u32, vec![0u8; 40]));
        let text = export_reg(&[RegExportEntry {
            root_name: "HKEY_CURRENT_USER",
            path: "K".into(),
            snapshot: &snap,
        }]);
        assert!(text.lines().all(|l| l.len() <= 80), "{text}");
        assert!(text.contains(",\\\r\n  00"), "{text}");
    }

    #[test]
    fn value_roundtrip_restores_type_and_data() {
        let sb = Sandbox::new();
        let (k, _) = sb.root.create_subkey(sb.key("Adv")).unwrap();
        set_raw(&k, "HideFileExt", REG_DWORD, 1u32.to_le_bytes().to_vec());
        let snap = snapshot_value(&sb.root, &sb.key("Adv"), "HideFileExt").unwrap();
        set_raw(&k, "HideFileExt", REG_SZ, utf16z("changed"));

        restore_value(&sb.root, &sb.key("Adv"), "HideFileExt", &snap).unwrap();

        let vals = raw_values(&sb.root, &sb.key("Adv"));
        assert_eq!(
            vals.get("HideFileExt"),
            Some(&(REG_DWORD as u32, 1u32.to_le_bytes().to_vec()))
        );
    }

    #[test]
    fn restoring_a_value_that_did_not_exist_deletes_it_and_keeps_siblings() {
        let sb = Sandbox::new();
        let (k, _) = sb.root.create_subkey(sb.key("Adv")).unwrap();
        k.set_value("Sibling", &"keep").unwrap();
        let snap = snapshot_value(&sb.root, &sb.key("Adv"), "Hidden").unwrap();
        assert!(!snap.exists);
        k.set_value("Hidden", &1u32).unwrap();

        restore_value(&sb.root, &sb.key("Adv"), "Hidden", &snap).unwrap();

        let names: Vec<String> = raw_values(&sb.root, &sb.key("Adv")).into_keys().collect();
        assert_eq!(names, vec!["Sibling".to_string()]);
    }

    #[test]
    fn missing_key_is_a_missing_value_and_restoring_it_is_a_no_op() {
        let sb = Sandbox::new();
        let snap = snapshot_value(&sb.root, &sb.key("Nope"), "X").unwrap();
        assert_eq!(snap, ValueSnapshot::default());

        restore_value(&sb.root, &sb.key("Nope"), "X", &snap).unwrap();

        assert!(!sb.exists("Nope"));
    }

    #[test]
    fn restoring_an_existing_value_recreates_its_key() {
        let sb = Sandbox::new();
        let snap = ValueSnapshot {
            exists: true,
            kind: REG_DWORD as u32,
            data: 2u32.to_le_bytes().to_vec(),
        };

        restore_value(&sb.root, &sb.key("New\\Key"), "LaunchTo", &snap).unwrap();

        let vals = raw_values(&sb.root, &sb.key("New\\Key"));
        assert_eq!(
            vals.get("LaunchTo"),
            Some(&(4, 2u32.to_le_bytes().to_vec()))
        );
    }

    #[test]
    fn export_values_sets_or_deletes_each_value_grouped_by_key() {
        let present = ValueSnapshot {
            exists: true,
            kind: REG_DWORD as u32,
            data: 1u32.to_le_bytes().to_vec(),
        };
        let absent = ValueSnapshot::default();
        let text = export_reg_full(
            &[],
            &[
                RegValueExport {
                    root_name: "HKEY_CURRENT_USER",
                    path: "Adv".into(),
                    name: "HideFileExt".into(),
                    snapshot: &present,
                },
                RegValueExport {
                    root_name: "HKEY_CURRENT_USER",
                    path: "Adv".into(),
                    name: "Hidden".into(),
                    snapshot: &absent,
                },
            ],
        );
        assert_eq!(
            text,
            concat!(
                "Windows Registry Editor Version 5.00\r\n\r\n",
                "[HKEY_CURRENT_USER\\Adv]\r\n",
                "\"HideFileExt\"=dword:00000001\r\n",
                "\"Hidden\"=-\r\n\r\n",
            )
        );
    }

    #[test]
    fn a_failed_delete_still_rewrites_the_snapshot_and_reports_it() {
        // RegDeleteTree is not atomic: it may delete part of a tree and then
        // fail. What it deleted must still be put back.
        let sb = Sandbox::new();
        let mut snap = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        snap.values
            .insert("V".into(), (REG_SZ as u32, utf16z("kept")));

        let result = restore_after_delete(
            &sb.root,
            &sb.key("K"),
            &snap,
            Err("Cannot delete K: access denied".into()),
        );

        let err = result.expect_err("the delete failure must be reported");
        assert!(err.contains("access denied"), "{err}");
        let vals = raw_values(&sb.root, &sb.key("K"));
        assert_eq!(vals.get("V"), Some(&(REG_SZ as u32, utf16z("kept"))));
    }

    #[test]
    fn write_reg_file_uses_utf16le_with_bom() {
        let dir = std::env::temp_dir().join(format!("wintune-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out.reg");
        write_reg_file(&file, "Aé").unwrap();
        let bytes = std::fs::read(&file).unwrap();
        assert_eq!(bytes, vec![0xFF, 0xFE, b'A', 0, 0xE9, 0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
