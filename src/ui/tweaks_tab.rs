//! The tweak tabs ("Explorer & Taskbar", "Start & Privacy"): per-user tweaks
//! with live status, pending changes applied in one go, and per-tweak or
//! per-tab "restore original". Each tab only ever applies, discards or
//! restores the tweaks of its own groups.

use super::confirm::{Confirm, ConfirmAction};
use super::jobs::{RunningJob, TweakOutcome};
use super::kit::ToastKind;
use super::ReportKind;
use super::{ActiveTab, WinTuneApp};
use crate::i18n::messages;
use crate::tasks::BackgroundJob;
use crate::theme::{accent_button, card_frame, draw_badge, get_palette};
use crate::tweaks::{
    apply_tweaks, catalog, load_tweak_store, restore_tweaks, tweak_status, CustomReason,
    OsRequirement, Refresh, Tweak, TweakBackupStore, TweakGroup, TweakOptions, TweakStatus,
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
                self.notify(ToastKind::Error, e.clone());
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
                    .unwrap_or_else(|e| TweakStatus::Custom(CustomReason::Unreadable(e)));
                (t.id, status)
            })
            .collect();
    }

    /// Tweaks of `group` matching the search box (title or description,
    /// case-insensitive), in catalog order.
    pub(super) fn visible_tweaks(&self, group: TweakGroup) -> Vec<&'static Tweak> {
        let query = self.tweak_search.trim().to_lowercase();
        let tr = self.tr;
        catalog()
            .iter()
            .filter(|t| t.group == group)
            .filter(|t| {
                query.is_empty()
                    || messages::tweak_title(tr, t.id)
                        .to_lowercase()
                        .contains(&query)
                    || messages::tweak_description(tr, t.id)
                        .to_lowercase()
                        .contains(&query)
            })
            .collect()
    }

    fn tweak_available(&self, tweak: &Tweak) -> bool {
        tweak.os == OsRequirement::Any || self.env.is_windows_11
    }

    fn tweak_is_custom(&self, id: &str) -> bool {
        matches!(self.tweak_statuses.get(id), Some(TweakStatus::Custom(_)))
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
                // A "Custom" value is neither on nor off: either choice is a change.
                (wanted != self.tweak_is_on(t.id) || self.tweak_is_custom(t.id))
                    .then(|| (t.id.to_string(), wanted))
            })
            .collect()
    }

    /// The tweak groups a tab shows (both tweaks tabs share this file).
    pub(super) fn tweak_groups_of(tab: ActiveTab) -> &'static [TweakGroup] {
        match tab {
            ActiveTab::StartPrivacy => &[TweakGroup::Start, TweakGroup::Privacy],
            _ => &[TweakGroup::FileExplorer, TweakGroup::Taskbar],
        }
    }

    /// Pending changes of the tweaks in `groups` only (one tab).
    pub(super) fn pending_tweak_changes_in(&self, groups: &[TweakGroup]) -> Vec<(String, bool)> {
        self.pending_tweak_changes()
            .into_iter()
            .filter(|(id, _)| {
                crate::tweaks::find_tweak(id).is_some_and(|t| groups.contains(&t.group))
            })
            .collect()
    }

    /// "Discard changes" of one tab: the other tab keeps its ticks.
    pub(super) fn discard_tweak_changes(&mut self, tab: ActiveTab) {
        let groups = Self::tweak_groups_of(tab);
        self.tweak_desired.retain(|id, _| {
            !crate::tweaks::find_tweak(id).is_some_and(|t| groups.contains(&t.group))
        });
    }

    /// History / status name of the module a tweaks tab belongs to.
    pub(super) fn tweaks_module_of(tab: ActiveTab) -> &'static str {
        match tab {
            ActiveTab::StartPrivacy => ActiveTab::StartPrivacy.title(),
            _ => ActiveTab::Tweaks.title(),
        }
    }

    /// Ids of this user's recorded originals that belong to `groups`.
    fn user_backups_in(&self, groups: &[TweakGroup]) -> Vec<String> {
        self.tweak_store
            .backups_of(&self.env.user_label)
            .map(|b| {
                b.keys()
                    .filter(|id| {
                        crate::tweaks::find_tweak(id).is_some_and(|t| groups.contains(&t.group))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// "Restore all originals" of one tab: only the tweaks that tab shows.
    pub(super) fn request_restore_all_tweaks(&mut self, tab: ActiveTab) {
        let ids = self.user_backups_in(Self::tweak_groups_of(tab));
        self.request_restore_tweaks(ids);
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
            self.notify(ToastKind::Error, msg);
            return;
        }
        let tab = self.active_tab;
        let changes = self.pending_tweak_changes_in(Self::tweak_groups_of(tab));
        let module = Self::tweaks_module_of(tab);
        if changes.is_empty() {
            self.notify(ToastKind::Info, "No tweak change to apply.");
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
                    module,
                }
            },
        );
        self.job = Some(RunningJob::Tweaks(job, module));
    }

    /// Asks before putting tweaks back to their state before WinTune.
    pub(super) fn request_restore_tweaks(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let tr = self.tr;
        let titles: Vec<String> = ids
            .iter()
            .filter_map(|id| crate::tweaks::find_tweak(id).map(|t| messages::tweak_title(tr, t.id)))
            .collect();
        self.pending_confirm = Some(Confirm {
            title: tr.t("confirm.restore_tweaks.title"),
            message: format!(
                "{}\n\n{}",
                tr.t_args("confirm.restore_tweaks.body", &[&ids.len()]),
                titles.join("\n")
            ),
            confirm_label: tr.t("confirm.restore"),
            action: ConfirmAction::RestoreTweaks(ids),
        });
    }

    pub(super) fn start_restore_tweaks(&mut self, ctx: &egui::Context, ids: Vec<String>) {
        if self.refuse_if_busy() {
            return;
        }
        if let Some(err) = &self.tweak_store_error {
            let msg = format!("Restore is blocked: {}", err);
            self.notify(ToastKind::Error, msg);
            return;
        }
        let env = self.env.clone();
        let mut store = self.tweak_store.clone();
        let (user, is_windows_11, store_path, reg_backup_dir) = self.tweak_options();
        let repaint_ctx = ctx.clone();
        let module = Self::tweaks_module_of(self.active_tab);
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
                    module,
                }
            },
        );
        self.job = Some(RunningJob::Tweaks(job, module));
    }

    pub(super) fn finish_tweaks_job(&mut self, outcome: TweakOutcome) {
        self.tweak_store = outcome.store;
        self.refresh_tweak_statuses();
        // Keep only ticks that still differ from the registry: the ones this
        // job wrote (or restored) drop out, the other tab's ticks stay.
        let statuses = self.tweak_statuses.clone();
        self.tweak_desired
            .retain(|id, wanted| match statuses.get(id) {
                Some(TweakStatus::On) => !*wanted,
                Some(TweakStatus::Custom(_)) => true,
                _ => *wanted,
            });
        let kind = if outcome.restored {
            ReportKind::RestoredTweaks
        } else {
            ReportKind::AppliedTweaks
        };
        self.show_report(kind, &outcome.report);
        let action = if outcome.restored { "Restore" } else { "Apply" };
        self.record_history(outcome.module, action, &outcome.report);
    }

    pub(super) fn show_tweaks_tab(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.ui_theme);
        let busy = self.is_busy();
        let pending = self
            .pending_tweak_changes_in(Self::tweak_groups_of(self.active_tab))
            .len();
        let tab = self.active_tab;
        let groups = Self::tweak_groups_of(tab);
        let backups = self.user_backups_in(groups);
        let blocked = self.tweak_store_error.is_some();

        egui::TopBottomPanel::top("tweaks_header")
            .frame(
                egui::Frame::none()
                    .fill(palette.bg_base)
                    .inner_margin(Margin::symmetric(16.0, 12.0)),
            )
            .show(ctx, |ui| {
                card_frame(self.ui_theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.heading(tab.title());
                            ui.label(
                                RichText::new(format!(
                                    "Per-user settings of {}. The first change of each setting records its original state, so it can be restored exactly.",
                                    self.env.user_label
                                ))
                                .color(palette.text_secondary),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let apply_reason = self.tweak_apply_disabled_reason();
                            let apply = ui.add_enabled(
                                apply_reason.is_none(),
                                accent_button(format!("Apply ({})", pending), self.ui_theme),
                            );
                            let apply = match &apply_reason {
                                Some(r) => apply.on_disabled_hover_text(r),
                                None => apply,
                            };
                            if apply.clicked() {
                                self.start_apply_tweaks(ctx);
                            }
                            let restore_reason = self.tweak_restore_all_disabled_reason(backups.len());
                            let restore = ui.add_enabled(
                                restore_reason.is_none(),
                                egui::Button::new(format!(
                                    "Restore all originals ({})",
                                    backups.len()
                                )),
                            );
                            let restore = match &restore_reason {
                                Some(r) => restore.on_disabled_hover_text(r),
                                None => restore,
                            };
                            if restore.clicked() {
                                self.request_restore_all_tweaks(tab);
                            }
                            if pending > 0 && ui.add_enabled(!busy, egui::Button::new("Discard changes")).clicked() {
                                self.discard_tweak_changes(self.active_tab);
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
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Search:").strong());
                    let search = ui.add(
                        egui::TextEdit::singleline(&mut self.tweak_search)
                            .id(super::polish::search_box_id(ActiveTab::Tweaks)
                                .expect("search box"))
                            .hint_text("extensions, taskbar, quick access…  Ctrl+F")
                            .desired_width(280.0),
                    );
                    self.focus_search_if_requested(&search);
                    if !self.tweak_search.is_empty() && ui.small_button("Clear").clicked() {
                        self.tweak_search.clear();
                    }
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut shown = 0;
                    for &group in groups {
                        let title = messages::group_title(self.tr, group);
                        let tweaks = self.visible_tweaks(group);
                        if tweaks.is_empty() {
                            continue;
                        }
                        shown += tweaks.len();
                        card_frame(self.ui_theme).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(RichText::new(title).size(15.0).strong());
                            for tweak in tweaks {
                                ui.separator();
                                self.tweak_row(ui, tweak, busy || blocked);
                            }
                        });
                        ui.add_space(10.0);
                    }
                    if shown == 0 {
                        ui.label(
                            RichText::new(format!(
                                "No tweak matches \"{}\".",
                                self.tweak_search.trim()
                            ))
                            .color(palette.text_secondary),
                        );
                    }
                });
            });
    }

    fn tweak_row(&mut self, ui: &mut egui::Ui, tweak: &'static Tweak, locked: bool) {
        let palette = get_palette(self.ui_theme);
        let available = self.tweak_available(tweak);
        let current_on = self.tweak_is_on(tweak.id);
        let mut wanted = self
            .tweak_desired
            .get(tweak.id)
            .copied()
            .unwrap_or(current_on);
        let custom = self.tweak_is_custom(tweak.id);
        let is_pending = available
            && (wanted != current_on || (custom && self.tweak_desired.contains_key(tweak.id)));
        let has_backup = self.tweak_store.has_backup(&self.env.user_label, tweak.id);

        ui.horizontal(|ui| {
            let checkbox = ui.add_enabled(available && !locked, egui::Checkbox::without_text(&mut wanted));
            if checkbox.changed() {
                if wanted == current_on && !custom {
                    self.tweak_desired.remove(tweak.id);
                } else {
                    self.tweak_desired.insert(tweak.id, wanted);
                }
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let title = RichText::new(messages::tweak_title(self.tr, tweak.id)).strong();
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
                ui.label(RichText::new(messages::tweak_description(self.tr, tweak.id)).size(12.0).color(palette.text_secondary));
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
            app.last_notice()
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

    fn visible_ids(app: &WinTuneApp) -> Vec<&'static str> {
        TweakGroup::ALL
            .into_iter()
            .flat_map(|g| app.visible_tweaks(g))
            .map(|t| t.id)
            .collect()
    }

    #[test]
    fn search_filters_on_title_and_description_ignoring_case() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);

        app.tweak_search = "TASK VIEW".into();
        assert_eq!(visible_ids(&app), ["hide_task_view"]);

        app.tweak_search = "quick access".into(); // matched in descriptions
        let ids = visible_ids(&app);
        assert!(
            ids.contains(&"hide_recent_files") && ids.contains(&"open_this_pc"),
            "{:?}",
            ids
        );

        app.tweak_search = "   ".into();
        assert_eq!(visible_ids(&app).len(), crate::tweaks::catalog().len());
    }

    #[test]
    fn each_tweaks_tab_has_its_own_groups_and_renders() {
        assert_eq!(
            WinTuneApp::tweak_groups_of(ActiveTab::Tweaks),
            [TweakGroup::FileExplorer, TweakGroup::Taskbar]
        );
        assert_eq!(
            WinTuneApp::tweak_groups_of(ActiveTab::StartPrivacy),
            [TweakGroup::Start, TweakGroup::Privacy]
        );
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.active_tab = ActiveTab::StartPrivacy;
        frame(&mut app, &ctx, RawInput::default());
    }

    #[test]
    fn restore_all_only_targets_the_tweaks_of_the_current_tab() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_desired.insert("show_extensions", true);
        app.start_apply_tweaks(&ctx); // Explorer & Taskbar tab
        run_until_idle(&mut app, &ctx);
        app.active_tab = ActiveTab::StartPrivacy;
        app.tweak_desired.insert("no_app_launch_tracking", true);
        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);

        app.request_restore_all_tweaks(ActiveTab::StartPrivacy);

        match app.pending_confirm.as_ref().map(|c| c.action.clone()) {
            Some(super::super::confirm::ConfirmAction::RestoreTweaks(ids)) => {
                assert_eq!(ids, vec!["no_app_launch_tracking".to_string()])
            }
            other => panic!("unexpected confirmation: {:?}", other),
        }
    }

    #[test]
    fn a_search_without_match_shows_nothing_and_still_renders() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_search = "zzz-no-such-tweak".into();

        assert!(visible_ids(&app).is_empty());
        frame(&mut app, &ctx, RawInput::default());
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
            app.last_notice().contains("blocked"),
            "{}",
            app.last_notice()
        );
        assert_eq!(dword(&fx, ADVANCED, "HideFileExt"), None);
    }
}
