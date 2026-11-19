//! Operation history: one JSON line per registry operation (apply, rollback,
//! tweak restore…) appended to `%ProgramData%\Wintune\history.jsonl`.
//! History is informational: a damaged line is skipped, never fatal.

use crate::associations::OperationReport;
use crate::clock;
use crate::models::app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Local time, e.g. `2026-11-20 01:30:00`.
    pub time: String,
    /// "File associations", "Explorer & Taskbar"…
    pub module: String,
    /// "Apply", "Rollback", "Restore"…
    pub action: String,
    /// Items that succeeded (extensions, tweak ids).
    pub succeeded: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// `.reg` safety net written before the operation, if any.
    #[serde(default)]
    pub reg_backup: Option<String>,
}

impl HistoryEntry {
    pub fn from_report(module: &str, action: &str, report: &OperationReport) -> Self {
        Self {
            time: clock::now_local(),
            module: module.to_string(),
            action: action.to_string(),
            succeeded: report.succeeded.clone(),
            errors: report.errors.clone(),
            warnings: report.warnings.clone(),
            reg_backup: report
                .reg_backup_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }

    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn default_history_path() -> PathBuf {
    app_data_dir().join("history.jsonl")
}

/// Appends one line (creating the file and its folder if needed).
pub fn append_entry(path: &Path, entry: &HistoryEntry) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()))
        .map_err(|e| format!("Cannot write {}: {}", path.display(), e))
}

/// The last `limit` entries, newest first. Missing file = empty history.
pub fn read_recent(path: &Path, limit: usize) -> Result<Vec<HistoryEntry>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Cannot read {}: {}", path.display(), e)),
    };
    Ok(text
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
        .take(limit)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file() -> (PathBuf, PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("wintune-history-{}-{}", std::process::id(), nanos));
        (dir.join("sub").join("history.jsonl"), dir)
    }

    fn entry(n: usize) -> HistoryEntry {
        HistoryEntry {
            time: format!("2026-11-20 01:00:{:02}", n),
            module: "File associations".into(),
            action: "Apply".into(),
            succeeded: vec![format!(".e{}", n)],
            errors: Vec::new(),
            warnings: Vec::new(),
            reg_backup: None,
        }
    }

    #[test]
    fn from_report_keeps_outcome_and_timestamps_it() {
        let report = OperationReport {
            succeeded: vec![".rs".into()],
            errors: vec![".x: boom".into()],
            warnings: vec!["w".into()],
            reg_backup_file: Some(PathBuf::from("C:\\b\\x.reg")),
        };

        let e = HistoryEntry::from_report("File associations", "Apply", &report);

        assert_eq!(e.module, "File associations");
        assert_eq!(e.action, "Apply");
        assert_eq!(e.succeeded, vec![".rs".to_string()]);
        assert_eq!(e.errors, vec![".x: boom".to_string()]);
        assert_eq!(e.warnings, vec!["w".to_string()]);
        assert_eq!(e.reg_backup.as_deref(), Some("C:\\b\\x.reg"));
        assert!(e.time.starts_with("20"), "{}", e.time);
        assert!(!e.is_success());
    }

    #[test]
    fn appended_entries_read_back_newest_first() {
        let (path, dir) = temp_file();
        for n in 0..3 {
            append_entry(&path, &entry(n)).unwrap();
        }

        let recent = read_recent(&path, 10).unwrap();

        assert_eq!(recent, vec![entry(2), entry(1), entry(0)]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn read_recent_keeps_only_the_last_entries() {
        let (path, dir) = temp_file();
        for n in 0..5 {
            append_entry(&path, &entry(n)).unwrap();
        }

        let recent = read_recent(&path, 2).unwrap();

        assert_eq!(recent, vec![entry(4), entry(3)]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn damaged_lines_are_skipped() {
        let (path, dir) = temp_file();
        append_entry(&path, &entry(0)).unwrap();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("{ not json\n\n");
        std::fs::write(&path, text).unwrap();
        append_entry(&path, &entry(1)).unwrap();

        let recent = read_recent(&path, 10).unwrap();

        assert_eq!(recent, vec![entry(1), entry(0)]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let (path, _dir) = temp_file();
        assert!(read_recent(&path, 10).unwrap().is_empty());
    }
}
