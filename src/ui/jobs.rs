//! Glue between the UI and [`crate::tasks`]: starts the long operations on a
//! worker thread and applies their outcome once they finish.

use super::{kit::ToastKind, ReportKind, WinTuneApp};
use crate::associations::{
    apply_extensions, find_rule_problems, rollback_extensions, save_backup_store, ApplyOptions,
    OperationReport,
};
use crate::context_menu::{
    apply_entries, entry_problems, restore_keys, MenuBackupStore, MenuOptions,
};
use crate::detector::{build_editor_command, editor_display_name};
use crate::gallery::{build_compressed_pak, PakStats};
use crate::models::{BackupStore, ExtensionConfig};
use crate::tasks::{BackgroundJob, JobPoll, Progress};
use eframe::egui;

/// Result of a registry job: what happened, and the backup store as it is
/// after the operation (it replaces the UI's copy).
pub(super) struct StoreOutcome {
    report: OperationReport,
    store: BackupStore,
}

/// Result of a tweaks job, with the tweak snapshot store after it.
pub(super) struct TweakOutcome {
    pub(super) report: OperationReport,
    pub(super) store: crate::tweaks::TweakBackupStore,
    /// True for "restore originals", false for "apply changes".
    pub(super) restored: bool,
    /// Module name for the history ("Explorer & Taskbar", "Start & Privacy").
    pub(super) module: &'static str,
}

/// Result of a context menu job, with the snapshot store after it.
pub(super) struct MenuOutcome {
    pub(super) report: OperationReport,
    pub(super) store: MenuBackupStore,
    /// True for "restore", false for "apply".
    pub(super) restored: bool,
}

pub(super) enum RunningJob {
    Apply(BackgroundJob<StoreOutcome>),
    Rollback(BackgroundJob<StoreOutcome>),
    BuildPak(BackgroundJob<Result<PakStats, String>>),
    /// A tweaks job: apply or restore, and the module (tab) it belongs to.
    Tweaks {
        job: BackgroundJob<TweakOutcome>,
        module: &'static str,
        /// True for "restore originals", false for "apply changes".
        restore: bool,
    },
    /// A context menu job: apply the list, or restore saved originals.
    ContextMenu {
        job: BackgroundJob<MenuOutcome>,
        restore: bool,
    },
}

impl RunningJob {
    pub(super) fn title(&self) -> &str {
        match self {
            RunningJob::Apply(j) => j.title(),
            RunningJob::Rollback(j) => j.title(),
            RunningJob::BuildPak(j) => j.title(),
            RunningJob::Tweaks { job, .. } => job.title(),
            RunningJob::ContextMenu { job, .. } => job.title(),
        }
    }

    /// Translation key of the text shown next to the progress bar.
    pub(super) fn title_key(&self) -> &'static str {
        match self {
            RunningJob::Apply(_) => "job.apply_rules",
            RunningJob::Rollback(_) => "job.rollback",
            RunningJob::BuildPak(_) => "job.build_pak",
            RunningJob::Tweaks { restore: true, .. } => "job.restore_tweaks",
            RunningJob::Tweaks { .. } => "job.apply_tweaks",
            RunningJob::ContextMenu { restore: true, .. } => "job.restore_menu",
            RunningJob::ContextMenu { .. } => "job.apply_menu",
        }
    }

    /// Translation key of the job's name as it reads inside a sentence
    /// (no trailing ellipsis).
    pub(super) fn name_key(&self) -> &'static str {
        match self.title_key() {
            "job.apply_rules" => "job.name.apply_rules",
            "job.rollback" => "job.name.rollback",
            "job.build_pak" => "job.name.build_pak",
            "job.restore_tweaks" => "job.name.restore_tweaks",
            "job.apply_menu" => "job.name.apply_menu",
            "job.restore_menu" => "job.name.restore_menu",
            _ => "job.name.apply_tweaks",
        }
    }

    /// (module, action) under which a failed run of this job is logged.
    pub(super) fn history_label(&self) -> Option<(&'static str, &'static str)> {
        match self {
            RunningJob::Apply(_) => Some(("File associations", "Apply")),
            RunningJob::Rollback(_) => Some(("File associations", "Rollback")),
            RunningJob::Tweaks {
                module,
                restore: true,
                ..
            } => Some((module, "Restore")),
            RunningJob::Tweaks { module, .. } => Some((module, "Apply")),
            RunningJob::ContextMenu { restore: true, .. } => Some(("Context menu", "Restore")),
            RunningJob::ContextMenu { .. } => Some(("Context menu", "Apply")),
            RunningJob::BuildPak(_) => None,
        }
    }

    pub(super) fn progress(&self) -> &Progress {
        match self {
            RunningJob::Apply(j) => j.progress(),
            RunningJob::Rollback(j) => j.progress(),
            RunningJob::BuildPak(j) => j.progress(),
            RunningJob::Tweaks { job, .. } => job.progress(),
            RunningJob::ContextMenu { job, .. } => job.progress(),
        }
    }
}

fn repaint(ctx: &egui::Context) -> impl Fn() + Send + Sync + 'static {
    let ctx = ctx.clone();
    move || ctx.request_repaint()
}

impl WinTuneApp {
    pub(super) fn refuse_if_busy(&mut self) -> bool {
        // The dialog blocks the mouse but not the keyboard (Tab + Space).
        if self.pending_confirm.is_some() {
            let text = self.tr.t("notice.answer_dialog");
            self.notify(ToastKind::Info, text);
            return true;
        }
        if let Some(job) = &self.job {
            let name = self.tr.t(job.name_key());
            let text = self.tr.t_args("notice.still_running", &[&name]);
            self.notify(ToastKind::Info, text);
            return true;
        }
        false
    }

    /// Applies the given rules to the real registry (HKLM + loaded user
    /// hives) on a background thread.
    pub(super) fn start_apply(&mut self, ctx: &egui::Context, rules: Vec<ExtensionConfig>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(err) = &self.backup_store_error {
            let detail = crate::i18n::messages::load_error(self.tr, err);
            let text = self.tr.t_args("notice.registry_blocked", &[&detail]);
            self.notify(ToastKind::Error, text);
            return;
        }
        let enabled: Vec<ExtensionConfig> = rules.iter().filter(|r| r.enabled).cloned().collect();
        let tr = self.tr;
        let problems: Vec<String> = find_rule_problems(&enabled)
            .iter()
            .map(|(_, p)| crate::i18n::messages::rule_problem(tr, p))
            .collect();
        if let Some(first) = problems.first() {
            let text = tr.t_args("notice.fix_problems", &[&problems.len(), first]);
            self.notify_details(ToastKind::Error, text, problems.join("\n"));
            return;
        }
        if self.config.code_executable.trim().is_empty() {
            let text = tr.t("notice.editor_missing");
            self.notify(ToastKind::Error, text);
            return;
        }

        let library = self.gallery.library();
        let icon_dest = self.config.icon_destination_folder.clone();
        let editor_command = build_editor_command(&self.config.code_executable);
        let editor_label = editor_display_name(&self.config.code_executable);
        let wsl_available = self.wsl_detected;
        let mut store = self.backup_store.clone();
        let env = self.env.clone();

        let job = BackgroundJob::spawn("Applying file associations", repaint(ctx), move |sink| {
            let resolve_icon =
                |cfg: &ExtensionConfig| library.export_icon_to(&cfg.icon_name, &icon_dest);
            let progress = |done: usize, total: usize, label: &str| sink.report(done, total, label);
            let opts = ApplyOptions {
                editor_command,
                editor_label,
                wsl_available,
                resolve_icon: &resolve_icon,
                backup_store_path: env.backup_store_path.clone(),
                reg_backup_dir: env.reg_backup_dir.clone(),
                progress: &progress,
            };
            let report = apply_extensions(&(env.assoc_target)(), &mut store, &rules, &opts);
            (env.notify_shell)();
            StoreOutcome { report, store }
        });
        self.job = Some(RunningJob::Apply(job));
    }

    pub(super) fn start_rollback(&mut self, ctx: &egui::Context, extensions: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(err) = &self.backup_store_error {
            let detail = crate::i18n::messages::load_error(self.tr, err);
            let text = self.tr.t_args("notice.rollback_blocked", &[&detail]);
            self.notify(ToastKind::Error, text);
            return;
        }

        let mut store = self.backup_store.clone();
        let env = self.env.clone();
        let job = BackgroundJob::spawn("Restoring extensions", repaint(ctx), move |sink| {
            let progress = |done: usize, total: usize, label: &str| sink.report(done, total, label);
            let mut report =
                rollback_extensions(&(env.assoc_target)(), &mut store, &extensions, &progress);
            if let Err(e) = save_backup_store(&store, &env.backup_store_path) {
                report.errors.push(format!("Cannot save snapshots: {}", e));
            }
            (env.notify_shell)();
            StoreOutcome { report, store }
        });
        self.job = Some(RunningJob::Rollback(job));
    }

    pub(super) fn start_build_pak(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        let source = self.config.icon_source_folder.clone();
        let pak = self.config.pak_file_path.clone();
        let job = BackgroundJob::spawn("Building gallery.pak", repaint(ctx), move |sink| {
            build_compressed_pak(&source, &pak, &|done, total| {
                sink.report(done, total, "Compressing icons")
            })
        });
        self.job = Some(RunningJob::BuildPak(job));
    }

    /// Why a context menu change cannot start: the unreadable store or config
    /// (which would be overwritten by the next save), or, for an apply, the
    /// entries that cannot be written. Shown as the toast and as the hover of
    /// the disabled Apply button.
    pub(super) fn menu_blocked_reason(&self, applying: bool) -> Option<String> {
        let tr = self.tr;
        let blocked = |e: &crate::persist::LoadError| {
            let detail = crate::i18n::messages::load_error(tr, e);
            tr.t_args("tweaks.blocked", &[&detail])
        };
        if let Some(e) = &self.menu_store_error {
            return Some(blocked(e));
        }
        if let Some(e) = &self.config_error {
            return Some(blocked(e));
        }
        if !applying {
            return None;
        }
        let entries = self.menu_entries();
        let lines: Vec<String> = entry_problems(entries, &self.env.programs)
            .iter()
            .map(|(id, problem)| {
                let name = entries
                    .iter()
                    .find(|e| &e.id == id)
                    .map(|e| e.label.trim())
                    .filter(|label| !label.is_empty())
                    .unwrap_or(id);
                tr.t_args(
                    "menu.problem_line",
                    &[&name, &crate::i18n::messages::entry_problem(tr, problem)],
                )
            })
            .collect();
        (!lines.is_empty()).then(|| tr.t_args("menu.blocked.problems", &[&lines.join("\n")]))
    }

    /// Writes the list of this account to the registry on a background thread.
    // The page that calls this arrives in the next task.
    #[allow(dead_code)]
    pub(super) fn start_menu_apply(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.menu_blocked_reason(true) {
            self.notify(ToastKind::Error, reason);
            return;
        }
        let entries = self.menu_entries().to_vec();
        self.spawn_menu_job(
            ctx,
            false,
            "Applying context menu entries",
            move |env, store, opts| {
                apply_entries(&(env.menu_target)(), store, &entries, &env.programs, opts)
            },
        );
    }

    /// Puts the given keys (as `entry_key` paths) back to their saved originals.
    // The page that calls this arrives in the next task.
    #[allow(dead_code)]
    pub(super) fn start_menu_restore(&mut self, ctx: &egui::Context, keys: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.menu_blocked_reason(false) {
            self.notify(ToastKind::Error, reason);
            return;
        }
        self.spawn_menu_job(
            ctx,
            true,
            "Restoring context menu entries",
            move |env, store, opts| restore_keys(&(env.menu_target)(), store, &keys, opts),
        );
    }

    fn spawn_menu_job(
        &mut self,
        ctx: &egui::Context,
        restored: bool,
        title: &'static str,
        run: impl FnOnce(&super::AppEnv, &mut MenuBackupStore, &MenuOptions) -> OperationReport
            + Send
            + 'static,
    ) {
        let env = self.env.clone();
        let mut store = self.menu_store.clone();
        let user = self.env.user_label.clone();
        let job = BackgroundJob::spawn(title, repaint(ctx), move |sink| {
            let progress = |done: usize, total: usize, label: &str| sink.report(done, total, label);
            let opts = MenuOptions {
                user,
                store_path: env.menu_store_path.clone(),
                reg_backup_dir: env.reg_backup_dir.clone(),
                progress: &progress,
            };
            let report = run(&env, &mut store, &opts);
            MenuOutcome {
                report,
                store,
                restored,
            }
        });
        self.job = Some(RunningJob::ContextMenu {
            job,
            restore: restored,
        });
    }

    /// Called every frame: forwards progress and handles a finished job.
    pub(super) fn poll_job(&mut self) {
        let Some(job) = self.job.as_mut() else {
            return;
        };
        let finished = match job {
            RunningJob::Apply(j) => match j.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Apply(outcome),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
            RunningJob::Rollback(j) => match j.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Rollback(outcome),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
            RunningJob::BuildPak(j) => match j.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(result) => Finished::BuildPak(result),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
            RunningJob::Tweaks { job, .. } => match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Tweaks(outcome),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
            RunningJob::ContextMenu { job, .. } => match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Menu(outcome),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
        };
        let title = job.title().to_string();
        let title_text = self.tr.t(job.name_key());
        let history_label = job.history_label();
        self.job = None;

        match finished {
            Finished::Apply(outcome) => {
                self.backup_store = outcome.store;
                let mut report = outcome.report;
                if let Err(e) = self.try_save_config() {
                    report
                        .warnings
                        .push(format!("Configuration not saved: {}", e));
                }
                self.show_report(ReportKind::AppliedRules, &report);
                self.record_history("File associations", "Apply", &report);
                self.request_status_refresh();
            }
            Finished::Rollback(outcome) => {
                self.backup_store = outcome.store;
                self.show_report(ReportKind::RestoredRules, &outcome.report);
                self.record_history("File associations", "Rollback", &outcome.report);
                self.request_status_refresh();
            }
            Finished::BuildPak(Ok(stats)) => {
                self.gallery
                    .reload(&self.config.icon_source_folder, &self.config.pak_file_path);
                let mb = |bytes: u64| format!("{:.1}", bytes as f64 / 1_048_576.0);
                let text = self.tr.t_args(
                    "notice.packed",
                    &[
                        &stats.count,
                        &mb(stats.bytes_before),
                        &mb(stats.bytes_after),
                    ],
                );
                self.notify(ToastKind::Success, text);
            }
            Finished::BuildPak(Err(e)) => {
                let text = self.tr.t_args("notice.pack_failed", &[&e]);
                self.notify(ToastKind::Error, text);
            }
            Finished::Tweaks(outcome) => self.finish_tweaks_job(outcome),
            Finished::Menu(outcome) => self.finish_menu_job(outcome),
            Finished::Panicked(msg) => {
                if let Some((module, action)) = history_label {
                    // It may have written part of its changes: keep a trace.
                    let report = OperationReport {
                        errors: vec![format!("{} failed unexpectedly: {}", title, msg)],
                        ..Default::default()
                    };
                    self.record_history(module, action, &report);
                }
                let text = self.tr.t_args("notice.job_panicked", &[&title_text, &msg]);
                self.notify(ToastKind::Error, text);
                // The registry may have changed: re-read what is on disk.
                self.reload_backup_store();
                self.reload_tweak_store();
                self.refresh_tweak_statuses();
                self.reload_menu_store();
                self.refresh_menu_statuses();
            }
        }
    }

    fn finish_menu_job(&mut self, outcome: MenuOutcome) {
        self.menu_store = outcome.store;
        (self.env.notify_shell)();
        let kind = if outcome.restored {
            ReportKind::RestoredMenu
        } else {
            ReportKind::AppliedMenu
        };
        self.show_report(kind, &outcome.report);
        let action = if outcome.restored { "Restore" } else { "Apply" };
        self.record_history("Context menu", action, &outcome.report);
        self.refresh_menu_statuses();
    }
}

enum Finished {
    Apply(StoreOutcome),
    Rollback(StoreOutcome),
    BuildPak(Result<PakStats, String>),
    Tweaks(TweakOutcome),
    Menu(MenuOutcome),
    Panicked(String),
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::gallery::PakStats;
    use egui::RawInput;

    #[test]
    fn a_report_with_errors_is_an_error_toast_with_details() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let report = OperationReport {
            succeeded: vec![".a".into()],
            errors: vec![".b: x".into()],
            ..Default::default()
        };

        app.show_report(super::super::ReportKind::AppliedRules, &report);

        let shown = app.last_notice();
        assert!(shown.contains("1 error(s)"), "{shown}");
        assert!(shown.contains("Error: .b: x"), "{shown}");
    }

    #[test]
    fn job_start_shows_progress_not_a_toast() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
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

        let out = frame(&mut app, &ctx, RawInput::default());

        assert!(app.toasts.is_empty());
        assert!(drawn_texts(&out).contains(&app.tr.t("job.build_pak")));
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn history_not_saved_is_its_own_error_toast() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        // A directory cannot be appended to.
        std::fs::create_dir_all(fx.dir.join("history-dir")).unwrap();
        app.env.history_path = fx.dir.join("history-dir");
        let rules = app.config.extensions.clone();

        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);

        assert!(app.toasts.len() >= 2, "{}", app.toasts.len());
        assert!(
            app.last_notice().contains("History not saved"),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn sentences_name_the_job_without_an_ellipsis() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
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
        assert_eq!(
            app.last_notice(),
            "Please wait: Building gallery.pak is still running."
        );
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);

        app.job = Some(RunningJob::Apply(BackgroundJob::spawn(
            "Applying file associations",
            || {},
            |_| -> StoreOutcome { panic!("boom") },
        )));
        run_until_idle(&mut app, &ctx);
        assert!(
            app.last_notice()
                .starts_with("Applying file associations failed unexpectedly: "),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn the_progress_bar_shows_counts_not_the_engine_label() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |sink| {
                sink.report(2, 5, "Compressing icons");
                release_rx.recv().unwrap();
                Ok(PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));
        let mut texts = Vec::new();
        for _ in 0..200 {
            texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
            if texts.contains(&"2/5".to_string()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(texts.contains(&"2/5".to_string()), "{texts:?}");
        assert!(
            !texts.iter().any(|t| t.contains("Compressing")),
            "{texts:?}"
        );
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    fn two_menu_entries(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        use crate::context_menu::{MenuEntry, Place, ProgramChoice};
        let mut config = fx.config();
        for (id, label) in [("e1", "Open one"), ("e2", "Open two")] {
            config
                .context_menu
                .entries_mut("TEST\\user")
                .push(MenuEntry {
                    id: id.into(),
                    label: label.into(),
                    program: ProgramChoice::Cmd,
                    places: [Place::Folder, Place::Background].into_iter().collect(),
                    icon: None,
                });
        }
        fx.app_with(ctx, config)
    }

    #[test]
    fn a_menu_apply_runs_in_background_and_is_logged() {
        use crate::context_menu::{entry_key, EntryStatus, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.refresh_menu_statuses();
        assert!(app
            .menu_statuses
            .values()
            .all(|s| *s == EntryStatus::NotApplied));

        app.start_menu_apply(&ctx);
        assert!(app.is_busy());
        run_until_idle(&mut app, &ctx);

        for id in ["e1", "e2"] {
            for place in [Place::Folder, Place::Background] {
                let key = fx.hkcu(&entry_key(place, id));
                assert!(fx.key_exists(&key), "{key}");
            }
        }
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Context menu", "Apply")
        );
        assert_eq!(app.menu_statuses.len(), 2);
        assert!(app
            .menu_statuses
            .values()
            .all(|s| *s == EntryStatus::Applied));
        assert!(app.menu_store.backups_of("TEST\\user").is_some());
        assert!(
            app.last_notice().starts_with("Applied 2 context menu"),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn a_menu_restore_runs_in_background_and_is_logged() {
        use crate::context_menu::{entry_key, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);

        let keys = vec![entry_key(Place::Folder, "e1")];
        app.start_menu_restore(&ctx, keys);
        assert!(app.is_busy());
        run_until_idle(&mut app, &ctx);

        assert!(!fx.key_exists(&fx.hkcu(&entry_key(Place::Folder, "e1"))));
        assert!(fx.key_exists(&fx.hkcu(&entry_key(Place::Folder, "e2"))));
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Context menu", "Restore")
        );
    }

    #[test]
    fn menu_apply_is_refused_while_the_store_is_unreadable() {
        use crate::context_menu::{entry_key, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        std::fs::write(&app.env.menu_store_path, "{ not json").unwrap();
        app.reload_menu_store();
        assert!(app.menu_store_error.is_some());

        app.start_menu_apply(&ctx);

        assert!(!app.is_busy());
        assert!(!fx.key_exists(&fx.hkcu(&entry_key(Place::Folder, "e1"))));
        assert!(
            app.last_notice().starts_with("Changes are blocked"),
            "{}",
            app.last_notice()
        );
        app.start_menu_restore(&ctx, vec![entry_key(Place::Folder, "e1")]);
        assert!(!app.is_busy());
    }

    #[test]
    fn menu_apply_is_refused_while_the_config_is_unreadable() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.config_error = Some(corrupt("config.json", "bad json"));
        app.start_menu_apply(&ctx);
        assert!(!app.is_busy());
        assert!(app.last_notice().starts_with("Changes are blocked"));
    }

    #[test]
    fn menu_apply_is_refused_with_the_problem_named() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.menu_entries_mut()[1].label = "  ".into();

        app.start_menu_apply(&ctx);

        assert!(!app.is_busy());
        let shown = app.last_notice();
        assert!(
            shown.contains("Open one") || shown.contains("e2"),
            "{shown}"
        );
        assert!(shown.contains("The label is empty."), "{shown}");
        assert!(!app.env.menu_store_path.exists(), "store written");
    }

    #[test]
    fn a_second_menu_job_is_refused_while_one_runs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        let release = hold_job(&mut app);
        app.start_menu_apply(&ctx);
        assert!(app.last_notice().contains("still running"));
        drop(release);
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn a_panicking_menu_job_is_logged_and_reloads_the_store() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.menu_store_error = Some(corrupt("context-menu-backups.json", "sentinel"));
        app.job = Some(RunningJob::ContextMenu {
            job: BackgroundJob::spawn(
                "Applying context menu entries",
                || {},
                |_| -> MenuOutcome { panic!("boom") },
            ),
            restore: false,
        });
        run_until_idle(&mut app, &ctx);
        assert_eq!(app.menu_store_error, None);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Context menu", "Apply")
        );
        assert!(app.last_notice().contains("failed unexpectedly"));
    }

    #[test]
    fn every_job_title_key_exists_in_english() {
        let tr = crate::i18n::Tr::english();
        for key in [
            "job.apply_rules",
            "job.rollback",
            "job.build_pak",
            "job.apply_tweaks",
            "job.restore_tweaks",
            "job.progress",
            "job.name.apply_rules",
            "job.name.rollback",
            "job.name.build_pak",
            "job.name.apply_tweaks",
            "job.name.restore_tweaks",
            "job.apply_menu",
            "job.restore_menu",
            "job.name.apply_menu",
            "job.name.restore_menu",
        ] {
            assert_ne!(tr.t(key), key);
        }
    }
}
