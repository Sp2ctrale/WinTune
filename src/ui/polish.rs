//! Small UX helpers shared by the tabs: Ctrl+F to the current tab's search
//! box, and the reason shown on a disabled button.

use super::{ActiveTab, WinTuneApp};
use eframe::egui;

/// Stable ids of the search boxes, so focus can be requested on them.
pub(super) fn search_box_id(tab: ActiveTab) -> Option<egui::Id> {
    match tab {
        ActiveTab::Associations => Some(egui::Id::new("search_associations")),
        ActiveTab::Tweaks | ActiveTab::StartPrivacy => Some(egui::Id::new("search_tweaks")),
        ActiveTab::Gallery => Some(egui::Id::new("search_gallery")),
        ActiveTab::Rollback => None,
    }
}

impl WinTuneApp {
    /// Ctrl+F: remember to focus the current tab's search box this frame.
    /// (No shortcut changes the registry: those stay mouse + confirmation.)
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending_confirm.is_some() {
            return;
        }
        let find = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F);
        if ctx.input_mut(|i| i.consume_shortcut(&find)) {
            self.focus_search = true;
        }
    }

    /// Call right after adding the current tab's search box.
    pub(super) fn focus_search_if_requested(&mut self, response: &egui::Response) {
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
    }

    /// Why "Apply Checked Extensions" is disabled, or `None` if it is enabled.
    pub(super) fn apply_disabled_reason(&self) -> Option<String> {
        if let Some(job) = &self.job {
            return Some(format!("Wait: {} is still running.", job.title()));
        }
        if let Some(e) = &self.backup_store_error {
            return Some(format!("Registry changes are blocked: {}", e));
        }
        if self.config.code_executable.trim().is_empty() {
            return Some("Set the default code editor first.".to_string());
        }
        let blockers = self.apply_blockers();
        (!blockers.is_empty()).then(|| format!("Fix these rules first:\n{}", blockers.join("\n")))
    }

    /// Common reasons first: a running job, then an unreadable store.
    fn busy_or_blocked(&self, store_error: &Option<String>) -> Option<String> {
        if let Some(job) = &self.job {
            return Some(format!("Wait: {} is still running.", job.title()));
        }
        store_error
            .as_ref()
            .map(|e| format!("Changes are blocked: {}", e))
    }

    pub(super) fn rollback_disabled_reason(&self) -> Option<String> {
        self.busy_or_blocked(&self.backup_store_error).or_else(|| {
            self.rollback_selection
                .is_empty()
                .then(|| "Tick at least one extension to restore.".to_string())
        })
    }

    pub(super) fn tweak_apply_disabled_reason(&self) -> Option<String> {
        self.busy_or_blocked(&self.tweak_store_error).or_else(|| {
            self.pending_tweak_changes()
                .is_empty()
                .then(|| "Nothing to apply: tick or untick a setting first.".to_string())
        })
    }

    pub(super) fn tweak_restore_all_disabled_reason(&self, backups: usize) -> Option<String> {
        self.busy_or_blocked(&self.tweak_store_error).or_else(|| {
            (backups == 0).then(|| {
                "Nothing to restore: WinTune has not changed any of these settings.".to_string()
            })
        })
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
        assert!(app.apply_disabled_reason().unwrap().contains("editor"));

        let mut app = fx.app_with_rules(&ctx, &[".foo", ".FOO"]);
        assert!(app
            .apply_disabled_reason()
            .unwrap()
            .contains("more than once"));

        app.backup_store_error = Some("backups.json is unreadable".into());
        assert!(app.apply_disabled_reason().unwrap().contains("unreadable"));
    }

    #[test]
    fn other_disabled_buttons_say_why_too() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.refresh_tweak_statuses();

        assert!(app.rollback_disabled_reason().unwrap().contains("Tick"));
        app.rollback_selection.insert(".foo".into());
        assert_eq!(app.rollback_disabled_reason(), None);

        assert!(app
            .tweak_apply_disabled_reason()
            .unwrap()
            .contains("Nothing to apply"));
        app.tweak_desired.insert("show_extensions", true);
        assert_eq!(app.tweak_apply_disabled_reason(), None);
        app.tweak_store_error = Some("tweaks-backups.json is unreadable".into());
        assert!(app
            .tweak_apply_disabled_reason()
            .unwrap()
            .contains("unreadable"));

        app.tweak_store_error = None;
        assert!(app
            .tweak_restore_all_disabled_reason(0)
            .unwrap()
            .contains("Nothing to restore"));
        assert_eq!(app.tweak_restore_all_disabled_reason(2), None);
    }
}
