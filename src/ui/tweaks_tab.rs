//! The tweak tabs ("Explorer & Taskbar", "Start & Privacy"): per-user tweaks
//! with live status, pending changes applied in one go, and per-tweak or
//! per-tab "restore original". Each tab only ever applies, discards or
//! restores the tweaks of its own groups.

use super::confirm::{Confirm, ConfirmAction};
use super::jobs::{RunningJob, TweakOutcome};
use super::kit::{self, space, text_size, PillKind, ToastKind};
use super::ReportKind;
use super::{ActiveTab, WinTuneApp};
use crate::i18n::{messages, Tr};
use crate::tasks::BackgroundJob;
use crate::tweaks::{
    apply_tweaks, catalog, load_tweak_store, restore_tweaks, tweak_status, CustomReason,
    OsRequirement, Refresh, Tweak, TweakBackupStore, TweakGroup, TweakOptions, TweakStatus,
};
use eframe::egui::{self, RichText};

/// When a tweak's change becomes visible, as a row's small secondary line.
pub(super) fn refresh_label(tr: Tr, r: Refresh) -> String {
    match r {
        Refresh::Live => tr.t("refresh.live"),
        Refresh::NewWindows => tr.t("refresh.new_windows"),
        Refresh::ExplorerRestart => tr.t("refresh.explorer_restart"),
    }
}

/// The hover text of `refresh_label`: how to see the change.
pub(super) fn refresh_hint(tr: Tr, r: Refresh) -> String {
    match r {
        Refresh::Live => tr.t("refresh.live_hint"),
        Refresh::NewWindows => tr.t("refresh.new_windows_hint"),
        Refresh::ExplorerRestart => tr.t("refresh.explorer_restart_hint"),
    }
}

impl WinTuneApp {
    /// Why a tweak's toggle is disabled: a running job, an unreadable
    /// `tweaks-backups.json`, then a Windows 11-only tweak on Windows 10.
    pub(super) fn tweak_toggle_disabled_reason(&self, tweak: &Tweak) -> Option<String> {
        self.busy_or_blocked(&self.tweak_store_error)
            .or_else(|| (!self.tweak_available(tweak)).then(|| self.tr.t("tweaks.win11_only")))
    }

    pub(super) fn reload_tweak_store(&mut self) {
        match load_tweak_store(&self.env.tweak_store_path) {
            Ok(store) => {
                self.tweak_store = store;
                self.tweak_store_error = None;
            }
            Err(e) => {
                self.tweak_store = TweakBackupStore::default();
                let text = messages::load_error(self.tr, &e);
                self.notify(ToastKind::Error, text);
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

    /// Whether wanting `wanted` for `tweak` is a change to apply: the one
    /// rule behind both the row's pending pill and `pending_tweak_changes`.
    /// `has_desired` says the user picked a value (in `tweak_desired`).
    /// Unavailable tweaks never are; a "Custom" value is neither on nor off,
    /// so either picked choice is a change.
    fn tweak_change_is_pending(&self, tweak: &Tweak, wanted: bool, has_desired: bool) -> bool {
        self.tweak_available(tweak)
            && (wanted != self.tweak_is_on(tweak.id)
                || (has_desired && self.tweak_is_custom(tweak.id)))
    }

    /// Changes the user asked for that differ from the registry, available
    /// tweaks only, in catalog order.
    pub(super) fn pending_tweak_changes(&self) -> Vec<(String, bool)> {
        catalog()
            .iter()
            .filter_map(|t| {
                let wanted = *self.tweak_desired.get(t.id)?;
                self.tweak_change_is_pending(t, wanted, true)
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

    /// The tweaks tab that shows tweak `id`, or `None` for an id the
    /// catalog does not know.
    pub(super) fn tab_of_tweak(id: &str) -> Option<ActiveTab> {
        let tweak = crate::tweaks::find_tweak(id)?;
        [ActiveTab::Tweaks, ActiveTab::StartPrivacy]
            .into_iter()
            .find(|tab| Self::tweak_groups_of(*tab).contains(&tweak.group))
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
        let tab = match tab {
            ActiveTab::StartPrivacy => ActiveTab::StartPrivacy,
            _ => ActiveTab::Tweaks,
        };
        tab.history_module().expect("a tweaks tab is a module")
    }

    /// History / status name of a tweak restore: the open tweaks tab's, or,
    /// from another page (History), the tab of the restored tweaks.
    fn restore_module_of(&self, ids: &[String]) -> &'static str {
        let tab = match self.active_tab {
            tab @ (ActiveTab::Tweaks | ActiveTab::StartPrivacy) => tab,
            _ => {
                let all_start = !ids.is_empty()
                    && ids
                        .iter()
                        .all(|id| Self::tab_of_tweak(id) == Some(ActiveTab::StartPrivacy));
                if all_start {
                    ActiveTab::StartPrivacy
                } else {
                    ActiveTab::Tweaks
                }
            }
        };
        Self::tweaks_module_of(tab)
    }

    /// Ids of this user's recorded originals that belong to `groups`.
    pub(super) fn user_backups_in(&self, groups: &[TweakGroup]) -> Vec<String> {
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
            let detail = messages::load_error(self.tr, err);
            let msg = self.tr.t_args("tweaks.blocked", &[&detail]);
            self.notify(ToastKind::Error, msg);
            return;
        }
        let tab = self.active_tab;
        let changes = self.pending_tweak_changes_in(Self::tweak_groups_of(tab));
        let module = Self::tweaks_module_of(tab);
        if changes.is_empty() {
            let msg = self.tr.t("tweaks.nothing_to_apply");
            self.notify(ToastKind::Info, msg);
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
        self.job = Some(RunningJob::Tweaks {
            job,
            module,
            restore: false,
        });
    }

    /// Asks before putting tweaks back to their state before WinTune.
    pub(super) fn request_restore_tweaks(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let tr = self.tr;
        // A tweak the catalog no longer knows is named by its raw id.
        let titles: Vec<String> = ids
            .iter()
            .map(|id| match crate::tweaks::find_tweak(id) {
                Some(t) => messages::tweak_title(tr, t.id),
                None => id.clone(),
            })
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
            let detail = messages::load_error(self.tr, err);
            let msg = self.tr.t_args("tweaks.blocked", &[&detail]);
            self.notify(ToastKind::Error, msg);
            return;
        }
        let env = self.env.clone();
        let mut store = self.tweak_store.clone();
        let (user, is_windows_11, store_path, reg_backup_dir) = self.tweak_options();
        let repaint_ctx = ctx.clone();
        let module = self.restore_module_of(&ids);
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
        self.job = Some(RunningJob::Tweaks {
            job,
            module,
            restore: true,
        });
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
        // Before the panel, so the bar spans the whole page width.
        self.tweaks_action_bar(ctx);
        Self::page_panel(ctx, |ui| {
            self.tweaks_header(ui);
            self.tweaks_search(ui);
            if let Some(e) = &self.tweak_store_error {
                let detail = messages::load_error(self.tr, e);
                let text = self.tr.t_args("tweaks.store_unreadable", &[&detail]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                ui.add_space(space::S);
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.tweak_sections(ui));
        });
    }

    /// The page's bar: how many changes this tab has pending with Discard
    /// and Apply, or the job's progress while one runs.
    fn tweaks_action_bar(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let tab = self.active_tab;
        let pending = self
            .pending_tweak_changes_in(Self::tweak_groups_of(tab))
            .len();
        let status = (pending > 0).then(|| tr.t_args("tweaks.pending_n", &[&pending]));
        let disabled_reason = self.tweak_apply_disabled_reason();
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("tweaks.apply");
        let discard_label = tr.t("tweaks.discard");

        let (mut apply, mut discard) = (false, false);
        // Right to left: Apply sits at the far right.
        self.page_action_bar(ctx, status.as_deref(), |ui| {
            let resp = ui.add_enabled(
                disabled_reason.is_none(),
                kit::primary_button(p, &apply_label),
            );
            let resp = match &disabled_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            apply = resp.clicked();
            discard = ui.add(kit::secondary_button(&discard_label)).clicked();
        });

        if apply {
            self.start_apply_tweaks(ctx);
        }
        if discard {
            self.discard_tweak_changes(tab);
        }
    }

    /// Breadcrumb, title, whose settings these are, and "Restore all
    /// originals" of this tab.
    fn tweaks_header(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let tab = self.active_tab;
        let backups = self.user_backups_in(Self::tweak_groups_of(tab)).len();
        let name = match tab {
            ActiveTab::StartPrivacy => tr.t("nav.start"),
            _ => tr.t("nav.explorer"),
        };
        let subtitle = tr.t_args("tweaks.subtitle", &[&self.env.user_label]);
        let restore_label = tr.t_args("tweaks.restore_all", &[&backups]);
        let restore_reason = self.tweak_restore_all_disabled_reason(backups);

        let mut restore_all = false;
        kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |ui| {
            let resp = ui.add_enabled(
                restore_reason.is_none(),
                kit::secondary_button(&restore_label),
            );
            let resp = match &restore_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            restore_all = resp.clicked();
        });
        if restore_all {
            self.request_restore_all_tweaks(tab);
        }
    }

    /// The search box (Ctrl+F) and its Clear button.
    fn tweaks_search(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        ui.horizontal(|ui| {
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.tweak_search)
                    .id(super::polish::search_box_id(ActiveTab::Tweaks).expect("search box"))
                    .hint_text(tr.t("tweaks.search"))
                    .desired_width(kit::SEARCH_WIDTH),
            );
            self.focus_search_if_requested(&search);
            if !self.tweak_search.is_empty() && ui.button(tr.t("tweaks.clear")).clicked() {
                self.tweak_search.clear();
            }
        });
        ui.add_space(space::L);
    }

    /// One section per group of this tab with a visible tweak, or what to
    /// show when the search matches nothing.
    fn tweak_sections(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let mut shown = 0;
        for &group in Self::tweak_groups_of(self.active_tab) {
            let tweaks = self.visible_tweaks(group);
            if tweaks.is_empty() {
                continue;
            }
            shown += tweaks.len();
            kit::section(ui, &messages::group_title(tr, group), |ui| {
                for tweak in tweaks {
                    self.tweak_row(ui, tweak);
                }
            });
        }
        if shown == 0 {
            let p = kit::current_palette(ui.ctx());
            let text = tr.t_args("tweaks.no_match", &[&self.tweak_search.trim()]);
            ui.label(
                RichText::new(text)
                    .size(text_size::BODY)
                    .color(p.text_secondary),
            );
        }
    }

    /// One tweak: title, description and when the change shows; then, right
    /// to left, the toggle, the `…` menu and the pills.
    fn tweak_row(&mut self, ui: &mut egui::Ui, tweak: &'static Tweak) {
        let tr = self.tr;
        let available = self.tweak_available(tweak);
        let current_on = self.tweak_is_on(tweak.id);
        let mut wanted = self
            .tweak_desired
            .get(tweak.id)
            .copied()
            .unwrap_or(current_on);
        let custom_hint = match self.tweak_statuses.get(tweak.id) {
            Some(TweakStatus::Custom(reason)) => Some(tr.t_args(
                "tweaks.custom_hint",
                &[&messages::custom_reason(tr, reason)],
            )),
            _ => None,
        };
        let custom = custom_hint.is_some();
        let desired = self.tweak_desired.contains_key(tweak.id);
        let has_backup = self.tweak_store.has_backup(&self.env.user_label, tweak.id);
        let toggle_reason = self.tweak_toggle_disabled_reason(tweak);
        let menu_reason = self.busy_or_blocked(&self.tweak_store_error);
        let title = messages::tweak_title(tr, tweak.id);
        let description = messages::tweak_description(tr, tweak.id);
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&title]);
        let restore_label = tr.t("tweaks.restore_original");
        let win11_hint = if available {
            tr.t("tweaks.win11_hint")
        } else {
            tr.t("tweaks.win11_only")
        };

        let mut changed = false;
        let mut restore = false;
        kit::SettingRow {
            icon: None,
            title: &title,
            description: Some(&description),
            hover: None,
        }
        .show_with_note(
            ui,
            &refresh_label(tr, tweak.refresh),
            &refresh_hint(tr, tweak.refresh),
            |ui| {
                changed = kit::toggle_enabled(ui, &mut wanted, toggle_reason.as_deref()).changed();
                if has_backup {
                    let items = [kit::MenuItem {
                        label: &restore_label,
                        enabled: menu_reason.is_none(),
                        disabled_hint: menu_reason.as_deref(),
                    }];
                    let salt = format!("tweak_menu_{}", tweak.id);
                    restore = kit::overflow_menu(ui, &salt, &more, &more_for, &items) == Some(0);
                }
                if let Some(hint) = &custom_hint {
                    kit::pill(ui, &tr.t("tweaks.pill.custom"), PillKind::Warning)
                        .on_hover_text(hint);
                }
                if tweak.os == OsRequirement::Windows11 {
                    kit::pill(ui, &tr.t("tweaks.pill.win11"), PillKind::Neutral)
                        .on_hover_text(&win11_hint);
                }
                if tweak.unofficial {
                    kit::pill(ui, &tr.t("tweaks.pill.unofficial"), PillKind::Warning)
                        .on_hover_text(tr.t("tweaks.unofficial_hint"));
                }
                // After the toggle, so the pill follows this frame's click (a
                // first click on a Custom tweak turns it on: `wanted` differs).
                if self.tweak_change_is_pending(tweak, wanted, desired) {
                    let label = if wanted {
                        tr.t("tweaks.pill.will_on")
                    } else {
                        tr.t("tweaks.pill.will_off")
                    };
                    kit::pill(ui, &label, PillKind::Accent);
                }
            },
        );

        if changed {
            if wanted == current_on && !custom {
                self.tweak_desired.remove(tweak.id);
            } else {
                self.tweak_desired.insert(tweak.id, wanted);
            }
        }
        if restore {
            self.request_restore_tweaks(vec![tweak.id.to_string()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::ActiveTab;
    use super::*;
    use egui::{Pos2, RawInput, Vec2, ViewportId};

    const ADVANCED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
    /// Size of a toggle and of a `…` button: no other widget of the tweak
    /// pages has these sizes.
    const TOGGLE: Vec2 = egui::vec2(40.0, 20.0);
    const MENU: Vec2 = kit::OVERFLOW_BUTTON;

    fn dword(fx: &TempIcons, rel: &str, name: &str) -> Option<u32> {
        fx.sandbox
            .root
            .open_subkey(fx.hkcu(rel))
            .ok()?
            .get_value(name)
            .ok()
    }

    fn set_dword(fx: &TempIcons, rel: &str, name: &str, value: u32) {
        let (key, _) = fx
            .sandbox
            .root
            .create_subkey(fx.hkcu(rel))
            .expect("create sandbox key");
        key.set_value(name, &value).expect("write sandbox value");
    }

    fn tweaks_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app_with_rules(ctx, &[".foo"]);
        app.refresh_tweak_statuses();
        app.active_tab = ActiveTab::Tweaks;
        app
    }

    /// A page tall enough to show every row without scrolling.
    fn tall_input(events: Vec<egui::Event>) -> RawInput {
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1180.0, 1600.0),
            )),
            events,
            ..Default::default()
        }
    }

    /// Hover, press, release at `pos`.
    fn click_at(app: &mut WinTuneApp, ctx: &egui::Context, pos: Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(app, ctx, tall_input(vec![egui::Event::PointerMoved(pos)]));
        frame(app, ctx, tall_input(vec![button(true)]));
        frame(app, ctx, tall_input(vec![button(false)]));
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        let out = frame(app, ctx, tall_input(vec![]));
        let pos = text_center(&out, text).unwrap_or_else(|| panic!("no text {text:?} drawn"));
        click_at(app, ctx, pos);
    }

    /// Center of the widget of exactly `size` on the row of tweak `id`, or
    /// `None` when the row has none. The row's description sits on the row's
    /// middle line, as do its trailing widgets; rows are at least 48 px apart.
    fn row_widget(app: &mut WinTuneApp, ctx: &egui::Context, id: &str, size: Vec2) -> Option<Pos2> {
        let out = frame(app, ctx, tall_input(vec![]));
        let description = messages::tweak_description(app.tr, id);
        let at = text_center(&out, &description).unwrap_or_else(|| panic!("row {id} not drawn"));
        ctx.viewport_for(ViewportId::ROOT, |v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, widgets)| widgets.iter())
                .filter(|w| w.rect.size() == size && w.rect.center().x > at.x)
                .map(|w| w.rect.center())
                .filter(|c| (c.y - at.y).abs() < 24.0)
                .min_by(|a, b| (a.y - at.y).abs().total_cmp(&(b.y - at.y).abs()))
        })
    }

    fn toggle_of(app: &mut WinTuneApp, ctx: &egui::Context, id: &str) -> Pos2 {
        row_widget(app, ctx, id, TOGGLE).unwrap_or_else(|| panic!("no toggle on row {id}"))
    }

    /// Moves onto `pos` and rests there long enough for a tooltip, and
    /// returns the texts drawn.
    fn hover_texts(app: &mut WinTuneApp, ctx: &egui::Context, pos: Pos2) -> Vec<String> {
        let away = pos - egui::vec2(0.0, 200.0);
        let mut texts = Vec::new();
        for i in 0..20 {
            let at = if i < 2 { away } else { pos };
            let mut input = tall_input(vec![egui::Event::PointerMoved(at)]);
            if i >= 6 {
                input.time = Some(ctx.input(|i| i.time) + 0.1);
            }
            texts = drawn_texts(&frame(app, ctx, input));
        }
        texts
    }

    fn explorer_groups() -> &'static [TweakGroup] {
        WinTuneApp::tweak_groups_of(ActiveTab::Tweaks)
    }

    #[test]
    fn the_tab_renders_on_windows_10_and_11() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        for win11 in [true, false] {
            let mut app = tweaks_app(&fx, &ctx);
            app.env.is_windows_11 = win11;
            app.tweak_desired.insert("show_extensions", true);
            let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
            for key in ["nav.explorer", "tweaks.pill.win11"] {
                assert!(texts.contains(&app.tr.t(key)), "{key} not drawn");
            }
            let title = messages::tweak_title(app.tr, "taskbar_left");
            assert!(texts.contains(&title), "a row is missing");
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
    fn clicking_a_toggle_marks_the_change_pending() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);

        let toggle = toggle_of(&mut app, &ctx, "show_extensions");
        click_at(&mut app, &ctx, toggle);

        assert_eq!(
            app.pending_tweak_changes_in(explorer_groups()),
            vec![("show_extensions".to_string(), true)]
        );
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(texts.contains(&app.tr.t("tweaks.pill.will_on")));

        // Flipping back to the registry value drops the change.
        click_at(&mut app, &ctx, toggle);
        assert!(app.tweak_desired.is_empty());
    }

    #[test]
    fn a_custom_tweak_shows_its_reason_and_either_choice_is_pending() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        set_dword(&fx, ADVANCED, "HideFileExt", 7);
        app.refresh_tweak_statuses();
        let Some(TweakStatus::Custom(reason)) = app.tweak_statuses.get("show_extensions").cloned()
        else {
            panic!("show_extensions is not Custom");
        };

        let custom = app.tr.t("tweaks.pill.custom");
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let pill = text_center(&out, &custom).expect("Custom pill drawn");
        let why = messages::custom_reason(app.tr, &reason);
        let hint = app.tr.t_args("tweaks.custom_hint", &[&why]);
        assert!(
            hover_texts(&mut app, &ctx, pill).contains(&hint),
            "no translated reason over the Custom pill"
        );

        app.tweak_desired.insert("show_extensions", false);
        assert_eq!(app.pending_tweak_changes_in(explorer_groups()).len(), 1);
        app.tweak_desired.insert("show_extensions", true);
        assert_eq!(app.pending_tweak_changes_in(explorer_groups()).len(), 1);
    }

    #[test]
    fn the_pending_pill_and_the_pending_changes_agree() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        set_dword(&fx, ADVANCED, "HideFileExt", 7);
        set_dword(&fx, ADVANCED, "Hidden", 1);
        app.refresh_tweak_statuses();
        app.env.is_windows_11 = false;
        assert!(matches!(
            app.tweak_statuses.get("show_extensions"),
            Some(TweakStatus::Custom(_))
        ));
        assert_eq!(
            app.tweak_statuses.get("show_hidden"),
            Some(&TweakStatus::On)
        );
        assert_eq!(
            app.tweak_statuses.get("hide_task_view"),
            Some(&TweakStatus::Off)
        );
        let (will_on, will_off) = (
            app.tr.t("tweaks.pill.will_on"),
            app.tr.t("tweaks.pill.will_off"),
        );

        // Custom, On, Off, and a Windows 11-only tweak on Windows 10.
        for id in [
            "show_extensions",
            "show_hidden",
            "hide_task_view",
            "taskbar_left",
        ] {
            for wanted in [None, Some(true), Some(false)] {
                app.tweak_desired.clear();
                if let Some(w) = wanted {
                    app.tweak_desired.insert(id, w);
                }
                let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
                let pills: Vec<bool> = texts
                    .iter()
                    .filter_map(|t| {
                        (*t == will_on)
                            .then_some(true)
                            .or((*t == will_off).then_some(false))
                    })
                    .collect();
                let pending: Vec<bool> = app
                    .pending_tweak_changes_in(explorer_groups())
                    .into_iter()
                    .map(|(_, w)| w)
                    .collect();
                assert_eq!(pills, pending, "{id} wanted {wanted:?}");
            }
        }
    }

    #[test]
    fn windows_11_only_toggles_are_disabled_on_windows_10() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.env.is_windows_11 = false;

        let toggle = toggle_of(&mut app, &ctx, "taskbar_left");
        let only = app.tr.t("tweaks.win11_only");
        assert!(
            hover_texts(&mut app, &ctx, toggle).contains(&only),
            "no reason over the disabled toggle"
        );
        click_at(&mut app, &ctx, toggle);
        assert!(app.tweak_desired.is_empty());
    }

    #[test]
    fn an_unreadable_store_shows_a_banner_and_locks_the_toggles() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        let mut app = tweaks_app(&fx, &ctx);
        app.reload_tweak_store();
        let error = app.tweak_store_error.clone().expect("store error");

        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        let banner = app.tr.t_args(
            "tweaks.store_unreadable",
            &[&messages::load_error(app.tr, &error)],
        );
        assert!(texts.contains(&banner), "no banner: {texts:?}");

        let toggle = toggle_of(&mut app, &ctx, "show_extensions");
        click_at(&mut app, &ctx, toggle);
        assert!(app.tweak_desired.is_empty());
    }

    #[test]
    fn the_toggle_reason_is_busy_then_blocked_then_windows_11() {
        use super::super::jobs::RunningJob;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.env.is_windows_11 = false;
        let left = crate::tweaks::find_tweak("taskbar_left").unwrap();
        let ext = crate::tweaks::find_tweak("show_extensions").unwrap();
        assert_eq!(app.tweak_toggle_disabled_reason(ext), None);
        assert_eq!(
            app.tweak_toggle_disabled_reason(left),
            Some(app.tr.t("tweaks.win11_only"))
        );

        let error = corrupt("tweaks-backups.json", "bad json");
        app.tweak_store_error = Some(error.clone());
        let blocked = app
            .tr
            .t_args("tweaks.blocked", &[&messages::load_error(app.tr, &error)]);
        assert_eq!(app.tweak_toggle_disabled_reason(left), Some(blocked));

        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                let _ = release_rx.recv();
                Ok(crate::gallery::PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));
        let busy = app.busy_or_blocked(&None).expect("busy reason");
        assert_eq!(app.tweak_toggle_disabled_reason(left), Some(busy));
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn restore_original_in_the_menu_asks_first() {
        use super::super::confirm::ConfirmAction;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_desired.insert("show_hidden", true);
        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);

        let menu = row_widget(&mut app, &ctx, "show_hidden", MENU).expect("… drawn");
        click_at(&mut app, &ctx, menu);
        let restore = app.tr.t("tweaks.restore_original");
        click_text(&mut app, &ctx, &restore);

        assert_eq!(
            app.pending_confirm.as_ref().map(|c| c.action.clone()),
            Some(ConfirmAction::RestoreTweaks(
                vec!["show_hidden".to_string()]
            ))
        );
    }

    #[test]
    fn the_menu_is_absent_without_a_snapshot() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);

        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(!texts.contains(&"\u{2026}".to_string()), "… drawn");
        for tweak in catalog()
            .iter()
            .filter(|t| explorer_groups().contains(&t.group))
        {
            assert_eq!(
                row_widget(&mut app, &ctx, tweak.id, MENU),
                None,
                "{}",
                tweak.id
            );
        }
    }

    #[test]
    fn an_open_menu_does_not_come_back_after_a_page_switch() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_desired.insert("show_hidden", true);
        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);

        let restore = app.tr.t("tweaks.restore_original");
        let menu = row_widget(&mut app, &ctx, "show_hidden", MENU).expect("… drawn");
        click_at(&mut app, &ctx, menu);
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(texts.contains(&restore), "the menu did not open");

        // Away and back through the real navigation.
        let (start, explorer) = (app.tr.t("nav.start"), app.tr.t("nav.explorer"));
        click_text(&mut app, &ctx, &start);
        assert_eq!(app.active_tab, ActiveTab::StartPrivacy);
        // Back, watching every frame of the click: the menu must not show,
        // not even for the frame in which the page comes back.
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let pos = text_center(&out, &explorer).expect("nav entry drawn");
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let mut shown = Vec::new();
        for events in [
            vec![egui::Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
            vec![],
            vec![],
        ] {
            shown.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(events))));
        }
        assert_eq!(app.active_tab, ActiveTab::Tweaks);
        assert!(!shown.contains(&restore), "the menu reopened by itself");
    }

    #[test]
    fn a_blocked_restore_starts_nothing_and_says_why() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_desired.insert("show_hidden", true);
        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);
        let applied = dword(&fx, ADVANCED, "Hidden");
        assert_eq!(applied, Some(1), "{}", app.last_notice());
        assert!(app.tweak_store.has_backup("TEST\\user", "show_hidden"));

        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        app.reload_tweak_store();
        let error = app.tweak_store_error.clone().expect("store error");

        app.start_restore_tweaks(&ctx, vec!["show_hidden".into()]);

        assert!(!app.is_busy(), "a job started");
        assert_eq!(
            app.last_notice(),
            app.tr
                .t_args("tweaks.blocked", &[&messages::load_error(app.tr, &error)])
        );
        assert_eq!(dword(&fx, ADVANCED, "Hidden"), applied);
    }

    #[test]
    fn the_windows_11_pill_says_why_on_windows_10() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.env.is_windows_11 = false;

        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let description = messages::tweak_description(app.tr, "taskbar_left");
        let row = text_center(&out, &description).expect("row drawn");
        let label = app.tr.t("tweaks.pill.win11");
        fn centers(shape: &egui::epaint::Shape, text: &str, found: &mut Vec<Pos2>) {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                    found.push(t.pos + t.galley.rect.center().to_vec2());
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().for_each(|s| centers(s, text, found));
                }
                _ => {}
            }
        }
        let mut pills = Vec::new();
        out.shapes
            .iter()
            .for_each(|c| centers(&c.shape, &label, &mut pills));
        let pill = pills
            .into_iter()
            .filter(|p| (p.y - row.y).abs() < 24.0)
            .min_by(|a, b| (a.y - row.y).abs().total_cmp(&(b.y - row.y).abs()))
            .expect("the Windows 11 pill of the row");

        let only = app.tr.t("tweaks.win11_only");
        assert!(
            hover_texts(&mut app, &ctx, pill).contains(&only),
            "no reason over the Windows 11 pill"
        );
    }

    #[test]
    fn refresh_labels_cover_every_variant() {
        let ctx = egui::Context::default();
        let fx = TempIcons::new(&["foo"]);
        let tr = tweaks_app(&fx, &ctx).tr;
        let all = [Refresh::Live, Refresh::NewWindows, Refresh::ExplorerRestart];
        let labels: std::collections::HashSet<String> =
            all.iter().map(|r| refresh_label(tr, *r)).collect();
        let hints: std::collections::HashSet<String> =
            all.iter().map(|r| refresh_hint(tr, *r)).collect();
        assert_eq!(labels.len(), 3);
        assert_eq!(hints.len(), 3);
        assert!(labels.iter().chain(&hints).all(|t| !t.is_empty()));
        assert_eq!(
            refresh_label(tr, Refresh::NewWindows),
            "Visible: in new windows"
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
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        let shown = messages::tweak_title(app.tr, "hide_task_view");
        let hidden = messages::tweak_title(app.tr, "show_extensions");
        assert!(texts.contains(&shown) && !texts.contains(&hidden));
        assert!(
            !texts.contains(&messages::group_title(app.tr, TweakGroup::FileExplorer)),
            "a group without a match is skipped"
        );

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
    fn the_clear_button_empties_the_search() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        let clear = app.tr.t("tweaks.clear");
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(!texts.contains(&clear), "Clear shown with an empty search");

        app.tweak_search = "task".into();
        click_text(&mut app, &ctx, &clear);
        assert!(app.tweak_search.is_empty());
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
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(texts.contains(&app.tr.t("nav.start")));
        assert!(texts.contains(&messages::group_title(app.tr, TweakGroup::Privacy)));
        assert!(!texts.contains(&messages::group_title(app.tr, TweakGroup::Taskbar)));
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

        let restore_all = app.tr.t_args("tweaks.restore_all", &[&1]);
        click_text(&mut app, &ctx, &restore_all);

        match app.pending_confirm.as_ref().map(|c| c.action.clone()) {
            Some(super::super::confirm::ConfirmAction::RestoreTweaks(ids)) => {
                assert_eq!(ids, vec!["no_app_launch_tracking".to_string()])
            }
            other => panic!("unexpected confirmation: {:?}", other),
        }
    }

    #[test]
    fn restore_all_is_disabled_with_its_reason_without_a_snapshot() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        let restore_all = app.tr.t_args("tweaks.restore_all", &[&0]);

        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let at = text_center(&out, &restore_all).expect("Restore all drawn");
        let reason = app.tr.t("tweaks.nothing_to_restore");
        assert!(hover_texts(&mut app, &ctx, at).contains(&reason));
        click_at(&mut app, &ctx, at);
        assert!(app.pending_confirm.is_none());
    }

    #[test]
    fn a_search_without_match_shows_nothing_and_still_renders() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.tweak_search = "zzz-no-such-tweak".into();

        assert!(visible_ids(&app).is_empty());
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        let none = app.tr.t_args("tweaks.no_match", &[&"zzz-no-such-tweak"]);
        assert!(texts.contains(&none), "{texts:?}");
    }

    #[test]
    fn windows_11_only_tweaks_are_never_sent_on_windows_10() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        app.env.is_windows_11 = false;
        app.tweak_desired.insert("taskbar_left", true);

        assert!(app.pending_tweak_changes().is_empty());
        // Nor shown as pending on the page.
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(!texts.contains(&app.tr.t("tweaks.pill.will_on")));
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
        assert_eq!(
            app.last_notice(),
            app.tr.t_args(
                "tweaks.blocked",
                &[&messages::load_error(
                    app.tr,
                    &app.tweak_store_error.clone().unwrap()
                )]
            )
        );
        assert_eq!(dword(&fx, ADVANCED, "HideFileExt"), None);
    }

    /// Holds a pak build until the returned sender is used (or dropped).
    fn hold_job(app: &mut WinTuneApp) -> std::sync::mpsc::Sender<()> {
        use super::super::jobs::RunningJob;
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                let _ = release_rx.recv();
                Ok(crate::gallery::PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));
        release_tx
    }

    /// One pending change on each tab; the app ends on Explorer & Taskbar.
    fn pending_on_both_tabs(app: &mut WinTuneApp) {
        app.tweak_desired.insert("show_extensions", true);
        app.tweak_desired.insert("no_app_launch_tracking", true);
        app.active_tab = ActiveTab::Tweaks;
    }

    fn one_pending(app: &WinTuneApp) -> String {
        app.tr.t_args("tweaks.pending_n", &[&1])
    }

    #[test]
    fn the_bar_appears_with_a_pending_change_and_applies_only_this_tab() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        pending_on_both_tabs(&mut app);

        let pending = one_pending(&app);
        for tab in [ActiveTab::Tweaks, ActiveTab::StartPrivacy] {
            app.active_tab = tab;
            let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
            assert!(texts.contains(&pending), "{tab:?}: {texts:?}");
        }

        // Start & Privacy is showing.
        let label = app.tr.t("tweaks.apply");
        click_text(&mut app, &ctx, &label);
        run_until_idle(&mut app, &ctx);

        assert_eq!(dword(&fx, ADVANCED, "Start_TrackProgs"), Some(0));
        assert_eq!(dword(&fx, ADVANCED, "HideFileExt"), None);
        assert_eq!(
            app.pending_tweak_changes_in(explorer_groups()),
            vec![("show_extensions".to_string(), true)]
        );
    }

    #[test]
    fn the_bar_is_absent_without_a_pending_change() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        for key in ["tweaks.apply", "tweaks.discard"] {
            assert!(!texts.contains(&app.tr.t(key)), "{key} drawn");
        }
    }

    #[test]
    fn discard_in_the_bar_only_clears_this_tab() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        pending_on_both_tabs(&mut app);

        let label = app.tr.t("tweaks.discard");
        click_text(&mut app, &ctx, &label);

        assert!(app.pending_tweak_changes_in(explorer_groups()).is_empty());
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(
            !texts.contains(&app.tr.t("tweaks.discard")),
            "bar still drawn"
        );
        assert_eq!(
            app.pending_tweak_changes_in(WinTuneApp::tweak_groups_of(ActiveTab::StartPrivacy))
                .len(),
            1
        );
        app.active_tab = ActiveTab::StartPrivacy;
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(texts.contains(&one_pending(&app)));
    }

    #[test]
    fn while_a_job_runs_the_tweak_bar_shows_progress_and_no_actions() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = tweaks_app(&fx, &ctx);
        pending_on_both_tabs(&mut app);
        let release = hold_job(&mut app);

        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        let title = app.tr.t("job.build_pak");
        assert_eq!(
            texts.iter().filter(|t| **t == title).count(),
            1,
            "one action bar, with the job's title: {texts:?}"
        );
        for key in ["tweaks.apply", "tweaks.discard"] {
            assert!(!texts.contains(&app.tr.t(key)), "{key} drawn during a job");
        }

        let toggle = toggle_of(&mut app, &ctx, "hide_recent_files");
        let busy = app.busy_or_blocked(&None).expect("busy reason");
        assert!(hover_texts(&mut app, &ctx, toggle).contains(&busy));
        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn an_unreadable_store_disables_apply_with_the_reason() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        let mut app = tweaks_app(&fx, &ctx);
        app.reload_tweak_store();
        let error = app.tweak_store_error.clone().expect("store error");
        app.tweak_desired.insert("show_extensions", true);

        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let apply = text_center(&out, &app.tr.t("tweaks.apply")).expect("Apply drawn");
        let blocked = app
            .tr
            .t_args("tweaks.blocked", &[&messages::load_error(app.tr, &error)]);
        assert!(
            hover_texts(&mut app, &ctx, apply).contains(&blocked),
            "no blocked reason over Apply"
        );
        click_at(&mut app, &ctx, apply);
        run_until_idle(&mut app, &ctx);
        assert!(!app.is_busy());
        assert_eq!(dword(&fx, ADVANCED, "HideFileExt"), None);
    }
}
