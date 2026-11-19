//! egui front-end. Each module of the app is one tab, in its own file:
//! adding a customization module means adding an [`ActiveTab`] variant, its
//! sidebar subtitle and a `show_*_tab` method in a new file.
//!
//! Long operations (apply, rollback, icon pack build) run on a background
//! thread through [`jobs`], so the window never freezes.

mod associations_tab;
mod confirm;
mod gallery_tab;
mod history_panel;
mod jobs;
mod rollback_tab;
mod rule_status;
#[cfg(test)]
mod safety_tests;
mod sidebar;
mod status_bar;
#[cfg(test)]
mod test_support;
mod tweaks_tab;

use crate::associations::{
    default_backup_store_path, default_reg_backup_dir, load_backup_store,
    notify_shell_associations, AssocTarget, OperationReport,
};
use crate::detector::{
    has_wsl_distribution, is_windows_11_build, resolve_vscode_executable, windows_build,
};
use crate::gallery::{IconCategory, IconGallery};
use crate::models::{app_data_dir, AppConfig, BackupStore};
use crate::theme::apply_fluent_theme;
use crate::tweaks::{
    current_user_label, default_tweak_store_path, notify_explorer_settings, TweakBackupStore,
    TweakStatus, TweakTarget,
};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use jobs::RunningJob;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything the UI reads or writes outside its own memory. Production uses
/// [`AppEnv::system`]; tests point it at the HKCU sandbox and a temp folder,
/// so whole UI flows (apply, rollback…) can run for real without risk.
#[derive(Clone)]
pub(crate) struct AppEnv {
    pub assoc_target: Arc<dyn Fn() -> AssocTarget + Send + Sync>,
    pub backup_store_path: PathBuf,
    pub reg_backup_dir: PathBuf,
    pub config_path: PathBuf,
    /// Tells Explorer that associations changed.
    pub notify_shell: fn(),
    pub tweak_target: Arc<dyn Fn() -> TweakTarget + Send + Sync>,
    pub tweak_store_path: PathBuf,
    /// Tells Explorer and the taskbar that settings changed.
    pub notify_settings: fn(),
    pub is_windows_11: bool,
    /// Account whose HKCU the tweaks change.
    pub user_label: String,
    /// Operation log (one JSON line per registry job).
    pub history_path: PathBuf,
}

impl AppEnv {
    pub(crate) fn system() -> Self {
        Self {
            assoc_target: Arc::new(AssocTarget::system),
            backup_store_path: default_backup_store_path(),
            reg_backup_dir: default_reg_backup_dir(),
            config_path: app_data_dir().join("config.json"),
            notify_shell: notify_shell_associations,
            tweak_target: Arc::new(TweakTarget::current_user),
            tweak_store_path: default_tweak_store_path(),
            notify_settings: notify_explorer_settings,
            is_windows_11: windows_build().is_some_and(is_windows_11_build),
            user_label: current_user_label(),
            history_path: crate::history::default_history_path(),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum ActiveTab {
    Associations,
    Tweaks,
    Gallery,
    Rollback,
}

impl ActiveTab {
    /// Sidebar order.
    pub(crate) const ALL: [ActiveTab; 4] = [
        ActiveTab::Associations,
        ActiveTab::Tweaks,
        ActiveTab::Gallery,
        ActiveTab::Rollback,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            ActiveTab::Associations => "File Associations",
            ActiveTab::Tweaks => "Explorer & Taskbar",
            ActiveTab::Gallery => "Icon Gallery",
            ActiveTab::Rollback => "Selective Rollback",
        }
    }
}

pub struct WinTuneApp {
    env: AppEnv,
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
    /// Destructive action waiting for the user's OK.
    pending_confirm: Option<confirm::Confirm>,
    /// Live registry status of each rule, by extension.
    rule_statuses: rule_status::StatusMap,
    status_job: Option<rule_status::StatusJob>,
    status_refresh_needed: bool,
    /// Original states of the tweaks (tweaks-backups.json).
    tweak_store: TweakBackupStore,
    /// Set when tweaks-backups.json cannot be read: tweak changes are blocked.
    tweak_store_error: Option<String>,
    tweak_statuses: HashMap<&'static str, TweakStatus>,
    /// Checkbox states the user changed and has not applied yet.
    tweak_desired: HashMap<&'static str, bool>,
    /// Filter typed in the tweaks tab (title / description).
    tweak_search: String,
    /// Recent operations, newest first.
    history: Vec<crate::history::HistoryEntry>,
    show_history: bool,
    /// Set when config.json exists but is unreadable: saving is refused so
    /// the user's rules are never replaced by the defaults.
    config_error: Option<String>,
}

impl WinTuneApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let env = AppEnv::system();
        let (mut config, config_error) = Self::load_or_init_config(&env.config_path);
        if config.code_executable.is_empty() {
            if let Some(found) = resolve_vscode_executable(None) {
                config.code_executable = found;
            }
        }
        let mut app = Self::with_config(&cc.egui_ctx, config, has_wsl_distribution(), env);
        if let Some(e) = &config_error {
            app.set_status(format!(
                "Using the default rules: {} Saving is disabled until it is fixed.",
                e
            ));
        }
        app.config_error = config_error;
        app.reload_backup_store();
        app.reload_tweak_store();
        app.refresh_tweak_statuses();
        app.reload_history();
        app.request_status_refresh();
        app
    }

    /// Builds the app from an explicit config and environment (used by tests).
    fn with_config(
        ctx: &egui::Context,
        config: AppConfig,
        wsl_detected: bool,
        env: AppEnv,
    ) -> Self {
        apply_fluent_theme(ctx, config.theme);

        let mut gallery = IconGallery::default();
        gallery.reload(&config.icon_source_folder, &config.pak_file_path);

        Self {
            env,
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
            pending_confirm: None,
            rule_statuses: Default::default(),
            status_job: None,
            status_refresh_needed: false,
            tweak_store: TweakBackupStore::default(),
            tweak_store_error: None,
            tweak_statuses: HashMap::new(),
            tweak_desired: HashMap::new(),
            tweak_search: String::new(),
            history: Vec::new(),
            show_history: false,
            config_error: None,
        }
    }

    /// One frame of the whole UI.
    fn ui(&mut self, ctx: &egui::Context) {
        self.poll_job();
        self.pump_status_refresh(ctx);
        self.guard_close_while_busy(ctx);

        self.show_sidebar(ctx);
        self.show_status_bar(ctx);
        self.show_history_panel(ctx);
        self.show_confirm_dialog(ctx);

        match self.active_tab {
            ActiveTab::Associations => self.show_associations_tab(ctx),
            ActiveTab::Tweaks => self.show_tweaks_tab(ctx),
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
        match load_backup_store(&self.env.backup_store_path) {
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

    /// `what` names the items, e.g. "extension(s)" or "tweak(s)".
    fn show_report(&mut self, verb: &str, what: &str, report: &OperationReport) {
        let mut msg = format!("{} {} {}", verb, report.succeeded.len(), what);
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

    /// The config, plus an error when the file exists but cannot be used.
    fn load_or_init_config(path: &Path) -> (AppConfig, Option<String>) {
        match crate::persist::load_json::<AppConfig>(path) {
            Ok(config) => (config, None),
            Err(e) => (AppConfig::default(), Some(e)),
        }
    }

    fn save_config(&mut self) {
        if let Some(e) = &self.config_error {
            let msg = format!("Configuration not saved: {}", e);
            self.set_status(msg);
            return;
        }
        let path = self.env.config_path.clone();
        match crate::persist::save_json(&self.config, &path) {
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
    use super::test_support::*;
    use super::*;
    use crate::gallery::PakStats;
    use crate::tasks::BackgroundJob;
    use egui::{RawInput, ViewportCommand};
    use std::time::{Duration, Instant};

    #[test]
    fn every_tab_renders_without_panicking() {
        let icons = TempIcons::new(&["rust", "python"]);
        let ctx = egui::Context::default();
        let mut app = icons.app(&ctx);

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
        let mut app = icons.app_with(&ctx, config);

        for tab in ActiveTab::ALL {
            app.active_tab = tab;
            frame(&mut app, &ctx, RawInput::default());
        }
    }

    #[test]
    fn closing_during_a_job_is_deferred_until_it_finishes() {
        let icons = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = icons.app(&ctx);
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
        let mut app = icons.app(&ctx);

        let out = frame(&mut app, &ctx, close_request());

        assert!(!commands(&out).contains(&ViewportCommand::CancelClose));
    }

    #[test]
    fn building_the_pak_runs_in_background_then_reloads_the_gallery() {
        let icons = TempIcons::new(&["rust", "python"]);
        let ctx = egui::Context::default();
        let mut app = icons.app(&ctx);

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
        let mut app = icons.app(&ctx);
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
