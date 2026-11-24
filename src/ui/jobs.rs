//! Glue between the UI and [`crate::tasks`]: starts the long operations on a
//! worker thread and applies their outcome once they finish.

use super::{kit::ToastKind, ReportKind, WinTuneApp};
use crate::associations::{
    apply_extensions, find_rule_problems, rollback_extensions, save_backup_store, ApplyOptions,
    OperationReport,
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
}

impl RunningJob {
    pub(super) fn title(&self) -> &str {
        match self {
            RunningJob::Apply(j) => j.title(),
            RunningJob::Rollback(j) => j.title(),
            RunningJob::BuildPak(j) => j.title(),
            RunningJob::Tweaks { job, .. } => job.title(),
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
            RunningJob::BuildPak(_) => None,
        }
    }

    pub(super) fn progress(&self) -> &Progress {
        match self {
            RunningJob::Apply(j) => j.progress(),
            RunningJob::Rollback(j) => j.progress(),
            RunningJob::BuildPak(j) => j.progress(),
            RunningJob::Tweaks { job, .. } => job.progress(),
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
        ] {
            assert_ne!(tr.t(key), key);
        }
    }
}
