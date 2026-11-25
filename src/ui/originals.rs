//! History's "Saved originals": every snapshot WinTune keeps (associations
//! from `backups.json`, this user's tweaks from `tweaks-backups.json`),
//! grouped by module, each restorable on its own or per module.

use super::kit::{self, space, text_size};
use super::{ActiveTab, WinTuneApp};
use crate::context_menu::{entry_key, Place};
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

    /// What a saved context menu key is called: the label of the listed
    /// entry whose verb key it is (in any place), else the key path.
    pub(super) fn menu_key_title(&self, key: &str) -> String {
        self.menu_entries()
            .iter()
            .find(|e| {
                !e.label.trim().is_empty() && Place::ALL.iter().any(|p| entry_key(*p, &e.id) == key)
            })
            .map_or_else(|| key.to_string(), |e| e.label.trim().to_string())
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
    let clicked = match (all, one) {
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
        assert!(drawn_texts(&out).contains(&app.tr.t_args("originals.saved_on", &[&saved])));

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
