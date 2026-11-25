//! Programs a menu entry can launch: detection, command lines and icons.

use std::path::{Path, PathBuf};

use super::{MenuEntry, Place, ProgramChoice};

/// Where each preset program lives on this machine (`None` = not installed).
#[derive(Debug, Clone, Default)]
pub struct Programs {
    pub vscode: Option<PathBuf>,
    pub terminal: Option<PathBuf>,
    pub powershell: Option<PathBuf>,
    pub cmd: Option<PathBuf>,
}

fn existing(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

fn env_path(var: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()))
}

impl Programs {
    pub fn detect() -> Self {
        let system_root = env_path("SystemRoot", r"C:\Windows");
        let local_app_data = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
        Self {
            vscode: crate::detector::resolve_vscode_executable(None)
                .map(PathBuf::from)
                .and_then(existing),
            terminal: local_app_data
                .map(|d| d.join(r"Microsoft\WindowsApps\wt.exe"))
                .and_then(existing),
            powershell: existing(
                system_root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
            ),
            cmd: existing(system_root.join(r"System32\cmd.exe")),
        }
    }

    /// The executable for a choice; a custom path only counts if the file exists.
    pub fn resolve(&self, choice: &ProgramChoice) -> Option<PathBuf> {
        match choice {
            ProgramChoice::VsCode => self.vscode.clone(),
            ProgramChoice::Terminal => self.terminal.clone(),
            ProgramChoice::PowerShell => self.powershell.clone(),
            ProgramChoice::Cmd => self.cmd.clone(),
            ProgramChoice::Custom(path) => {
                if path.trim().is_empty() {
                    None
                } else {
                    existing(PathBuf::from(path))
                }
            }
        }
    }
}

/// The `command` value of the verb. Every placeholder is quoted so a path with
/// spaces, apostrophes or accents reaches the program as one argument. A drive
/// root ends in a backslash, so drives use `"%1."` to keep the closing quote.
pub fn command_line(choice: &ProgramChoice, exe: &Path, place: Place) -> String {
    let exe = exe.display();
    match choice {
        ProgramChoice::VsCode | ProgramChoice::Custom(_) => {
            let dot = if place == Place::Drive { "." } else { "" };
            format!(r#""{exe}" "{}{dot}""#, place.placeholder())
        }
        ProgramChoice::Terminal => format!(r#""{exe}" -d "%V""#),
        ProgramChoice::PowerShell => {
            format!(r#""{exe}" -noexit -command Set-Location -literalPath '%V'"#)
        }
        ProgramChoice::Cmd => format!(r#""{exe}" /s /k pushd "%V""#),
    }
}

/// The `Icon` value: the entry's own icon if set, else the program's exe.
pub fn icon_value(entry: &MenuEntry, exe: &Path) -> String {
    match &entry.icon {
        Some(icon) => format!("\"{icon}\""),
        None => format!("\"{}\"", exe.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn entry(icon: Option<&str>) -> MenuEntry {
        MenuEntry {
            id: "e1".into(),
            label: "Open".into(),
            program: ProgramChoice::Cmd,
            places: BTreeSet::from([Place::Folder]),
            icon: icon.map(String::from),
        }
    }

    #[test]
    fn command_lines_quote_the_placeholder_for_each_place() {
        let exe = Path::new(r"C:\Apps\My App\app.exe");
        let custom = ProgramChoice::Custom(exe.display().to_string());
        assert_eq!(
            command_line(&custom, exe, Place::Folder),
            r#""C:\Apps\My App\app.exe" "%1""#
        );
        assert_eq!(
            command_line(&custom, exe, Place::Background),
            r#""C:\Apps\My App\app.exe" "%V""#
        );
        assert_eq!(
            command_line(&custom, exe, Place::Drive),
            r#""C:\Apps\My App\app.exe" "%1.""#
        );
        assert_eq!(
            command_line(&custom, exe, Place::Files),
            r#""C:\Apps\My App\app.exe" "%1""#
        );
        let wt = Path::new(r"C:\Users\u\AppData\Local\Microsoft\WindowsApps\wt.exe");
        assert_eq!(
            command_line(&ProgramChoice::Terminal, wt, Place::Background),
            r#""C:\Users\u\AppData\Local\Microsoft\WindowsApps\wt.exe" -d "%V""#
        );
        let ps = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        assert_eq!(
            command_line(&ProgramChoice::PowerShell, ps, Place::Folder),
            r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -noexit -command Set-Location -literalPath '%V'"#
        );
        let cmd = Path::new(r"C:\Windows\System32\cmd.exe");
        assert_eq!(
            command_line(&ProgramChoice::Cmd, cmd, Place::Drive),
            r#""C:\Windows\System32\cmd.exe" /s /k pushd "%V""#
        );
    }

    #[test]
    fn icon_value_prefers_the_custom_icon_and_quotes_both() {
        let exe = Path::new(r"C:\Apps\My App\app.exe");
        assert_eq!(icon_value(&entry(None), exe), r#""C:\Apps\My App\app.exe""#);
        assert_eq!(
            icon_value(&entry(Some(r"C:\Icons\a b.ico")), exe),
            r#""C:\Icons\a b.ico""#
        );
    }

    #[test]
    fn custom_programs_resolve_only_when_the_file_exists() {
        let file = std::env::temp_dir().join("wintune-cm-programs-resolve.exe");
        std::fs::write(&file, b"x").unwrap();
        let programs = Programs::default();
        let path = file.display().to_string();
        assert_eq!(
            programs.resolve(&ProgramChoice::Custom(path)),
            Some(file.clone())
        );
        assert_eq!(
            programs.resolve(&ProgramChoice::Custom(r"C:\missing.exe".into())),
            None
        );
        assert_eq!(
            programs.resolve(&ProgramChoice::Custom(String::new())),
            None
        );
        assert_eq!(programs.resolve(&ProgramChoice::Cmd), None);
        let programs = Programs {
            cmd: Some(file.clone()),
            ..Programs::default()
        };
        assert_eq!(programs.resolve(&ProgramChoice::Cmd), Some(file.clone()));
        let _ = std::fs::remove_file(file);
    }
}
