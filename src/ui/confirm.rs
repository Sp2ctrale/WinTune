//! Confirmation dialog for destructive actions (removing a rule, rolling back
//! extensions). The request is stored on the app; nothing happens until the
//! user accepts it in the dialog.

use super::{kit, WinTuneApp};
use crate::backups::Snapshot;
use crate::context_menu::fileinfo::decode_utf16le;
use crate::environment::{is_path_name, removed_entries, split_entries, EnvScan};
use crate::i18n::Tr;
use eframe::egui;

/// Entry names listed in the restore question before it gives a count.
const MAX_NAMED: usize = 3;

/// The distinct `names` joined by commas, or a count (`many_key`, "{0}
/// apps") past `MAX_NAMED`.
fn named_list(tr: Tr, names: impl Iterator<Item = String>, many_key: &str) -> String {
    let mut distinct: Vec<String> = Vec::new();
    for name in names {
        if !distinct.contains(&name) {
            distinct.push(name);
        }
    }
    if distinct.len() > MAX_NAMED {
        tr.t_args(many_key, &[&distinct.len()])
    } else {
        distinct.join(", ")
    }
}

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
    /// Applies the context menu list and pending changes, once the side
    /// effects of the hides were read.
    ApplyMenu,
    /// Startup switch slot ids whose original must be put back.
    RestoreStartup(Vec<String>),
    /// Writes the pending environment changes, once the user saw what they
    /// do beyond the plain edit.
    ApplyEnvironment,
    /// Variable slot ids (a rename's partners included) whose original must
    /// be put back.
    RestoreEnvironment(Vec<String>),
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
        let named = named_list(
            tr,
            keys.iter().map(|k| self.menu_key_title(k)),
            "menu.restore_many",
        );
        self.pending_confirm = Some(Confirm {
            title: tr.t("menu.restore_title"),
            message: tr.t_args("menu.restore_message", &[&named]),
            confirm_label: tr.t("confirm.restore"),
            action: ConfirmAction::RestoreMenuKeys(keys),
        });
    }

    /// Asks before putting startup switches (slot ids) back to what they
    /// were before WinTune changed them.
    pub(super) fn request_restore_startup(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let tr = self.tr;
        let named = named_list(
            tr,
            ids.iter().map(|id| self.startup_slot_name(id)),
            "startup.restore_many",
        );
        self.pending_confirm = Some(Confirm {
            title: tr.t("startup.restore_title"),
            message: tr.t_args("startup.restore_message", &[&named]),
            confirm_label: tr.t("confirm.restore"),
            action: ConfirmAction::RestoreStartup(ids),
        });
    }

    /// Asks before an environment Apply, with one paragraph per warning.
    pub(super) fn request_apply_environment(&mut self, lines: &[String]) {
        let tr = self.tr;
        self.pending_confirm = Some(Confirm {
            title: tr.t("env.apply_title"),
            message: tr.t_args("env.apply_message", &[&lines.join("\n\n")]),
            confirm_label: tr.t("tweaks.apply"),
            action: ConfirmAction::ApplyEnvironment,
        });
    }

    /// Asks before putting variables (slot ids) back to what they were before
    /// WinTune changed them. The partners of a rename come with them; a
    /// restored `Path` names the folders that go and come back.
    #[allow(dead_code)] // The page (Task 6) uses it; remove then.
    pub(super) fn request_restore_environment(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let tr = self.tr;
        let user = self.env.user_label.clone();
        let all = self.env_store.with_partners(&user, &ids);
        let named = named_list(
            tr,
            all.iter().map(|id| self.env_slot_name(id)),
            "env.restore_many",
        );
        let mut paragraphs = vec![tr.t_args("env.restore_message", &[&named])];
        if let Some(path_id) = all.iter().find(|id| is_path_name(&self.env_slot_name(id))) {
            paragraphs.extend(self.restore_path_diff(&user, path_id));
        }
        self.pending_confirm = Some(Confirm {
            title: tr.t("env.restore_title"),
            message: paragraphs.join("\n\n"),
            confirm_label: tr.t("confirm.restore"),
            action: ConfirmAction::RestoreEnvironment(all),
        });
    }

    /// The `Path` folders a restore removes and puts back, as paragraphs.
    fn restore_path_diff(&self, user: &str, path_id: &str) -> Vec<String> {
        let tr = self.tr;
        let original = self
            .env_store
            .snapshots
            .find_id(user, path_id)
            .and_then(|id| self.env_store.snapshots.backups_of(user)?.get(id))
            .and_then(|backup| match &backup.snapshot {
                Snapshot::Value(v) if v.exists && matches!(v.kind, 1 | 2) => {
                    Some(split_entries(&decode_utf16le(&v.data)))
                }
                _ => None,
            })
            .unwrap_or_default();
        let current = self
            .env_scan
            .as_ref()
            .map(EnvScan::user_path_entries)
            .unwrap_or_default();
        let mut paragraphs = Vec::new();
        for (key, entries) in [
            (
                "env.confirm.path_removed",
                removed_entries(&current, &original),
            ),
            (
                "env.confirm.path_added",
                removed_entries(&original, &current),
            ),
        ] {
            if !entries.is_empty() {
                paragraphs.push(tr.t_args(key, &[&entries.join("\n")]));
            }
        }
        paragraphs
    }

    /// Asks before an Apply whose hides do more than remove a menu item:
    /// one line per side effect.
    pub(super) fn request_apply_menu(&mut self, warnings: &[String]) {
        let tr = self.tr;
        self.pending_confirm = Some(Confirm {
            title: tr.t("menu.apply_title"),
            message: tr.t_args("menu.apply_message", &[&warnings.join("\n")]),
            confirm_label: tr.t("menu.apply"),
            action: ConfirmAction::ApplyMenu,
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
            ConfirmAction::ApplyMenu => self.run_menu_apply(ctx),
            ConfirmAction::RestoreStartup(ids) => self.start_startup_restore(ctx, ids),
            ConfirmAction::ApplyEnvironment => self.run_env_apply(ctx),
            ConfirmAction::RestoreEnvironment(ids) => self.start_env_restore(ctx, ids),
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
        self.refresh_menu_statuses_if_idle();
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
    use super::super::environment_state::VarKey;
    use super::super::test_support::*;
    use super::*;
    use crate::environment::{EnvWarning, VarEdit};
    use crate::i18n::messages;
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

    const GIVE_ACCESS: &str = "{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}";

    fn blocked_value(fx: &TempIcons, clsid: &str) -> bool {
        let path = fx.hkcu(crate::context_menu::BLOCKED_KEY);
        fx.sandbox
            .root
            .open_subkey(path)
            .is_ok_and(|k| k.get_raw_value(clsid).is_ok())
    }

    #[test]
    fn hiding_give_access_asks_first_with_its_side_effect() {
        use super::super::menu_existing_state::PendingExisting;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_clsid(GIVE_ACCESS);
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        assert!(app
            .menu_scan
            .as_ref()
            .unwrap()
            .find("win:give_access")
            .is_some());
        app.menu_pending.insert(
            "win:give_access".into(),
            PendingExisting {
                hidden: Some(true),
                label: None,
            },
        );

        app.start_menu_apply(&ctx);

        let confirm = app.pending_confirm.as_ref().expect("asks first");
        assert_eq!(confirm.action, ConfirmAction::ApplyMenu);
        let side_effect = app.tr.t("menu.also.sharing_tab");
        assert!(
            confirm.message.contains(&side_effect),
            "{}",
            confirm.message
        );
        assert!(
            confirm.message.contains(&app.tr.t("menu.win.give_access")),
            "{}",
            confirm.message
        );
        assert_eq!(confirm.confirm_label, app.tr.t("menu.apply"));
        assert!(!app.is_busy(), "applied before the answer");
        assert!(!blocked_value(&fx, GIVE_ACCESS));

        app.resolve_confirm(&ctx, true);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);
        assert!(blocked_value(&fx, GIVE_ACCESS), "{}", app.last_notice());
        assert!(app.menu_pending.is_empty());
    }

    #[test]
    fn hiding_another_programs_handler_names_it_in_the_question() {
        use super::super::menu_existing_state::PendingExisting;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let clsid = "{23170F69-40C1-278A-1000-000100020000}";
        let machine = fx.machine_key();
        let handler = r"Software\Classes\*\shellex\ContextMenuHandlers\7-Zip";
        let (k, _) = machine.create_subkey(handler).unwrap();
        k.set_value("", &clsid).unwrap();
        let server = format!(r"Software\Classes\CLSID\{clsid}\InprocServer32");
        let (k, _) = machine.create_subkey(server).unwrap();
        k.set_value("", &r"C:\Apps\7-zip.dll").unwrap();
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        let id = format!("clsid:{clsid}");
        assert!(app.menu_scan.as_ref().unwrap().find(&id).is_some());
        app.menu_pending.insert(
            id,
            PendingExisting {
                hidden: Some(true),
                label: None,
            },
        );

        app.start_menu_apply(&ctx);
        let confirm = app.pending_confirm.as_ref().expect("asks first");
        let line = app.tr.t_args("menu.also.handler", &[&"7-Zip"]);
        assert!(confirm.message.contains(&line), "{}", confirm.message);

        // Cancelled: nothing written, the change stays pending.
        app.resolve_confirm(&ctx, false);
        assert!(!app.is_busy());
        assert!(!blocked_value(&fx, clsid));
        assert_eq!(app.menu_pending.len(), 1);
    }

    #[test]
    fn hiding_a_verb_applies_without_asking() {
        use super::super::menu_existing_state::PendingExisting;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        let id = app.menu_scan.as_ref().unwrap().others[0].id.clone();
        app.menu_pending.insert(
            id,
            PendingExisting {
                hidden: Some(true),
                label: None,
            },
        );
        app.start_menu_apply(&ctx);
        assert!(app.pending_confirm.is_none());
        assert!(app.is_busy());
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn restoring_a_startup_app_asks_first_and_restores() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_run(false, "App", r#""C:\Apps\app.exe""#);
        let mut app = fx.app(&ctx);
        app.request_startup_scan();
        run_until_startup_scanned(&mut app, &ctx);
        app.startup_pending.insert("run:app".into(), false);
        app.start_startup_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        assert!(fx.user_switch("App").is_some(), "{}", app.last_notice());

        let id = crate::startup::Approved::Run.slot("App").id();
        app.request_restore_startup(vec![id.clone()]);
        let confirm = app.pending_confirm.as_ref().expect("asks first");
        assert_eq!(confirm.action, ConfirmAction::RestoreStartup(vec![id]));
        assert!(confirm.message.contains("App"), "{}", confirm.message);
        assert!(!app.is_busy(), "restored without confirmation");

        app.resolve_confirm(&ctx, true);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);

        assert_eq!(fx.user_switch("App"), None);
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Startup apps", "Restore")
        );
    }

    #[test]
    fn a_startup_restore_names_at_most_three_apps() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let slots = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|n| crate::startup::Approved::Run.slot(n).id())
                .collect()
        };
        app.request_restore_startup(slots(&["A", "B"]));
        assert!(app
            .pending_confirm
            .as_ref()
            .unwrap()
            .message
            .contains("A, B"));
        app.request_restore_startup(slots(&["A", "B", "C", "D"]));
        assert!(
            app.pending_confirm
                .as_ref()
                .unwrap()
                .message
                .contains("4 apps"),
            "{}",
            app.pending_confirm.as_ref().unwrap().message
        );
        app.pending_confirm = None;
        app.request_restore_startup(Vec::new());
        assert!(app.pending_confirm.is_none());
    }

    fn scanned_env_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_env_scan();
        run_until_env_scanned(&mut app, ctx);
        app
    }

    fn pending_edit(app: &mut WinTuneApp, original: &str, wanted: Option<(&str, &str)>) {
        app.env_pending.vars.insert(
            VarKey::Existing(original.to_lowercase()),
            VarEdit {
                original: Some(original.to_string()),
                wanted: wanted.map(|(n, v)| (n.to_string(), v.to_string())),
            },
        );
    }

    #[test]
    fn deleting_a_protected_variable_asks_first() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "TEMP", 1, r"C:\t");
        let mut app = scanned_env_app(&fx, &ctx);
        pending_edit(&mut app, "TEMP", None);

        app.start_env_apply(&ctx);

        let confirm = app.pending_confirm.as_ref().expect("asks first");
        assert_eq!(confirm.action, ConfirmAction::ApplyEnvironment);
        let warning = messages::env_warning(app.tr, &EnvWarning::Removes("TEMP".into()));
        assert!(confirm.message.contains(&warning), "{}", confirm.message);
        let deleted = app.tr.t_args("env.confirm.deleted", &[&"TEMP"]);
        assert!(confirm.message.contains(&deleted), "{}", confirm.message);
        assert_eq!(confirm.confirm_label, app.tr.t("tweaks.apply"));
        assert!(!app.is_busy(), "applied before the answer");
        assert!(fx.user_env("TEMP").is_some());

        app.resolve_confirm(&ctx, true);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);
        assert_eq!(fx.user_env("TEMP"), None, "{}", app.last_notice());
    }

    #[test]
    fn a_plain_edit_applies_without_asking() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "FOO", 1, "1");
        let mut app = scanned_env_app(&fx, &ctx);
        pending_edit(&mut app, "FOO", Some(("FOO", "2")));

        app.start_env_apply(&ctx);

        assert!(app.pending_confirm.is_none());
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);
        assert_eq!(fx.user_env("FOO"), Some((1, "2".to_string())));
    }

    #[test]
    fn a_path_changed_elsewhere_is_named_in_the_confirmation() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A;B");
        let mut app = scanned_env_app(&fx, &ctx);
        app.env_path_edit_mut().unwrap().move_down(0);
        // An installer added a folder after the edit started.
        fx.seed_env(false, "Path", 2, "A;B;C");
        app.request_env_scan();
        run_until_env_scanned(&mut app, &ctx);

        app.start_env_apply(&ctx);

        let confirm = app.pending_confirm.as_ref().expect("asks first");
        let elsewhere = messages::env_warning(app.tr, &EnvWarning::PathChangedElsewhere);
        assert!(confirm.message.contains(&elsewhere), "{}", confirm.message);
        let removed = app.tr.t_args("env.confirm.path_removed", &[&"C"]);
        assert!(confirm.message.contains(&removed), "{}", confirm.message);
        assert!(!app.is_busy());
        assert_eq!(fx.user_env("Path"), Some((2, "A;B;C".to_string())));
    }

    #[test]
    fn restoring_path_names_removed_and_added_back_entries() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A;B");
        let mut app = scanned_env_app(&fx, &ctx);
        {
            let edit = app.env_path_edit_mut().unwrap();
            edit.remove(1);
            edit.add("C".into());
        }
        app.start_env_apply(&ctx);
        // Dropping B asks first.
        assert!(app.pending_confirm.is_some(), "{}", app.last_notice());
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert_eq!(fx.user_env("Path"), Some((2, "A;C".to_string())));

        // A program then added D.
        fx.seed_env(false, "Path", 2, "A;C;D");
        app.request_env_scan();
        run_until_env_scanned(&mut app, &ctx);

        let id = crate::environment::env_slot(crate::environment::PATH_NAME).id();
        app.request_restore_environment(vec![id]);
        let confirm = app.pending_confirm.as_ref().expect("asks first");
        let removed = app.tr.t_args("env.confirm.path_removed", &[&"C\nD"]);
        let added = app.tr.t_args("env.confirm.path_added", &[&"B"]);
        assert!(confirm.message.contains(&removed), "{}", confirm.message);
        assert!(confirm.message.contains(&added), "{}", confirm.message);
        assert_eq!(confirm.confirm_label, app.tr.t("confirm.restore"));
        assert!(!app.is_busy(), "restored without confirmation");

        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert_eq!(
            fx.user_env("Path"),
            Some((2, "A;B".to_string())),
            "{}",
            app.last_notice()
        );
        let last = app.history.first().expect("history entry");
        assert_eq!(
            (last.module.as_str(), last.action.as_str()),
            ("Environment variables", "Restore")
        );
    }

    #[test]
    fn restoring_one_half_of_a_rename_restores_both() {
        use crate::environment::env_slot;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "OLD", 1, "1");
        let mut app = scanned_env_app(&fx, &ctx);
        pending_edit(&mut app, "OLD", Some(("NEW", "1")));
        app.start_env_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        assert_eq!(fx.user_env("NEW"), Some((1, "1".to_string())));
        assert_eq!(fx.user_env("OLD"), None);

        let (old, new) = (env_slot("OLD").id(), env_slot("NEW").id());
        app.request_restore_environment(vec![new.clone()]);
        let confirm = app.pending_confirm.as_ref().expect("asks first");
        assert_eq!(
            confirm.action,
            ConfirmAction::RestoreEnvironment(vec![new, old])
        );
        assert!(confirm.message.contains("OLD"), "{}", confirm.message);
        assert!(confirm.message.contains("NEW"), "{}", confirm.message);

        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert_eq!(fx.user_env("OLD"), Some((1, "1".to_string())));
        assert_eq!(fx.user_env("NEW"), None);
    }

    #[test]
    fn restoring_nothing_asks_nothing_and_many_variables_are_counted() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.request_restore_environment(Vec::new());
        assert!(app.pending_confirm.is_none());
        let ids = ["A", "B", "C", "D"].map(|n| crate::environment::env_slot(n).id());
        app.request_restore_environment(ids.to_vec());
        let message = &app.pending_confirm.as_ref().unwrap().message;
        assert!(message.contains("4 variables"), "{message}");
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
