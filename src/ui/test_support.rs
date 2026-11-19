//! Test fixture shared by the UI tests: an app wired to a temp folder and
//! the HKCU registry sandbox (never HKLM, HKU or %ProgramData%).

use super::{AppEnv, WinTuneApp};
use crate::associations::AssocTarget;
use crate::models::AppConfig;
use crate::regsnap::test_support::Sandbox;
use eframe::egui::{self, FullOutput, RawInput, ViewportCommand, ViewportEvent, ViewportId};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Temp folder (icons, config, backups) + HKCU registry sandbox. The app
/// built from it never touches HKLM, HKU or %ProgramData%.
pub(super) struct TempIcons {
    pub(super) dir: PathBuf,
    pub(super) sandbox: Sandbox,
}

pub(super) const TEST_HIVE: &str = "S-1-5-21-test";

impl TempIcons {
    pub(super) fn env(&self) -> AppEnv {
        let base = self.sandbox.path.clone();
        AppEnv {
            assoc_target: Arc::new(move || AssocTarget {
                machine_root: RegKey::predef(HKEY_CURRENT_USER),
                machine_root_name: "HKEY_CURRENT_USER".into(),
                machine_classes: format!("{}\\Machine\\Classes", base),
                users_root: RegKey::predef(HKEY_CURRENT_USER),
                users_root_name: "HKEY_CURRENT_USER".into(),
                user_hives: vec![format!("{}\\Users\\{}", base, TEST_HIVE)],
            }),
            backup_store_path: self.dir.join("backups.json"),
            reg_backup_dir: self.dir.join("reg-backups"),
            config_path: self.dir.join("config.json"),
            notify_shell: || {},
            tweak_target: {
                let base = format!("{}\\HKCU", self.sandbox.path);
                Arc::new(move || crate::tweaks::TweakTarget {
                    root: RegKey::predef(HKEY_CURRENT_USER),
                    root_name: "HKEY_CURRENT_USER".into(),
                    base: base.clone(),
                })
            },
            tweak_store_path: self.dir.join("tweaks-backups.json"),
            notify_settings: || {},
            is_windows_11: true,
            user_label: "TEST\\user".into(),
            history_path: self.dir.join("history.jsonl"),
        }
    }

    pub(super) fn app(&self, ctx: &egui::Context) -> WinTuneApp {
        self.app_with(ctx, self.config())
    }

    pub(super) fn app_with(&self, ctx: &egui::Context, config: AppConfig) -> WinTuneApp {
        WinTuneApp::with_config(ctx, config, false, self.env())
    }

    /// Path of a key under the sandbox's machine `Classes`.
    pub(super) fn classes(&self, rel: &str) -> String {
        format!("{}\\Machine\\Classes\\{}", self.sandbox.path, rel)
    }

    pub(super) fn new(names: &[&str]) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wintune-ui-{}-{}", std::process::id(), nanos));
        fs::create_dir_all(dir.join("icons")).unwrap();
        for name in names {
            let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
            let img = ico::IconImage::from_rgba_data(32, 32, vec![200; 32 * 32 * 4]);
            icon.add_entry(ico::IconDirEntry::encode(&img).unwrap());
            let file = fs::File::create(dir.join("icons").join(format!("{}.ico", name))).unwrap();
            icon.write(file).unwrap();
        }
        Self {
            dir,
            sandbox: Sandbox::new(),
        }
    }

    pub(super) fn config(&self) -> AppConfig {
        let path = |p: &Path| p.to_string_lossy().into_owned();
        AppConfig {
            icon_source_folder: path(&self.dir.join("icons")),
            icon_destination_folder: path(&self.dir.join("installed")),
            pak_file_path: path(&self.dir.join("gallery.pak")),
            ..AppConfig::default()
        }
    }
}

impl Drop for TempIcons {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub(super) fn frame(app: &mut WinTuneApp, ctx: &egui::Context, input: RawInput) -> FullOutput {
    ctx.run(input, |ctx| app.ui(ctx))
}

pub(super) fn close_request() -> RawInput {
    let mut input = RawInput::default();
    input
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    input
}

pub(super) fn commands(out: &FullOutput) -> Vec<ViewportCommand> {
    out.viewport_output
        .get(&ViewportId::ROOT)
        .map(|v| v.commands.clone())
        .unwrap_or_default()
}

/// A valid "open in editor" rule using the fixture icon `foo`.
pub(super) fn rule(ext: &str) -> crate::models::ExtensionConfig {
    let stem = ext.trim_start_matches('.').to_uppercase();
    crate::models::ExtensionConfig {
        enabled: true,
        extension: ext.to_string(),
        prog_id: format!("Custom{}File", stem),
        description: format!("{} File", stem),
        icon_name: "foo".to_string(),
        reset_existing: false,
        reset_user_choice: false,
        mode: crate::models::DoubleClickMode::OpenInEditor,
        launcher_command: None,
        elevated_command: None,
        perceived_type: "text".to_string(),
        content_type: "text/plain".to_string(),
        preserve_existing_properties: true,
        extra_verbs: Vec::new(),
    }
}

impl TempIcons {
    /// App whose rules are exactly `exts` and whose editor is set, ready to
    /// apply into the sandbox.
    pub(super) fn app_with_rules(&self, ctx: &egui::Context, exts: &[&str]) -> WinTuneApp {
        let mut config = self.config();
        config.code_executable = r"C:\Editor\editor.exe".to_string();
        config.extensions = exts.iter().map(|e| rule(e)).collect();
        self.app_with(ctx, config)
    }

    /// Path of a key in the sandbox's per-user (HKCU) area.
    pub(super) fn hkcu(&self, rel: &str) -> String {
        format!("{}\\HKCU\\{}", self.sandbox.path, rel)
    }

    pub(super) fn key_exists(&self, path: &str) -> bool {
        self.sandbox.root.open_subkey(path).is_ok()
    }
}

pub(super) fn run_until_idle(app: &mut WinTuneApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.is_busy() && Instant::now() < deadline {
        frame(app, ctx, RawInput::default());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.is_busy(), "job did not finish");
}
