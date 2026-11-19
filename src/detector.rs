use regex::Regex;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

pub fn get_loaded_user_sids() -> Vec<String> {
    let hku = RegKey::predef(HKEY_USERS);
    let re = Regex::new(r"^S-1-(5-21|12-1)-[\d-]+$").unwrap();
    hku.enum_keys()
        .filter_map(|k| k.ok())
        .filter(|k| re.is_match(k))
        .collect()
}

pub fn has_wsl_distribution() -> bool {
    let hku = RegKey::predef(HKEY_USERS);
    let re = Regex::new(r"^\{.+\}$").unwrap();
    for sid in get_loaded_user_sids() {
        let path = format!(
            "{}\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss",
            sid
        );
        if let Ok(key) = hku.open_subkey_with_flags(&path, KEY_READ) {
            if key
                .enum_keys()
                .filter_map(|k| k.ok())
                .any(|k| re.is_match(&k))
            {
                return true;
            }
        }
    }
    false
}

fn get_registered_vscode_path() -> Option<PathBuf> {
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let key = hkcr
        .open_subkey_with_flags("Applications\\Code.exe\\shell\\open\\command", KEY_READ)
        .ok()?;
    let val: String = key.get_value("").ok()?;
    let re = Regex::new(r#"^"([^"]+)""#).ok()?;
    let caps = re.captures(&val)?;
    let path = PathBuf::from(caps.get(1)?.as_str());
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

fn get_vscode_from_path_env() -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let cmd_candidate = dir.join("code.cmd");
        if cmd_candidate.is_file() {
            if let Some(install_root) = cmd_candidate.parent().and_then(|p| p.parent()) {
                let exe = install_root.join("Code.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
        let exe_candidate = dir.join("Code.exe");
        if exe_candidate.is_file() {
            return Some(exe_candidate);
        }
    }
    None
}

fn get_default_vscode_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for var in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(root) = std::env::var(var) {
            if var == "LOCALAPPDATA" {
                paths.push(PathBuf::from(root).join("Programs\\Microsoft VS Code\\Code.exe"));
            } else {
                paths.push(PathBuf::from(root).join("Microsoft VS Code\\Code.exe"));
            }
        }
    }
    paths
}

pub fn resolve_vscode_executable(custom_path: Option<&str>) -> Option<String> {
    if let Some(custom) = custom_path {
        if !custom.trim().is_empty() && Path::new(custom).is_file() {
            return Some(custom.to_string());
        }
    }
    if let Some(p) = get_registered_vscode_path() {
        return Some(p.to_string_lossy().to_string());
    }
    if let Some(p) = get_vscode_from_path_env() {
        return Some(p.to_string_lossy().to_string());
    }
    for p in get_default_vscode_paths() {
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

pub fn convert_to_per_user_path(path: &str) -> String {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let prefix = format!("{}\\", local_app_data.trim_end_matches('\\'));
        if path.to_lowercase().starts_with(&prefix.to_lowercase()) {
            let suffix = &path[prefix.len()..];
            return format!("%LOCALAPPDATA%\\{}", suffix);
        }
    }
    path.to_string()
}

pub fn build_editor_command(code_exe: &str) -> String {
    let portable = convert_to_per_user_path(code_exe);
    format!("\"{}\" \"%1\"", portable)
}

/// Windows build number (`CurrentBuildNumber`), e.g. 26100.
pub fn windows_build() -> Option<u32> {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion", KEY_READ)
        .ok()?
        .get_value::<String, _>("CurrentBuildNumber")
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Windows 11 kept the "10.0" version: it is told apart by build >= 22000.
pub fn is_windows_11_build(build: u32) -> bool {
    build >= 22000
}

/// Human name of the editor for the "Edit with …" menu entry.
pub fn editor_display_name(executable: &str) -> String {
    let stem = Path::new(executable.trim())
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match stem.to_lowercase().as_str() {
        "" => "Editor".to_string(),
        "code" => "VS Code".to_string(),
        "code - insiders" => "VS Code Insiders".to_string(),
        _ => stem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_display_name_recognises_vscode_variants() {
        assert_eq!(editor_display_name(r"C:\VS\Code.exe"), "VS Code");
        assert_eq!(
            editor_display_name(r"C:\VS\Code - Insiders.exe"),
            "VS Code Insiders"
        );
    }

    #[test]
    fn editor_display_name_falls_back_to_file_stem() {
        assert_eq!(editor_display_name(r"C:\Tools\notepad++.exe"), "notepad++");
        assert_eq!(editor_display_name(""), "Editor");
    }

    #[test]
    fn windows_build_is_read_from_the_registry() {
        let build = windows_build().expect("CurrentBuildNumber not found");
        assert!(build >= 10240, "{}", build); // first Windows 10 build
    }

    #[test]
    fn windows_11_starts_at_build_22000() {
        assert!(!is_windows_11_build(19045));
        assert!(!is_windows_11_build(20348)); // Server 2022
        assert!(is_windows_11_build(22000));
        assert!(is_windows_11_build(26100));
    }

    #[test]
    fn editor_command_quotes_path_and_argument() {
        assert_eq!(
            build_editor_command(r"C:\X\Code.exe"),
            r#""C:\X\Code.exe" "%1""#
        );
    }
}
