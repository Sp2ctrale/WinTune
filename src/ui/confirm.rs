//! Confirmation dialog for destructive actions (removing a rule, rolling back
//! extensions). The request is stored on the app; nothing happens until the
//! user accepts it in the dialog.

use super::{kit, WinTuneApp};
use eframe::egui;

/// Entry names listed in the restore question before it gives a count.
const MAX_NAMED: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ConfirmAction {
    /// `index` is a hint: the rule is found again by `extension` in case the
    /// list changed while the dialog was open.
    RemoveRule {
        index: usize,
        extension: String,
    },
    Rollback(Vec<String>),
    /// Tweak ids whose original state must be put back.
    RestoreTweaks(Vec<String>),
    /// Context menu registry keys (`entry_key` paths) whose original must be
    /// put back.
    RestoreMenuKeys(Vec<String>),
    /// Takes a context menu entry out of the list; its registry keys go at
    /// the next Apply.
    RemoveMenuEntry {
        id: String,
    },
}

#[derive(Debug, Clone)]
pub(super) struct Confirm {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub action: ConfirmAction,
}

impl WinTuneApp {
    pub(super) fn request_remove_rule(&mut self, index: usize) {
        let Some(rule) = self.config.extensions.get(index) else {
            return;
        };
        let extension = rule.extension.clone();
        let tr = self.tr;
        self.pending_confirm = Some(Confirm {
            title: tr.t("confirm.remove_rule.title"),
            message: format!(
                "{}\n\n{}",
                tr.t_args("confirm.remove_rule.body", &[&extension]),
                tr.t("confirm.remove_rule.note")
            ),
            confirm_label: tr.t("confirm.remove"),
            action: ConfirmAction::RemoveRule { index, extension },
        });
    }

    /// Asks before restoring the original of one rule (the associations
    /// page's "Restore original").
    pub(super) fn request_restore_rule(&mut self, extension: &str) {
        self.confirm_rollback(vec![extension.to_string()]);
    }

    /// Asks before restoring every saved association original (History's
    /// "Restore all" of the associations group).
    pub(super) fn request_restore_all_rules(&mut self) {
        if self.backup_store.backups.is_empty() {
            return;
        }
        let mut extensions: Vec<String> = self.backup_store.backups.keys().cloned().collect();
        extensions.sort();
        self.confirm_rollback(extensions);
    }

    /// Asks before taking a context menu entry out of the list.
    pub(super) fn request_remove_menu_entry(&mut self, id: &str) {
        let Some(entry) = self.menu_entries().iter().find(|e| e.id == id) else {
            return;
        };
        let name = entry.label.trim();
        let name = if name.is_empty() { id } else { name };
        let tr = self.tr;
        self.pending_confirm = Some(Confirm {
            title: tr.t("menu.remove_title"),
            message: tr.t_args("menu.remove_message", &[&name]),
            confirm_label: tr.t("menu.remove"),
            action: ConfirmAction::RemoveMenuEntry { id: id.to_string() },
        });
    }

    /// Asks before putting context menu keys back to what was there before
    /// WinTune changed them (History's "Restore original").
    pub(super) fn request_restore_menu_keys(&mut self, keys: Vec<String>) {
        if keys.is_empty() {
            return;
        }
        let tr = self.tr;
        let mut names: Vec<String> = Vec::new();
        for name in keys.iter().map(|k| self.menu_key_title(k)) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let named = if names.len() > MAX_NAMED {
            tr.t_args("menu.restore_many", &[&names.len()])
        } else {
            names.join(", ")
        };
        self.pending_confirm = Some(Confirm {
            title: tr.t("menu.restore_title"),
            message: tr.t_args("menu.restore_message", &[&named]),
            confirm_label: tr.t("confirm.restore"),
            action: ConfirmAction::RestoreMenuKeys(keys),
        });
    }

    fn confirm_rollback(&mut self, extensions: Vec<String>) {
        let tr = self.tr;
        self.pending_confirm = Some(Confirm {
            title: tr.t("confirm.rollback.title"),
            message: format!(
                "{}\n\n{}\n\n{}",
                tr.t_args("confirm.rollback.body", &[&extensions.len()]),
                extensions.join(", "),
                tr.t("confirm.rollback.note")
            ),
            confirm_label: tr.t("confirm.restore"),
            action: ConfirmAction::Rollback(extensions),
        });
    }

    pub(super) fn resolve_confirm(&mut self, ctx: &egui::Context, accepted: bool) {
        let Some(confirm) = self.pending_confirm.take() else {
            return;
        };
        if !accepted {
            let text = self.tr.t("notice.cancelled");
            self.notify(kit::ToastKind::Info, text);
            return;
        }
        match confirm.action {
            ConfirmAction::RemoveRule { index, extension } => self.remove_rule(index, &extension),
            ConfirmAction::Rollback(extensions) => self.start_rollback(ctx, extensions),
            ConfirmAction::RestoreTweaks(ids) => self.start_restore_tweaks(ctx, ids),
            ConfirmAction::RestoreMenuKeys(keys) => self.start_menu_restore(ctx, keys),
            ConfirmAction::RemoveMenuEntry { id } => self.remove_menu_entry(&id),
        }
    }

    /// Takes the entry out of the list. Its keys stay in the registry until
    /// the next Apply (they show as pending), and the original stays in History.
    fn remove_menu_entry(&mut self, id: &str) {
        if !self.menu_entries().iter().any(|e| e.id == id) {
            return;
        }
        self.menu_entries_mut().retain(|e| e.id != id);
        if self.selected_menu_entry.as_deref() == Some(id) {
            self.selected_menu_entry = None;
        }
        self.refresh_menu_statuses();
    }

    fn remove_rule(&mut self, hint: usize, extension: &str) {
        let rules = &self.config.extensions;
        let index = if rules.get(hint).is_some_and(|r| r.extension == extension) {
            Some(hint)
        } else {
            rules.iter().position(|r| r.extension == extension)
        };
        let Some(index) = index else {
            let text = self.tr.t_args("notice.rule_gone", &[&extension]);
            self.notify(kit::ToastKind::Info, text);
            return;
        };
        self.config.extensions.remove(index);
        if self.selected_ext_index >= self.config.extensions.len() {
            self.selected_ext_index = self.config.extensions.len().saturating_sub(1);
        }
        let text = self.tr.t_args("notice.rule_removed", &[&extension]);
        self.notify(kit::ToastKind::Success, text);
    }

    /// Modal kit dialog. Focus starts on Cancel, so Enter cancels; Escape
    /// cancels too. Nothing runs until the user presses the confirm button.
    pub(super) fn show_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = &self.pending_confirm else {
            return;
        };
        let (title, message, confirm_label) = (
            confirm.title.clone(),
            confirm.message.clone(),
            confirm.confirm_label.clone(),
        );
        let cancel_label = self.tr.t("dialog.cancel");

        let mut decision: Option<bool> = None;
        kit::dialog(ctx, "confirm", &title, 440.0, |ui| {
            ui.label(message);
            ui.add_space(kit::space::L);
            decision = kit::dialog_buttons(ui, "confirm", &confirm_label, &cancel_label);
        });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            decision = Some(false);
        }
        if let Some(accepted) = decision {
            self.resolve_confirm(ctx, accepted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use egui::RawInput;

    fn exts(app: &WinTuneApp) -> Vec<String> {
        app.config
            .extensions
            .iter()
            .map(|r| r.extension.clone())
            .collect()
    }

    #[test]
    fn removing_a_rule_waits_for_confirmation() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);

        app.request_remove_rule(1);

        assert!(app.pending_confirm.is_some());
        assert_eq!(exts(&app), [".a", ".b"]);
    }

    #[test]
    fn cancelling_keeps_the_rule() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.request_remove_rule(1);

        app.resolve_confirm(&ctx, false);

        assert!(app.pending_confirm.is_none());
        assert_eq!(exts(&app), [".a", ".b"]);
    }

    #[test]
    fn accepting_removes_exactly_that_rule() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b", ".c"]);
        app.request_remove_rule(1);

        app.resolve_confirm(&ctx, true);

        assert!(app.pending_confirm.is_none());
        assert_eq!(exts(&app), [".a", ".c"]);
    }

    #[test]
    fn accepting_after_the_list_changed_removes_the_requested_extension() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.request_remove_rule(1);
        app.config.extensions.insert(0, rule(".z"));

        app.resolve_confirm(&ctx, true);

        assert_eq!(exts(&app), [".z", ".a"]);
    }

    #[test]
    fn restoring_all_rules_waits_for_confirmation_then_restores() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);
        assert!(fx.key_exists(&fx.classes(".foo")), "{}", app.last_notice());

        app.request_restore_all_rules();
        assert!(app.pending_confirm.is_some());
        assert!(!app.is_busy(), "rollback started without confirmation");
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);

        assert!(!fx.key_exists(&fx.classes(".foo")), "{}", app.last_notice());
        assert!(app.backup_store.backups.is_empty());
    }

    #[test]
    fn restoring_all_rules_with_no_saved_original_asks_nothing() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        app.request_restore_all_rules();

        assert!(app.pending_confirm.is_none());
    }

    #[test]
    fn the_dialog_renders_on_every_tab() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a"]);
        app.request_remove_rule(0);
        assert!(app.pending_confirm.is_some());

        for tab in super::super::ActiveTab::ALL {
            app.active_tab = tab;
            frame(&mut app, &ctx, RawInput::default());
        }
        assert!(
            app.pending_confirm.is_some(),
            "rendering must not resolve it"
        );
    }

    #[test]
    fn the_confirm_dialog_starts_on_cancel_and_enter_cancels() {
        use egui::{Event, Key, Modifiers};
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.request_remove_rule(0);
        frame(&mut app, &ctx, RawInput::default());
        frame(&mut app, &ctx, RawInput::default());

        let enter = |pressed| RawInput {
            events: vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        };
        frame(&mut app, &ctx, enter(true));
        frame(&mut app, &ctx, enter(false));

        assert!(app.pending_confirm.is_none());
        assert_eq!(exts(&app), [".a", ".b"]);
    }
}
