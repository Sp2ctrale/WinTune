//! Glue between the UI and [`crate::tasks`]: starts the long operations on a
//! worker thread and applies their outcome once they finish.

use super::environment_state::{EnvPending, VarKey};
use super::{kit::ToastKind, ReportKind, WinTuneApp};
use crate::associations::{
    apply_extensions, find_rule_problems, rollback_extensions, save_backup_store, ApplyOptions,
    OperationReport,
};
use crate::context_menu::{apply_menu, entry_problems, restore_keys, MenuBackupStore, MenuOptions};
use crate::detector::{build_editor_command, editor_display_name};
use crate::environment::{
    apply_environment, change_warnings, is_path_name, removed_entries, restore_environment,
    EnvBackupStore, EnvChange, EnvOptions, EnvTarget, EnvWarning, PathBasis,
};
use crate::gallery::{build_compressed_pak, PakStats};
use crate::models::{BackupStore, ExtensionConfig};
use crate::startup::{
    apply_startup, now_filetime, restore_startup, StartupBackupStore, StartupOptions,
};
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

/// Result of a startup apps job, with the snapshot store after it.
pub(super) struct StartupOutcome {
    pub(super) report: OperationReport,
    pub(super) store: StartupBackupStore,
    /// True for "restore", false for "apply".
    pub(super) restored: bool,
}

/// Result of an environment variables job, with the snapshot store after it.
pub(super) struct EnvOutcome {
    pub(super) report: OperationReport,
    pub(super) store: EnvBackupStore,
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
    /// A startup apps job: apply the pending switches, or restore originals.
    Startup {
        job: BackgroundJob<StartupOutcome>,
        restore: bool,
    },
    /// An environment variables job: apply the pending changes, or restore
    /// originals.
    Environment {
        job: BackgroundJob<EnvOutcome>,
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
            RunningJob::Startup { job, .. } => job.title(),
            RunningJob::Environment { job, .. } => job.title(),
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
            RunningJob::Startup { restore: true, .. } => "job.restore_startup",
            RunningJob::Startup { .. } => "job.apply_startup",
            RunningJob::Environment { restore: true, .. } => "job.restore_environment",
            RunningJob::Environment { .. } => "job.apply_environment",
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
            "job.apply_startup" => "job.name.apply_startup",
            "job.restore_startup" => "job.name.restore_startup",
            "job.apply_environment" => "job.name.apply_environment",
            "job.restore_environment" => "job.name.restore_environment",
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
            RunningJob::Startup { restore: true, .. } => Some(("Startup apps", "Restore")),
            RunningJob::Startup { .. } => Some(("Startup apps", "Apply")),
            RunningJob::Environment { restore: true, .. } => {
                Some(("Environment variables", "Restore"))
            }
            RunningJob::Environment { .. } => Some(("Environment variables", "Apply")),
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
            RunningJob::Startup { job, .. } => job.progress(),
            RunningJob::Environment { job, .. } => job.progress(),
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
        if let Some(reason) = self.menu_store_blocked_reason() {
            return Some(reason);
        }
        if let Some(e) = &self.config_error {
            let detail = crate::i18n::messages::load_error(tr, e);
            return Some(tr.t_args("tweaks.blocked", &[&detail]));
        }
        if !applying {
            return None;
        }
        let entries = self.menu_entries();
        let mut lines: Vec<String> = entry_problems(entries, &self.env.programs)
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
        lines.extend(self.menu_rename_problems());
        (!lines.is_empty()).then(|| tr.t_args("menu.blocked.problems", &[&lines.join("\n")]))
    }

    /// Why a restore cannot start: only an unreadable snapshot store. A restore
    /// never writes the config, so an unreadable config does not stop it.
    fn menu_store_blocked_reason(&self) -> Option<String> {
        self.menu_store_error.as_ref().map(|e| {
            let detail = crate::i18n::messages::load_error(self.tr, e);
            self.tr.t_args("tweaks.blocked", &[&detail])
        })
    }

    /// Writes the list of this account and the pending hides and renames to
    /// the registry in one background job. A hide with side effects asks
    /// first (`ConfirmAction::ApplyMenu`).
    pub(super) fn start_menu_apply(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.menu_blocked_reason(true) {
            self.notify(ToastKind::Error, reason);
            return;
        }
        let warnings = self.menu_apply_warnings();
        if !warnings.is_empty() {
            self.request_apply_menu(&warnings);
            return;
        }
        self.run_menu_apply(ctx);
    }

    /// Apply without asking (the side effects were confirmed, or there are
    /// none). Re-checks what may have changed while the question was open.
    pub(super) fn run_menu_apply(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.menu_blocked_reason(true) {
            self.notify(ToastKind::Error, reason);
            return;
        }
        let entries = self.menu_entries().to_vec();
        let changes = self.menu_existing_changes();
        self.spawn_menu_job(
            ctx,
            false,
            "Applying context menu entries",
            move |env, store, opts| {
                apply_menu(
                    &(env.menu_target)(),
                    store,
                    &entries,
                    &changes,
                    &env.programs,
                    opts,
                )
            },
        );
    }

    /// Puts the given keys (as `entry_key` paths) back to their saved originals.
    pub(super) fn start_menu_restore(&mut self, ctx: &egui::Context, keys: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.menu_store_blocked_reason() {
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
            // Here, not on the UI thread: Explorer can take seconds to answer.
            (env.notify_shell)();
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

    /// Why a startup change cannot start: the unreadable snapshot store
    /// (which the next save would overwrite). Shown as the toast and as the
    /// hover of the disabled Apply button.
    pub(super) fn startup_blocked_reason(&self) -> Option<String> {
        self.startup_store_error.as_ref().map(|e| {
            let detail = crate::i18n::messages::load_error(self.tr, e);
            self.tr.t_args("tweaks.blocked", &[&detail])
        })
    }

    /// Writes the pending on/off choices to the registry in one background
    /// job. No confirmation: nothing is deleted, and History can undo it.
    pub(super) fn start_startup_apply(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.startup_blocked_reason() {
            self.notify(ToastKind::Error, reason);
            return;
        }
        let changes = self.startup_changes();
        if changes.is_empty() {
            return;
        }
        self.spawn_startup_job(
            ctx,
            false,
            "Applying startup changes",
            move |env, store, opts| apply_startup(&(env.startup_target)(), store, &changes, opts),
        );
    }

    /// Puts the given switches (slot ids) back to their saved originals.
    pub(super) fn start_startup_restore(&mut self, ctx: &egui::Context, ids: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.startup_blocked_reason() {
            self.notify(ToastKind::Error, reason);
            return;
        }
        self.spawn_startup_job(
            ctx,
            true,
            "Restoring startup apps",
            move |env, store, opts| restore_startup(&(env.startup_target)(), store, &ids, opts),
        );
    }

    fn spawn_startup_job(
        &mut self,
        ctx: &egui::Context,
        restored: bool,
        title: &'static str,
        run: impl FnOnce(&super::AppEnv, &mut StartupBackupStore, &StartupOptions) -> OperationReport
            + Send
            + 'static,
    ) {
        let env = self.env.clone();
        let mut store = self.startup_store.clone();
        let user = self.env.user_label.clone();
        let job = BackgroundJob::spawn(title, repaint(ctx), move |sink| {
            let progress = |done: usize, total: usize, label: &str| sink.report(done, total, label);
            let opts = StartupOptions {
                user,
                store_path: env.startup_store_path.clone(),
                reg_backup_dir: env.reg_backup_dir.clone(),
                progress: &progress,
                switch_works: env.startup_switch_works,
                now: now_filetime(),
            };
            let report = run(&env, &mut store, &opts);
            StartupOutcome {
                report,
                store,
                restored,
            }
        });
        self.job = Some(RunningJob::Startup {
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
            RunningJob::Startup { job, .. } => match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Startup(outcome),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
            RunningJob::Environment { job, .. } => match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Environment(outcome),
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
            Finished::Startup(outcome) => self.finish_startup_job(outcome),
            Finished::Environment(outcome) => self.finish_env_job(outcome),
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
                self.reload_startup_store();
                self.reload_env_store();
            }
        }
        // Any job may have changed the menu (or the snapshots behind it).
        self.request_menu_scan();
        self.request_startup_scan();
        self.request_env_scan();
    }

    /// Why an environment change cannot start: the unreadable snapshot store
    /// (which the next save would overwrite). Shown as the toast and as the
    /// hover of the disabled Apply button.
    pub(super) fn env_blocked_reason(&self) -> Option<String> {
        self.env_store_error.as_ref().map(|e| {
            let detail = crate::i18n::messages::load_error(self.tr, e);
            self.tr.t_args("tweaks.blocked", &[&detail])
        })
    }

    /// Writes the pending changes in one background job. It asks first when
    /// the changes could surprise (`env_apply_lines`); otherwise History can
    /// undo it.
    pub(super) fn start_env_apply(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.env_blocked_reason() {
            self.notify(ToastKind::Error, reason);
            return;
        }
        let changes = self.env_changes();
        if changes.is_empty() {
            return;
        }
        let lines = self.env_apply_lines(&changes);
        let basis = self.env_path_basis();
        if lines.is_empty() {
            self.run_env_apply(ctx, basis);
        } else {
            self.request_apply_environment(&lines, basis);
        }
    }

    /// What the user should know before `changes` are written: warnings,
    /// deleted variables, and the `Path` folders that will go.
    pub(super) fn env_apply_lines(&self, changes: &[EnvChange]) -> Vec<String> {
        let tr = self.tr;
        let touches_path = changes
            .iter()
            .any(|c| c.slot_names().into_iter().any(is_path_name));
        let mut warnings = change_warnings(changes, self.merged_path_len(changes));
        if touches_path && self.env_path_changed_elsewhere() {
            warnings.push(EnvWarning::PathChangedElsewhere);
        }
        let mut lines: Vec<String> = warnings
            .iter()
            .map(|w| crate::i18n::messages::env_warning(tr, w))
            .collect();
        for change in changes {
            if let EnvChange::Delete { name } = change {
                if !is_path_name(name) {
                    lines.push(tr.t_args("env.confirm.deleted", &[name]));
                }
            }
        }
        if let (true, Some(edit), Some(scan)) =
            (touches_path, &self.env_pending.path, &self.env_scan)
        {
            let removed = removed_entries(&scan.user_path_entries(), &edit.texts());
            if !removed.is_empty() {
                lines.push(tr.t_args("env.confirm.path_removed", &[&removed.join("\n")]));
            }
        }
        lines
    }

    /// Length (UTF-16 units) of the `Path` programs will see once the user
    /// `Path` of `changes` is written: the system one, `;`, then the user
    /// one, both expanded. `None` when no change writes the `Path`.
    fn merged_path_len(&self, changes: &[EnvChange]) -> Option<usize> {
        let wanted = changes.iter().find_map(|c| match c {
            EnvChange::Add { name, value } | EnvChange::Edit { name, value }
                if is_path_name(name) =>
            {
                Some(value)
            }
            _ => None,
        })?;
        let resolver = self.env_resolver();
        let units = |text: &str| resolver.expand(text).text.encode_utf16().count();
        let system = self
            .env_scan
            .as_ref()
            .and_then(|s| s.system.iter().find(|v| v.is_path()))
            .and_then(|v| v.value.as_deref())
            .unwrap_or("");
        let user = units(wanted);
        Some(if system.is_empty() {
            user
        } else {
            units(system) + 1 + user
        })
    }

    /// The write itself, once any question was answered yes. `path_basis` is
    /// the `Path` the question was computed from: if the registry no longer
    /// holds it, the engine refuses a `Path` write, and the rescan after the
    /// job makes the next Apply ask again with what changed.
    pub(super) fn run_env_apply(&mut self, ctx: &egui::Context, path_basis: PathBasis) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.env_blocked_reason() {
            self.notify(ToastKind::Error, reason);
            return;
        }
        let changes = self.env_changes();
        if changes.is_empty() {
            return;
        }
        self.spawn_env_job(
            ctx,
            false,
            "Applying environment changes",
            move |target, store, opts| {
                apply_environment(target, store, &changes, &path_basis, opts)
            },
        );
    }

    /// Puts the given variables (slot ids) back to their saved originals.
    pub(super) fn start_env_restore(&mut self, ctx: &egui::Context, ids: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(reason) = self.env_blocked_reason() {
            self.notify(ToastKind::Error, reason);
            return;
        }
        self.spawn_env_job(
            ctx,
            true,
            "Restoring environment variables",
            move |target, store, opts| restore_environment(target, store, &ids, opts),
        );
    }

    fn spawn_env_job(
        &mut self,
        ctx: &egui::Context,
        restored: bool,
        title: &'static str,
        run: impl FnOnce(&EnvTarget, &mut EnvBackupStore, &EnvOptions) -> OperationReport
            + Send
            + 'static,
    ) {
        let env = self.env.clone();
        let mut store = self.env_store.clone();
        let user = self.env.user_label.clone();
        let job = BackgroundJob::spawn(title, repaint(ctx), move |sink| {
            let progress = |done: usize, total: usize, label: &str| sink.report(done, total, label);
            let opts = EnvOptions {
                user,
                store_path: env.env_store_path.clone(),
                reg_backup_dir: env.reg_backup_dir.clone(),
                progress: &progress,
                broadcast: &*env.notify_environment,
            };
            let report = run(&(env.env_target)(), &mut store, &opts);
            EnvOutcome {
                report,
                store,
                restored,
            }
        });
        self.job = Some(RunningJob::Environment {
            job,
            restore: restored,
        });
    }

    fn finish_env_job(&mut self, outcome: EnvOutcome) {
        self.env_store = outcome.store;
        // A clean apply did everything asked. After a refused or partial
        // one the choices stay, so Apply can be tried again; the next scan
        // drops those the registry already matches.
        if !outcome.restored && outcome.report.errors.is_empty() {
            self.env_pending = EnvPending::default();
        }
        // An edit of a restored variable was made on the value the restore
        // replaced: kept, Apply would undo the restore.
        if outcome.restored {
            for name in &outcome.report.succeeded {
                if is_path_name(name) {
                    self.env_pending.path = None;
                } else {
                    self.env_pending
                        .vars
                        .remove(&VarKey::Existing(name.to_ascii_lowercase()));
                }
            }
        }
        let kind = if outcome.restored {
            ReportKind::RestoredEnvironment
        } else {
            ReportKind::AppliedEnvironment
        };
        self.show_report(kind, &outcome.report);
        let action = if outcome.restored { "Restore" } else { "Apply" };
        self.record_history("Environment variables", action, &outcome.report);
    }

    fn finish_startup_job(&mut self, outcome: StartupOutcome) {
        self.startup_store = outcome.store;
        // A clean apply did everything asked. After a refused or partial
        // one the choices stay, so Apply can be tried again; the next scan
        // drops those the registry already matches.
        if !outcome.restored && outcome.report.errors.is_empty() {
            self.startup_pending.clear();
        }
        let kind = if outcome.restored {
            ReportKind::RestoredStartup
        } else {
            ReportKind::AppliedStartup
        };
        self.show_report(kind, &outcome.report);
        let action = if outcome.restored { "Restore" } else { "Apply" };
        self.record_history("Startup apps", action, &outcome.report);
    }

    fn finish_menu_job(&mut self, outcome: MenuOutcome) {
        self.menu_store = outcome.store;
        // A clean apply did everything asked. After a refused or partial
        // one the choices stay, so Apply can be tried again; the next scan
        // drops those the registry already matches.
        if !outcome.restored && outcome.report.errors.is_empty() {
            self.menu_pending.clear();
        }
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
    Startup(StartupOutcome),
    Environment(EnvOutcome),
    Panicked(String),
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::environment::VarEdit;
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
    fn a_menu_restore_still_runs_while_the_config_is_unreadable() {
        use crate::context_menu::{entry_key, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        app.config_error = Some(corrupt("config.json", "bad json"));

        // Apply is blocked (it saves the config), a restore never writes it.
        app.start_menu_apply(&ctx);
        assert!(!app.is_busy());
        let key = entry_key(Place::Folder, "e1");
        app.request_restore_menu_keys(vec![key.clone()]);
        app.resolve_confirm(&ctx, true);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);
        assert!(!fx.key_exists(&fx.hkcu(&key)));
        let saved = app.menu_store.backups_of("TEST\\user").unwrap();
        assert!(saved.get(&key).is_none());
    }

    #[test]
    fn the_menu_toast_counts_written_entries_and_names_removed_keys_apart() {
        use crate::context_menu::{entry_key, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);

        app.menu_entries_mut().retain(|e| e.id != "e2");
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);

        let toast = app.last_notice();
        assert!(
            toast.starts_with("Applied 1 context menu entry(ies), removed 2"),
            "{toast}"
        );
        assert!(!toast.contains("Software"), "{toast}");
        let last = app.history.first().expect("history entry");
        assert!(
            last.succeeded
                .contains(&"Removed entry \u{b7} Folders".to_string()),
            "{:?}",
            last.succeeded
        );
        assert!(!last.succeeded.iter().any(|n| n.contains("Software")));
        assert!(!fx.key_exists(&fx.hkcu(&entry_key(Place::Folder, "e2"))));

        // Only removals: no "Applied 0".
        app.menu_entries_mut().clear();
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        let toast = app.last_notice();
        assert!(
            toast.starts_with("Removed 2 context menu entry(ies)"),
            "{toast}"
        );
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
    fn one_apply_writes_entries_and_hides() {
        use super::super::menu_existing_state::PendingExisting;
        use crate::context_menu::{entry_key, ExistingState, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = two_menu_entries(&fx, &ctx);
        app.menu_entries_mut().retain(|e| e.id == "e1");
        scan_now(&mut app, &ctx);
        let id = app.menu_scan.as_ref().unwrap().others[0].id.clone();
        app.menu_pending.insert(
            id.clone(),
            PendingExisting {
                hidden: Some(true),
                label: None,
            },
        );
        let history_before = app.history.len();

        app.start_menu_apply(&ctx);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);

        assert!(fx.key_exists(&fx.hkcu(&entry_key(Place::Folder, "e1"))));
        let copy = fx.hkcu(r"Software\Classes\Directory\shell\AppVerb");
        let hidden = fx
            .sandbox
            .root
            .open_subkey(&copy)
            .is_ok_and(|k| k.get_raw_value("ProgrammaticAccessOnly").is_ok());
        assert!(hidden, "{}", app.last_notice());
        assert_eq!(app.history.len(), history_before + 1);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Context menu", "Apply")
        );
        assert!(last.errors.is_empty(), "{:?}", last.errors);
        assert!(app.menu_pending.is_empty());

        run_until_scanned(&mut app, &ctx);
        let row = app.menu_scan.as_ref().unwrap().find(&id).expect("row");
        assert_eq!(row.state, ExistingState::HiddenByWinTune);
        assert!(row.has_snapshot);
    }

    #[test]
    fn a_failed_menu_apply_keeps_pending() {
        use super::super::menu_existing_state::PendingExisting;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = two_menu_entries(&fx, &ctx);
        let hide = PendingExisting {
            hidden: Some(true),
            label: None,
        };
        // A panic, then a refusal reported as errors.
        let jobs: [fn(&crate::tasks::ProgressSink) -> MenuOutcome; 2] = [
            |_| panic!("boom"),
            |_| MenuOutcome {
                report: OperationReport {
                    errors: vec!["refused".into()],
                    ..Default::default()
                },
                store: Default::default(),
                restored: false,
            },
        ];
        for job in jobs {
            app.menu_pending.insert("verb:x|y".into(), hide.clone());
            app.job = Some(RunningJob::ContextMenu {
                job: BackgroundJob::spawn("Applying context menu entries", || {}, job),
                restore: false,
            });
            run_until_idle(&mut app, &ctx);
            assert_eq!(app.menu_pending.get("verb:x|y"), Some(&hide));
        }
    }

    /// One seeded user Run item `App`, scanned.
    fn scanned_startup_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        fx.seed_run(false, "App", r#""C:\Apps\app.exe""#);
        let mut app = fx.app(ctx);
        app.request_startup_scan();
        run_until_startup_scanned(&mut app, ctx);
        assert_eq!(app.startup_scan.as_ref().map(|s| s.items.len()), Some(1));
        app
    }

    #[test]
    fn a_startup_apply_turns_off_logs_and_clears_pending() {
        use crate::startup::StartupState;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_startup_app(&fx, &ctx);
        app.startup_pending.insert("run:app".into(), false);

        app.start_startup_apply(&ctx);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);

        let switch = fx.user_switch("App").expect("switch written");
        assert_eq!(switch.len(), 12);
        assert_eq!(switch[..4], [3, 0, 0, 0]);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Startup apps", "Apply")
        );
        assert!(last.errors.is_empty(), "{:?}", last.errors);
        assert!(app.startup_pending.is_empty());
        assert!(
            app.last_notice().starts_with("1 startup change(s) applied"),
            "{}",
            app.last_notice()
        );

        run_until_startup_scanned(&mut app, &ctx);
        let scan = app.startup_scan.as_ref().unwrap();
        assert_eq!(scan.find("run:app").unwrap().state, StartupState::Off);
    }

    #[test]
    fn a_refused_startup_apply_keeps_the_choice() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_startup_app(&fx, &ctx);
        app.startup_pending.insert("run:app".into(), false);
        // The switch became a shape WinTune does not know, after the scan.
        let (k, _) = fx
            .sandbox
            .root
            .create_subkey(fx.hkcu(&crate::startup::Approved::Run.key()))
            .unwrap();
        k.set_value("App", &3u32).unwrap();
        // An id the scan does not list is never pruned: it shows that the
        // refusal did not clear the choices.
        app.startup_pending.insert("run:ghost".into(), false);

        app.start_startup_apply(&ctx);
        run_until_idle(&mut app, &ctx);

        assert!(app.last_notice().contains("error"), "{}", app.last_notice());
        assert!(app.startup_pending.contains_key("run:ghost"));
        assert_eq!(fx.user_switch("App"), Some(vec![3, 0, 0, 0]), "overwritten");
        assert!(!app.env.startup_store_path.exists(), "store written");
        let last = app.history.first().expect("history entry");
        assert_eq!(last.errors.len(), 1, "{:?}", last.errors);

        // Right after the job finishes, before any rescan (which may prune
        // the choice once the item reads as locked), the choice is kept.
        app.startup_pending.insert("run:app".into(), false);
        app.finish_startup_job(StartupOutcome {
            report: OperationReport {
                errors: vec!["refused".into()],
                ..Default::default()
            },
            store: Default::default(),
            restored: false,
        });
        assert_eq!(app.startup_pending.get("run:app"), Some(&false));
        assert_eq!(app.startup_pending.len(), 2);
    }

    #[test]
    fn an_unreadable_startup_store_blocks_apply_and_restore() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_startup_app(&fx, &ctx);
        app.startup_pending.insert("run:app".into(), false);
        app.startup_store_error = Some(corrupt("startup-backups.json", "bad json"));

        app.start_startup_apply(&ctx);
        assert!(!app.is_busy());
        assert!(
            app.last_notice().starts_with("Changes are blocked"),
            "{}",
            app.last_notice()
        );
        assert_eq!(fx.user_switch("App"), None);

        let id = crate::startup::Approved::Run.slot("App").id();
        app.start_startup_restore(&ctx, vec![id.clone()]);
        assert!(!app.is_busy());

        // The same restore runs once the store is readable again.
        app.startup_store_error = None;
        app.start_startup_restore(&ctx, vec![id]);
        assert!(app.is_busy());
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn a_startup_apply_with_nothing_pending_starts_nothing() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_startup_app(&fx, &ctx);
        app.start_startup_apply(&ctx);
        assert!(!app.is_busy());
    }

    #[test]
    fn a_panicking_startup_job_is_logged_and_reloads_the_store() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_startup_app(&fx, &ctx);
        app.startup_store_error = Some(corrupt("startup-backups.json", "sentinel"));
        app.startup_pending.insert("run:app".into(), false);
        app.job = Some(RunningJob::Startup {
            job: BackgroundJob::spawn(
                "Applying startup changes",
                || {},
                |_| -> StartupOutcome { panic!("boom") },
            ),
            restore: false,
        });
        run_until_idle(&mut app, &ctx);
        assert_eq!(app.startup_store_error, None);
        assert_eq!(app.startup_pending.len(), 1);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Startup apps", "Apply")
        );
        assert!(app.last_notice().contains("failed unexpectedly"));
    }

    fn scanned_env_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_env_scan();
        run_until_env_scanned(&mut app, ctx);
        assert!(app.env_scan.is_some());
        app
    }

    fn set_var(app: &mut WinTuneApp, original: Option<&str>, name: &str, value: &str) {
        let key = match original {
            Some(o) => VarKey::Existing(o.to_ascii_lowercase()),
            None => VarKey::New(name.to_ascii_lowercase()),
        };
        app.env_pending.vars.insert(
            key,
            VarEdit {
                original: original.map(str::to_string),
                wanted: Some((name.to_string(), value.to_string())),
            },
        );
    }

    #[test]
    fn an_environment_apply_writes_broadcasts_once_logs_and_clears_pending() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_env_app(&fx, &ctx);
        set_var(&mut app, None, "FOO", "1");

        app.start_env_apply(&ctx);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);

        assert_eq!(fx.user_env("FOO"), Some((1, "1".to_string())));
        assert_eq!(fx.broadcast_count(), 1);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Environment variables", "Apply")
        );
        assert!(last.errors.is_empty(), "{:?}", last.errors);
        assert_eq!(app.env_pending, EnvPending::default());
        assert!(
            app.last_notice()
                .starts_with("1 environment change(s) applied"),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn an_untouched_path_is_never_written() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 1, "A;;B;");
        fx.seed_env(false, "X", 1, "1");
        let mut app = scanned_env_app(&fx, &ctx);
        set_var(&mut app, Some("X"), "X", "2");

        app.start_env_apply(&ctx);
        run_until_idle(&mut app, &ctx);

        assert_eq!(fx.user_env("X"), Some((1, "2".to_string())));
        assert_eq!(fx.user_env("Path"), Some((1, "A;;B;".to_string())));
    }

    /// Polls only the job, never a frame: a frame would also run the scan
    /// the job requests when it ends.
    fn finish_job_without_scanning(app: &mut WinTuneApp) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.is_busy() && std::time::Instant::now() < deadline {
            app.poll_job();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!app.is_busy(), "job did not finish");
    }

    #[test]
    fn a_path_edit_on_a_stale_scan_is_refused_and_the_rescan_names_the_new_folder() {
        use crate::environment::EnvWarning;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A;B");
        let mut app = scanned_env_app(&fx, &ctx);
        app.env_path_edit_mut().unwrap().move_down(0);
        // An installer appends its folder; the page has not read it again.
        fx.seed_env(false, "Path", 2, "A;B;C");

        // A move only: nothing to ask, as far as the stale scan shows.
        app.start_env_apply(&ctx);
        assert!(app.pending_confirm.is_none());
        finish_job_without_scanning(&mut app);

        assert_eq!(fx.user_env("Path"), Some((2, "A;B;C".to_string())));
        assert_eq!(fx.broadcast_count(), 0);
        assert!(!app.env.env_store_path.exists(), "store written");
        let reg_files = std::fs::read_dir(&app.env.reg_backup_dir).map_or(0, |d| d.count());
        assert_eq!(reg_files, 0, ".reg written");
        assert!(app.env_pending.path.is_some(), "the choice was dropped");
        let last = app.history.first().expect("history entry");
        assert_eq!(last.errors.len(), 1, "{:?}", last.errors);
        assert!(
            last.errors[0].contains("changed by another program"),
            "{:?}",
            last.errors
        );

        // The rescan the job asked for makes the next Apply ask, naming C.
        run_until_env_scanned(&mut app, &ctx);
        app.start_env_apply(&ctx);
        let confirm = app.pending_confirm.as_ref().expect("asks first");
        let elsewhere =
            crate::i18n::messages::env_warning(app.tr, &EnvWarning::PathChangedElsewhere);
        assert!(confirm.message.contains(&elsewhere), "{}", confirm.message);
        let removed = app.tr.t_args("env.confirm.path_removed", &[&"C"]);
        assert!(confirm.message.contains(&removed), "{}", confirm.message);
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert_eq!(fx.user_env("Path"), Some((2, "B;A".to_string())));
    }

    #[test]
    fn a_path_changed_while_the_question_is_open_is_refused() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A;B");
        let mut app = scanned_env_app(&fx, &ctx);
        app.env_path_edit_mut().unwrap().remove(1);
        app.start_env_apply(&ctx);
        // The question names B only.
        assert!(app.pending_confirm.is_some());
        // An installer adds C, and a scan reads it before the answer.
        fx.seed_env(false, "Path", 2, "A;B;C");
        app.request_env_scan();
        run_until_env_scanned(&mut app, &ctx);

        app.resolve_confirm(&ctx, true);
        finish_job_without_scanning(&mut app);

        assert_eq!(fx.user_env("Path"), Some((2, "A;B;C".to_string())));
        assert_eq!(fx.broadcast_count(), 0);
        assert!(app.env_pending.path.is_some(), "the choice was dropped");
    }

    #[test]
    fn a_refused_environment_apply_keeps_the_choices() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "X", 1, "1");
        let mut app = scanned_env_app(&fx, &ctx);
        set_var(&mut app, Some("X"), "X", "2");
        // The variable vanished after the scan.
        let key = fx
            .sandbox
            .root
            .open_subkey_with_flags(
                fx.hkcu(crate::environment::USER_ENV_KEY),
                winreg::enums::KEY_SET_VALUE,
            )
            .unwrap();
        key.delete_value("X").unwrap();

        app.start_env_apply(&ctx);
        // The scan a frame would run prunes the choice.
        finish_job_without_scanning(&mut app);

        assert!(app.last_notice().contains("error"), "{}", app.last_notice());
        assert!(app
            .env_pending
            .vars
            .contains_key(&VarKey::Existing("x".into())));
        assert_eq!(fx.broadcast_count(), 0);
        assert!(!app.env.env_store_path.exists(), "store written");
        let last = app.history.first().expect("history entry");
        assert_eq!(last.errors.len(), 1, "{:?}", last.errors);
    }

    #[test]
    fn a_restore_drops_the_pending_edits_of_what_it_restored() {
        use crate::environment::{env_slot, PATH_NAME};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A;B");
        fx.seed_env(false, "X", 1, "1");
        fx.seed_env(false, "Y", 1, "1");
        let mut app = scanned_env_app(&fx, &ctx);
        app.env_path_edit_mut().unwrap().add("C".into());
        set_var(&mut app, Some("X"), "X", "2");
        app.start_env_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        run_until_env_scanned(&mut app, &ctx);
        assert_eq!(fx.user_env("Path"), Some((2, "A;B;C".to_string())));

        // New edits of the restored slots, and of another variable.
        app.env_path_edit_mut().unwrap().add("D".into());
        set_var(&mut app, Some("X"), "X", "3");
        set_var(&mut app, Some("Y"), "Y", "2");
        app.start_env_restore(&ctx, vec![env_slot(PATH_NAME).id(), env_slot("X").id()]);
        run_until_idle(&mut app, &ctx);
        run_until_env_scanned(&mut app, &ctx);

        assert_eq!(fx.user_env("Path"), Some((2, "A;B".to_string())));
        assert_eq!(fx.user_env("X"), Some((1, "1".to_string())));
        assert_eq!(app.env_pending.path, None, "Apply would undo the restore");
        assert!(!app
            .env_pending
            .vars
            .contains_key(&VarKey::Existing("x".into())));
        assert!(app
            .env_pending
            .vars
            .contains_key(&VarKey::Existing("y".into())));
    }

    #[test]
    fn an_unreadable_environment_store_blocks_apply_and_restore() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "X", 1, "1");
        let mut app = scanned_env_app(&fx, &ctx);
        set_var(&mut app, Some("X"), "X", "2");
        assert_eq!(app.env_pending_count(), 1);
        app.env_store_error = Some(corrupt("environment-backups.json", "bad json"));

        app.start_env_apply(&ctx);
        assert!(!app.is_busy());
        assert!(
            app.last_notice().starts_with("Changes are blocked"),
            "{}",
            app.last_notice()
        );
        assert_eq!(fx.user_env("X"), Some((1, "1".to_string())));

        app.notify(ToastKind::Info, "marker");
        let id = crate::environment::env_slot("X").id();
        app.start_env_restore(&ctx, vec![id.clone()]);
        assert!(!app.is_busy());
        assert!(
            app.last_notice().starts_with("Changes are blocked"),
            "{}",
            app.last_notice()
        );

        // The same restore runs once the store is readable again.
        app.env_store_error = None;
        app.start_env_restore(&ctx, vec![id]);
        assert!(app.is_busy());
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn an_environment_apply_with_nothing_pending_starts_nothing() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_env_app(&fx, &ctx);
        app.start_env_apply(&ctx);
        assert!(!app.is_busy());
        assert_eq!(fx.broadcast_count(), 0);
    }

    #[test]
    fn a_panicking_environment_job_is_logged_and_reloads_the_store() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = scanned_env_app(&fx, &ctx);
        app.env_store_error = Some(corrupt("environment-backups.json", "sentinel"));
        set_var(&mut app, None, "FOO", "1");
        app.job = Some(RunningJob::Environment {
            job: BackgroundJob::spawn(
                "Applying environment changes",
                || {},
                |_| -> EnvOutcome { panic!("boom") },
            ),
            restore: false,
        });
        run_until_idle(&mut app, &ctx);
        assert_eq!(app.env_store_error, None);
        assert_eq!(app.env_pending_count(), 1);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Environment variables", "Apply")
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
            "job.apply_startup",
            "job.restore_startup",
            "job.name.apply_startup",
            "job.name.restore_startup",
            "job.apply_environment",
            "job.restore_environment",
            "job.name.apply_environment",
            "job.name.restore_environment",
        ] {
            assert_ne!(tr.t(key), key);
        }
    }
}
