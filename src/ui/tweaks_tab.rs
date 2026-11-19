//! "Explorer & Taskbar" tab: per-user tweaks with live status, pending
//! changes applied in one go, and per-tweak / global "restore original".

use super::confirm::{Confirm, ConfirmAction};
use super::jobs::{RunningJob, TweakOutcome};
use super::WinTuneApp;
use crate::tasks::BackgroundJob;
use crate::theme::{accent_button, card_frame, draw_badge, get_palette};
use crate::tweaks::{
    apply_tweaks, catalog, load_tweak_store, restore_tweaks, tweak_status, OsRequirement, Refresh,
    Tweak, TweakBackupStore, TweakGroup, TweakOptions, TweakStatus,
};
use eframe::egui::{self, Margin, RichText};

impl WinTuneApp {
    pub(super) fn reload_tweak_store(&mut self) {
        match load_tweak_store(&self.env.tweak_store_path) {
            Ok(store) => {
                self.tweak_store = store;
                self.tweak_store_error = None;
            }
            Err(e) => {
                self.tweak_store = TweakBackupStore::default();
                self.set_status(e.clone());
                self.tweak_store_error = Some(e);
            }
        }
    }

    /// Re-reads the 9 tweak values (a few registry reads: cheap, but never
    /// done per frame).
    pub(super) fn refresh_tweak_statuses(&mut self) {
        let target = (self.env.tweak_target)();
        self.tweak_statuses = catalog()
            .iter()
            .map(|t| {
                let status = tweak_status(&target, t)
                    .unwrap_or_else(|e| TweakStatus::Custom(format!("unreadable: {}", e)));
                (t.id, status)
            })
            .collect();
    }

    fn tweak_available(&self, tweak: &Tweak) -> bool {
        tweak.os == OsRequirement::Any || self.env.is_windows_11
    }

    fn tweak_is_on(&self, id: &str) -> bool {
        self.tweak_statuses.get(id) == Some(&TweakStatus::On)
    }

    /// Changes the user asked for that differ from the registry, available
    /// tweaks only, in catalog order.
    pub(super) fn pending_tweak_changes(&self) -> Vec<(String, bool)> {
        catalog()
            .iter()
            .filter(|t| self.tweak_available(t))
            .filter_map(|t| {
                let wanted = *self.tweak_desired.get(t.id)?;
                (wanted != self.tweak_is_on(t.id)).then(|| (t.id.to_string(), wanted))
            })
            .collect()
    }

    fn user_backups(&self) -> Vec<String> {
        self.tweak_store
            .backups_of(&self.env.user_label)
            .map(|b| b.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn tweak_options(&self) -> (String, bool, std::path::PathBuf, std::path::PathBuf) {
        (
            self.env.user_label.clone(),
            self.env.is_windows_11,
            self.env.tweak_store_path.clone(),
            self.env.reg_backup_dir.clone(),
        )
    }

    pub(super) fn start_apply_tweaks(&mut self, ctx: &egui::Context) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(err) = &self.tweak_store_error {
            let msg = format!("Tweak changes are blocked: {}", err);
            self.set_status(msg);
            return;
        }
        let changes = self.pending_tweak_changes();
        if changes.is_empty() {
            self.set_status("No tweak change to apply.");
            return;
        }

        let env = self.env.clone();
        let mut store = self.tweak_store.clone();
        let (user, is_windows_11, store_path, reg_backup_dir) = self.tweak_options();
        let repaint_ctx = ctx.clone();
        let job = BackgroundJob::spawn(
            "Applying tweaks",
            move || repaint_ctx.request_repaint(),
            move |sink| {
                let progress = |d: usize, t: usize, l: &str| sink.report(d, t, l);
                let opts = TweakOptions {
                    user,
                    is_windows_11,
                    store_path,
                    reg_backup_dir,
                    progress: &progress,
                };
                let report = apply_tweaks(&(env.tweak_target)(), &mut store, &changes, &opts);
                (env.notify_settings)();
                TweakOutcome {
                    report,
                    store,
                    restored: false,
                }
            },
        );
        self.job = Some(RunningJob::Tweaks(job));
        self.set_status("Applying tweaks…");
    }

    /// Asks before putting tweaks back to their state before WinTune.
    pub(super) fn request_restore_tweaks(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let titles: Vec<&str> = ids
            .iter()
            .filter_map(|id| crate::tweaks::find_tweak(id).map(|t| t.title))
            .collect();
        self.pending_confirm = Some(Confirm {
            title: "Restore original settings".to_string(),
            message: format!(
                "Put {} setting(s) back exactly as they were before WinTune changed them?\n\n{}",
                ids.len(),
                titles.join("\n")
            ),
            confirm_label: "Restore".to_string(),
            action: ConfirmAction::RestoreTweaks(ids),
        });
    }

    pub(super) fn start_restore_tweaks(&mut self, ctx: &egui::Context, ids: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(err) = &self.tweak_store_error {
            let msg = format!("Restore is blocked: {}", err);
            self.set_status(msg);
            return;
        }
        let env = self.env.clone();
        let mut store = self.tweak_store.clone();
        let (user, is_windows_11, store_path, reg_backup_dir) = self.tweak_options();
        let repaint_ctx = ctx.clone();
        let job = BackgroundJob::spawn(
            "Restoring original settings",
            move || repaint_ctx.request_repaint(),
            move |sink| {
                let progress = |d: usize, t: usize, l: &str| sink.report(d, t, l);
                let opts = TweakOptions {
                    user,
                    is_windows_11,
                    store_path,
                    reg_backup_dir,
                    progress: &progress,
                };
                let report = restore_tweaks(&(env.tweak_target)(), &mut store, &ids, &opts);
                (env.notify_settings)();
                TweakOutcome {
                    report,
                    store,
                    restored: true,
                }
            },
        );
        self.job = Some(RunningJob::Tweaks(job));
        self.set_status("Restoring original settings…");
    }

    pub(super) fn finish_tweaks_job(&mut self, outcome: TweakOutcome) {
        self.tweak_store = outcome.store;
        self.tweak_desired.clear();
        self.refresh_tweak_statuses();
        let verb = if outcome.restored {
            "Restored"
        } else {
            "Applied"
        };
        self.show_report(verb, "tweak(s)", &outcome.report);
    }

    pub(super) fn show_tweaks_tab(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.config.theme);
        let busy = self.is_busy();
        let pending = self.pending_tweak_changes().len();
        let backups = self.user_backups();
        let blocked = self.tweak_store_error.is_some();

        egui::TopBottomPanel::top("tweaks_header")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.config.theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.heading("Explorer & Taskbar");
                            ui.label(
                                RichText::new(format!(
                                    "Per-user settings of {}. The first change of each setting records its original state, so it can be restored exactly.",
                                    self.env.user_label
                                ))
                                .color(palette.text_secondary),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !busy && !blocked && pending > 0,
                                    accent_button(format!("Apply ({})", pending), self.config.theme),
                                )
                                .clicked()
                            {
                                self.start_apply_tweaks(ctx);
                            }
                            if ui
                                .add_enabled(
                                    !busy && !blocked && !backups.is_empty(),
                                    egui::Button::new(format!(
                                        "Restore all originals ({})",
                                        backups.len()
                                    )),
                                )
                                .clicked()
                            {
                                self.request_restore_tweaks(backups.clone());
                            }
                            if pending > 0 && ui.add_enabled(!busy, egui::Button::new("Discard changes")).clicked() {
                                self.tweak_desired.clear();
                            }
                        });
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 4.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (group, title) in [
                        (TweakGroup::FileExplorer, "File Explorer"),
                        (TweakGroup::Taskbar, "Taskbar"),
                    ] {
                        card_frame(self.config.theme).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new(title).size(15.0).strong());
                            for tweak in catalog().iter().filter(|t| t.group == group) {
                                ui.separator();
                                self.tweak_row(ui, tweak, busy || blocked);
                            }
                        });
                        ui.add_space(10.0);
                    }
                });
            });
    }

    fn tweak_row(&mut self, ui: &mut egui::Ui, tweak: &'static Tweak, locked: bool) {
        let palette = get_palette(self.config.theme);
        let available = self.tweak_available(tweak);
        let current_on = self.tweak_is_on(tweak.id);
        let mut wanted = self
            .tweak_desired
            .get(tweak.id)
            .copied()
            .unwrap_or(current_on);
        let is_pending = available && wanted != current_on;
        let has_backup = self.tweak_store.has_backup(&self.env.user_label, tweak.id);

        ui.horizontal(|ui| {
            let checkbox = ui.add_enabled(available && !locked, egui::Checkbox::without_text(&mut wanted));
            if checkbox.changed() {
                if wanted == current_on {
                    self.tweak_desired.remove(tweak.id);
                } else {
                    self.tweak_desired.insert(tweak.id, wanted);
                }
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let title = RichText::new(tweak.title).strong();
                    ui.label(if available { title } else { title.color(palette.text_secondary) });
                    match self.tweak_statuses.get(tweak.id) {
                        Some(TweakStatus::On) => {
                            draw_badge(ui, "On", palette.success);
                        }
                        Some(TweakStatus::Custom(detail)) => {
                            draw_badge(ui, "Custom", palette.warning)
                                .on_hover_text(format!("Set to a value WinTune does not write ({}).", detail));
                        }
                        Some(TweakStatus::Off) | None => {}
                    }
                    if is_pending {
                        draw_badge(ui, if wanted { "Will turn on" } else { "Will turn off" }, palette.accent);
                    }
                    let refresh = match tweak.refresh {
                        Refresh::Live => "Instant",
                        Refresh::NewWindows => "New windows",
                        Refresh::ExplorerRestart => "Explorer restart",
                    };
                    draw_badge(ui, refresh, palette.text_secondary).on_hover_text(match tweak.refresh {
                        Refresh::Live => "Visible immediately.",
                        Refresh::NewWindows => "Visible in newly opened Explorer windows (or press F5).",
                        Refresh::ExplorerRestart => "Visible after restarting Explorer (Task Manager > Windows Explorer > Restart) or signing out.",
                    });
                    if tweak.os == OsRequirement::Windows11 {
                        draw_badge(ui, "Windows 11", palette.text_secondary)
                            .on_hover_text(if available { "Windows 11 only." } else { "Not available on this version of Windows." });
                    }
                    if tweak.unofficial {
                        draw_badge(ui, "Unofficial", palette.warning)
                            .on_hover_text("Relies on undocumented Windows behaviour that an update could change.");
                    }
                });
                ui.label(RichText::new(tweak.description).size(12.0).color(palette.text_secondary));
            });
            if has_backup {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!locked, egui::Button::new("Restore original"))
                        .clicked()
                    {
                        self.request_restore_tweaks(vec![tweak.id.to_string()]);
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::ActiveTab;
    use super::*;
    use egui::RawInput;

    const ADVANCED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";

    fn dword(fx: &TempIcons, rel: &str, name: &str) -> Option<u32> {
        fx.sandbox
            .root
            .open_subkey(fx.hkcu(rel))
            .ok()?
            .get_value(name)
            .ok()
    }

    fn tweaks_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app_with_rules(ctx, &[".foo"]);
        app.refresh_tweak_statuses();
        app.active_tab = ActiveTab::Tweaks;
        app
    }

    #[test]
    fn the_tab_renders_on_windows_10_and_11() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        for win11 in [true, false] {
            let mut app = tweaks_app(&fx, &ctx);
            app.env.is_windows_11 = win11;
            app.tweak_desired.insert("show_extensions", true);
            frame(&mut app, &ctx, RawInput::default());
        }
    }

    #[test]
    fn toggling_marks_a_change_pending_and_toggling_back_clears_it() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);

        app.tweak_desired.insert("show_extensions", true);
        assert_eq!(
            app.pending_tweak_changes(),
            vec![("show_extensions".to_string(), true)]
        );

        app.tweak_desired.insert("show_extensions", false);
        assert!(
            app.pending_tweak_changes().is_empty(),
            "same as the registry: nothing to do"
        );
    }

    #[test]
    fn apply_writes_the_pending_tweaks_then_refreshes() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_desired.insert("show_extensions", true);

        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            dword(&fx, ADVANCED, "HideFileExt"),
            Some(0),
            "{}",
            app.status_message
        );
        assert_eq!(
            app.tweak_statuses.get("show_extensions"),
            Some(&TweakStatus::On)
        );
        assert!(app.pending_tweak_changes().is_empty());
        assert!(app.tweak_store.has_backup("TEST\\user", "show_extensions"));
    }

    #[test]
    fn restore_asks_first_then_puts_the_original_back() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_desired.insert("show_hidden", true);
        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);
        assert_eq!(dword(&fx, ADVANCED, "Hidden"), Some(1));

        app.request_restore_tweaks(vec!["show_hidden".into()]);
        assert!(app.pending_confirm.is_some());
        assert!(!app.is_busy());
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            dword(&fx, ADVANCED, "Hidden"),
            None,
            "value did not exist before"
        );
        assert!(!app.tweak_store.has_backup("TEST\\user", "show_hidden"));
    }

    #[test]
    fn windows_11_only_tweaks_are_never_sent_on_windows_10() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.env.is_windows_11 = false;
        app.tweak_desired.insert("taskbar_left", true);

        assert!(app.pending_tweak_changes().is_empty());
    }

    #[test]
    fn an_unreadable_snapshot_file_blocks_tweak_changes() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        let mut app = tweaks_app(&fx, &ctx);
        app.reload_tweak_store();
        app.tweak_desired.insert("show_extensions", true);

        app.start_apply_tweaks(&ctx);

        assert!(!app.is_busy());
        assert!(
            app.status_message.contains("blocked"),
            "{}",
            app.status_message
        );
        assert_eq!(dword(&fx, ADVANCED, "HideFileExt"), None);
    }
}
