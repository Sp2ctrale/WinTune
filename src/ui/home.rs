//! The Home dashboard: one tile per module, a banner when settings drifted,
//! and the latest operations.

use super::{kit, ActiveTab, WinTuneApp};
use crate::associations::RuleStatus;
use crate::context_menu::{EntryStatus, ExistingState};
use crate::i18n::messages;
use crate::startup::StartupState;
use crate::tweaks::TweakStatus;
use eframe::egui;

/// Operations listed under "Recent".
const RECENT_LIMIT: usize = 5;

/// What one module tile shows.
pub(super) struct ModuleSummary {
    pub page: ActiveTab,
    pub icon: kit::Icon,
    pub status: String,
    pub drift: usize,
}

impl WinTuneApp {
    /// One summary per Customize page, in navigation order.
    pub(super) fn module_summaries(&self) -> Vec<ModuleSummary> {
        let tr = self.tr;
        let tweak_summary = |page: ActiveTab, icon: kit::Icon| {
            let groups = Self::tweak_groups_of(page);
            let (mut total, mut on, mut custom) = (0usize, 0usize, 0usize);
            for tweak in crate::tweaks::catalog()
                .iter()
                .filter(|t| groups.contains(&t.group))
            {
                total += 1;
                match self.tweak_statuses.get(tweak.id) {
                    Some(TweakStatus::On) => on += 1,
                    Some(TweakStatus::Custom(_)) => custom += 1,
                    _ => {}
                }
            }
            let mut status = tr.t_args("home.tweaks_status", &[&on, &total]);
            if custom > 0 {
                status.push_str(&tr.t_args("home.custom_suffix", &[&custom]));
            }
            ModuleSummary {
                page,
                icon,
                status,
                drift: custom,
            }
        };

        let enabled = self.config.extensions.iter().filter(|r| r.enabled).count();
        let changed = self
            .rule_statuses
            .values()
            .filter(|s| matches!(s, RuleStatus::Drifted(_)))
            .count();
        let menu_count = self.menu_entries().len();
        // 0 until the first scan ends.
        let menu_hidden = self.menu_scan.as_ref().map_or(0, |scan| {
            scan.windows
                .iter()
                .chain(&scan.others)
                .filter(|e| e.state == ExistingState::HiddenByWinTune)
                .count()
        });
        let menu_changed = self
            .menu_entries()
            .iter()
            .filter(|e| matches!(self.menu_statuses.get(&e.id), Some(EntryStatus::Changed(_))))
            .count();
        // 0 until the first scan ends.
        let (startup_count, startup_off) = self.startup_scan.as_ref().map_or((0, 0), |scan| {
            let off = scan
                .items
                .iter()
                .filter(|i| i.state != StartupState::On)
                .count();
            (scan.items.len(), off)
        });
        // 0 until the first scan ends.
        let (env_vars, env_path) = self.env_scan.as_ref().map_or((0, 0), |scan| {
            (scan.user.len(), scan.user_path_entries().len())
        });
        vec![
            ModuleSummary {
                page: ActiveTab::Associations,
                icon: kit::Icon::Associations,
                status: tr.t_args("home.rules_status", &[&enabled, &changed]),
                drift: changed,
            },
            tweak_summary(ActiveTab::Tweaks, kit::Icon::Explorer),
            tweak_summary(ActiveTab::StartPrivacy, kit::Icon::Start),
            ModuleSummary {
                page: ActiveTab::ContextMenu,
                icon: kit::Icon::ContextMenu,
                status: tr.t_args("home.menu_status", &[&menu_count, &menu_hidden]),
                drift: menu_changed,
            },
            ModuleSummary {
                page: ActiveTab::Startup,
                icon: kit::Icon::Startup,
                status: tr.t_args("home.startup_status", &[&startup_count, &startup_off]),
                drift: 0,
            },
            ModuleSummary {
                page: ActiveTab::Environment,
                icon: kit::Icon::Environment,
                status: tr.t_args("home.environment_status", &[&env_vars, &env_path]),
                drift: 0,
            },
        ]
    }

    /// The first page where something drifted.
    pub(super) fn drift_target(&self) -> Option<ActiveTab> {
        self.module_summaries()
            .into_iter()
            .find(|s| s.drift > 0)
            .map(|s| s.page)
    }

    pub(super) fn show_home(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let p = kit::current_palette(ctx);
        let summaries = self.module_summaries();
        let total_drift: usize = summaries.iter().map(|s| s.drift).sum();
        let target = self.drift_target();
        let title = tr.t("home.title");
        let subtitle = tr.t("home.subtitle");
        let drift_text = tr.t_args("home.drift", &[&total_drift]);
        let review = tr.t("home.review");
        let recent_title = tr.t("home.recent");
        let no_recent = tr.t("home.no_recent");
        let view_all = tr.t("home.view_all");
        let tile_title = |page: ActiveTab| match page {
            ActiveTab::Associations => tr.t("nav.associations"),
            ActiveTab::Tweaks => tr.t("nav.explorer"),
            ActiveTab::StartPrivacy => tr.t("nav.start"),
            ActiveTab::Startup => tr.t("nav.startup"),
            ActiveTab::Environment => tr.t("nav.environment"),
            ActiveTab::MouseKeyboard => tr.t("nav.mouse_keyboard"),
            _ => tr.t("nav.context_menu"),
        };
        let history = &self.history;
        let mut open: Option<ActiveTab> = None;

        Self::page_panel(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                kit::page_header(ui, &[], &title, Some(&subtitle), |_| {});
                if let Some(target) = target {
                    if kit::banner(ui, kit::BannerKind::Warning, &drift_text, Some(&review)) {
                        open = Some(target);
                    }
                }
                for row in summaries.chunks(2) {
                    ui.columns(2, |cols| {
                        for (col, s) in cols.iter_mut().zip(row) {
                            let r =
                                kit::tile(col, s.icon, &tile_title(s.page), &s.status, true, None);
                            if r.clicked() {
                                open = Some(s.page);
                            }
                        }
                    });
                    ui.add_space(kit::space::M);
                }
                ui.add_space(kit::space::M);
                kit::section(ui, &recent_title, |ui| {
                    if history.is_empty() {
                        ui.label(egui::RichText::new(&no_recent).color(p.text_secondary));
                    } else {
                        egui::Grid::new("home_recent")
                            .num_columns(4)
                            .spacing([kit::space::XL, kit::space::XS])
                            .show(ui, |ui| {
                                for entry in history.iter().take(RECENT_LIMIT) {
                                    ui.label(
                                        egui::RichText::new(&entry.time).color(p.text_secondary),
                                    );
                                    ui.label(messages::history_module(tr, &entry.module));
                                    ui.label(
                                        egui::RichText::new(messages::history_action(
                                            tr,
                                            &entry.action,
                                        ))
                                        .strong(),
                                    );
                                    let n = entry.succeeded.len();
                                    ui.label(if entry.is_success() {
                                        egui::RichText::new(tr.t_args("history.ok", &[&n]))
                                            .color(p.success)
                                    } else {
                                        egui::RichText::new(tr.t_args(
                                            "history.ok_errors",
                                            &[&n, &entry.errors.len()],
                                        ))
                                        .color(p.warning)
                                    });
                                    ui.end_row();
                                }
                            });
                    }
                    ui.add_space(kit::space::S);
                    if ui.link(&view_all).clicked() {
                        open = Some(ActiveTab::History);
                    }
                });
            });
        });

        if let Some(page) = open {
            self.open_page(page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::associations::DriftReason;
    use crate::tweaks::CustomReason;
    use egui::{Event, Modifiers, PointerButton, RawInput};

    fn status_of(app: &WinTuneApp, page: ActiveTab) -> (String, usize) {
        let s = app
            .module_summaries()
            .into_iter()
            .find(|s| s.page == page)
            .expect("summary for page");
        (s.status, s.drift)
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        frame(app, ctx, RawInput::default());
        let out = frame(app, ctx, RawInput::default());
        let at = text_center(&out, text).unwrap_or_else(|| panic!("{text:?} not drawn"));
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        for events in [
            vec![Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ] {
            let input = RawInput {
                events,
                ..Default::default()
            };
            frame(app, ctx, input);
        }
    }

    fn set_one_custom(app: &mut WinTuneApp) {
        // A Start & Privacy tweak.
        let id = crate::tweaks::catalog()
            .iter()
            .find(|t| WinTuneApp::tweak_groups_of(ActiveTab::StartPrivacy).contains(&t.group))
            .unwrap()
            .id;
        app.tweak_statuses.insert(
            id,
            TweakStatus::Custom(CustomReason::Unreadable("x".into())),
        );
    }

    #[test]
    fn summaries_count_enabled_and_changed_rules() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".aaa", ".bbb", ".ccc"]);
        app.config.extensions[2].enabled = false;
        app.rule_statuses.insert(".aaa".into(), RuleStatus::Applied);
        app.rule_statuses.insert(
            ".bbb".into(),
            RuleStatus::Drifted(vec![DriftReason::DefaultMissing {
                ext: ".bbb".into(),
                expected: "x".into(),
            }]),
        );
        let (status, drift) = status_of(&app, ActiveTab::Associations);
        assert_eq!(status, "2 active · 1 changed");
        assert_eq!(drift, 1);
    }

    #[test]
    fn summaries_count_tweaks_on_per_page() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.tweak_statuses.clear();
        app.tweak_statuses
            .insert("show_extensions", TweakStatus::On);
        let (status, drift) = status_of(&app, ActiveTab::Tweaks);
        assert!(status.starts_with("1 of "), "{status}");
        assert_eq!(drift, 0);
    }

    #[test]
    fn the_drift_banner_opens_the_first_drifted_page() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.rule_statuses.clear();
        app.tweak_statuses.clear();
        set_one_custom(&mut app);
        app.active_tab = ActiveTab::Home;
        click_text(&mut app, &ctx, "Review");
        assert_eq!(app.active_tab, ActiveTab::StartPrivacy);
    }

    #[test]
    fn no_drift_no_banner() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.rule_statuses.clear();
        app.tweak_statuses.clear();
        app.active_tab = ActiveTab::Home;
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(!drawn_texts(&out).iter().any(|t| t == "Review"));
    }

    #[test]
    fn a_tile_opens_its_page() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.active_tab = ActiveTab::Home;
        let status = status_of(&app, ActiveTab::StartPrivacy).0;
        click_text(&mut app, &ctx, &status);
        assert_eq!(app.active_tab, ActiveTab::StartPrivacy);
    }

    #[test]
    fn recent_lists_at_most_five_entries() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        for i in 1..=7 {
            app.record_history(
                "File associations",
                &format!("A{i}"),
                &crate::associations::OperationReport::default(),
            );
        }
        app.active_tab = ActiveTab::Home;
        let out = frame(&mut app, &ctx, RawInput::default());
        let texts = drawn_texts(&out);
        for a in ["A7", "A6", "A5", "A4", "A3"] {
            assert!(texts.iter().any(|t| t == a), "{a} missing");
        }
        assert!(!texts.iter().any(|t| t == "A2"));
    }

    #[test]
    fn the_context_menu_tile_counts_entries_and_changes() {
        use crate::context_menu::{MenuEntry, Place, ProgramChoice};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut config = fx.config();
        for (id, label) in [("e1", "One"), ("e2", "Two")] {
            config
                .context_menu
                .entries_mut("TEST\\user")
                .push(MenuEntry {
                    id: id.into(),
                    label: label.into(),
                    program: ProgramChoice::Cmd,
                    places: [Place::Folder].into_iter().collect(),
                    icon: None,
                });
        }
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = fx.app_with(&ctx, config);
        app.rule_statuses.clear();
        app.tweak_statuses.clear();
        // Before the first scan ends, nothing counts as hidden.
        let before = app.tr.t_args("home.menu_status", &[&2, &0]);
        assert_eq!(status_of(&app, ActiveTab::ContextMenu).0, before);
        scan_now(&mut app, &ctx);
        let id = app.menu_scan.as_ref().unwrap().others[0].id.clone();
        app.menu_pending.insert(
            id,
            super::super::menu_existing_state::PendingExisting {
                hidden: Some(true),
                label: None,
            },
        );
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        run_until_scanned(&mut app, &ctx);
        let expected = app.tr.t_args("home.menu_status", &[&2, &1]);
        assert_eq!(expected, "2 entries, 1 hidden");
        assert_eq!(
            status_of(&app, ActiveTab::ContextMenu),
            (expected.clone(), 0)
        );
        assert_eq!(app.drift_target(), None);

        // Edited outside WinTune: the command of one key changes.
        let key = crate::context_menu::entry_key(Place::Folder, "e2");
        let path = fx.hkcu(&format!("{key}\\command"));
        let (command, _) = fx.sandbox.root.create_subkey(&path).unwrap();
        command.set_value("", &"other.exe").unwrap();
        app.refresh_menu_statuses();

        assert_eq!(
            status_of(&app, ActiveTab::ContextMenu),
            (expected.clone(), 1)
        );
        assert_eq!(app.drift_target(), Some(ActiveTab::ContextMenu));
        app.active_tab = ActiveTab::Home;
        let texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
        assert!(texts.contains(&expected), "{texts:?}");
    }

    #[test]
    fn the_startup_tile_counts_apps_and_turned_off() {
        use crate::startup::{Approved, Origin, Source, StartupItem, StartupScan, StartupState};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.rule_statuses.clear();
        app.tweak_statuses.clear();
        // Both counts are 0 until the first scan.
        let before = app.tr.t_args("home.startup_status", &[&0, &0]);
        assert_eq!(status_of(&app, ActiveTab::Startup), (before, 0));

        let item = |name: &str, state| StartupItem {
            id: format!("run:{name}"),
            approved: Approved::Run,
            name: name.into(),
            display_name: name.into(),
            program: format!(r"C:\Apps\{name}.exe"),
            origins: vec![Origin {
                source: Source::UserRun,
                detail: String::new(),
            }],
            program_missing: false,
            state,
            locked: None,
            has_snapshot: false,
        };
        app.startup_scan = Some(StartupScan {
            items: vec![item("a", StartupState::On), item("b", StartupState::Off)],
        });
        let expected = app.tr.t_args("home.startup_status", &[&2, &1]);
        assert_eq!(expected, "2 apps, 1 turned off");
        assert_eq!(status_of(&app, ActiveTab::Startup), (expected.clone(), 0));

        app.active_tab = ActiveTab::Home;
        let out = {
            frame(&mut app, &ctx, RawInput::default());
            frame(&mut app, &ctx, RawInput::default())
        };
        let texts = drawn_texts(&out);
        assert!(texts.contains(&app.tr.t("nav.startup")), "{texts:?}");
        click_text(&mut app, &ctx, &expected);
        assert_eq!(app.active_tab, ActiveTab::Startup);
    }

    #[test]
    fn the_environment_tile_counts_variables_and_path_folders() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.rule_statuses.clear();
        app.tweak_statuses.clear();
        // Both counts are 0 until the first scan.
        let before = app.tr.t_args("home.environment_status", &[&0, &0]);
        assert_eq!(status_of(&app, ActiveTab::Environment), (before, 0));

        fx.seed_env(false, "FOO", 1, "1");
        fx.seed_env(false, "BAR", 1, "2");
        fx.seed_env(false, "Path", 2, "A;B");
        app.request_env_scan();
        run_until_env_scanned(&mut app, &ctx);
        let expected = app.tr.t_args("home.environment_status", &[&3, &2]);
        assert_eq!(expected, "3 variables, 2 folders in Path");
        assert_eq!(
            status_of(&app, ActiveTab::Environment),
            (expected.clone(), 0)
        );

        app.active_tab = ActiveTab::Home;
        click_text(&mut app, &ctx, &expected);
        assert_eq!(app.active_tab, ActiveTab::Environment);
    }

    #[test]
    fn home_renders_with_no_rules_no_history() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.config.extensions.clear();
        app.history.clear();
        app.active_tab = ActiveTab::Home;
        let out = frame(&mut app, &ctx, RawInput::default());
        let texts = drawn_texts(&out);
        assert!(texts.iter().any(|t| t == "No operation yet."));
        assert!(texts.iter().any(|t| t == "0 active · 0 changed"));
        assert!(texts.iter().any(|t| t == "0 entries, 0 hidden"));
    }
}
