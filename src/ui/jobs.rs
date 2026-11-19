//! Glue between the UI and [`crate::tasks`]: starts the long operations on a
//! worker thread and applies their outcome once they finish.

use super::WinTuneApp;
use crate::associations::{
    apply_extensions, rollback_extensions, save_backup_store, ApplyOptions, OperationReport,
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
}

pub(super) enum RunningJob {
    Apply(BackgroundJob<StoreOutcome>),
    Rollback(BackgroundJob<StoreOutcome>),
    BuildPak(BackgroundJob<Result<PakStats, String>>),
    Tweaks(BackgroundJob<TweakOutcome>),
}

impl RunningJob {
    pub(super) fn title(&self) -> &str {
        match self {
            RunningJob::Apply(j) => j.title(),
            RunningJob::Rollback(j) => j.title(),
            RunningJob::BuildPak(j) => j.title(),
            RunningJob::Tweaks(j) => j.title(),
        }
    }

    pub(super) fn progress(&self) -> &Progress {
        match self {
            RunningJob::Apply(j) => j.progress(),
            RunningJob::Rollback(j) => j.progress(),
            RunningJob::BuildPak(j) => j.progress(),
            RunningJob::Tweaks(j) => j.progress(),
        }
    }
}

fn repaint(ctx: &egui::Context) -> impl Fn() + Send + Sync + 'static {
    let ctx = ctx.clone();
    move || ctx.request_repaint()
}

impl WinTuneApp {
    pub(super) fn refuse_if_busy(&mut self) -> bool {
        if let Some(job) = &self.job {
            let msg = format!("Please wait: {} is still running.", job.title());
            self.set_status(msg);
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
            let msg = format!("Registry changes are blocked: {}", err);
            self.set_status(msg);
            return;
        }
        let problems = super::associations_tab::problems_of_enabled(&rules);
        if let Some(first) = problems.first() {
            self.status_message = format!(
                "Nothing applied: fix {} rule problem(s) first ({}).",
                problems.len(),
                first
            );
            self.status_details = problems.join("\n");
            return;
        }
        if self.config.code_executable.trim().is_empty() {
            self.set_status("Set the default code editor before applying associations.");
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
        self.set_status("Applying file associations…");
    }

    pub(super) fn start_rollback(&mut self, ctx: &egui::Context, extensions: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(err) = &self.backup_store_error {
            let msg = format!("Rollback is blocked: {}", err);
            self.set_status(msg);
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
        self.set_status("Restoring extensions…");
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
        self.set_status("Building gallery.pak…");
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
            RunningJob::Tweaks(j) => match j.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(outcome) => Finished::Tweaks(outcome),
                JobPoll::Panicked(msg) => Finished::Panicked(msg),
            },
        };
        let title = job.title().to_string();
        self.job = None;

        match finished {
            Finished::Apply(outcome) => {
                self.backup_store = outcome.store;
                self.save_config();
                self.show_report("Applied", "extension(s)", &outcome.report);
                self.record_history("File associations", "Apply", &outcome.report);
                self.request_status_refresh();
            }
            Finished::Rollback(outcome) => {
                self.backup_store = outcome.store;
                self.rollback_selection.clear();
                self.show_report("Restored", "extension(s)", &outcome.report);
                self.record_history("File associations", "Rollback", &outcome.report);
                self.request_status_refresh();
            }
            Finished::BuildPak(Ok(stats)) => {
                self.gallery
                    .reload(&self.config.icon_source_folder, &self.config.pak_file_path);
                self.set_status(format!(
                    "Packed {} icons: {:.1} MB -> {:.1} MB",
                    stats.count,
                    stats.bytes_before as f64 / 1_048_576.0,
                    stats.bytes_after as f64 / 1_048_576.0
                ));
            }
            Finished::BuildPak(Err(e)) => self.set_status(format!("Pack error: {}", e)),
            Finished::Tweaks(outcome) => self.finish_tweaks_job(outcome),
            Finished::Panicked(msg) => {
                self.set_status(format!("{} failed unexpectedly: {}", title, msg));
                // The registry may have changed: re-read what is on disk.
                self.reload_backup_store();
                self.reload_tweak_store();
                self.refresh_tweak_statuses();
            }
        }
    }
}

enum Finished {
    Apply(StoreOutcome),
    Rollback(StoreOutcome),
    BuildPak(Result<PakStats, String>),
    Tweaks(TweakOutcome),
    Panicked(String),
}
