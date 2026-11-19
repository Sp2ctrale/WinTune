//! egui front-end. Each module of the app is one tab, in its own file:
//! adding a customization module means adding an [`ActiveTab`] variant, its
//! sidebar subtitle and a `show_*_tab` method in a new file.
//!
//! Long operations (apply, rollback, icon pack build) run on a background
//! thread through [`jobs`], so the window never freezes.

mod associations_tab;
mod gallery_tab;
mod jobs;
mod rollback_tab;
mod sidebar;
mod status_bar;

use crate::associations::{default_backup_store_path, load_backup_store, OperationReport};
use crate::detector::{has_wsl_distribution, resolve_vscode_executable};
use crate::gallery::{IconCategory, IconGallery};
use crate::models::{app_data_dir, AppConfig, BackupStore};
use crate::theme::apply_fluent_theme;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use jobs::RunningJob;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum ActiveTab {
    Associations,
    Gallery,
    Rollback,
}

impl ActiveTab {
    /// Sidebar order.
    pub(crate) const ALL: [ActiveTab; 3] = [
        ActiveTab::Associations,
        ActiveTab::Gallery,
        ActiveTab::Rollback,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            ActiveTab::Associations => "File Associations",
            ActiveTab::Gallery => "Icon Gallery",
            ActiveTab::Rollback => "Selective Rollback",
        }
    }
}

pub struct WinTuneApp {
    config: AppConfig,
    gallery: IconGallery,
    backup_store: BackupStore,
    active_tab: ActiveTab,
    selected_ext_index: usize,
    ext_search_query: String,
    gallery_search_query: String,
    gallery_category: IconCategory,
    assign_icon_to_selected_ext: bool,
    rollback_selection: HashSet<String>,
    status_message: String,
    /// Full error / warning list of the last operation (status-bar tooltip).
    status_details: String,
    /// Set when backups.json cannot be read: registry changes are blocked so
    /// the next save cannot overwrite the recorded snapshots.
    backup_store_error: Option<String>,
    wsl_detected: bool,
    new_ext_input: String,
    logo_texture: Option<TextureHandle>,
    /// At most one long operation at a time.
    job: Option<RunningJob>,
    /// The user asked to close while a job was running: close when it ends.
    close_when_idle: bool,
}

impl WinTuneApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut config = Self::load_or_init_config();
        if config.code_executable.is_empty() {
            if let Some(found) = resolve_vscode_executable(None) {
                config.code_executable = found;
            }
        }
        let mut app = Self::with_config(&cc.egui_ctx, config, has_wsl_distribution());
        app.reload_backup_store();
        app
    }

    /// Builds the app without reading `%ProgramData%` (used by tests too).
    fn with_config(ctx: &egui::Context, config: AppConfig, wsl_detected: bool) -> Self {
        apply_fluent_theme(ctx, config.theme);

        let mut gallery = IconGallery::default();
        gallery.reload(&config.icon_source_folder, &config.pak_file_path);

        Self {
            config,
            gallery,
            backup_store: BackupStore::default(),
            active_tab: ActiveTab::Associations,
            selected_ext_index: 0,
            ext_search_query: String::new(),
            gallery_search_query: String::new(),
            gallery_category: IconCategory::FilesOnly,
            assign_icon_to_selected_ext: false,
            rollback_selection: HashSet::new(),
            status_message: "Ready.".to_string(),
            status_details: String::new(),
            backup_store_error: None,
            wsl_detected,
            new_ext_input: String::new(),
            logo_texture: Self::load_app_logo(ctx),
            job: None,
            close_when_idle: false,
        }
    }

    /// One frame of the whole UI.
    fn ui(&mut self, ctx: &egui::Context) {
        self.poll_job();
        self.guard_close_while_busy(ctx);

        self.show_sidebar(ctx);
        self.show_status_bar(ctx);

        match self.active_tab {
            ActiveTab::Associations => self.show_associations_tab(ctx),
            ActiveTab::Gallery => self.show_gallery_tab(ctx),
            ActiveTab::Rollback => self.show_rollback_tab(ctx),
        }
    }

    fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = message.into();
        self.status_details.clear();
    }

    fn reload_backup_store(&mut self) {
        match load_backup_store(&default_backup_store_path()) {
            Ok(store) => {
                self.backup_store = store;
                self.backup_store_error = None;
            }
            Err(e) => {
                self.backup_store = BackupStore::default();
                self.set_status(e.clone());
                self.backup_store_error = Some(e);
            }
        }
    }

    fn show_report(&mut self, verb: &str, report: &OperationReport) {
        let mut msg = format!("{} {} extension(s)", verb, report.succeeded.len());
        if let Some(first) = report.errors.first() {
            msg.push_str(&format!(
                ", {} error(s) (first: {})",
                report.errors.len(),
                first
            ));
        }
        if !report.warnings.is_empty() {
            msg.push_str(&format!(", {} warning(s)", report.warnings.len()));
        }
        msg.push('.');
        if report.reg_backup_file.is_some() {
            msg.push_str(" A .reg backup of the previous state was saved.");
        }

        let mut details: Vec<String> = Vec::new();
        details.extend(report.errors.iter().map(|e| format!("Error: {}", e)));
        details.extend(report.warnings.iter().map(|w| format!("Warning: {}", w)));
        if let Some(file) = &report.reg_backup_file {
            details.push(format!(".reg backup: {}", file.display()));
        }

        self.status_message = msg;
        self.status_details = details.join("\n");
    }

    fn load_app_logo(ctx: &egui::Context) -> Option<TextureHandle> {
        let png_bytes = include_bytes!("../../assets/WinTune.png");
        let img = image::load_from_memory(png_bytes).ok()?;
        let rgba = img.thumbnail(64, 64).to_rgba8();
        let (w, h) = rgba.dimensions();
        let color_img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
        Some(ctx.load_texture("wintune_logo", color_img, TextureOptions::LINEAR))
    }

    fn config_path() -> PathBuf {
        app_data_dir().join("config.json")
    }

    fn load_or_init_config() -> AppConfig {
        fs::read_to_string(Self::config_path())
            .ok()
            .and_then(|content| serde_json::from_str::<AppConfig>(&content).ok())
            .unwrap_or_default()
    }

    fn save_config(&mut self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let result = serde_json::to_string_pretty(&self.config)
            .map_err(|e| e.to_string())
            .and_then(|json| fs::write(&path, json).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.set_status("Configuration saved."),
            Err(e) => self.set_status(format!(
                "Cannot save configuration to {}: {}",
                path.display(),
                e
            )),
        }
    }

    /// Never let the window close in the middle of a registry write.
    fn guard_close_while_busy(&mut self, ctx: &egui::Context) {
        if self.is_busy() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_when_idle = true;
            self.set_status("An operation is running: WinTune will close as soon as it finishes.");
        }
        if self.close_when_idle && !self.is_busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for WinTuneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::jobs::RunningJob;
    use super::*;
    use crate::gallery::PakStats;
    use crate::tasks::BackgroundJob;
    use egui::{FullOutput, RawInput, ViewportCommand, ViewportEvent, ViewportId};
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Temp icon folder + pak path, removed on drop.
    struct TempIcons {
        dir: PathBuf,
    }

    impl TempIcons {
        fn new(names: &[&str]) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("wintune-ui-{}-{}", std::process::id(), nanos));
            fs::create_dir_all(dir.join("icons")).unwrap();
            for name in names {
                let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
                let img = ico::IconImage::from_rgba_data(32, 32, vec![200; 32 * 32 * 4]);
                icon.add_entry(ico::IconDirEntry::encode(&img).unwrap());
                let file =
                    fs::File::create(dir.join("icons").join(format!("{}.ico", name))).unwrap();
                icon.write(file).unwrap();
            }
            Self { dir }
        }

        fn config(&self) -> AppConfig {
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

    fn frame(app: &mut WinTuneApp, ctx: &egui::Context, input: RawInput) -> FullOutput {
        ctx.run(input, |ctx| app.ui(ctx))
    }

    fn close_request() -> RawInput {
        let mut input = RawInput::default();
        input
            .viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .events
            .push(ViewportEvent::Close);
        input
    }

    fn commands(out: &FullOutput) -> Vec<ViewportCommand> {
        out.viewport_output
            .get(&ViewportId::ROOT)
            .map(|v| v.commands.clone())
            .unwrap_or_default()
    }

    fn run_until_idle(app: &mut WinTuneApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.is_busy() && Instant::now() < deadline {
            frame(app, ctx, RawInput::default());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!app.is_busy(), "job did not finish");
    }

    #[test]
    fn every_tab_renders_without_panicking() {
        let icons = TempIcons::new(&["rust", "python"]);
        let ctx = egui::Context::default();
        let mut app = WinTuneApp::with_config(&ctx, icons.config(), false);

        for tab in ActiveTab::ALL {
            app.active_tab = tab;
            for _ in 0..3 {
                frame(&mut app, &ctx, RawInput::default());
            }
        }
    }

    #[test]
    fn tabs_render_with_no_rules_and_no_icons() {
        let icons = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut config = icons.config();
        config.extensions.clear();
        let mut app = WinTuneApp::with_config(&ctx, config, false);

        for tab in ActiveTab::ALL {
            app.active_tab = tab;
            frame(&mut app, &ctx, RawInput::default());
        }
    }

    #[test]
    fn closing_during_a_job_is_deferred_until_it_finishes() {
        let icons = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = WinTuneApp::with_config(&ctx, icons.config(), false);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                release_rx.recv().unwrap();
                Ok(PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));

        let out = frame(&mut app, &ctx, close_request());
        assert!(commands(&out).contains(&ViewportCommand::CancelClose));

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut closed = false;
        while !closed && Instant::now() < deadline {
            closed = commands(&frame(&mut app, &ctx, RawInput::default()))
                .contains(&ViewportCommand::Close);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(closed, "window was never closed after the job ended");
    }

    #[test]
    fn closing_when_idle_is_not_blocked() {
        let icons = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = WinTuneApp::with_config(&ctx, icons.config(), false);

        let out = frame(&mut app, &ctx, close_request());

        assert!(!commands(&out).contains(&ViewportCommand::CancelClose));
    }

    #[test]
    fn building_the_pak_runs_in_background_then_reloads_the_gallery() {
        let icons = TempIcons::new(&["rust", "python"]);
        let ctx = egui::Context::default();
        let mut app = WinTuneApp::with_config(&ctx, icons.config(), false);

        app.start_build_pak(&ctx);
        assert!(app.is_busy());
        run_until_idle(&mut app, &ctx);

        assert!(
            app.status_message.starts_with("Packed 2 icons"),
            "{}",
            app.status_message
        );
        assert!(Path::new(&app.config.pak_file_path).is_file());
        assert_eq!(app.gallery.names().len(), 2);
    }

    #[test]
    fn a_second_job_is_refused_while_one_is_running() {
        let icons = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = WinTuneApp::with_config(&ctx, icons.config(), false);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                release_rx.recv().unwrap();
                Ok(PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));

        app.start_build_pak(&ctx);

        assert!(
            app.status_message.contains("still running"),
            "{}",
            app.status_message
        );
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }
}
