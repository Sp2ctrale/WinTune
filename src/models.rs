pub use crate::regsnap::RegistryKeySnapshot;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Which look the app uses. Serialized in `config.json`: keep the variant
/// names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    /// Follow the Windows light / dark setting.
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DoubleClickMode {
    OpenInEditor,
    ExecuteScript,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomContextCommand {
    pub verb_key: String,
    pub menu_label: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtensionConfig {
    pub enabled: bool,
    pub extension: String,
    pub prog_id: String,
    pub description: String,
    pub icon_name: String,
    pub reset_existing: bool,
    pub reset_user_choice: bool,
    pub mode: DoubleClickMode,
    pub launcher_command: Option<String>,
    pub elevated_command: Option<String>,
    pub perceived_type: String,
    pub content_type: String,
    pub preserve_existing_properties: bool,
    pub extra_verbs: Vec<CustomContextCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOverrideSnapshot {
    pub sid: String,
    pub user_classes_ext: RegistryKeySnapshot,
    pub user_file_exts: RegistryKeySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionBackup {
    pub extension: String,
    /// ProgID written by the most recent apply (shown in History).
    #[serde(default)]
    pub custom_prog_id: String,
    #[serde(default)]
    pub icon_installed_path: String,
    /// When the original state was captured (local time).
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub last_applied: Option<String>,
    /// `HKLM\SOFTWARE\Classes\<ext>` before WinTune first touched it.
    pub hklm_extension_key: RegistryKeySnapshot,
    /// Every ProgID key WinTune has written for this extension, captured
    /// before its first write. Restoring them all undoes ProgID renames too.
    #[serde(default)]
    pub prog_id_snapshots: BTreeMap<String, RegistryKeySnapshot>,
    #[serde(default)]
    pub user_overrides: Vec<UserOverrideSnapshot>,
    /// A rollback already restored the machine-wide keys; only the overrides
    /// of users who were signed out at the time are still pending.
    #[serde(default)]
    pub machine_restored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackupStore {
    pub backups: BTreeMap<String, ExtensionBackup>,
}

impl BackupStore {
    /// Registry keys are case-insensitive, so `.FOO` and `.foo` share one
    /// snapshot: returns the stored key matching `extension`, if any.
    pub fn key_for(&self, extension: &str) -> Option<&str> {
        self.backups
            .keys()
            .find(|k| k.eq_ignore_ascii_case(extension))
            .map(|k| k.as_str())
    }

    pub fn find(&self, extension: &str) -> Option<&ExtensionBackup> {
        self.key_for(extension).and_then(|k| self.backups.get(k))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: ThemeMode,
    pub code_executable: String,
    pub icon_source_folder: String,
    pub icon_destination_folder: String,
    pub pak_file_path: String,
    pub extensions: Vec<ExtensionConfig>,
    /// UI language code; unknown codes fall back to English.
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "en".to_string()
}

fn program_data_dir() -> PathBuf {
    PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string()))
}

/// `%ProgramData%\Wintune`: config, backups, icon pack and `.reg` exports.
pub fn app_data_dir() -> PathBuf {
    program_data_dir().join("Wintune")
}

impl Default for AppConfig {
    fn default() -> Self {
        let base = app_data_dir();
        Self {
            theme: ThemeMode::System,
            code_executable: String::new(),
            icon_source_folder: base.join("icons").to_string_lossy().into_owned(),
            icon_destination_folder: program_data_dir()
                .join("ico")
                .to_string_lossy()
                .into_owned(),
            pak_file_path: base.join("gallery.pak").to_string_lossy().into_owned(),
            extensions: default_extensions(),
            language: default_language(),
        }
    }
}

pub fn default_extensions() -> Vec<ExtensionConfig> {
    let raw = [
        (
            ".html",
            "CustomHTMLFile",
            "HTML Document",
            "html",
            false,
            "text",
            "text/html",
        ),
        (
            ".htm",
            "CustomHTMFile",
            "HTM Document",
            "html",
            false,
            "text",
            "text/html",
        ),
        (
            ".css",
            "CustomCSSFile",
            "CSS Stylesheet",
            "css",
            false,
            "text",
            "text/css",
        ),
        (
            ".scss",
            "CustomSCSSFile",
            "SCSS Stylesheet",
            "sass",
            false,
            "text",
            "text/x-scss",
        ),
        (
            ".sass",
            "CustomSASSFile",
            "SASS Stylesheet",
            "sass",
            false,
            "text",
            "text/x-sass",
        ),
        (
            ".less",
            "CustomLESSFile",
            "LESS Stylesheet",
            "less",
            false,
            "text",
            "text/plain",
        ),
        (
            ".js",
            "CustomJSFile",
            "JavaScript File",
            "javascript",
            false,
            "text",
            "text/javascript",
        ),
        (
            ".jsx",
            "CustomJSXFile",
            "JavaScript React File",
            "react",
            true,
            "text",
            "text/jsx",
        ),
        (
            ".ts",
            "CustomTSFile",
            "TypeScript File",
            "typescript",
            false,
            "text",
            "text/typescript",
        ),
        (
            ".tsx",
            "CustomTSXFile",
            "TypeScript React File",
            "react_ts",
            false,
            "text",
            "text/tsx",
        ),
        (
            ".mjs",
            "CustomMJSFile",
            "JavaScript Module",
            "javascript",
            false,
            "text",
            "text/javascript",
        ),
        (
            ".json",
            "CustomJSONFile",
            "JSON File",
            "json",
            false,
            "text",
            "application/json",
        ),
        (
            ".yaml",
            "CustomYAMLFile",
            "YAML File",
            "yaml",
            false,
            "text",
            "application/x-yaml",
        ),
        (
            ".yml",
            "CustomYMLFile",
            "YAML File",
            "yaml",
            false,
            "text",
            "application/x-yaml",
        ),
        (
            ".toml",
            "CustomTOMLFile",
            "TOML File",
            "toml",
            false,
            "text",
            "application/toml",
        ),
        (
            ".py",
            "CustomPythonFile",
            "Python File",
            "python",
            false,
            "text",
            "text/x-python",
        ),
        (
            ".pyw",
            "CustomPythonWFile",
            "Python Windows File",
            "python",
            false,
            "text",
            "text/x-python",
        ),
        (
            ".php",
            "CustomPHPFile",
            "PHP File",
            "php",
            false,
            "text",
            "application/x-httpd-php",
        ),
        (
            ".java",
            "CustomJavaFile",
            "Java Source File",
            "java",
            false,
            "text",
            "text/x-java-source",
        ),
        (
            ".class",
            "CustomJavaClassFile",
            "Java Class File",
            "javaclass",
            false,
            "",
            "application/java-vm",
        ),
        (
            ".kt",
            "CustomKotlinFile",
            "Kotlin Source File",
            "kotlin",
            true,
            "text",
            "text/plain",
        ),
        (
            ".kts",
            "CustomKotlinScriptFile",
            "Kotlin Script File",
            "kotlin",
            true,
            "text",
            "text/plain",
        ),
        (
            ".scala",
            "CustomScalaFile",
            "Scala Source File",
            "scala",
            true,
            "text",
            "text/plain",
        ),
        (
            ".sbt",
            "CustomSBTFile",
            "SBT Build File",
            "sbt",
            true,
            "text",
            "text/plain",
        ),
        (
            ".groovy",
            "CustomGroovyFile",
            "Groovy Source File",
            "groovy",
            true,
            "text",
            "text/plain",
        ),
        (
            ".gradle",
            "CustomGradleFile",
            "Gradle Build File",
            "gradle",
            true,
            "text",
            "text/plain",
        ),
        (
            ".c",
            "CustomCFile",
            "C Source File",
            "c",
            false,
            "text",
            "text/x-c",
        ),
        (
            ".cpp",
            "CustomCPPFile",
            "C++ Source File",
            "cpp",
            false,
            "text",
            "text/x-c++src",
        ),
        (
            ".h",
            "CustomHFile",
            "C Header File",
            "h",
            false,
            "text",
            "text/x-chdr",
        ),
        (
            ".hpp",
            "CustomHPPFile",
            "C++ Header File",
            "hpp",
            false,
            "text",
            "text/x-c++hdr",
        ),
        (
            ".cs",
            "CustomCSharpFile",
            "C# Source File",
            "csharp",
            false,
            "text",
            "text/plain",
        ),
        (
            ".go",
            "CustomGoFile",
            "Go Source File",
            "go",
            false,
            "text",
            "text/plain",
        ),
        (
            ".rs",
            "CustomRustFile",
            "Rust Source File",
            "rust",
            false,
            "text",
            "text/rust",
        ),
        (
            ".rb",
            "CustomRubyFile",
            "Ruby Source File",
            "ruby",
            false,
            "text",
            "text/x-ruby",
        ),
        (
            ".swift",
            "CustomSwiftFile",
            "Swift Source File",
            "swift",
            true,
            "text",
            "text/plain",
        ),
        (
            ".r",
            "CustomRFile",
            "R Script File",
            "r",
            true,
            "text",
            "text/plain",
        ),
        (
            ".lua",
            "CustomLuaFile",
            "Lua Script File",
            "lua",
            true,
            "text",
            "text/plain",
        ),
        (
            ".dart",
            "CustomDartFile",
            "Dart Source File",
            "dart",
            true,
            "text",
            "application/vnd.dart",
        ),
        (
            ".pl",
            "CustomPerlFile",
            "Perl Script File",
            "perl",
            true,
            "text",
            "text/x-perl",
        ),
        (
            ".pm",
            "CustomPerlModuleFile",
            "Perl Module File",
            "perl",
            true,
            "text",
            "text/x-perl",
        ),
        (
            ".vue",
            "CustomVueFile",
            "Vue Component",
            "vue",
            false,
            "text",
            "text/plain",
        ),
        (
            ".svelte",
            "CustomSvelteFile",
            "Svelte Component",
            "svelte",
            false,
            "text",
            "text/plain",
        ),
        (
            ".xml",
            "CustomXMLFile",
            "XML File",
            "xml",
            false,
            "text",
            "application/xml",
        ),
        (
            ".svg",
            "CustomSVGFile",
            "SVG Image",
            "svg",
            false,
            "image",
            "image/svg+xml",
        ),
        (
            ".md",
            "CustomMarkdownFile",
            "Markdown File",
            "markdown",
            false,
            "text",
            "text/markdown",
        ),
        (
            ".mdx",
            "CustomMDXFile",
            "MDX File",
            "mdx",
            false,
            "text",
            "text/markdown",
        ),
        (
            ".mmd",
            "CustomMMDFile",
            "MMD Document",
            "mermaid",
            false,
            "text",
            "text/plain",
        ),
        (
            ".tex",
            "CustomTexFile",
            "LaTeX Document",
            "latex.clone",
            true,
            "text",
            "application/x-tex",
        ),
        (
            ".bib",
            "CustomBibFile",
            "Bibliography File",
            "bibliography",
            true,
            "text",
            "text/plain",
        ),
        (
            ".ipynb",
            "CustomJupyterFile",
            "Jupyter Notebook",
            "jupyter",
            true,
            "text",
            "application/x-ipynb+json",
        ),
        (
            ".sh",
            "CustomShellFile",
            "Shell Script",
            "console",
            false,
            "text",
            "application/x-sh",
        ),
        (
            ".bat",
            "CustomBatchFile",
            "Batch File",
            "console",
            false,
            "text",
            "application/x-bat",
        ),
        (
            ".cmd",
            "CustomCMDFile",
            "Command File",
            "console",
            false,
            "text",
            "application/cmd",
        ),
        (
            ".ps1",
            "CustomPowerShellFile",
            "PowerShell Script",
            "powershell",
            false,
            "text",
            "text/plain",
        ),
        (
            ".vbs",
            "CustomVBSFile",
            "VBScript File",
            "visualstudio",
            true,
            "text",
            "text/vbscript",
        ),
        (
            ".reg",
            "CustomRegFile",
            "Registry File",
            "regedit",
            true,
            "text",
            "text/plain",
        ),
        (
            ".env",
            "CustomEnvFile",
            "Environment File",
            "settings",
            false,
            "text",
            "text/plain",
        ),
        (
            ".ini",
            "CustomIniFile",
            "Configuration File",
            "settings",
            false,
            "text",
            "text/plain",
        ),
        (
            ".conf",
            "CustomConfFile",
            "Configuration File",
            "settings",
            true,
            "text",
            "text/plain",
        ),
        (
            ".config",
            "CustomConfigFile",
            "Configuration File",
            "settings",
            true,
            "text",
            "text/plain",
        ),
        (
            ".editorconfig",
            "CustomEditorConfigFile",
            "EditorConfig File",
            "editorconfig",
            true,
            "text",
            "text/plain",
        ),
        (
            ".gitignore",
            "CustomGitignoreFile",
            "Git Ignore File",
            "git",
            false,
            "text",
            "text/plain",
        ),
        (
            ".dockerignore",
            "CustomDockerignoreFile",
            "Docker Ignore File",
            "docker",
            false,
            "text",
            "text/plain",
        ),
        (
            ".dockerfile",
            "CustomDockerfile",
            "Dockerfile",
            "docker",
            false,
            "text",
            "text/plain",
        ),
        (
            ".sql",
            "CustomSQLFile",
            "SQL File",
            "database",
            false,
            "text",
            "application/sql",
        ),
        (
            ".hql",
            "CustomHQLFile",
            "HQL File",
            "hive",
            false,
            "text",
            "text/plain",
        ),
        (
            ".csv",
            "CustomCSVFile",
            "CSV File",
            "table",
            true,
            "text",
            "text/csv",
        ),
        (
            ".http",
            "CustomHTTPFile",
            "HTTP Request File",
            "http",
            true,
            "text",
            "text/plain",
        ),
        (
            ".log",
            "CustomLogFile",
            "Log File",
            "log",
            true,
            "text",
            "text/plain",
        ),
    ];

    let user_choice_resets = [
        ".bib",
        ".conf",
        ".config",
        ".csv",
        ".editorconfig",
        ".http",
        ".ipynb",
        ".jsx",
        ".log",
        ".reg",
        ".tex",
        ".vbs",
    ];

    raw.into_iter()
        .map(|(ext, prog_id, desc, icon, reset, perceived, content)| {
            let (mode, launcher, elevated) = match prog_id {
                "CustomBatchFile" | "CustomCMDFile" => (
                    DoubleClickMode::ExecuteScript,
                    Some("\"%1\" %*".to_string()),
                    Some("cmd.exe /C \"%1\" %*".to_string()),
                ),
                "CustomVBSFile" => (
                    DoubleClickMode::ExecuteScript,
                    Some("wscript.exe \"%1\" %*".to_string()),
                    None,
                ),
                "CustomRegFile" => (
                    DoubleClickMode::ExecuteScript,
                    Some("regedit.exe \"%1\"".to_string()),
                    None,
                ),
                "CustomPowerShellFile" => (
                    DoubleClickMode::ExecuteScript,
                    Some("powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%1\"".to_string()),
                    None,
                ),
                "CustomShellFile" => (
                    DoubleClickMode::ExecuteScript,
                    Some("powershell.exe -NoProfile -Command \"Set-Location -LiteralPath ([IO.Path]::GetDirectoryName('%1')); wsl.exe -e bash ('./' + [IO.Path]::GetFileName('%1'))\"".to_string()),
                    None,
                ),
                _ => (DoubleClickMode::OpenInEditor, None, None),
            };

            ExtensionConfig {
                enabled: true,
                extension: ext.to_string(),
                prog_id: prog_id.to_string(),
                description: desc.to_string(),
                icon_name: icon.to_string(),
                reset_existing: reset,
                reset_user_choice: user_choice_resets.contains(&ext),
                mode,
                launcher_command: launcher,
                elevated_command: elevated,
                perceived_type: perceived.to_string(),
                content_type: content.to_string(),
                preserve_existing_properties: true,
                extra_verbs: Vec::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_only_machine_wide_paths() {
        let cfg = AppConfig::default();
        let base = app_data_dir();
        assert_eq!(cfg.icon_source_folder, base.join("icons").to_string_lossy());
        assert_eq!(
            cfg.pak_file_path,
            base.join("gallery.pak").to_string_lossy()
        );
    }

    #[test]
    fn default_rules_are_all_valid() {
        for rule in default_extensions() {
            assert!(
                crate::associations::validate_extension(&rule).is_ok(),
                "{}",
                rule.extension
            );
        }
    }
}
