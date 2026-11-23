//! Live per-rule status ("Applied" / "Changed"), computed on a background
//! thread (never per frame): at startup, after every apply or rollback, and
//! when the user asks for a refresh.

use super::WinTuneApp;
use crate::associations::{rule_status, RuleStatus, StatusContext};
use crate::detector::build_editor_command;
use crate::tasks::{BackgroundJob, JobPoll};
use crate::theme::{draw_badge, get_palette, ThemeMode};
use eframe::egui::{self, RichText};
use std::collections::HashMap;

pub(super) type StatusMap = HashMap<String, RuleStatus>;
pub(super) type StatusJob = BackgroundJob<StatusMap>;

impl WinTuneApp {
    /// Schedules a refresh; it starts as soon as no registry job is running.
    pub(super) fn request_status_refresh(&mut self) {
        self.status_refresh_needed = true;
    }

    /// Called every frame: collects a finished refresh and starts a pending one.
    pub(super) fn pump_status_refresh(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.status_job.as_mut() {
            match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(map) => self.rule_statuses = map,
                JobPoll::Panicked(msg) => {
                    self.set_status(format!("Could not read the rules' status: {}", msg))
                }
            }
            self.status_job = None;
        }

        // Reading while a job writes would report transient differences.
        if !self.status_refresh_needed || self.is_busy() {
            return;
        }
        self.status_refresh_needed = false;

        let env = self.env.clone();
        let rules = self.config.extensions.clone();
        let backups = self.backup_store.clone();
        let status_ctx = StatusContext {
            editor_command: build_editor_command(&self.config.code_executable),
            wsl_available: self.wsl_detected,
            icon_dir: self.config.icon_destination_folder.clone(),
        };
        let repaint_ctx = ctx.clone();
        self.status_job = Some(BackgroundJob::spawn(
            "Reading rule status",
            move || repaint_ctx.request_repaint(),
            move |_| {
                let target = (env.assoc_target)();
                rules
                    .iter()
                    .map(|rule| {
                        let backup = backups.find(&rule.extension);
                        let status = rule_status(&target, rule, backup, &status_ctx);
                        (rule.extension.clone(), status)
                    })
                    .collect()
            },
        ));
    }
}

/// "Applied" / "Changed" pill for a rule (nothing when not applied or not
/// computed yet). "Changed" explains itself on hover.
pub(super) fn status_badge(ui: &mut egui::Ui, theme: ThemeMode, status: Option<&RuleStatus>) {
    let palette = get_palette(theme);
    match status {
        Some(RuleStatus::Applied) => {
            draw_badge(ui, "Applied", palette.success)
                .on_hover_text("The registry matches this rule.");
        }
        Some(RuleStatus::Drifted(reasons)) => {
            draw_badge(ui, "Changed", palette.warning).on_hover_text(format!(
                "Applied, but the registry differs now:
• {}

Apply again to restore it.",
                reasons
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(
                        "
• "
                    )
            ));
        }
        Some(RuleStatus::NotApplied) | None => {}
    }
}

/// Full list of differences, shown in the rule's detail panel.
pub(super) fn status_details(ui: &mut egui::Ui, theme: ThemeMode, status: Option<&RuleStatus>) {
    if let Some(RuleStatus::Drifted(reasons)) = status {
        let palette = get_palette(theme);
        ui.add_space(4.0);
        for reason in reasons {
            ui.label(RichText::new(format!("• {}", reason)).color(palette.warning));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::jobs::RunningJob;
    use super::super::test_support::*;
    use super::*;
    use crate::associations::DriftReason;
    use crate::gallery::PakStats;
    use egui::RawInput;
    use std::time::{Duration, Instant};

    fn run_until_status_ready(app: &mut WinTuneApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while (app.status_refresh_needed || app.status_job.is_some()) && Instant::now() < deadline {
            frame(app, ctx, RawInput::default());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            app.status_job.is_none() && !app.status_refresh_needed,
            "status never refreshed"
        );
    }

    #[test]
    fn refresh_runs_in_background_and_reports_not_applied() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        app.request_status_refresh();
        run_until_status_ready(&mut app, &ctx);

        assert_eq!(app.rule_statuses.get(".foo"), Some(&RuleStatus::NotApplied));
    }

    #[test]
    fn status_becomes_applied_after_an_apply() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let rules = app.config.extensions.clone();

        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);
        run_until_status_ready(&mut app, &ctx);

        assert_eq!(
            app.rule_statuses.get(".foo"),
            Some(&RuleStatus::Applied),
            "{:?}",
            app.rule_statuses
        );
    }

    #[test]
    fn a_refresh_waits_for_the_running_job() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
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

        app.request_status_refresh();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.status_job.is_none(), "refresh started during a job");

        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        run_until_status_ready(&mut app, &ctx);
        assert_eq!(app.rule_statuses.get(".foo"), Some(&RuleStatus::NotApplied));
    }

    #[test]
    fn the_tabs_render_with_statuses() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".bar"]);
        app.rule_statuses.insert(".foo".into(), RuleStatus::Applied);
        app.rule_statuses.insert(
            ".bar".into(),
            RuleStatus::Drifted(vec![DriftReason::ProgIdMissing {
                prog_id: "x".into(),
            }]),
        );
        app.selected_ext_index = 1;

        for tab in super::super::ActiveTab::ALL {
            app.active_tab = tab;
            frame(&mut app, &ctx, RawInput::default());
        }
    }
}
