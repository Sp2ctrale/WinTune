//! Small JSON stores (snapshots, history…) with the same guarantees
//! everywhere: a missing file is an empty store, an unreadable one is an
//! error (never silently replaced), and saves are atomic.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// A missing file is `T::default()`. A corrupt one is an error: returning a
/// default would make the next save erase what the file holds.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    match fs::read_to_string(path) {
        Ok(data) => serde_json::from_str(&data).map_err(|e| {
            format!(
                "{} is unreadable ({}). It was left untouched: fix or move it before applying changes.",
                path.display(),
                e
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("Cannot read {}: {}", path.display(), e)),
    }
}

/// Atomic save: write a temp file next to `path`, then rename it over.
pub fn save_json<T: Serialize>(value: &T, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    fs::write(&tmp, json).map_err(|e| format!("Cannot write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| format!("Cannot replace {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn temp_dir() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("wintune-persist-{}-{}", std::process::id(), nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_is_the_default_value() {
        let dir = temp_dir();
        let v: BTreeMap<String, u32> = load_json(&dir.join("none.json")).unwrap();
        assert!(v.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_an_error_and_is_left_untouched() {
        let dir = temp_dir();
        let path = dir.join("store.json");
        fs::write(&path, "{ nope").unwrap();

        let r: Result<BTreeMap<String, u32>, String> = load_json(&path);

        assert!(r.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ nope");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrips_and_leaves_no_temp_file() {
        let dir = temp_dir();
        let path = dir.join("sub").join("store.json");
        let mut v = BTreeMap::new();
        v.insert("a".to_string(), 1u32);

        save_json(&v, &path).unwrap();
        let back: BTreeMap<String, u32> = load_json(&path).unwrap();

        assert_eq!(back, v);
        assert_eq!(fs::read_dir(dir.join("sub")).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
