//! egui front-end. Each module of the app is one tab, in its own file:
//! adding a customization module means adding an `ActiveTab` variant, its
//! navigation entry and a `show_*_tab` method in a new file.
//!
//! Long operations (apply, rollback, icon pack build) run on a background
//! thread through `jobs`, so the window never freezes.

mod associations_tab;
mod autosave;
mod confirm;
mod gallery_tab;
mod history_panel;
mod home;
mod icon_grid;
mod icon_picker;
mod jobs;
pub mod kit;
mod polish;
#[cfg(test)]
mod review2_tests;
mod rollback_tab;
mod rule_status;
#[cfg(test)]
mod safety_tests;
mod settings;
mod shell;
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
use crate::i18n::Tr;
use crate::models::{app_data_dir, AppConfig, BackupStore};
use crate::theme::ThemeMode;
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
    /// Folder holding config.json, backups and the history.
    pub data_dir: PathBuf,
    /// Opens a folder in Explorer.
    pub open_folder: Arc<dyn Fn(&Path) + Send + Sync>,
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
            data_dir: app_data_dir(),
            open_folder: Arc::new(|path| {
                // Nothing to report to from here: a failed spawn is ignored.
                let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
            }),
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum ActiveTab {
    Home,
    Associations,
    Tweaks,
    StartPrivacy,
    Gallery,
    History,
    Rollback,
    Settings,
    /// Not built yet: shown in the navigation, disabled.
    ContextMenu,
}

impl ActiveTab {
    /// Every page that can be opened, in navigation order (the tests walk it).
    #[cfg(test)]
    pub(crate) const ALL: [ActiveTab; 8] = [
        ActiveTab::Home,
        ActiveTab::Associations,
        ActiveTab::Tweaks,
        ActiveTab::StartPrivacy,
        ActiveTab::Gallery,
        ActiveTab::History,
        ActiveTab::Rollback,
        ActiveTab::Settings,
    ];

    /// English module label written to history.jsonl (not a navigation
    /// label: those are translated). Empty for pages that are not modules.
    pub(crate) fn title(self) -> &'static str {
        match self {
            ActiveTab::Associations => "File Associations",
            ActiveTab::Tweaks => "Explorer & Taskbar",
            ActiveTab::StartPrivacy => "Start & Privacy",
            ActiveTab::Gallery => "Icon Gallery",
            ActiveTab::Rollback => "Selective Rollback",
            ActiveTab::Home | ActiveTab::History | ActiveTab::Settings | ActiveTab::ContextMenu => {
                ""
            }
        }
    }

    /// Pages that draw their own action bar (with the job's progress while
    /// one runs). The shell draws the progress bar for the others.
    pub(super) fn owns_action_bar(self) -> bool {
        matches!(self, ActiveTab::Associations)
    }
}

/// What a finished job did, to word its toast.
#[derive(Clone, Copy)]
pub(super) enum ReportKind {
    AppliedRules,
    RestoredRules,
    AppliedTweaks,
    RestoredTweaks,
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
    /// The icon picker dialog, when open.
    icon_picker: Option<icon_picker::IconPicker>,
    rollback_selection: HashSet<String>,
    /// Transient messages, drawn bottom-right.
    toasts: kit::Toasts,
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
    tr: Tr,
    /// The theme actually shown this frame (Light or Dark, never System):
    /// what the old palette helpers receive.
    ui_theme: ThemeMode,
    /// Ctrl+F was pressed: focus the current tab's search box.
    focus_search: bool,
    /// Recent operations, newest first.
    history: Vec<crate::history::HistoryEntry>,
    /// Set when config.json exists but is unreadable: saving is refused so
    /// the user's rules are never replaced by the defaults.
    config_error: Option<String>,
    /// Debounced writes of config.json.
    autosave: autosave::Autosave,
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
        shell::round_borderless_window(cc);
        let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
        kit::fonts::install(&cc.egui_ctx, &std::path::Path::new(&windir).join("Fonts"));
        if let Some(e) = &config_error {
            let text = app.tr.t_args("notice.config_unreadable", &[e]);
            app.notify(kit::ToastKind::Error, text);
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
        kit::install_styles(ctx);
        kit::set_theme_mode(ctx, config.theme);
        let ui_theme = kit::resolved(config.theme, ctx);
        let tr = Tr::new(&config.language);
        let autosave = autosave::Autosave::new(&config);

        let mut gallery = IconGallery::default();
        gallery.reload(&config.icon_source_folder, &config.pak_file_path);

        Self {
            env,
            config,
            gallery,
            backup_store: BackupStore::default(),
            active_tab: ActiveTab::Home,
            selected_ext_index: 0,
            ext_search_query: String::new(),
            gallery_search_query: String::new(),
            gallery_category: IconCategory::FilesOnly,
            icon_picker: None,
            rollback_selection: HashSet::new(),
            toasts: kit::Toasts::default(),
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
            tr,
            ui_theme,
            focus_search: false,
            history: Vec::new(),
            config_error: None,
            autosave,
        }
    }

    /// One frame of the whole UI.
    fn ui(&mut self, ctx: &egui::Context) {
        self.ui_theme = kit::resolved(self.config.theme, ctx);
        self.poll_job();
        self.handle_shortcuts(ctx);
        self.pump_status_refresh(ctx);
        self.guard_close_while_busy(ctx);

        // Before the nav: the dialog claims the keyboard first (it is an Area,
        // so the panel layout does not depend on the order).
        self.show_confirm_dialog(ctx);
        self.show_icon_picker(ctx);
        self.show_caption(ctx);
        self.show_nav(ctx);
        if !self.active_tab.owns_action_bar() {
            self.job_progress_bar(ctx);
        }
        self.show_autosave_banner(ctx);

        match self.active_tab {
            ActiveTab::Home | ActiveTab::ContextMenu => self.show_home(ctx),
            ActiveTab::History => self.show_history_page(ctx),
            ActiveTab::Settings => self.show_settings(ctx),
            ActiveTab::Associations => self.show_associations_tab(ctx),
            ActiveTab::Tweaks | ActiveTab::StartPrivacy => self.show_tweaks_tab(ctx),
            ActiveTab::Gallery => self.show_gallery_tab(ctx),
            ActiveTab::Rollback => self.show_rollback_tab(ctx),
        }

        self.toasts.show(ctx, &self.tr.t("toast.clear_all"));
        // After every widget, so the resize cursor wins at the window edges.
        if let Some(direction) = kit::resize_edge(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
        // Last, so an edit made by a widget this frame is noticed this frame.
        self.pump_autosave(ctx);
    }

    /// Switches the language now and remembers it. An unshipped code is
    /// stored as the English fallback actually in use.
    pub(super) fn set_language(&mut self, code: &str) {
        self.tr = Tr::new(code);
        self.config.language = self.tr.code().to_string();
    }

    fn set_theme(&mut self, ctx: &egui::Context, mode: ThemeMode) {
        self.config.theme = mode;
        kit::set_theme_mode(ctx, mode);
        self.ui_theme = kit::resolved(mode, ctx);
    }

    fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    fn notify(&mut self, kind: kit::ToastKind, text: impl Into<String>) {
        self.toasts.push(kind, text);
    }

    /// `details` show on hover over the toast.
    fn notify_details(
        &mut self,
        kind: kit::ToastKind,
        text: impl Into<String>,
        details: impl Into<String>,
    ) {
        self.toasts.push_details(kind, text, details);
    }

    /// Text (and details) of the most recent toast, even after it expired.
    #[cfg(test)]
    fn last_notice(&self) -> String {
        self.toasts.latest().unwrap_or_default()
    }

    fn reload_backup_store(&mut self) {
        match load_backup_store(&self.env.backup_store_path) {
            Ok(store) => {
                self.backup_store = store;
                self.backup_store_error = None;
            }
            Err(e) => {
                self.backup_store = BackupStore::default();
                self.notify(kit::ToastKind::Error, e.clone());
                self.backup_store_error = Some(e);
            }
        }
    }

    /// Toast for a finished registry job: an error toast when something
    /// failed, with the full error / warning list on hover.
    fn show_report(&mut self, kind: ReportKind, report: &OperationReport) {
        let tr = self.tr;
        let count = report.succeeded.len();
        let mut msg = match kind {
            ReportKind::AppliedRules => tr.t_args("report.applied_rules", &[&count]),
            ReportKind::RestoredRules => tr.t_args("report.restored_rules", &[&count]),
            ReportKind::AppliedTweaks => tr.t_args("report.applied_tweaks", &[&count]),
            ReportKind::RestoredTweaks => tr.t_args("report.restored_tweaks", &[&count]),
        };
        if let Some(first) = report.errors.first() {
            msg.push_str(&tr.t_args("report.errors", &[&report.errors.len(), first]));
        }
        if !report.warnings.is_empty() {
            msg.push_str(&tr.t_args("report.warnings", &[&report.warnings.len()]));
        }
        msg.push('.');
        if report.reg_backup_file.is_some() {
            msg.push(' ');
            msg.push_str(&tr.t("report.reg_backup"));
        }

        let mut details: Vec<String> = Vec::new();
        details.extend(
            report
                .errors
                .iter()
                .map(|e| tr.t_args("report.detail_error", &[e])),
        );
        details.extend(
            report
                .warnings
                .iter()
                .map(|w| tr.t_args("report.detail_warning", &[w])),
        );
        if let Some(file) = &report.reg_backup_file {
            details.push(tr.t_args("report.detail_reg", &[&file.display()]));
        }

        let toast = if report.errors.is_empty() {
            kit::ToastKind::Success
        } else {
            kit::ToastKind::Error
        };
        self.notify_details(toast, msg, details.join("\n"));
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

    /// Saves config.json; refused while the file on disk is unreadable.
    fn try_save_config(&self) -> Result<(), String> {
        if let Some(e) = &self.config_error {
            return Err(e.clone());
        }
        let path = &self.env.config_path;
        // The persist error already names the file: it is the raw detail
        // shown after the translated summary.
        crate::persist::save_json(&self.config, path)
    }

    /// Never let the window close in the middle of a registry write.
    fn guard_close_while_busy(&mut self, ctx: &egui::Context) {
        if self.is_busy() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_when_idle = true;
            let text = self.tr.t("notice.close_after_job");
            self.notify(kit::ToastKind::Info, text);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.is_busy() {
            self.flush_config();
        }
        if self.close_when_idle && !self.is_busy() {
            self.flush_config();
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
    fn configs_without_language_load_as_english() {
        let c: AppConfig = serde_json::from_str(
            r#"{"theme":"Dark","code_executable":"","icon_source_folder":"","icon_destination_folder":"","pak_file_path":"","extensions":[]}"#,
        )
        .unwrap();
        assert_eq!(c.language, "en");
        assert_eq!(c.theme, crate::theme::ThemeMode::Dark);
    }

    #[test]
    fn an_unshipped_language_renders_in_english() {
        let icons = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut config = icons.config();
        config.language = "de".into();
        let app = icons.app_with(&ctx, config);
        assert_eq!(app.tr.code(), "en");
    }

    #[test]
    fn set_language_switches_immediately() {
        let icons = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = icons.app(&ctx);
        assert_eq!(app.tr.code(), "en");
        app.set_language("fr");
        assert_eq!(app.tr.code(), "fr");
        assert_eq!(app.config.language, "fr");
        frame(&mut app, &ctx, RawInput::default());
        app.set_language("de");
        assert_eq!(app.config.language, "en");
    }

    #[test]
    fn the_app_renders_a_frame_in_every_theme_mode() {
        use crate::theme::ThemeMode;
        let icons = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let mut config = icons.config();
            config.theme = mode;
            let mut app = icons.app_with(&ctx, config);
            frame(&mut app, &ctx, RawInput::default());
            assert_ne!(app.ui_theme, ThemeMode::System);
            if mode != ThemeMode::System {
                assert_eq!(app.ui_theme, mode);
            }
        }
    }

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
            app.last_notice().starts_with("Packed 2 icons"),
            "{}",
            app.last_notice()
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
            app.last_notice().contains("still running"),
            "{}",
            app.last_notice()
        );
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }
}
