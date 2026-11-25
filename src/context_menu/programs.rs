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
            // Presets were detected once at startup: check they are still there.
            ProgramChoice::VsCode => self.vscode.clone().filter(|p| p.is_file()),
            ProgramChoice::Terminal => self.terminal.clone().filter(|p| p.is_file()),
            // The preset starts through cmd.exe, so both must exist.
            ProgramChoice::PowerShell => self
                .cmd
                .as_ref()
                .filter(|p| p.is_file())
                .and(self.powershell.clone().filter(|p| p.is_file())),
            ProgramChoice::Cmd => self.cmd.clone().filter(|p| p.is_file()),
            ProgramChoice::Custom(path) => {
                let path = super::clean_program_path(path);
                if path.is_empty() {
                    None
                } else {
                    existing(PathBuf::from(path))
                }
            }
        }
    }
}

/// The `command` value of the verb, or `None` when a program it needs is missing.
/// Every placeholder is quoted so a path with spaces, apostrophes or accents
/// reaches the program as one argument. A drive root ends in a backslash, which
/// would swallow the closing quote, so Drive uses `"%1."` / `"%V."` and
/// Background (also shown on a drive root's empty area) uses `"%V\."`; both
/// dots name the same folder. The Command Prompt and PowerShell presets go
/// through `cmd.exe` (its quoting ignores apostrophes); their one known limit is
/// that cmd expands `%VAR%` text inside folder names. Windows Terminal's `-d`
/// cannot take a folder name that contains a semicolon (known limit).
pub fn command_line(choice: &ProgramChoice, programs: &Programs, place: Place) -> Option<String> {
    let exe = programs.resolve(choice)?;
    let exe = exe.display();
    Some(match choice {
        ProgramChoice::VsCode | ProgramChoice::Custom(_) => {
            let arg = match place {
                Place::Background => r"%V\.",
                Place::Drive => "%1.",
                Place::Folder | Place::Files => "%1",
            };
            format!(r#""{exe}" "{arg}""#)
        }
        ProgramChoice::Terminal => {
            let arg = match place {
                Place::Background => r"%V\.",
                Place::Drive => "%V.",
                Place::Folder | Place::Files => "%V",
            };
            format!(r#""{exe}" -d "{arg}""#)
        }
        ProgramChoice::PowerShell => {
            let cmd = programs.cmd.as_ref()?.display();
            format!(r#""{cmd}" /s /c "pushd "%V" && "{exe}" -NoExit""#)
        }
        ProgramChoice::Cmd => format!(r#""{exe}" /s /k pushd "%V""#),
    })
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

    /// Presets that exist on disk, in a folder of their own.
    fn presets(name: &str) -> (Programs, PathBuf) {
        let dir = std::env::temp_dir().join(format!("wintune cm presets {name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |n: &str| {
            let path = dir.join(n);
            std::fs::write(&path, b"x").unwrap();
            path
        };
        let programs = Programs {
            vscode: None,
            terminal: Some(file("wt.exe")),
            powershell: Some(file("powershell.exe")),
            cmd: Some(file("cmd.exe")),
        };
        (programs, dir)
    }

    #[test]
    fn command_lines_quote_the_placeholder_for_each_place() {
        let dir = std::env::temp_dir().join("wintune cm app");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("app.exe");
        std::fs::write(&exe, b"x").unwrap();
        let (programs, pdir) = presets("lines");
        let custom = ProgramChoice::Custom(exe.display().to_string());
        let line = |c: &ProgramChoice, p| command_line(c, &programs, p).unwrap();
        let q = exe.display();
        assert_eq!(line(&custom, Place::Folder), format!(r#""{q}" "%1""#));
        assert_eq!(line(&custom, Place::Background), format!(r#""{q}" "%V\.""#));
        assert_eq!(line(&custom, Place::Drive), format!(r#""{q}" "%1.""#));
        assert_eq!(line(&custom, Place::Files), format!(r#""{q}" "%1""#));
        let _ = std::fs::remove_dir_all(&dir);

        let wt = programs.terminal.clone().unwrap();
        let wt = wt.display();
        let terminal = ProgramChoice::Terminal;
        assert_eq!(line(&terminal, Place::Folder), format!(r#""{wt}" -d "%V""#));
        assert_eq!(
            line(&terminal, Place::Background),
            format!(r#""{wt}" -d "%V\.""#)
        );
        assert_eq!(line(&terminal, Place::Drive), format!(r#""{wt}" -d "%V.""#));
        let cmd = programs.cmd.clone().unwrap();
        let ps = programs.powershell.clone().unwrap();
        for place in Place::ALL {
            assert_eq!(
                line(&ProgramChoice::PowerShell, place),
                format!(
                    r#""{}" /s /c "pushd "%V" && "{}" -NoExit""#,
                    cmd.display(),
                    ps.display()
                )
            );
            assert_eq!(
                line(&ProgramChoice::Cmd, place),
                format!(r#""{}" /s /k pushd "%V""#, cmd.display())
            );
        }
        let _ = std::fs::remove_dir_all(&pdir);
    }

    #[test]
    fn command_line_is_none_when_a_needed_program_is_missing() {
        let (full, dir) = presets("missing");
        let only_ps = Programs {
            powershell: full.powershell.clone(),
            ..Programs::default()
        };
        let ps = ProgramChoice::PowerShell;
        assert_eq!(command_line(&ps, &only_ps, Place::Folder), None);
        assert_eq!(only_ps.resolve(&ps), None);
        let only_cmd = Programs {
            cmd: full.cmd.clone(),
            ..Programs::default()
        };
        assert_eq!(command_line(&ps, &only_cmd, Place::Folder), None);
        let terminal = ProgramChoice::Terminal;
        assert_eq!(command_line(&terminal, &only_cmd, Place::Folder), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_preset_that_vanished_since_startup_no_longer_resolves() {
        let (programs, dir) = presets("vanished");
        assert!(programs.resolve(&ProgramChoice::Terminal).is_some());
        assert!(programs.resolve(&ProgramChoice::PowerShell).is_some());
        std::fs::remove_file(programs.terminal.as_ref().unwrap()).unwrap();
        std::fs::remove_file(programs.powershell.as_ref().unwrap()).unwrap();
        assert_eq!(programs.resolve(&ProgramChoice::Terminal), None);
        assert_eq!(programs.resolve(&ProgramChoice::PowerShell), None);
        assert_eq!(
            command_line(&ProgramChoice::Terminal, &programs, Place::Folder),
            None
        );
        std::fs::remove_file(programs.cmd.as_ref().unwrap()).unwrap();
        assert_eq!(programs.resolve(&ProgramChoice::Cmd), None);
        let _ = std::fs::remove_dir_all(dir);
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
