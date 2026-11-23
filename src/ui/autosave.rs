//! Debounced autosave of config.json: an edit is written one second after
//! the last change, and always before the window closes.

use super::{kit, WinTuneApp};
use crate::models::AppConfig;
use eframe::egui;
use std::time::Duration;

/// Quiet time after the last edit before the config is written.
pub(super) const AUTOSAVE_DELAY_SECONDS: f64 = 1.0;

pub(super) struct Autosave {
    /// The last copy written, or the one loaded at startup.
    saved: AppConfig,
    /// The config as of the last frame.
    seen: AppConfig,
    /// Time of the last edit, while a save is waiting.
    changed_at: Option<f64>,
    /// Why the last save failed (shown as a banner).
    pub error: Option<String>,
}

impl Autosave {
    pub(super) fn new(config: &AppConfig) -> Self {
        Self {
            saved: config.clone(),
            seen: config.clone(),
            changed_at: None,
            error: None,
        }
    }

    /// True when the config differs from what is on disk.
    pub(super) fn pending(&self) -> bool {
        self.seen != self.saved
    }
}

impl WinTuneApp {
    /// Called every frame: notices edits and saves once they have settled.
    pub(super) fn pump_autosave(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.config != self.autosave.seen {
            self.autosave.seen = self.config.clone();
            self.autosave.changed_at = Some(now);
        }
        if self.config_error.is_some() {
            // The file on disk is unreadable: never replace it.
            self.autosave.changed_at = None;
            return;
        }
        if let Some(changed_at) = self.autosave.changed_at {
            let waited = now - changed_at;
            if waited >= AUTOSAVE_DELAY_SECONDS {
                self.autosave.changed_at = None;
                self.save_now();
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(AUTOSAVE_DELAY_SECONDS - waited));
            }
        }
    }

    /// Saves now if the config differs from the file (close, banner retry).
    pub(super) fn flush_config(&mut self) {
        if self.config_error.is_some() {
            return;
        }
        if self.config != self.autosave.seen {
            self.autosave.seen = self.config.clone();
        }
        if self.autosave.pending() {
            self.autosave.changed_at = None;
            self.save_now();
        }
    }

    /// Banner above the page while the last save failed.
    pub(super) fn show_autosave_banner(&mut self, ctx: &egui::Context) {
        let Some(error) = self.autosave.error.clone() else {
            return;
        };
        let text = self.tr.t_args("autosave.failed", &[&error]);
        let retry = self.tr.t("autosave.retry");
        let mut clicked = false;
        egui::TopBottomPanel::top("autosave_banner").show(ctx, |ui| {
            clicked = kit::banner(ui, kit::BannerKind::Error, &text, Some(&retry));
        });
        if clicked {
            self.flush_config();
        }
    }

    fn save_now(&mut self) {
        match self.try_save_config() {
            Ok(()) => {
                self.autosave.saved = self.config.clone();
                self.autosave.error = None;
            }
            Err(e) => self.autosave.error = Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use egui::{RawInput, ViewportCommand};
    use std::fs;
    use std::time::Instant;

    fn at(t: f64) -> RawInput {
        RawInput {
            time: Some(t),
            ..Default::default()
        }
    }

    fn close_at(t: f64) -> RawInput {
        let mut input = close_request();
        input.time = Some(t);
        input
    }

    fn on_disk(fx: &TempIcons) -> String {
        fs::read_to_string(fx.dir.join("config.json")).unwrap_or_default()
    }

    #[test]
    fn an_edit_is_saved_after_the_delay_not_before() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.config.code_executable = "edit-one.exe".into();

        frame(&mut app, &ctx, at(0.0));
        frame(&mut app, &ctx, at(0.5));
        assert!(!on_disk(&fx).contains("edit-one.exe"));

        frame(&mut app, &ctx, at(1.1));
        assert!(on_disk(&fx).contains("edit-one.exe"));
    }

    #[test]
    fn each_edit_restarts_the_delay() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.config.code_executable = "first.exe".into();
        frame(&mut app, &ctx, at(0.0));
        app.config.code_executable = "second.exe".into();
        frame(&mut app, &ctx, at(0.8));

        frame(&mut app, &ctx, at(1.2));
        assert_eq!(on_disk(&fx), "");

        frame(&mut app, &ctx, at(1.9));
        assert!(on_disk(&fx).contains("second.exe"));
    }

    #[test]
    fn closing_right_after_an_edit_saves_it() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.config.code_executable = "late.exe".into();
        frame(&mut app, &ctx, at(0.0));

        frame(&mut app, &ctx, close_at(0.1));

        assert!(on_disk(&fx).contains("late.exe"));
    }

    #[test]
    fn closing_after_a_job_saves_edits_made_during_it() {
        use super::super::jobs::RunningJob;
        use crate::gallery::PakStats;
        use crate::tasks::BackgroundJob;

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
        app.config.code_executable = "during-job.exe".into();
        let out = frame(&mut app, &ctx, close_at(0.0));
        assert!(commands(&out).contains(&ViewportCommand::CancelClose));
        assert!(!on_disk(&fx).contains("during-job.exe"));

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let mut closed = false;
        let mut t = 0.1;
        while !closed && Instant::now() < deadline {
            closed = commands(&frame(&mut app, &ctx, at(t))).contains(&ViewportCommand::Close);
            t += 0.01;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(closed, "window was never closed after the job ended");
        assert!(on_disk(&fx).contains("during-job.exe"));
    }

    #[test]
    fn an_unreadable_config_is_never_overwritten_by_autosave() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let path = fx.dir.join("config.json");
        fs::write(&path, "{ broken").unwrap();
        let (config, error) = WinTuneApp::load_or_init_config(&path);
        let mut app = fx.app_with(&ctx, config);
        app.config_error = error;
        app.config.code_executable = "edit.exe".into();

        for t in 0..=5 {
            frame(&mut app, &ctx, at(t as f64));
        }
        frame(&mut app, &ctx, close_at(5.5));

        assert_eq!(fs::read_to_string(&path).unwrap(), "{ broken");
        assert!(app.autosave.error.is_none());
    }

    #[test]
    fn a_failed_save_shows_one_banner_until_a_retry_succeeds() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let blocker = fx.dir.join("blocker");
        fs::write(&blocker, "a file").unwrap();
        app.env.config_path = blocker.join("config.json");
        app.config.code_executable = "edit.exe".into();

        frame(&mut app, &ctx, at(0.0));
        frame(&mut app, &ctx, at(2.0));
        assert!(app.autosave.error.is_some());
        assert!(app.autosave.changed_at.is_none());

        for t in 3..8 {
            frame(&mut app, &ctx, at(t as f64));
            assert!(app.autosave.changed_at.is_none());
        }

        app.env.config_path = fx.dir.join("config.json");
        app.flush_config();
        assert!(app.autosave.error.is_none());
        assert!(on_disk(&fx).contains("edit.exe"));
    }

    #[test]
    fn set_language_with_an_unreadable_config_switches_but_writes_nothing() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let path = fx.dir.join("config.json");
        fs::write(&path, "{ broken").unwrap();
        let mut app = fx.app(&ctx);
        app.config_error = Some("unreadable".into());

        app.set_language("fr");
        for t in 0..=5 {
            frame(&mut app, &ctx, at(t as f64));
        }

        assert_eq!(app.tr.code(), "fr");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ broken");
    }

    #[test]
    fn the_banner_is_drawn_and_its_retry_button_saves() {
        use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.config.code_executable = "retry.exe".into();
        app.autosave.error = Some("disk full".into());
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let input = |t: f64, events: Vec<Event>| RawInput {
            time: Some(t),
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };

        frame(&mut app, &ctx, input(0.0, vec![]));
        let out = frame(&mut app, &ctx, input(0.01, vec![]));
        let banner = app.tr.t_args("autosave.failed", &[&"disk full"]);
        assert!(drawn_texts(&out).contains(&banner), "banner not drawn");
        let retry = text_center(&out, &app.tr.t("autosave.retry")).expect("retry button drawn");

        let button = |pressed| Event::PointerButton {
            pos: retry,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        frame(
            &mut app,
            &ctx,
            input(0.02, vec![Event::PointerMoved(retry)]),
        );
        frame(&mut app, &ctx, input(0.03, vec![button(true)]));
        assert!(app.autosave.error.is_some(), "saved before the release");
        frame(&mut app, &ctx, input(0.04, vec![button(false)]));

        assert!(app.autosave.error.is_none());
        assert!(on_disk(&fx).contains("retry.exe"));
    }
}
