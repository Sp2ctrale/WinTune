//! Small JSON stores (snapshots, history…) with the same guarantees
//! everywhere: a missing file is an empty store, an unreadable one is an
//! error (never silently replaced), and saves are atomic.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Why a store could not be loaded. `detail` is the raw serde or OS error.
/// The UI translates it (`i18n::messages::load_error`); `Display` is the
/// English sentence kept for logs and reports.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// The file exists but is not valid JSON for the store.
    Corrupt { path: PathBuf, detail: String },
    /// The file exists but cannot be read.
    Unreadable { path: PathBuf, detail: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Corrupt { path, detail } => write!(
                f,
                "{} is unreadable ({}). It was left untouched: fix or move it before applying changes.",
                path.display(),
                detail
            ),
            LoadError::Unreadable { path, detail } => {
                write!(f, "Cannot read {}: {}", path.display(), detail)
            }
        }
    }
}

/// A missing file is `T::default()`. A corrupt one is an error: returning a
/// default would make the next save erase what the file holds.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> Result<T, LoadError> {
    match fs::read_to_string(path) {
        Ok(data) => serde_json::from_str(&data).map_err(|e| LoadError::Corrupt {
            path: path.to_path_buf(),
            detail: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(LoadError::Unreadable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        }),
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

        let r: Result<BTreeMap<String, u32>, LoadError> = load_json(&path);

        assert!(r.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ nope");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_error_display_is_unchanged() {
        let dir = temp_dir();
        let path = dir.join("store.json");
        fs::write(&path, "{ nope").unwrap();
        let serde = serde_json::from_str::<BTreeMap<String, u32>>("{ nope")
            .unwrap_err()
            .to_string();

        let corrupt = load_json::<BTreeMap<String, u32>>(&path).unwrap_err();

        assert_eq!(
            corrupt,
            LoadError::Corrupt {
                path: path.clone(),
                detail: serde.clone()
            }
        );
        assert_eq!(
            corrupt.to_string(),
            format!(
                "{} is unreadable ({}). It was left untouched: fix or move it before applying changes.",
                path.display(),
                serde
            )
        );

        // A directory cannot be read as a file.
        let os = fs::read_to_string(&dir).unwrap_err().to_string();
        let unreadable = load_json::<BTreeMap<String, u32>>(&dir).unwrap_err();

        assert_eq!(
            unreadable,
            LoadError::Unreadable {
                path: dir.clone(),
                detail: os.clone()
            }
        );
        assert_eq!(
            unreadable.to_string(),
            format!("Cannot read {}: {}", dir.display(), os)
        );
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
