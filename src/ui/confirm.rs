//! Confirmation dialog for destructive actions (removing a rule, rolling back
//! extensions). The request is stored on the app; nothing happens until the
//! user accepts it in the dialog.

use super::WinTuneApp;
use eframe::egui;

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
        self.pending_confirm = Some(Confirm {
            title: "Remove rule".to_string(),
            message: format!(
                "Remove the rule for {} from the list?\n\nThis only edits WinTune's list: an association already applied stays in the registry until you restore it from the Rollback tab.",
                extension
            ),
            confirm_label: "Remove".to_string(),
            action: ConfirmAction::RemoveRule { index, extension },
        });
    }

    /// Asks before restoring every extension selected in the Rollback tab.
    pub(super) fn request_rollback(&mut self) {
        if self.rollback_selection.is_empty() {
            return;
        }
        let mut extensions: Vec<String> = self.rollback_selection.iter().cloned().collect();
        extensions.sort();
        self.pending_confirm = Some(Confirm {
            title: "Restore original state".to_string(),
            message: format!(
                "Restore {} extension(s) to their state before WinTune?\n\n{}\n\nTheir custom icons and commands will be removed and their snapshots deleted.",
                extensions.len(),
                extensions.join(", ")
            ),
            confirm_label: "Restore".to_string(),
            action: ConfirmAction::Rollback(extensions),
        });
    }

    pub(super) fn resolve_confirm(&mut self, ctx: &egui::Context, accepted: bool) {
        let Some(confirm) = self.pending_confirm.take() else {
            return;
        };
        if !accepted {
            self.set_status("Cancelled.");
            return;
        }
        match confirm.action {
            ConfirmAction::RemoveRule { index, extension } => self.remove_rule(index, &extension),
            ConfirmAction::Rollback(extensions) => self.start_rollback(ctx, extensions),
            ConfirmAction::RestoreTweaks(ids) => self.start_restore_tweaks(ctx, ids),
        }
    }

    fn remove_rule(&mut self, hint: usize, extension: &str) {
        let rules = &self.config.extensions;
        let index = if rules.get(hint).is_some_and(|r| r.extension == extension) {
            Some(hint)
        } else {
            rules.iter().position(|r| r.extension == extension)
        };
        let Some(index) = index else {
            self.set_status(format!("{} is no longer in the list.", extension));
            return;
        };
        self.config.extensions.remove(index);
        if self.selected_ext_index >= self.config.extensions.len() {
            self.selected_ext_index = self.config.extensions.len().saturating_sub(1);
        }
        self.set_status(format!("Removed the rule for {}.", extension));
    }

    /// Modal dialog: a translucent layer swallows clicks on the panels behind.
    pub(super) fn show_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = &self.pending_confirm else {
            return;
        };
        let (title, message, confirm_label) = (
            confirm.title.clone(),
            confirm.message.clone(),
            confirm.confirm_label.clone(),
        );

        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("confirm_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.painter()
                    .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
                ui.allocate_rect(screen, egui::Sense::click());
            });

        let mut decision: Option<bool> = None;
        egui::Window::new(title)
            .id(egui::Id::new("confirm_dialog"))
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(message);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(crate::theme::accent_button(
                            confirm_label,
                            self.config.theme,
                        ))
                        .clicked()
                    {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
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
    fn rollback_waits_for_confirmation_then_restores() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);
        assert!(fx.key_exists(&fx.classes(".foo")), "{}", app.status_message);
        app.rollback_selection.insert(".foo".to_string());

        app.request_rollback();
        assert!(app.pending_confirm.is_some());
        assert!(!app.is_busy(), "rollback started without confirmation");
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);

        assert!(
            !fx.key_exists(&fx.classes(".foo")),
            "{}",
            app.status_message
        );
        assert!(app.backup_store.backups.is_empty());
    }

    #[test]
    fn rollback_with_nothing_selected_asks_nothing() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        app.request_rollback();

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
}
