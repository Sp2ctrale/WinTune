//! egui front-end. Each module of the app is one tab, in its own file:
//! adding a customization module means adding an `ActiveTab` variant, its
//! navigation entry and a `show_*_tab` method in a new file.
//!
//! Long operations (apply, rollback, icon pack build) run on a background
//! thread through `jobs`, so the window never freezes.

mod associations_tab;
mod autosave;
mod confirm;
mod context_menu_existing;
mod context_menu_tab;
mod environment_state;
mod environment_tab;
mod gallery_tab;
mod history_panel;
mod home;
mod icon_grid;
mod icon_picker;
mod jobs;
pub mod kit;
mod menu_existing_state;
mod mouse_keyboard_state;
mod mouse_keyboard_tab;
mod originals;
mod polish;
#[cfg(test)]
mod review2_tests;
mod rule_status;
#[cfg(test)]
mod safety_tests;
mod settings;
mod shell;
mod startup_state;
mod startup_tab;
#[cfg(test)]
mod test_support;
mod tweaks_tab;
mod visual_effects_state;

use crate::associations::{
    default_backup_store_path, default_reg_backup_dir, load_backup_store,
    notify_shell_associations, AssocTarget, OperationReport,
};
use crate::backups::RegTarget;
use crate::context_menu::{
    default_menu_store_path, entry_status, fileinfo, load_menu_store, orphan_keys, EntryStatus,
    MenuBackupStore, MenuEntry, MenuTarget, Programs, Scan,
};
use crate::detector::{
    has_wsl_distribution, is_windows_11_build, resolve_vscode_executable, windows_build,
};
use crate::environment::{
    broadcast_environment, default_env_store_path, EnvBackupStore, EnvScan, EnvTarget,
};
use crate::gallery::{IconCategory, IconGallery};
use crate::i18n::{messages, Tr};
use crate::models::ThemeMode;
use crate::models::{app_data_dir, AppConfig, BackupStore};
use crate::mouse_keyboard::{
    default_mk_store_path, session_account_check, AccountCheck, InputApi, MkBackupStore, MkScan,
    SettingId, SpiInput, Wanted,
};
use crate::persist::LoadError;
use crate::startup::{
    default_startup_store_path, Source, StartupBackupStore, StartupScan, StartupTarget,
};
use crate::tweaks::{
    current_user_label, default_tweak_store_path, notify_explorer_settings, TweakBackupStore,
    TweakStatus, TweakTarget,
};
use crate::visual_effects::{
    default_ve_store_path, EffectId, EffectsApi, SettingValue as VeValue, SpiEffects,
    VeBackupStore, VeScan,
};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use jobs::RunningJob;
use std::collections::{BTreeMap, HashMap};
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
    /// Where the context menu entries are written (HKCU).
    pub menu_target: Arc<dyn Fn() -> MenuTarget + Send + Sync>,
    pub menu_store_path: PathBuf,
    /// Where the startup switches are read (HKLM, folders) and written (HKCU).
    pub startup_target: Arc<dyn Fn() -> StartupTarget + Send + Sync>,
    pub startup_store_path: PathBuf,
    /// Whether a per-user switch turns items of a source off.
    pub startup_switch_works: fn(Source) -> bool,
    /// Where the environment variables are read (HKCU, HKLM) and written (HKCU).
    pub env_target: Arc<dyn Fn() -> EnvTarget + Send + Sync>,
    pub env_store_path: PathBuf,
    /// Tells running programs that the environment changed.
    pub notify_environment: Arc<dyn Fn() + Send + Sync>,
    /// A variable of this process: the fallback of `%name%` expansion.
    pub process_var: fn(&str) -> Option<String>,
    /// Where each preset program of the context menu lives.
    pub programs: Programs,
    /// `%SystemRoot%`: menu entries whose program is under it are Windows'
    /// own and never listed.
    pub windows_dir: PathBuf,
    /// Resolves an `@file,-id` menu label (`fileinfo::resolve_indirect`).
    pub resolve_indirect: fn(&str) -> Option<String>,
    /// A program's product name (`fileinfo::product_name`).
    pub product_name: fn(&Path) -> Option<String>,
    /// Reads and writes the mouse and keyboard settings.
    pub input: Arc<dyn InputApi>,
    /// Whether WinTune runs as the user signed in to this session.
    pub input_account: Arc<dyn Fn() -> AccountCheck + Send + Sync>,
    pub mk_store_path: PathBuf,
    /// Where the `.reg` net reads `Control Panel` (read only).
    pub mk_reg_source: Arc<dyn Fn() -> RegTarget + Send + Sync>,
    /// Reads and writes the visual effects. The account check and the
    /// `.reg` source are Mouse & keyboard's (`input_account`,
    /// `mk_reg_source`).
    pub effects: Arc<dyn EffectsApi>,
    pub ve_store_path: PathBuf,
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
            menu_target: Arc::new(MenuTarget::current_user),
            menu_store_path: default_menu_store_path(),
            startup_target: Arc::new(StartupTarget::current_user),
            startup_store_path: default_startup_store_path(),
            startup_switch_works: Source::per_user_switch_works,
            env_target: Arc::new(EnvTarget::current_user),
            env_store_path: default_env_store_path(),
            notify_environment: Arc::new(broadcast_environment),
            process_var: crate::environment::process_var,
            programs: Programs::detect(),
            windows_dir: std::env::var_os("SystemRoot")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows")),
            resolve_indirect: fileinfo::resolve_indirect,
            product_name: fileinfo::product_name,
            input: Arc::new(SpiInput),
            input_account: Arc::new(session_account_check),
            mk_store_path: default_mk_store_path(),
            mk_reg_source: Arc::new(RegTarget::current_user),
            effects: Arc::new(SpiEffects),
            ve_store_path: default_ve_store_path(),
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
    Settings,
    ContextMenu,
    Startup,
    Environment,
    MouseKeyboard,
}

impl ActiveTab {
    /// Every page that can be opened, in navigation order (the tests walk it).
    #[cfg(test)]
    pub(crate) const ALL: [ActiveTab; 11] = [
        ActiveTab::Home,
        ActiveTab::Associations,
        ActiveTab::Tweaks,
        ActiveTab::StartPrivacy,
        ActiveTab::ContextMenu,
        ActiveTab::Startup,
        ActiveTab::Environment,
        ActiveTab::MouseKeyboard,
        ActiveTab::Gallery,
        ActiveTab::History,
        ActiveTab::Settings,
    ];

    /// English module label stored in history.jsonl (not a navigation
    /// label: those are translated). `None` for pages with no module.
    pub(crate) fn history_module(self) -> Option<&'static str> {
        match self {
            ActiveTab::Associations => Some("File associations"),
            ActiveTab::Tweaks => Some("Explorer & Taskbar"),
            ActiveTab::StartPrivacy => Some("Start & Privacy"),
            ActiveTab::ContextMenu => Some("Context menu"),
            ActiveTab::Startup => Some("Startup apps"),
            ActiveTab::Environment => Some("Environment variables"),
            ActiveTab::MouseKeyboard => Some("Mouse & keyboard"),
            ActiveTab::Home | ActiveTab::Gallery | ActiveTab::History | ActiveTab::Settings => None,
        }
    }

    /// Pages that draw their own action bar (with the job's progress while
    /// one runs). The shell draws the progress bar for the others.
    pub(super) fn owns_action_bar(self) -> bool {
        matches!(
            self,
            ActiveTab::Associations
                | ActiveTab::Tweaks
                | ActiveTab::StartPrivacy
                | ActiveTab::ContextMenu
                | ActiveTab::Startup
                | ActiveTab::Environment
                | ActiveTab::MouseKeyboard
        )
    }
}

/// What a finished job did, to word its toast.
#[derive(Clone, Copy)]
pub(super) enum ReportKind {
    AppliedRules,
    RestoredRules,
    AppliedTweaks,
    RestoredTweaks,
    AppliedMenu,
    RestoredMenu,
    AppliedStartup,
    RestoredStartup,
    AppliedEnvironment,
    RestoredEnvironment,
    AppliedMouseKeyboard,
    RestoredMouseKeyboard,
    AppliedVisualEffects,
    RestoredVisualEffects,
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
    /// Transient messages, drawn bottom-right.
    toasts: kit::Toasts,
    /// Set when backups.json cannot be read: registry changes are blocked so
    /// the next save cannot overwrite the recorded snapshots.
    backup_store_error: Option<LoadError>,
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
    tweak_store_error: Option<LoadError>,
    tweak_statuses: HashMap<&'static str, TweakStatus>,
    /// Checkbox states the user changed and has not applied yet.
    tweak_desired: HashMap<&'static str, bool>,
    /// Filter typed in the tweaks tab (title / description).
    tweak_search: String,
    /// Original states of the context menu keys (context-menu-backups.json).
    menu_store: MenuBackupStore,
    /// Set when context-menu-backups.json cannot be read: menu changes are blocked.
    menu_store_error: Option<LoadError>,
    /// Live registry state of each entry of the current list, by entry id.
    menu_statuses: HashMap<String, EntryStatus>,
    /// `WinTune.*` keys in the registry that no entry of the list owns.
    menu_orphans: Vec<String>,
    /// Entry open in the editor, by id.
    selected_menu_entry: Option<String>,
    /// Entries already in the right-click menu, as last read; `None` until
    /// the first scan ends. Kept while a new scan runs.
    menu_scan: Option<Scan>,
    /// The scan running now, if any (the page shows it as loading).
    menu_scan_job: Option<menu_existing_state::ScanJob>,
    menu_scan_needed: bool,
    /// Hide / rename changes not applied yet, by `ExistingEntry::id`.
    menu_pending: BTreeMap<String, menu_existing_state::PendingExisting>,
    /// The rename dialog, while open: entry id and the text being edited.
    menu_rename: Option<(String, String)>,
    /// Filter typed in the Other programs section (label / origin).
    menu_search: String,
    /// Original switches of the startup apps (startup-backups.json).
    startup_store: StartupBackupStore,
    /// Set when startup-backups.json cannot be read: startup changes are blocked.
    startup_store_error: Option<LoadError>,
    /// Startup apps as last read; `None` until the first scan ends. Kept
    /// while a new scan runs.
    startup_scan: Option<StartupScan>,
    startup_scan_job: Option<startup_state::ScanJob>,
    startup_scan_needed: bool,
    /// The wanted "on" state of the apps the user toggled and has not
    /// applied yet, by `StartupItem::id`.
    startup_pending: BTreeMap<String, bool>,
    /// Filter typed in the Startup apps page (name / program).
    startup_search: String,
    /// Original variables (environment-backups.json).
    env_store: EnvBackupStore,
    /// Set when environment-backups.json cannot be read: changes are blocked.
    env_store_error: Option<LoadError>,
    /// Variables as last read; `None` until the first scan ends. Kept while a
    /// new scan runs.
    env_scan: Option<EnvScan>,
    env_scan_job: Option<environment_state::EnvScanJob>,
    env_scan_needed: bool,
    /// Edits not applied yet.
    env_pending: environment_state::EnvPending,
    /// Whether a `Path` folder exists, by expanded text: the last scan's
    /// answers, plus folders checked since.
    env_folders: HashMap<String, bool>,
    /// Filter typed in the Environment variables page.
    env_search: String,
    /// The variable or folder dialog, while open.
    env_dialog: Option<environment_state::EnvDialog>,
    /// The folder typed in the Path section's add box.
    env_new_entry: String,
    /// Original mouse and keyboard settings (mouse-keyboard-backups.json).
    mk_store: MkBackupStore,
    /// Set when mouse-keyboard-backups.json cannot be read: changes are blocked.
    mk_store_error: Option<LoadError>,
    /// Settings as last read; `None` until the first scan ends. Kept while a
    /// new scan runs.
    mk_scan: Option<MkScan>,
    mk_scan_job: Option<mouse_keyboard_state::MkScanJob>,
    mk_scan_needed: bool,
    /// Choices not applied yet, by setting.
    mk_pending: BTreeMap<SettingId, Wanted>,
    /// Filter typed in the Mouse & keyboard page (titles).
    mk_search: String,
    /// The double-click test target of the Mouse & keyboard page.
    mk_click_test: mouse_keyboard_tab::ClickTest,
    /// The text typed in the key repeat test field.
    mk_key_test: String,
    /// Original visual effects (visual-effects-backups.json).
    ve_store: VeBackupStore,
    /// Set when visual-effects-backups.json cannot be read: changes are
    /// blocked.
    ve_store_error: Option<LoadError>,
    /// Effects as last read; `None` until the first scan ends. Kept while a
    /// new scan runs.
    ve_scan: Option<VeScan>,
    ve_scan_job: Option<visual_effects_state::VeScanJob>,
    ve_scan_needed: bool,
    /// Choices not applied yet, by effect.
    ve_pending: BTreeMap<EffectId, VeValue>,
    /// Filter typed in the Visual effects page (titles). Read by the page
    /// (Task 5).
    #[allow(dead_code)]
    ve_search: String,
    tr: Tr,
    /// The theme actually shown this frame (Light or Dark, never System):
    /// what the old palette helpers receive.
    ui_theme: ThemeMode,
    /// Ctrl+F was pressed: focus the current tab's search box.
    focus_search: bool,
    /// Set when the page changes: the next frame closes open popups (egui
    /// keeps one popup id, so a `…` menu would reappear when its page comes
    /// back). `open_page` has no `ctx`, so the frame does it.
    close_popups_next_frame: bool,
    /// Recent operations, newest first.
    history: Vec<crate::history::HistoryEntry>,
    /// Module filter of the History page.
    history_filter: history_panel::HistoryFilter,
    /// "Show all" was clicked: the Operations list is not cut to the 20
    /// most recent entries. Reset when the filter changes.
    history_show_all: bool,
    /// Set when config.json exists but is unreadable: saving is refused so
    /// the user's rules are never replaced by the defaults.
    config_error: Option<LoadError>,
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
        app.adopt_config_error(config_error);
        app.reload_backup_store();
        app.reload_tweak_store();
        app.refresh_tweak_statuses();
        app.reload_menu_store();
        app.refresh_menu_statuses();
        app.reload_history();
        app.request_status_refresh();
        app.request_menu_scan();
        app.reload_startup_store();
        app.request_startup_scan();
        app.reload_env_store();
        app.request_env_scan();
        app.reload_mk_store();
        app.request_mk_scan();
        app.reload_ve_store();
        app.request_ve_scan();
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
            menu_store: MenuBackupStore::default(),
            menu_store_error: None,
            menu_statuses: HashMap::new(),
            menu_orphans: Vec::new(),
            selected_menu_entry: None,
            menu_scan: None,
            menu_scan_job: None,
            menu_scan_needed: false,
            menu_pending: BTreeMap::new(),
            menu_rename: None,
            menu_search: String::new(),
            startup_store: StartupBackupStore::default(),
            startup_store_error: None,
            startup_scan: None,
            startup_scan_job: None,
            startup_scan_needed: false,
            startup_pending: BTreeMap::new(),
            startup_search: String::new(),
            env_store: EnvBackupStore::default(),
            env_store_error: None,
            env_scan: None,
            env_scan_job: None,
            env_scan_needed: false,
            env_pending: Default::default(),
            env_folders: HashMap::new(),
            env_search: String::new(),
            env_dialog: None,
            env_new_entry: String::new(),
            mk_store: MkBackupStore::default(),
            mk_store_error: None,
            mk_scan: None,
            mk_scan_job: None,
            mk_scan_needed: false,
            mk_pending: BTreeMap::new(),
            mk_search: String::new(),
            mk_click_test: Default::default(),
            mk_key_test: String::new(),
            ve_store: VeBackupStore::default(),
            ve_store_error: None,
            ve_scan: None,
            ve_scan_job: None,
            ve_scan_needed: false,
            ve_pending: BTreeMap::new(),
            ve_search: String::new(),
            tr,
            ui_theme,
            focus_search: false,
            close_popups_next_frame: false,
            history: Vec::new(),
            history_filter: history_panel::HistoryFilter::All,
            history_show_all: false,
            config_error: None,
            autosave,
        }
    }

    /// One frame of the whole UI.
    fn ui(&mut self, ctx: &egui::Context) {
        self.ui_theme = kit::resolved(self.config.theme, ctx);
        if std::mem::take(&mut self.close_popups_next_frame) {
            ctx.memory_mut(|m| m.close_popup());
        }
        self.poll_job();
        self.handle_shortcuts(ctx);
        self.pump_status_refresh(ctx);
        self.pump_menu_scan(ctx);
        self.pump_startup_scan(ctx);
        self.pump_env_scan(ctx);
        self.pump_mk_scan(ctx);
        self.pump_ve_scan(ctx);
        self.guard_close_while_busy(ctx);

        // Before the nav: the dialog claims the keyboard first (it is an Area,
        // so the panel layout does not depend on the order).
        self.show_confirm_dialog(ctx);
        self.show_icon_picker(ctx);
        self.show_menu_rename_dialog(ctx);
        self.show_env_dialog(ctx);
        self.show_caption(ctx);
        self.show_nav(ctx);
        if !self.active_tab.owns_action_bar() {
            self.job_progress_bar(ctx);
        }
        self.show_autosave_banner(ctx);

        match self.active_tab {
            ActiveTab::Home => self.show_home(ctx),
            ActiveTab::History => self.show_history_page(ctx),
            ActiveTab::Settings => self.show_settings(ctx),
            ActiveTab::Associations => self.show_associations_tab(ctx),
            ActiveTab::Tweaks | ActiveTab::StartPrivacy => self.show_tweaks_tab(ctx),
            ActiveTab::Gallery => self.show_gallery_tab(ctx),
            ActiveTab::ContextMenu => self.show_context_menu_tab(ctx),
            ActiveTab::Startup => self.show_startup_tab(ctx),
            ActiveTab::Environment => self.show_environment_tab(ctx),
            ActiveTab::MouseKeyboard => self.show_mouse_keyboard_tab(ctx),
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
                let text = messages::load_error(self.tr, &e);
                self.notify(kit::ToastKind::Error, text);
                self.backup_store_error = Some(e);
            }
        }
    }

    pub(super) fn reload_menu_store(&mut self) {
        match load_menu_store(&self.env.menu_store_path) {
            Ok(store) => {
                self.menu_store = store;
                self.menu_store_error = None;
            }
            Err(e) => {
                self.menu_store = MenuBackupStore::default();
                let text = messages::load_error(self.tr, &e);
                self.notify(kit::ToastKind::Error, text);
                self.menu_store_error = Some(e);
            }
        }
    }

    /// Re-reads the registry state of every entry (a few values each, never
    /// done per frame) and the keys of entries no longer in the list.
    pub(super) fn refresh_menu_statuses(&mut self) {
        let target = (self.env.menu_target)();
        let entries = self.menu_entries();
        let statuses = entries
            .iter()
            .map(|e| (e.id.clone(), entry_status(&target, e, &self.env.programs)))
            .collect();
        let orphans = orphan_keys(&target, entries);
        self.menu_statuses = statuses;
        self.menu_orphans = orphans;
    }

    /// Like `refresh_menu_statuses`, but not while a job runs: the registry is
    /// changing under it, and the job's end re-reads the statuses anyway.
    pub(super) fn refresh_menu_statuses_if_idle(&mut self) {
        if !self.is_busy() {
            self.refresh_menu_statuses();
        }
    }

    /// The entries of the signed-in account: another account's are never shown.
    pub(super) fn menu_entries(&self) -> &[MenuEntry] {
        self.config.context_menu.entries_of(&self.env.user_label)
    }

    /// Editing the list changes `config`, which the autosave notices.
    pub(super) fn menu_entries_mut(&mut self) -> &mut Vec<MenuEntry> {
        self.config.context_menu.entries_mut(&self.env.user_label)
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
            ReportKind::AppliedMenu => {
                // Entries written; keys taken away are counted apart.
                let removed = report.removed.len();
                let applied = tr.t_args("report.applied_menu", &[&count]);
                match (count, removed) {
                    (_, 0) => applied,
                    (0, n) => tr.t_args("report.removed_menu", &[&n]),
                    (_, n) => format!(
                        "{applied}, {}",
                        tr.t_args("report.removed_menu_also", &[&n])
                    ),
                }
            }
            ReportKind::RestoredMenu => tr.t_args("report.restored_menu", &[&count]),
            ReportKind::AppliedStartup => tr.t_args("report.applied_startup", &[&count]),
            ReportKind::RestoredStartup => tr.t_args("report.restored_startup", &[&count]),
            ReportKind::AppliedEnvironment => tr.t_args("report.applied_environment", &[&count]),
            ReportKind::RestoredEnvironment => tr.t_args("report.restored_environment", &[&count]),
            ReportKind::AppliedMouseKeyboard => {
                tr.t_args("report.applied_mouse_keyboard", &[&count])
            }
            ReportKind::RestoredMouseKeyboard => {
                tr.t_args("report.restored_mouse_keyboard", &[&count])
            }
            ReportKind::AppliedVisualEffects => {
                tr.t_args("report.applied_visual_effects", &[&count])
            }
            ReportKind::RestoredVisualEffects => {
                tr.t_args("report.restored_visual_effects", &[&count])
            }
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
    fn load_or_init_config(path: &Path) -> (AppConfig, Option<LoadError>) {
        match crate::persist::load_json::<AppConfig>(path) {
            Ok(config) => (config, None),
            Err(e) => (AppConfig::default(), Some(e)),
        }
    }

    /// Startup: remembers why config.json could not be loaded (which blocks
    /// every save of it) and says so in a toast.
    fn adopt_config_error(&mut self, error: Option<LoadError>) {
        if let Some(e) = &error {
            let detail = messages::load_error(self.tr, e);
            let text = self.tr.t_args("notice.config_unreadable", &[&detail]);
            self.notify(kit::ToastKind::Error, text);
        }
        self.config_error = error;
    }

    /// Saves config.json; refused while the file on disk is unreadable.
    /// The error is English: it ends up in the report and the history.
    fn try_save_config(&self) -> Result<(), String> {
        if let Some(e) = &self.config_error {
            return Err(e.to_string());
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
        assert_eq!(c.theme, crate::models::ThemeMode::Dark);
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
        use crate::models::ThemeMode;
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
    fn another_user_sees_an_empty_list() {
        use crate::context_menu::{MenuEntry, Place, ProgramChoice};
        let icons = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut config = icons.config();
        config
            .context_menu
            .entries_mut("TEST\\user")
            .push(MenuEntry {
                id: "e1".into(),
                label: "Open".into(),
                program: ProgramChoice::Cmd,
                places: [Place::Folder].into_iter().collect(),
                icon: None,
            });
        let mut app = icons.app_with(&ctx, config.clone());
        assert_eq!(app.menu_entries().len(), 1);

        let mut env = icons.env();
        env.user_label = "OTHER\\user".into();
        let mut other = WinTuneApp::with_config(&ctx, config, false, env);
        assert!(other.menu_entries().is_empty());
        // Editing the other account's list leaves the first one alone.
        other.menu_entries_mut().clear();
        assert_eq!(app.menu_entries_mut().len(), 1);
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
