//! Which file is behind a menu entry, and whether it is Windows' own.
//!
//! Read-only. When the file cannot be told for sure, callers treat the entry
//! as Windows' own: a Windows entry must never be offered as another
//! program's.

use std::path::Path;

use winreg::enums::*;

use super::MenuTarget;

/// Where Windows looks up a bare program name before `PATH`.
const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";

/// The default value of `rel`, from the user root, else the machine root.
fn default_text(target: &MenuTarget, rel: &str) -> Option<String> {
    [&target.user, &target.machine].into_iter().find_map(|t| {
        t.root
            .open_subkey_with_flags(t.path(rel), KEY_READ)
            .ok()?
            .get_value::<String, _>("")
            .ok()
    })
}

/// `%NAME%` replaced by the variable's value; unknown names stay as written.
pub(super) fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) if !name.is_empty() => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            // Keep the closing `%`: it may open the next variable.
            _ => {
                out.push('%');
                out.push_str(name);
                rest = &after[end..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The program of a command line: the first quoted token, else the text up
/// to the first `.exe`, else the first word; variables expanded.
pub(super) fn command_program(command: &str) -> Option<String> {
    let command = command.trim();
    let program = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next().unwrap_or_default()
    } else if let Some(end) = command.to_ascii_lowercase().find(".exe") {
        &command[..end + 4]
    } else {
        command.split_whitespace().next().unwrap_or_default()
    };
    let program = expand_env(program.trim());
    (!program.is_empty()).then_some(program)
}

/// The file of a resource reference: `@file.dll,-101`, `"C:\x.exe",0` or
/// `C:\x.ico`. `None` for a package string (`@{...}`) or empty text.
pub(super) fn resource_file(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text.strip_prefix('@').unwrap_or(text);
    if text.starts_with('{') {
        return None;
    }
    let file = match text.rsplit_once(',') {
        Some((file, index)) if index.trim().parse::<i32>().is_ok() => file,
        _ => text,
    };
    let file = expand_env(file.trim().trim_matches('"').trim());
    (!file.is_empty()).then_some(file)
}

/// `file` as an absolute path. A bare name (`rundll32.exe`) is looked up in
/// `System32`, the Windows folder, then App Paths; `None` when it is in none
/// of them, so the caller cannot mistake a Windows program for another one.
pub(super) fn resolve(target: &MenuTarget, file: &str, windows_dir: &Path) -> Option<String> {
    if file.contains(['\\', '/']) {
        return Some(file.to_string());
    }
    let mut names = vec![file.to_string()];
    if Path::new(file).extension().is_none() {
        names.push(format!("{file}.exe"));
    }
    let dirs = [windows_dir.join("System32"), windows_dir.to_path_buf()];
    let in_windows = dirs
        .iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
        .map(|found| found.display().to_string());
    in_windows.or_else(|| {
        names.iter().find_map(|name| {
            let path = default_text(target, &format!(r"{APP_PATHS}\{name}"))?;
            let path = expand_env(path.trim().trim_matches('"'));
            path.contains('\\').then_some(path)
        })
    })
}

/// `file` is `dir` or inside it (case-insensitive, whole components only).
pub(super) fn under_dir(file: &str, dir: &Path) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let file = norm(file);
    let dir = norm(&dir.to_string_lossy());
    !dir.is_empty()
        && file
            .strip_prefix(&dir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

pub(super) fn is_braced_guid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 38
        && bytes[0] == b'{'
        && bytes[37] == b'}'
        && bytes[1..37].iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// The DLL behind a CLSID (`InprocServer32`, user then machine), expanded
/// but not resolved.
pub(super) fn clsid_server(target: &MenuTarget, clsid: &str) -> Option<String> {
    let rel = format!(r"Software\Classes\CLSID\{clsid}\InprocServer32");
    let server = expand_env(default_text(target, &rel)?.trim().trim_matches('"'));
    (!server.is_empty()).then_some(server)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_paths_and_guids_are_parsed() {
        let program = |c: &str| command_program(c);
        assert_eq!(
            program(r#""C:\A b\x.exe" "%1""#).as_deref(),
            Some(r"C:\A b\x.exe")
        );
        assert_eq!(
            program(r"C:\A b\X.EXE %1").as_deref(),
            Some(r"C:\A b\X.EXE")
        );
        assert_eq!(program("rundll32 x.dll,Y").as_deref(), Some("rundll32"));
        assert_eq!(program("  "), None);
        let root = std::env::var("SystemRoot").expect("SystemRoot is always set");
        assert_eq!(expand_env(r"%SystemRoot%\x"), format!(r"{root}\x"));
        assert_eq!(
            expand_env("%WINTUNE_NOT_A_VARIABLE%%SystemRoot%"),
            format!("%WINTUNE_NOT_A_VARIABLE%{root}")
        );
        assert_eq!(expand_env("100% sure"), "100% sure");
        let windows = Path::new(r"C:\Windows\");
        assert!(under_dir(r"c:\windows\System32\x.dll", windows));
        assert!(under_dir(r"C:/Windows/x.exe", windows));
        assert!(!under_dir(r"C:\Windows2\x.exe", windows));
        assert!(!under_dir(r"D:\C\Windows\x.exe", windows));
        assert!(is_braced_guid("{596ab062-B4D2-4215-9F74-E9109B0A8153}"));
        assert!(!is_braced_guid("596AB062-B4D2-4215-9F74-E9109B0A8153"));
        assert!(!is_braced_guid("{596AB062-B4D2-4215-9F74-E9109B0A815G}"));
    }

    #[test]
    fn resource_references_name_their_file() {
        let file = |t: &str| resource_file(t);
        assert_eq!(file("@efscore.dll,-101").as_deref(), Some("efscore.dll"));
        assert_eq!(
            file(r#""C:\a b\x.exe",0"#).as_deref(),
            Some(r"C:\a b\x.exe")
        );
        assert_eq!(file(r"C:\x.ico").as_deref(), Some(r"C:\x.ico"));
        let root = std::env::var("SystemRoot").expect("SystemRoot is always set");
        assert_eq!(
            file(r"@%SystemRoot%\System32\shell32.dll,-8506"),
            Some(format!(r"{root}\System32\shell32.dll"))
        );
        assert_eq!(file("@{Microsoft.App?ms-resource://x}"), None);
        assert_eq!(file("  "), None);
    }
}
