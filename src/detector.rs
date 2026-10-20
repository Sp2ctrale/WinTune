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
        let path = format!("{}\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss", sid);
        if let Ok(key) = hku.open_subkey_with_flags(&path, KEY_READ) {
            if key.enum_keys().filter_map(|k| k.ok()).any(|k| re.is_match(&k)) {
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
