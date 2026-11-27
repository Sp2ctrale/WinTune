//! History's "Saved originals": every snapshot WinTune keeps (associations
//! from `backups.json`, this user's tweaks from `tweaks-backups.json`),
//! grouped by module, each restorable on its own or per module.

use super::kit::{self, space, text_size};
use super::{ActiveTab, WinTuneApp};
use crate::backups::Slot;
use crate::context_menu::{catalog_clsid, entry_key, Place, KEY_PREFIX};
use crate::i18n::{messages, Tr};
use crate::tweaks::find_tweak;
use eframe::egui::{self, RichText};

/// The module a saved original belongs to, in the order the groups show.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum OriginalsModule {
    Associations,
    Explorer,
    Start,
    ContextMenu,
    Startup,
    Environment,
    /// A tweak id the catalog no longer knows (e.g. removed in a later
    /// version): listed with its raw id, still restorable.
    Other,
}

impl OriginalsModule {
    const TWEAK_MODULES: [Self; 3] = [Self::Explorer, Self::Start, Self::Other];

    /// The module of tweak `id`: the tweaks tab that shows it, or `Other`.
    pub(super) fn of_tweak(id: &str) -> Self {
        match WinTuneApp::tab_of_tweak(id) {
            Some(ActiveTab::Tweaks) => Self::Explorer,
            Some(ActiveTab::StartPrivacy) => Self::Start,
            _ => Self::Other,
        }
    }

    fn title(self, tr: Tr) -> String {
        match self {
            Self::Associations => tr.t("history.module.associations"),
            Self::Explorer => tr.t("history.module.explorer"),
            Self::Start => tr.t("history.module.start"),
            Self::ContextMenu => tr.t("history.module.context_menu"),
            Self::Startup => tr.t("history.module.startup"),
            Self::Environment => tr.t("history.module.environment"),
            Self::Other => tr.t("originals.other"),
        }
    }
}

/// One saved original as listed: `key` is the extension or the tweak id.
struct OriginalRow {
    key: String,
    title: String,
    description: String,
}

/// A restore button that was clicked; acted on once the page is drawn.
enum RestoreRequest {
    AllRules,
    Rule(String),
    Tweaks(Vec<String>),
    MenuKeys(Vec<String>),
    Startup(Vec<String>),
    Environment(Vec<String>),
}

/// A secondary button that, given a reason, is disabled and says why on
/// hover. Returns whether it was clicked.
fn restore_button(ui: &mut egui::Ui, label: &str, disabled_reason: Option<&str>) -> bool {
    let resp = ui.add_enabled(disabled_reason.is_none(), kit::secondary_button(label));
    match disabled_reason {
        Some(reason) => resp.on_disabled_hover_text(reason),
        None => resp,
    }
    .clicked()
}

impl WinTuneApp {
    /// The saved originals of `module`, in key order. Tweaks are this
    /// user's only.
    fn originals_of(&self, module: OriginalsModule) -> Vec<OriginalRow> {
        let tr = self.tr;
        if module == OriginalsModule::Associations {
            return self
                .backup_store
                .backups
                .iter()
                .map(|(ext, backup)| {
                    let original = backup
                        .hklm_extension_key
                        .default_value
                        .clone()
                        .unwrap_or_else(|| tr.t("originals.none"));
                    OriginalRow {
                        key: ext.clone(),
                        title: ext.clone(),
                        description: tr
                            .t_args("originals.assoc_desc", &[&original, &backup.timestamp]),
                    }
                })
                .collect();
        }
        if module == OriginalsModule::ContextMenu {
            let Some(backups) = self.menu_store.backups_of(&self.env.user_label) else {
                return Vec::new();
            };
            return backups
                .iter()
                .map(|(key, backup)| OriginalRow {
                    key: key.clone(),
                    title: self.menu_key_title(key),
                    description: self.menu_key_description(key, &backup.timestamp),
                })
                .collect();
        }
        if module == OriginalsModule::Startup {
            let Some(backups) = self.startup_store.backups_of(&self.env.user_label) else {
                return Vec::new();
            };
            return backups
                .iter()
                .map(|(key, backup)| OriginalRow {
                    key: key.clone(),
                    title: self.startup_slot_name(key),
                    description: tr.t_args("originals.saved_on", &[&backup.timestamp]),
                })
                .collect();
        }
        if module == OriginalsModule::Environment {
            let Some(backups) = self.env_store.snapshots.backups_of(&self.env.user_label) else {
                return Vec::new();
            };
            return backups
                .iter()
                .map(|(key, backup)| OriginalRow {
                    key: key.clone(),
                    title: self.env_slot_name(key),
                    description: tr.t_args("originals.saved_on", &[&backup.timestamp]),
                })
                .collect();
        }
        let Some(backups) = self.tweak_store.backups_of(&self.env.user_label) else {
            return Vec::new();
        };
        let ids = match module {
            OriginalsModule::Explorer => {
                self.user_backups_in(Self::tweak_groups_of(ActiveTab::Tweaks))
            }
            OriginalsModule::Start => {
                self.user_backups_in(Self::tweak_groups_of(ActiveTab::StartPrivacy))
            }
            _ => backups
                .keys()
                .filter(|id| OriginalsModule::of_tweak(id) == OriginalsModule::Other)
                .cloned()
                .collect(),
        };
        ids.into_iter()
            .filter_map(|id| {
                let backup = backups.get(&id)?;
                let title = match find_tweak(&id) {
                    Some(t) if module != OriginalsModule::Other => messages::tweak_title(tr, t.id),
                    _ => id.clone(),
                };
                Some(OriginalRow {
                    description: tr.t_args("originals.saved_on", &[&backup.timestamp]),
                    key: id,
                    title,
                })
            })
            .collect()
    }

    /// What a saved context menu slot is called and where it applies. A
    /// `WinTune.*` key: the label of the listed entry whose verb key it is,
    /// else a generic "removed entry", with the place of its class path. Any
    /// other slot (a hidden or renamed existing entry): the scanned row's
    /// original name (else its name), else the catalog name of a Windows
    /// CLSID, else the verb name or CLSID from the id, with no place.
    pub(super) fn menu_key_name(&self, key: &str) -> (String, Option<Place>) {
        let slot = Slot::from_id(key);
        let own_key = match &slot {
            Slot::Key(path) => path
                .rsplit('\\')
                .next()
                .is_some_and(|name| name.to_lowercase().starts_with(&KEY_PREFIX.to_lowercase())),
            Slot::Value { .. } => false,
        };
        if !own_key {
            let scanned = self.menu_scan.as_ref().and_then(|s| s.by_slot(key));
            let name = match (scanned, slot) {
                // A renamed row by the name it had: that is what comes back.
                (Some(entry), _) => entry
                    .original_label
                    .clone()
                    .unwrap_or_else(|| self.existing_name(entry)),
                (None, Slot::Value { name, .. }) => match catalog_clsid(&name) {
                    Some(item) => self.tr.t(item.name_key),
                    None => name,
                },
                (None, Slot::Key(path)) => path.rsplit('\\').next().unwrap_or_default().to_string(),
            };
            return (name, None);
        }
        let place = Place::ALL.into_iter().find(|p| {
            key.strip_prefix(r"Software\Classes\")
                .and_then(|rest| rest.strip_prefix(p.class_path()))
                .is_some_and(|rest| rest.starts_with(r"\shell\"))
        });
        let label = place.and_then(|p| {
            self.menu_entries()
                .iter()
                .find(|e| !e.label.trim().is_empty() && entry_key(p, &e.id) == key)
                .map(|e| e.label.trim().to_string())
        });
        (
            label.unwrap_or_else(|| self.tr.t("menu.removed_entry")),
            place,
        )
    }

    /// Row title for a saved context menu key (see `menu_key_name`).
    pub(super) fn menu_key_title(&self, key: &str) -> String {
        self.menu_key_name(key).0
    }

    /// "Folders · Saved 2026-…" (the place alone when it is unknown).
    fn menu_key_description(&self, key: &str, timestamp: &str) -> String {
        let saved = self.tr.t_args("originals.saved_on", &[&timestamp]);
        match self.menu_key_name(key).1 {
            Some(p) => format!("{} \u{b7} {saved}", messages::place_name(self.tr, p)),
            None => saved,
        }
    }

    /// The "Saved originals" part of History: one section per module with
    /// snapshots, each with "Restore all" and a "Restore original" per row.
    /// An unreadable store shows a banner instead of its sections.
    pub(super) fn show_originals(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let p = kit::current_palette(ui.ctx());
        kit::section_title(ui, &tr.t("originals.title"));
        ui.label(
            RichText::new(tr.t("originals.subtitle"))
                .size(text_size::SECONDARY)
                .color(p.text_secondary),
        );
        ui.add_space(space::S);

        let assoc_reason = self.busy_or_blocked(&self.backup_store_error);
        let tweak_reason = self.busy_or_blocked(&self.tweak_store_error);
        let menu_reason = self.busy_or_blocked(&self.menu_store_error);
        let startup_reason = self.busy_or_blocked(&self.startup_store_error);
        let env_reason = self.busy_or_blocked(&self.env_store_error);
        let mut request = None;
        let mut shown = 0;

        match &self.backup_store_error {
            Some(e) => {
                let text = tr.t_args("originals.unreadable", &[&messages::load_error(tr, e)]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                shown += 1;
            }
            None => {
                let rows = self.originals_of(OriginalsModule::Associations);
                if !rows.is_empty() {
                    shown += 1;
                    let reason = assoc_reason.as_deref();
                    originals_section(
                        ui,
                        tr,
                        OriginalsModule::Associations,
                        rows,
                        reason,
                        &mut request,
                    );
                }
            }
        }
        match &self.tweak_store_error {
            Some(e) => {
                let text = tr.t_args("originals.unreadable", &[&messages::load_error(tr, e)]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                shown += 1;
            }
            None => {
                for module in OriginalsModule::TWEAK_MODULES {
                    let rows = self.originals_of(module);
                    if !rows.is_empty() {
                        shown += 1;
                        // The engine rejects an id it no longer knows.
                        let unknown = tr.t("originals.unknown_tweak");
                        let reason = match (&tweak_reason, module) {
                            (Some(busy), _) => Some(busy.as_str()),
                            (None, OriginalsModule::Other) => Some(unknown.as_str()),
                            (None, _) => None,
                        };
                        originals_section(ui, tr, module, rows, reason, &mut request);
                    }
                }
            }
        }
        match &self.menu_store_error {
            Some(e) => {
                let text = tr.t_args("originals.unreadable", &[&messages::load_error(tr, e)]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                shown += 1;
            }
            None => {
                let rows = self.originals_of(OriginalsModule::ContextMenu);
                if !rows.is_empty() {
                    shown += 1;
                    let reason = menu_reason.as_deref();
                    originals_section(
                        ui,
                        tr,
                        OriginalsModule::ContextMenu,
                        rows,
                        reason,
                        &mut request,
                    );
                }
            }
        }
        match &self.startup_store_error {
            Some(e) => {
                let text = tr.t_args("originals.unreadable", &[&messages::load_error(tr, e)]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                shown += 1;
            }
            None => {
                let rows = self.originals_of(OriginalsModule::Startup);
                if !rows.is_empty() {
                    shown += 1;
                    let reason = startup_reason.as_deref();
                    originals_section(ui, tr, OriginalsModule::Startup, rows, reason, &mut request);
                }
            }
        }
        match &self.env_store_error {
            Some(e) => {
                let text = tr.t_args("originals.unreadable", &[&messages::load_error(tr, e)]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                shown += 1;
            }
            None => {
                let rows = self.originals_of(OriginalsModule::Environment);
                if !rows.is_empty() {
                    shown += 1;
                    let reason = env_reason.as_deref();
                    originals_section(
                        ui,
                        tr,
                        OriginalsModule::Environment,
                        rows,
                        reason,
                        &mut request,
                    );
                }
            }
        }
        if shown == 0 {
            kit::card(ui, |ui| {
                kit::empty_state(ui, kit::Icon::Restore, &tr.t("originals.empty"), None);
            });
        }

        match request {
            Some(RestoreRequest::AllRules) => self.request_restore_all_rules(),
            Some(RestoreRequest::Rule(ext)) => self.request_restore_rule(&ext),
            Some(RestoreRequest::Tweaks(ids)) => self.request_restore_tweaks(ids),
            Some(RestoreRequest::MenuKeys(keys)) => self.request_restore_menu_keys(keys),
            Some(RestoreRequest::Startup(ids)) => self.request_restore_startup(ids),
            Some(RestoreRequest::Environment(ids)) => self.request_restore_environment(ids),
            None => {}
        }
    }
}

/// One module's section: "Restore all" on its title line, then one row per
/// saved original. Every button is disabled with `disabled_reason`.
fn originals_section(
    ui: &mut egui::Ui,
    tr: Tr,
    module: OriginalsModule,
    rows: Vec<OriginalRow>,
    disabled_reason: Option<&str>,
    request: &mut Option<RestoreRequest>,
) {
    let all_label = tr.t_args("originals.restore_all", &[&rows.len()]);
    let restore_label = tr.t("originals.restore");
    let mut all = false;
    let mut one: Option<String> = None;
    kit::section_with_action(
        ui,
        &module.title(tr),
        |ui| all = restore_button(ui, &all_label, disabled_reason),
        |ui| {
            for row in &rows {
                kit::SettingRow {
                    icon: None,
                    title: &row.title,
                    description: Some(&row.description),
                    hover: None,
                }
                .show(ui, |ui| {
                    if restore_button(ui, &restore_label, disabled_reason) {
                        one = Some(row.key.clone());
                    }
                });
            }
        },
    );
    let is_rules = module == OriginalsModule::Associations;
    let is_menu = module == OriginalsModule::ContextMenu;
    let is_startup = module == OriginalsModule::Startup;
    let is_env = module == OriginalsModule::Environment;
    let clicked = match (all, one) {
        (true, _) if is_env => {
            RestoreRequest::Environment(rows.into_iter().map(|r| r.key).collect())
        }
        (false, Some(key)) if is_env => RestoreRequest::Environment(vec![key]),
        (true, _) if is_startup => {
            RestoreRequest::Startup(rows.into_iter().map(|r| r.key).collect())
        }
        (false, Some(key)) if is_startup => RestoreRequest::Startup(vec![key]),
        (true, _) if is_rules => RestoreRequest::AllRules,
        (true, _) if is_menu => RestoreRequest::MenuKeys(rows.into_iter().map(|r| r.key).collect()),
        (false, Some(key)) if is_menu => RestoreRequest::MenuKeys(vec![key]),
        (true, _) => RestoreRequest::Tweaks(rows.into_iter().map(|r| r.key).collect()),
        (false, Some(key)) if is_rules => RestoreRequest::Rule(key),
        (false, Some(key)) => RestoreRequest::Tweaks(vec![key]),
        (false, None) => return,
    };
    *request = Some(clicked);
}

#[cfg(test)]
mod tests {
    use super::super::confirm::ConfirmAction;
    use super::super::test_support::*;
    use super::super::ActiveTab;
    use super::*;
    use egui::{FullOutput, Pos2};

    /// Draws the page a few times (so it settles) and returns the last
    /// frame.
    fn settle(app: &mut WinTuneApp, ctx: &egui::Context) -> FullOutput {
        for _ in 0..2 {
            frame(app, ctx, tall_input(vec![]));
        }
        frame(app, ctx, tall_input(vec![]))
    }

    /// Right edge of the sidebar, whose entries share the modules' names.
    const SIDEBAR_RIGHT: f32 = 220.0;

    /// Where `text` is drawn inside the saved originals (between their
    /// heading and the Operations heading, right of the sidebar), if it is.
    fn in_originals(app: &WinTuneApp, out: &FullOutput, text: &str) -> Option<Pos2> {
        let heading = text_center(out, &app.tr.t("originals.title"))?;
        let end = text_center(out, &app.tr.t("history.operations"))?;
        text_centers(out, text)
            .into_iter()
            .find(|c| c.x > SIDEBAR_RIGHT && c.y > heading.y && c.y < end.y)
    }

    /// Center of the button labelled `label` on the line of `row` (a row
    /// title or a section title) in the saved originals.
    fn button_on_row(app: &mut WinTuneApp, ctx: &egui::Context, row: &str, label: &str) -> Pos2 {
        let out = settle(app, ctx);
        let at = in_originals(app, &out, row).unwrap_or_else(|| panic!("{row:?} not drawn"));
        text_centers(&out, label)
            .into_iter()
            .filter(|c| c.x > at.x && (c.y - at.y).abs() < 24.0)
            .min_by(|a, b| (a.y - at.y).abs().total_cmp(&(b.y - at.y).abs()))
            .unwrap_or_else(|| panic!("no {label:?} on the line of {row:?}"))
    }

    fn apply_rules(app: &mut WinTuneApp, ctx: &egui::Context) {
        let rules = app.config.extensions.clone();
        app.start_apply(ctx, rules);
        run_until_idle(app, ctx);
    }

    fn apply_tweak(app: &mut WinTuneApp, ctx: &egui::Context, tab: ActiveTab, id: &'static str) {
        app.refresh_tweak_statuses();
        app.active_tab = tab;
        app.tweak_desired.insert(id, true);
        app.start_apply_tweaks(ctx);
        run_until_idle(app, ctx);
        assert!(
            app.tweak_store.has_backup("TEST\\user", id),
            "{id}: {}",
            app.last_notice()
        );
    }

    /// `.foo` applied, `show_hidden` (Explorer) and `no_app_launch_tracking`
    /// (Start) applied, History open.
    fn three_modules(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app_with_rules(ctx, &[".foo"]);
        apply_rules(&mut app, ctx);
        assert!(
            app.backup_store.find(".foo").is_some(),
            "{}",
            app.last_notice()
        );
        apply_tweak(&mut app, ctx, ActiveTab::Tweaks, "show_hidden");
        apply_tweak(
            &mut app,
            ctx,
            ActiveTab::StartPrivacy,
            "no_app_launch_tracking",
        );
        app.open_page(ActiveTab::History);
        app
    }

    fn pending(app: &WinTuneApp) -> Option<ConfirmAction> {
        app.pending_confirm.as_ref().map(|c| c.action.clone())
    }

    fn restore_label(app: &WinTuneApp) -> String {
        app.tr.t("originals.restore")
    }

    fn restore_all(app: &WinTuneApp, n: usize) -> String {
        app.tr.t_args("originals.restore_all", &[&n])
    }

    /// `name` edited to `value` through Apply, then History open.
    fn edited_variable(
        fx: &TempIcons,
        ctx: &egui::Context,
        name: &str,
        wanted: (&str, &str),
    ) -> WinTuneApp {
        use super::super::environment_state::VarKey;
        use crate::environment::VarEdit;
        fx.seed_env(false, name, 1, "old");
        let mut app = fx.app(ctx);
        app.request_env_scan();
        run_until_env_scanned(&mut app, ctx);
        app.env_pending.vars.insert(
            VarKey::Existing(name.to_ascii_lowercase()),
            VarEdit {
                original: Some(name.to_string()),
                wanted: Some((wanted.0.to_string(), wanted.1.to_string())),
            },
        );
        app.start_env_apply(ctx);
        run_until_idle(&mut app, ctx);
        assert!(
            app.env_store
                .snapshots
                .backups_of("TEST\\user")
                .is_some_and(|b| !b.is_empty()),
            "{}",
            app.last_notice()
        );
        app.open_page(ActiveTab::History);
        app
    }

    #[test]
    fn a_changed_variable_is_listed_in_saved_originals_and_restores() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = edited_variable(&fx, &ctx, "FOO", ("FOO", "new"));
        assert_eq!(fx.user_env("FOO"), Some((1, "new".to_string())));
        let slot = crate::environment::env_slot("FOO").id();

        let out = settle(&mut app, &ctx);
        let module = app.tr.t("history.module.environment");
        let m = in_originals(&app, &out, &module).expect("group title drawn");
        let r = in_originals(&app, &out, "FOO").expect("variable row drawn");
        assert!(r.y > m.y);
        let saved = app.env_store.snapshots.backups_of("TEST\\user").unwrap()[&slot]
            .timestamp
            .clone();
        let desc = app.tr.t_args("originals.saved_on", &[&saved]);
        assert!(drawn_texts(&out).contains(&desc), "{:?}", drawn_texts(&out));

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, "FOO", &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreEnvironment(vec![slot]))
        );
        assert!(!app.is_busy(), "a restore started without confirmation");

        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert_eq!(
            fx.user_env("FOO"),
            Some((1, "old".to_string())),
            "{}",
            app.last_notice()
        );
        assert_eq!(fx.broadcast_count(), 2);
        let out = settle(&mut app, &ctx);
        assert!(in_originals(&app, &out, &module).is_none());
        assert!(in_originals(&app, &out, "FOO").is_none());
    }

    #[test]
    fn a_renamed_pair_restores_together_from_history() {
        use crate::environment::env_slot;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = edited_variable(&fx, &ctx, "OLD", ("NEW", "old"));
        let (old, new) = (env_slot("OLD").id(), env_slot("NEW").id());

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, "OLD", &label);
        click_at(&mut app, &ctx, at);

        let Some(ConfirmAction::RestoreEnvironment(mut ids)) = pending(&app) else {
            panic!("no restore asked: {:?}", pending(&app));
        };
        ids.sort();
        let mut both = vec![old, new];
        both.sort();
        assert_eq!(ids, both);
    }

    #[test]
    fn an_unreadable_environment_store_shows_a_banner() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = edited_variable(&fx, &ctx, "FOO", ("FOO", "new"));
        std::fs::write(&app.env.env_store_path, "{ broken").unwrap();
        app.reload_env_store();
        let error = app.env_store_error.clone().expect("store error");

        let out = settle(&mut app, &ctx);
        let banner = app.tr.t_args(
            "originals.unreadable",
            &[&messages::load_error(app.tr, &error)],
        );
        assert!(drawn_texts(&out).contains(&banner), "no banner");
        let module = app.tr.t("history.module.environment");
        assert!(in_originals(&app, &out, &module).is_none());
        assert!(in_originals(&app, &out, "FOO").is_none());
    }

    #[test]
    fn each_module_lists_its_snapshots_and_restore_asks_first() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);
        let tr = app.tr;

        let out = settle(&mut app, &ctx);
        let texts = drawn_texts(&out);
        assert!(texts.contains(&tr.t("originals.subtitle")), "{texts:?}");
        let modules = [
            tr.t("history.module.associations"),
            tr.t("history.module.explorer"),
            tr.t("history.module.start"),
        ];
        let items = [
            ".foo".to_string(),
            messages::tweak_title(tr, "show_hidden"),
            messages::tweak_title(tr, "no_app_launch_tracking"),
        ];
        let mut last_y = 0.0;
        for (module, item) in modules.iter().zip(&items) {
            let m = in_originals(&app, &out, module)
                .unwrap_or_else(|| panic!("{module:?} not drawn: {texts:?}"));
            let i = in_originals(&app, &out, item)
                .unwrap_or_else(|| panic!("{item:?} not drawn: {texts:?}"));
            assert!(m.y > last_y && i.y > m.y, "{module:?} out of order");
            last_y = i.y;
        }
        let backup = app.backup_store.find(".foo").unwrap().clone();
        let desc = tr.t_args(
            "originals.assoc_desc",
            &[&tr.t("originals.none"), &backup.timestamp],
        );
        assert!(texts.contains(&desc), "{desc:?} not drawn: {texts:?}");
        let saved = app.tweak_store.backups_of("TEST\\user").unwrap()["show_hidden"]
            .timestamp
            .clone();
        assert!(texts.contains(&tr.t_args("originals.saved_on", &[&saved])));
        assert!(!texts.contains(&tr.t("originals.other")));

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, ".foo", &label);
        click_at(&mut app, &ctx, at);

        assert_eq!(
            pending(&app),
            Some(ConfirmAction::Rollback(vec![".foo".to_string()]))
        );
        assert!(!app.is_busy(), "a restore started without confirmation");
        assert!(app.backup_store.find(".foo").is_some());
    }

    #[test]
    fn restore_all_on_a_tweak_module_only_targets_that_module() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);

        let explorer = app.tr.t("history.module.explorer");
        let label = restore_all(&app, 1);
        let at = button_on_row(&mut app, &ctx, &explorer, &label);
        click_at(&mut app, &ctx, at);

        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreTweaks(
                vec!["show_hidden".to_string()]
            ))
        );
        assert!(!app.is_busy());
    }

    #[test]
    fn restore_all_on_associations_targets_every_rule() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".bar"]);
        apply_rules(&mut app, &ctx);
        assert_eq!(app.backup_store.backups.len(), 2, "{}", app.last_notice());
        app.open_page(ActiveTab::History);

        let assoc = app.tr.t("history.module.associations");
        let label = restore_all(&app, 2);
        let at = button_on_row(&mut app, &ctx, &assoc, &label);
        click_at(&mut app, &ctx, at);

        assert_eq!(
            pending(&app),
            Some(ConfirmAction::Rollback(vec![
                ".bar".to_string(),
                ".foo".to_string()
            ]))
        );
        assert!(!app.is_busy());
    }

    #[test]
    fn a_start_restore_from_history_runs_once_confirmed_and_is_logged_under_start() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);

        let start = app.tr.t("history.module.start");
        let label = restore_all(&app, 1);
        let at = button_on_row(&mut app, &ctx, &start, &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreTweaks(vec![
                "no_app_launch_tracking".to_string()
            ]))
        );
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);

        assert!(!app
            .tweak_store
            .has_backup("TEST\\user", "no_app_launch_tracking"));
        assert!(app.tweak_store.has_backup("TEST\\user", "show_hidden"));
        let e = &app.history[0];
        assert_eq!(
            (e.module.as_str(), e.action.as_str()),
            ("Start & Privacy", "Restore")
        );
    }

    #[test]
    fn request_restore_all_rules_does_nothing_without_a_snapshot() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        app.request_restore_all_rules();

        assert!(app.pending_confirm.is_none());
    }

    #[test]
    fn a_broken_backups_file_blocks_only_the_associations_group() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);
        std::fs::write(fx.dir.join("backups.json"), "{ broken").unwrap();
        app.reload_backup_store();
        let error = app.backup_store_error.clone().expect("store error");

        let out = settle(&mut app, &ctx);
        let texts = drawn_texts(&out);
        let banner = app.tr.t_args(
            "originals.unreadable",
            &[&messages::load_error(app.tr, &error)],
        );
        assert!(texts.contains(&banner), "no banner: {texts:?}");
        let assoc = app.tr.t("history.module.associations");
        assert!(in_originals(&app, &out, &assoc).is_none());
        assert!(!texts.contains(&".foo".to_string()));
        // Only the two tweak groups have buttons left.
        assert_eq!(text_centers(&out, &restore_all(&app, 1)).len(), 2);
        assert_eq!(text_centers(&out, &restore_label(&app)).len(), 2);

        let title = messages::tweak_title(app.tr, "show_hidden");
        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, &title, &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreTweaks(
                vec!["show_hidden".to_string()]
            ))
        );
    }

    #[test]
    fn a_broken_tweak_store_blocks_only_the_tweak_groups() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);
        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        app.reload_tweak_store();
        let error = app.tweak_store_error.clone().expect("store error");

        let out = settle(&mut app, &ctx);
        let texts = drawn_texts(&out);
        let banner = app.tr.t_args(
            "originals.unreadable",
            &[&messages::load_error(app.tr, &error)],
        );
        assert!(texts.contains(&banner), "no banner: {texts:?}");
        for key in ["history.module.explorer", "history.module.start"] {
            assert!(
                in_originals(&app, &out, &app.tr.t(key)).is_none(),
                "{key} drawn"
            );
        }
        let title = messages::tweak_title(app.tr, "show_hidden");
        assert!(!texts.contains(&title));
        // Only the associations group has buttons left.
        assert_eq!(text_centers(&out, &restore_label(&app)).len(), 1);

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, ".foo", &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::Rollback(vec![".foo".to_string()]))
        );
    }

    #[test]
    fn an_unknown_tweak_id_is_listed_under_other_and_cannot_be_restored() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.tweak_store
            .users
            .entry("TEST\\user".to_string())
            .or_default()
            .insert(
                "gone_tweak".to_string(),
                crate::tweaks::TweakBackup {
                    timestamp: "2026-01-02 03:04:05".into(),
                    snapshot: crate::tweaks::TweakSnapshot::Value(Default::default()),
                },
            );
        app.active_tab = ActiveTab::History;

        let out = settle(&mut app, &ctx);
        let other = app.tr.t("originals.other");
        let o = in_originals(&app, &out, &other).expect("Other drawn");
        let g = in_originals(&app, &out, "gone_tweak").expect("raw id drawn");
        assert!(g.y > o.y);
        assert_eq!(
            super::OriginalsModule::of_tweak("gone_tweak"),
            super::OriginalsModule::Other
        );

        // The engine rejects an unknown id, so both buttons are disabled
        // with a reason and nothing is asked.
        let reason = app.tr.t("originals.unknown_tweak");
        for (row, label) in [
            ("gone_tweak".to_string(), restore_label(&app)),
            (other, restore_all(&app, 1)),
        ] {
            let at = button_on_row(&mut app, &ctx, &row, &label);
            assert!(
                hover_texts(&mut app, &ctx, at).contains(&reason),
                "no reason over {label:?} of {row:?}"
            );
            click_at(&mut app, &ctx, at);
            assert!(app.pending_confirm.is_none(), "{label:?} of {row:?} asked");
        }
    }

    /// One entry "Open one" in the Folder place, applied, History open.
    fn applied_menu_entry(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        use crate::context_menu::{MenuEntry, Place, ProgramChoice};
        let mut config = fx.config();
        config
            .context_menu
            .entries_mut("TEST\\user")
            .push(MenuEntry {
                id: "e1".into(),
                label: "Open one".into(),
                program: ProgramChoice::Cmd,
                places: [Place::Folder].into_iter().collect(),
                icon: None,
            });
        let mut app = fx.app_with(ctx, config);
        app.start_menu_apply(ctx);
        run_until_idle(&mut app, ctx);
        assert!(
            app.menu_store.backups_of("TEST\\user").is_some(),
            "{}",
            app.last_notice()
        );
        app.open_page(ActiveTab::History);
        app
    }

    #[test]
    fn the_context_menu_group_lists_and_restores_after_confirm() {
        use crate::context_menu::{entry_key, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = applied_menu_entry(&fx, &ctx);
        let key = fx.hkcu(&entry_key(Place::Folder, "e1"));
        assert!(fx.key_exists(&key));

        let out = settle(&mut app, &ctx);
        let module = app.tr.t("history.module.context_menu");
        let m = in_originals(&app, &out, &module).expect("group title drawn");
        let r = in_originals(&app, &out, "Open one").expect("entry label drawn");
        assert!(r.y > m.y);
        let saved = app.menu_store.backups_of("TEST\\user").unwrap()
            [&entry_key(Place::Folder, "e1")]
            .timestamp
            .clone();
        let place = messages::place_name(app.tr, Place::Folder);
        let desc = format!(
            "{place} \u{b7} {}",
            app.tr.t_args("originals.saved_on", &[&saved])
        );
        assert!(drawn_texts(&out).contains(&desc));

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, "Open one", &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreMenuKeys(vec![entry_key(
                Place::Folder,
                "e1"
            )]))
        );
        assert!(!app.is_busy(), "a restore started without confirmation");
        assert!(fx.key_exists(&key));

        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert!(!fx.key_exists(&key));
        assert!(app
            .menu_store
            .backups_of("TEST\\user")
            .is_none_or(|b| b.is_empty()));
        let e = &app.history[0];
        assert_eq!(
            (e.module.as_str(), e.action.as_str()),
            ("Context menu", "Restore")
        );
    }

    #[test]
    fn a_two_place_entry_gives_distinguishable_rows_and_a_deduped_confirm() {
        use crate::context_menu::{entry_key, MenuEntry, Place, ProgramChoice};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut config = fx.config();
        config
            .context_menu
            .entries_mut("TEST\\user")
            .push(MenuEntry {
                id: "e1".into(),
                label: "Open one".into(),
                program: ProgramChoice::Cmd,
                places: [Place::Folder, Place::Background].into_iter().collect(),
                icon: None,
            });
        let mut app = fx.app_with(&ctx, config);
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        app.open_page(ActiveTab::History);

        let out = settle(&mut app, &ctx);
        let texts = drawn_texts(&out);
        for place in [Place::Folder, Place::Background] {
            let name = messages::place_name(app.tr, place);
            let saved = app.menu_store.backups_of("TEST\\user").unwrap()[&entry_key(place, "e1")]
                .timestamp
                .clone();
            let desc = format!(
                "{name} \u{b7} {}",
                app.tr.t_args("originals.saved_on", &[&saved])
            );
            assert!(texts.contains(&desc), "{desc:?} not drawn: {texts:?}");
        }

        let label = restore_all(&app, 2);
        let module = app.tr.t("history.module.context_menu");
        let at = button_on_row(&mut app, &ctx, &module, &label);
        click_at(&mut app, &ctx, at);
        let confirm = app.pending_confirm.as_ref().expect("confirm");
        assert_eq!(
            confirm.message.matches("Open one").count(),
            1,
            "{}",
            confirm.message
        );
    }

    #[test]
    fn a_removed_entry_is_named_generically_with_its_place() {
        use crate::context_menu::{entry_key, Place};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = applied_menu_entry(&fx, &ctx);
        app.menu_entries_mut().clear();

        let out = settle(&mut app, &ctx);
        let texts = drawn_texts(&out);
        let title = app.tr.t("menu.removed_entry");
        assert!(in_originals(&app, &out, &title).is_some(), "{texts:?}");
        let name = messages::place_name(app.tr, Place::Folder);
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with(&format!("{name} \u{b7} "))),
            "{texts:?}"
        );
        let key = entry_key(Place::Folder, "e1");
        assert!(!texts.iter().any(|t| t.contains(&key)), "{texts:?}");
    }

    #[test]
    fn a_hidden_entry_is_listed_by_its_name_and_restores() {
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
        run_until_idle(&mut app, &ctx);
        run_until_scanned(&mut app, &ctx);
        let key = r"Software\Classes\Directory\shell\AppVerb";
        let copy = fx.hkcu(key);
        assert!(fx.key_exists(&copy), "{}", app.last_notice());
        app.open_page(ActiveTab::History);

        let out = settle(&mut app, &ctx);
        let module = app.tr.t("history.module.context_menu");
        let m = in_originals(&app, &out, &module).expect("group title drawn");
        let r = in_originals(&app, &out, "Open with App").expect("scanned label drawn");
        assert!(r.y > m.y);
        let saved = app.menu_store.backups_of("TEST\\user").unwrap()[key]
            .timestamp
            .clone();
        let desc = app.tr.t_args("originals.saved_on", &[&saved]);
        assert!(drawn_texts(&out).contains(&desc), "{:?}", drawn_texts(&out));

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, "Open with App", &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreMenuKeys(vec![key.to_string()]))
        );
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert!(!fx.key_exists(&copy), "{}", app.last_notice());
    }

    #[test]
    fn a_saved_slot_the_scan_does_not_know_is_named_from_its_id() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        let clsid = "{11111111-2222-3333-4444-555555555555}";
        let value = format!("{}\\\\{clsid}", crate::context_menu::BLOCKED_KEY);
        assert_eq!(app.menu_key_name(&value), (clsid.to_string(), None));
        let verb = r"Software\Classes\Directory\shell\SomeVerb";
        assert_eq!(app.menu_key_name(verb), ("SomeVerb".to_string(), None));
    }

    #[test]
    fn a_saved_catalog_clsid_the_scan_does_not_list_uses_the_catalog_name() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        assert!(app.menu_scan.is_none());
        let value = format!(
            "{}\\\\{}",
            crate::context_menu::BLOCKED_KEY,
            "{596ab062-b4d2-4215-9f74-e9109b0a8153}"
        );
        assert_eq!(
            app.menu_key_name(&value),
            (app.tr.t("menu.win.previous_versions"), None)
        );
    }

    #[test]
    fn a_renamed_row_is_titled_by_its_original_name() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let copy = r"Software\Classes\Directory\shell\AppVerb";
        let (k, _) = fx.sandbox.root.create_subkey(fx.hkcu(copy)).unwrap();
        k.set_value("MUIVerb", &"Renamed").unwrap();
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        assert_eq!(app.menu_scan.as_ref().unwrap().others[0].label, "Renamed");
        assert_eq!(app.menu_key_name(copy), ("Open with App".to_string(), None));
    }

    #[test]
    fn a_saved_catalog_slot_uses_the_translated_name() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_clsid("{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}");
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        let value = format!(
            "{}\\\\{}",
            crate::context_menu::BLOCKED_KEY,
            "{f81e9010-6ea4-11ce-a7ff-00aa003ca9f6}"
        );
        assert_eq!(
            app.menu_key_name(&value).0,
            app.tr.t("menu.win.give_access")
        );
    }

    #[test]
    fn a_broken_menu_store_blocks_only_its_group() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);
        std::fs::write(&app.env.menu_store_path, "{ broken").unwrap();
        app.reload_menu_store();
        let error = app.menu_store_error.clone().expect("store error");

        let out = settle(&mut app, &ctx);
        let texts = drawn_texts(&out);
        let banner = app.tr.t_args(
            "originals.unreadable",
            &[&messages::load_error(app.tr, &error)],
        );
        assert!(texts.contains(&banner), "no banner: {texts:?}");
        // The three other groups keep their buttons.
        assert_eq!(text_centers(&out, &restore_label(&app)).len(), 3);
        assert!(app.menu_store.users.is_empty());

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, ".foo", &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::Rollback(vec![".foo".to_string()]))
        );
    }

    #[test]
    fn a_broken_menu_store_hides_the_group_and_its_entries() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = applied_menu_entry(&fx, &ctx);
        std::fs::write(&app.env.menu_store_path, "{ broken").unwrap();
        app.reload_menu_store();

        let out = settle(&mut app, &ctx);
        let module = app.tr.t("history.module.context_menu");
        assert!(in_originals(&app, &out, &module).is_none());
        assert!(!drawn_texts(&out).contains(&"Open one".to_string()));
    }

    /// "App" turned off through Apply, History open.
    fn turned_off_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        fx.seed_run(false, "App", r#""C:\Apps\app.exe""#);
        let mut app = fx.app(ctx);
        app.request_startup_scan();
        run_until_startup_scanned(&mut app, ctx);
        app.startup_pending.insert("run:app".into(), false);
        app.start_startup_apply(ctx);
        run_until_idle(&mut app, ctx);
        assert!(fx.user_switch("App").is_some(), "{}", app.last_notice());
        app.open_page(ActiveTab::History);
        app
    }

    #[test]
    fn a_turned_off_app_is_listed_in_saved_originals_and_restores() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = turned_off_app(&fx, &ctx);
        let slot = crate::startup::Approved::Run.slot("App").id();

        let out = settle(&mut app, &ctx);
        let module = app.tr.t("history.module.startup");
        let m = in_originals(&app, &out, &module).expect("group title drawn");
        let r = in_originals(&app, &out, "App").expect("app row drawn");
        assert!(r.y > m.y);
        let saved = app.startup_store.backups_of("TEST\\user").unwrap()[&slot]
            .timestamp
            .clone();
        let desc = app.tr.t_args("originals.saved_on", &[&saved]);
        assert!(drawn_texts(&out).contains(&desc), "{:?}", drawn_texts(&out));

        let label = restore_label(&app);
        let at = button_on_row(&mut app, &ctx, "App", &label);
        click_at(&mut app, &ctx, at);
        assert_eq!(
            pending(&app),
            Some(ConfirmAction::RestoreStartup(vec![slot]))
        );
        assert!(!app.is_busy(), "a restore started without confirmation");

        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);
        assert!(fx.user_switch("App").is_none(), "{}", app.last_notice());
        let out = settle(&mut app, &ctx);
        assert!(in_originals(&app, &out, &module).is_none());
        assert!(in_originals(&app, &out, "App").is_none());
    }

    #[test]
    fn an_unreadable_startup_store_shows_a_banner() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = turned_off_app(&fx, &ctx);
        std::fs::write(&app.env.startup_store_path, "{ broken").unwrap();
        app.reload_startup_store();
        let error = app.startup_store_error.clone().expect("store error");

        let out = settle(&mut app, &ctx);
        let banner = app.tr.t_args(
            "originals.unreadable",
            &[&messages::load_error(app.tr, &error)],
        );
        assert!(drawn_texts(&out).contains(&banner), "no banner");
        let module = app.tr.t("history.module.startup");
        assert!(in_originals(&app, &out, &module).is_none());
        assert!(in_originals(&app, &out, "App").is_none());
    }

    #[test]
    fn tweak_ids_map_to_their_module() {
        use super::OriginalsModule;
        assert_eq!(
            OriginalsModule::of_tweak("show_hidden"),
            OriginalsModule::Explorer
        );
        assert_eq!(
            OriginalsModule::of_tweak("no_app_launch_tracking"),
            OriginalsModule::Start
        );
        for tweak in crate::tweaks::catalog() {
            assert_ne!(
                OriginalsModule::of_tweak(tweak.id),
                OriginalsModule::Other,
                "{}",
                tweak.id
            );
        }
    }

    #[test]
    fn while_a_job_runs_every_restore_is_disabled() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = three_modules(&fx, &ctx);
        let release = hold_job(&mut app);
        let busy = app.busy_or_blocked(&None).expect("busy reason");
        let tr = app.tr;

        let restore = restore_label(&app);
        let one = restore_all(&app, 1);
        let buttons = [
            (".foo".to_string(), restore.clone()),
            (messages::tweak_title(tr, "show_hidden"), restore.clone()),
            (
                messages::tweak_title(tr, "no_app_launch_tracking"),
                restore.clone(),
            ),
            (tr.t("history.module.associations"), one.clone()),
            (tr.t("history.module.explorer"), one.clone()),
            (tr.t("history.module.start"), one.clone()),
        ];
        for (row, label) in buttons {
            let at = button_on_row(&mut app, &ctx, &row, &label);
            assert!(
                hover_texts(&mut app, &ctx, at).contains(&busy),
                "no busy reason over {label:?} of {row:?}"
            );
            click_at(&mut app, &ctx, at);
            assert!(app.pending_confirm.is_none(), "{label:?} of {row:?} asked");
        }
        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn opening_history_reloads_the_stores_unless_busy() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        // Idle: opening History reloads both stores.
        app.backup_store_error = Some(corrupt("backups.json", "sentinel"));
        app.tweak_store_error = Some(corrupt("tweaks-backups.json", "sentinel"));
        app.open_page(ActiveTab::History);
        assert_eq!(app.backup_store_error, None);
        assert_eq!(app.tweak_store_error, None);

        // Under a running job they are left alone.
        app.open_page(ActiveTab::Home);
        let release = hold_job(&mut app);
        app.backup_store_error = Some(corrupt("backups.json", "sentinel"));
        app.tweak_store_error = Some(corrupt("tweaks-backups.json", "sentinel"));
        app.open_page(ActiveTab::History);
        assert_eq!(
            app.backup_store_error,
            Some(corrupt("backups.json", "sentinel"))
        );
        assert_eq!(
            app.tweak_store_error,
            Some(corrupt("tweaks-backups.json", "sentinel"))
        );
        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn with_no_snapshot_the_section_says_so() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.open_page(ActiveTab::History);

        let texts = drawn_texts(&settle(&mut app, &ctx));
        for key in ["originals.title", "originals.empty"] {
            assert!(texts.contains(&app.tr.t(key)), "{key} not drawn: {texts:?}");
        }
        assert!(!texts.contains(&restore_label(&app)));
        assert!(!texts.iter().any(|t| t.starts_with("Restore all")));
    }
}
