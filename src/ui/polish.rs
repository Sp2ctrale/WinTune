//! Small UX helpers shared by the tabs: Ctrl+F to the current tab's search
//! box, and the reason shown on a disabled button.

use super::{ActiveTab, WinTuneApp};
use crate::i18n::messages;
use crate::persist::LoadError;
use eframe::egui;

/// Stable ids of the search boxes, so focus can be requested on them.
pub(super) fn search_box_id(tab: ActiveTab) -> Option<egui::Id> {
    match tab {
        ActiveTab::Associations => Some(egui::Id::new("search_associations")),
        ActiveTab::Tweaks | ActiveTab::StartPrivacy => Some(egui::Id::new("search_tweaks")),
        ActiveTab::Gallery => Some(egui::Id::new("search_gallery")),
        ActiveTab::ContextMenu => Some(egui::Id::new("search_menu_others")),
        ActiveTab::Startup => Some(egui::Id::new("search_startup")),
        ActiveTab::Home | ActiveTab::History | ActiveTab::Settings => None,
    }
}

impl WinTuneApp {
    /// Ctrl+F: remember to focus the current tab's search box this frame.
    /// (No shortcut changes the registry: those stay mouse + confirmation.)
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending_confirm.is_some()
            || self.icon_picker.is_some()
            || self.menu_rename.is_some()
        {
            return;
        }
        let find = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F);
        if ctx.input_mut(|i| i.consume_shortcut(&find)) {
            self.focus_search = search_box_id(self.active_tab).is_some();
        }
    }

    /// Call right after adding the current tab's search box. The box is
    /// scrolled into view too: it can sit below the fold (Context menu's
    /// Other programs).
    pub(super) fn focus_search_if_requested(&mut self, response: &egui::Response) {
        if self.focus_search {
            response.request_focus();
            response.scroll_to_me(None);
            self.focus_search = false;
        }
    }

    /// Why "Apply all enabled" is disabled, or `None` if it is enabled.
    pub(super) fn apply_disabled_reason(&self) -> Option<String> {
        if let Some(reason) = self.assoc_apply_blocked() {
            return Some(reason);
        }
        let blockers = self.apply_blockers();
        (!blockers.is_empty()).then(|| {
            self.tr
                .t_args("assoc.blocked.problems", &[&blockers.join("\n")])
        })
    }

    /// Reasons that block any association apply: a running job, an
    /// unreadable backups.json, no editor.
    fn assoc_apply_blocked(&self) -> Option<String> {
        if let Some(job) = &self.job {
            let name = self.tr.t(job.name_key());
            return Some(self.tr.t_args("notice.still_running", &[&name]));
        }
        if let Some(e) = &self.backup_store_error {
            let detail = messages::load_error(self.tr, e);
            return Some(self.tr.t_args("notice.registry_blocked", &[&detail]));
        }
        if self.config.code_executable.trim().is_empty() {
            return Some(self.tr.t("assoc.blocked.editor"));
        }
        None
    }

    /// Why the action bar's Apply (this rule only) is disabled. Same reasons
    /// as `apply_disabled_reason`, except that other rules' problems do not
    /// count: only this rule's own.
    pub(super) fn apply_rule_disabled_reason(
        &self,
        index: usize,
        problems: &[(usize, crate::associations::RuleProblem)],
    ) -> Option<String> {
        if let Some(reason) = self.assoc_apply_blocked() {
            return Some(reason);
        }
        problems
            .iter()
            .any(|(i, _)| *i == index)
            .then(|| self.tr.t("assoc.blocked.this_rule"))
    }

    /// Common reasons first: a running job, then an unreadable store.
    pub(super) fn busy_or_blocked(&self, store_error: &Option<LoadError>) -> Option<String> {
        if let Some(job) = &self.job {
            let name = self.tr.t(job.name_key());
            return Some(self.tr.t_args("notice.still_running", &[&name]));
        }
        store_error.as_ref().map(|e| {
            let detail = messages::load_error(self.tr, e);
            self.tr.t_args("tweaks.blocked", &[&detail])
        })
    }

    pub(super) fn tweak_apply_disabled_reason(&self) -> Option<String> {
        self.busy_or_blocked(&self.tweak_store_error).or_else(|| {
            self.pending_tweak_changes_in(Self::tweak_groups_of(self.active_tab))
                .is_empty()
                .then(|| self.tr.t("tweaks.nothing_to_apply"))
        })
    }

    pub(super) fn tweak_restore_all_disabled_reason(&self, backups: usize) -> Option<String> {
        self.busy_or_blocked(&self.tweak_store_error)
            .or_else(|| (backups == 0).then(|| self.tr.t("tweaks.nothing_to_restore")))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    fn key(key: egui::Key, modifiers: Modifiers) -> RawInput {
        RawInput {
            events: vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            modifiers,
            ..Default::default()
        }
    }

    #[test]
    fn ctrl_f_focuses_the_search_box_of_the_current_tab() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        for tab in [
            ActiveTab::Associations,
            ActiveTab::Tweaks,
            ActiveTab::StartPrivacy,
            ActiveTab::Gallery,
        ] {
            let mut app = fx.app_with_rules(&ctx, &[".foo"]);
            app.active_tab = tab;
            frame(&mut app, &ctx, RawInput::default());

            frame(&mut app, &ctx, key(Key::F, Modifiers::COMMAND));
            frame(&mut app, &ctx, RawInput::default());

            let id = search_box_id(tab).expect("tab has a search box");
            assert!(
                ctx.memory(|m| m.has_focus(id)),
                "search box of {:?} not focused",
                tab
            );
        }
    }

    #[test]
    fn escape_cancels_the_confirmation_dialog() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.request_remove_rule(0);

        frame(&mut app, &ctx, key(Key::Escape, Modifiers::NONE));

        assert!(app.pending_confirm.is_none());
        assert_eq!(app.config.extensions.len(), 1, "Esc must not confirm");
    }

    #[test]
    fn a_disabled_apply_button_says_why() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();

        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        assert_eq!(app.apply_disabled_reason(), None);

        app.config.code_executable.clear();
        assert_eq!(
            app.apply_disabled_reason().unwrap(),
            app.tr.t("assoc.blocked.editor")
        );

        let mut app = fx.app_with_rules(&ctx, &[".foo", ".FOO"]);
        let reason = app.apply_disabled_reason().unwrap();
        assert!(reason.contains("more than once"), "{reason}");
        assert!(reason.starts_with(&app.tr.t_args("assoc.blocked.problems", &[&""])));

        let error = corrupt("backups.json", "bad json");
        app.backup_store_error = Some(error.clone());
        assert_eq!(
            app.apply_disabled_reason().unwrap(),
            app.tr.t_args(
                "notice.registry_blocked",
                &[&messages::load_error(app.tr, &error)]
            )
        );
    }

    #[test]
    fn other_disabled_buttons_say_why_too() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.refresh_tweak_statuses();

        assert_eq!(
            app.tweak_apply_disabled_reason().unwrap(),
            app.tr.t("tweaks.nothing_to_apply")
        );
        app.tweak_desired.insert("show_extensions", true);
        assert_eq!(app.tweak_apply_disabled_reason(), None);
        let error = corrupt("tweaks-backups.json", "bad json");
        app.tweak_store_error = Some(error.clone());
        assert_eq!(
            app.tweak_apply_disabled_reason().unwrap(),
            app.tr
                .t_args("tweaks.blocked", &[&messages::load_error(app.tr, &error)])
        );

        app.tweak_store_error = None;
        assert_eq!(
            app.tweak_restore_all_disabled_reason(0).unwrap(),
            app.tr.t("tweaks.nothing_to_restore")
        );
        assert_eq!(app.tweak_restore_all_disabled_reason(2), None);
    }
}
