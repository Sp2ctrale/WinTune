//! The shell around the pages: grouped navigation, page switching with its
//! safety guards.

use super::{kit, ActiveTab, WinTuneApp, APP_VERSION};
use eframe::egui::{self, Frame, Margin};

/// Gives the borderless window the Windows 11 rounded corners and drop
/// shadow it lost with its title bar. Best effort: older Windows ignores
/// the corner preference, and a failure only leaves square corners.
pub(super) fn round_borderless_window(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_ROUND,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;

    let Ok(handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let hwnd = win32.hwnd.get() as HWND;
    let corner = DWMWCP_ROUND;
    // A 1 px frame margin is what makes DWM draw the shadow.
    let margins = MARGINS {
        cxLeftWidth: 1,
        cxRightWidth: 1,
        cyTopHeight: 1,
        cyBottomHeight: 1,
    };
    // SAFETY: `hwnd` is this process's live window; both calls only read
    // the pointed-to values for the duration of the call.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&corner) as u32,
        );
        DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

impl WinTuneApp {
    /// Our own caption strip: the window has no Windows title bar.
    pub(super) fn show_caption(&mut self, ctx: &egui::Context) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let (minimize, maximize, restore, close) = (
            self.tr.t("window.minimize"),
            self.tr.t("window.maximize"),
            self.tr.t("window.restore"),
            self.tr.t("window.close"),
        );
        let labels = kit::CaptionLabels {
            minimize: &minimize,
            maximize: &maximize,
            restore: &restore,
            close: &close,
        };
        let command = match kit::caption_bar(ctx, maximized, &labels) {
            Some(kit::CaptionAction::Minimize) => egui::ViewportCommand::Minimized(true),
            Some(kit::CaptionAction::ToggleMaximize) => {
                egui::ViewportCommand::Maximized(!maximized)
            }
            // Goes through the usual close request: a running job or an
            // unsaved config is handled as for Alt+F4.
            Some(kit::CaptionAction::Close) => egui::ViewportCommand::Close,
            Some(kit::CaptionAction::StartDrag) => egui::ViewportCommand::StartDrag,
            None => return,
        };
        ctx.send_viewport_cmd(command);
    }

    /// Switches page. Stores are never reloaded under a running job.
    pub(super) fn open_page(&mut self, page: ActiveTab) {
        if page == ActiveTab::History && !self.is_busy() {
            self.reload_backup_store();
            self.reload_tweak_store();
            self.reload_menu_store();
            self.reload_startup_store();
            self.reload_env_store();
            self.reload_mk_store();
        }
        if page == ActiveTab::Associations && self.active_tab != page {
            self.request_status_refresh();
        }
        if matches!(page, ActiveTab::Tweaks | ActiveTab::StartPrivacy) && !self.is_busy() {
            self.reload_tweak_store();
            self.refresh_tweak_statuses();
        }
        if page == ActiveTab::ContextMenu && !self.is_busy() {
            self.reload_menu_store();
            self.refresh_menu_statuses();
        }
        if page == ActiveTab::ContextMenu && self.active_tab != page {
            self.request_menu_scan();
        }
        if page == ActiveTab::Startup && !self.is_busy() {
            self.reload_startup_store();
        }
        if page == ActiveTab::Startup && self.active_tab != page {
            self.request_startup_scan();
        }
        if page == ActiveTab::Environment && !self.is_busy() {
            self.reload_env_store();
        }
        if page == ActiveTab::Environment && self.active_tab != page {
            self.request_env_scan();
        }
        if page == ActiveTab::MouseKeyboard && !self.is_busy() {
            self.reload_mk_store();
        }
        if page == ActiveTab::MouseKeyboard && self.active_tab != page {
            self.request_mk_scan();
        }
        if self.active_tab != page {
            self.focus_search = false;
            self.close_popups_next_frame = true;
        }
        self.active_tab = page;
    }

    /// Number of unapplied changes on a tweak page, shown as its badge.
    fn nav_badge(&self, page: ActiveTab) -> Option<usize> {
        matches!(page, ActiveTab::Tweaks | ActiveTab::StartPrivacy)
            .then(|| {
                self.pending_tweak_changes_in(Self::tweak_groups_of(page))
                    .len()
            })
            .filter(|n| *n > 0)
    }

    pub(super) fn show_nav(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let home = tr.t("nav.home");
        let associations = tr.t("nav.associations");
        let explorer = tr.t("nav.explorer");
        let start = tr.t("nav.start");
        let context_menu = tr.t("nav.context_menu");
        let startup = tr.t("nav.startup");
        let environment = tr.t("nav.environment");
        let mouse_keyboard = tr.t("nav.mouse_keyboard");
        let icons = tr.t("nav.icons");
        let history = tr.t("nav.history");
        let settings = tr.t("nav.settings");
        let customize = tr.t("nav.group.customize");
        let tools = tr.t("nav.group.tools");

        let this = &*self;
        let item = |key: ActiveTab, icon: kit::Icon, label| kit::NavItem {
            key,
            icon,
            label,
            badge: this.nav_badge(key),
            enabled: true,
            disabled_hint: None,
        };
        let groups = [
            kit::NavGroup {
                title: None,
                items: vec![item(ActiveTab::Home, kit::Icon::Home, &home)],
            },
            kit::NavGroup {
                title: Some(&customize),
                items: vec![
                    item(
                        ActiveTab::Associations,
                        kit::Icon::Associations,
                        &associations,
                    ),
                    item(ActiveTab::Tweaks, kit::Icon::Explorer, &explorer),
                    item(ActiveTab::StartPrivacy, kit::Icon::Start, &start),
                    item(
                        ActiveTab::ContextMenu,
                        kit::Icon::ContextMenu,
                        &context_menu,
                    ),
                    item(ActiveTab::Startup, kit::Icon::Startup, &startup),
                    item(ActiveTab::Environment, kit::Icon::Environment, &environment),
                    item(ActiveTab::MouseKeyboard, kit::Icon::Mouse, &mouse_keyboard),
                ],
            },
            kit::NavGroup {
                title: Some(&tools),
                items: vec![
                    item(ActiveTab::Gallery, kit::Icon::Icons, &icons),
                    item(ActiveTab::History, kit::Icon::History, &history),
                ],
            },
        ];
        let footer = [item(ActiveTab::Settings, kit::Icon::Settings, &settings)];
        let caption = format!("v{}", APP_VERSION);
        let brand = kit::Brand {
            logo: self.logo_texture.as_ref().map(|t| t.id()),
            name: "WinTune",
            caption: &caption,
        };

        let mut selected = self.active_tab;
        if kit::nav_sidebar(ctx, &brand, &mut selected, &groups, &footer) {
            if selected != self.active_tab {
                // The click lands before the page draws, so an open `…` menu
                // never sees it; egui keeps one popup id and would show the
                // menu again when its page comes back.
                ctx.memory_mut(|m| m.close_popup());
            }
            self.open_page(selected);
        }
    }

    /// The action bar in its busy state while a job runs. Returns whether
    /// it was drawn.
    pub(super) fn job_progress_bar(&self, ctx: &egui::Context) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let p = job.progress();
        let label = if p.total > 0 {
            self.tr.t_args("job.progress", &[&p.done, &p.total])
        } else {
            String::new()
        };
        let status = self.tr.t(job.title_key());
        kit::action_bar(
            ctx,
            &status,
            kit::ActionBarState::Busy {
                label: &label,
                fraction: p.fraction(),
            },
            |_| {},
        );
        true
    }

    /// The action bar of a page that owns it: the job's progress while one
    /// runs (no actions), else `status` with `actions`, else nothing.
    pub(super) fn page_action_bar(
        &self,
        ctx: &egui::Context,
        status: Option<&str>,
        actions: impl FnOnce(&mut egui::Ui),
    ) {
        if self.job_progress_bar(ctx) {
            return;
        }
        if let Some(status) = status {
            kit::action_bar(ctx, status, kit::ActionBarState::Idle, actions);
        }
    }

    /// Central panel with the kit's page margins.
    pub(super) fn page_panel(ctx: &egui::Context, body: impl FnOnce(&mut egui::Ui)) {
        egui::CentralPanel::default()
            .frame(Frame::central_panel(&ctx.style()).inner_margin(Margin::same(kit::space::XL)))
            .show(ctx, body);
    }
}

#[cfg(test)]
mod tests {
    use super::super::jobs::RunningJob;
    use super::super::test_support::*;
    use super::*;
    use crate::gallery::PakStats;
    use crate::tasks::BackgroundJob;
    use egui::{Event, PointerButton, RawInput};

    const NAV_LABELS: [&str; 8] = [
        "Home",
        "File associations",
        "Explorer & Taskbar",
        "Start & Privacy",
        "Context menu",
        "Icons",
        "History",
        "Settings",
    ];

    #[test]
    fn the_app_opens_on_home() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        assert_eq!(app.active_tab, ActiveTab::Home);
    }

    #[test]
    fn the_nav_has_no_rollback_entry_and_follows_the_spec_order() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let out = frame(&mut app, &ctx, RawInput::default());
        // Top to bottom on screen (the pinned footer is painted first).
        // Home's own tiles repeat some labels; the sidebar is leftmost.
        let mut shown: Vec<(f32, String)> = NAV_LABELS
            .iter()
            .filter_map(|label| {
                let at = text_center(&out, label)?;
                Some((at.y, label.to_string()))
            })
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        let shown: Vec<String> = shown.into_iter().map(|(_, t)| t).collect();
        assert_eq!(shown, NAV_LABELS);
        assert!(
            text_center(&out, "Rollback").is_none(),
            "the nav still has a Rollback entry"
        );
        // The group titles keep the spec's grouping around those pages.
        let customize = text_center(&out, &app.tr.t("nav.group.customize")).expect("Customize");
        let tools = text_center(&out, &app.tr.t("nav.group.tools")).expect("Tools");
        let y_of = |label: &str| shown_y(&out, label);
        assert!(customize.y < y_of("File associations") && y_of("Context menu") < tools.y);
        assert!(tools.y < y_of("Icons") && y_of("History") < y_of("Settings"));
    }

    fn shown_y(out: &egui::FullOutput, label: &str) -> f32 {
        text_center(out, label).expect(label).y
    }

    #[test]
    fn the_context_menu_page_opens_from_the_nav() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let out = frame(&mut app, &ctx, RawInput::default());

        // Home's own tile repeats the label; the sidebar is leftmost.
        let label = app.tr.t("nav.context_menu");
        let row = text_centers(&out, &label)
            .into_iter()
            .min_by(|a, b| a.x.total_cmp(&b.x))
            .expect("Context menu row");
        click_at_with(&mut app, &ctx, input_with, row);
        assert_eq!(app.active_tab, ActiveTab::ContextMenu);

        let texts = drawn_texts(&frame(&mut app, &ctx, RawInput::default()));
        let subtitle = app.tr.t_args("menu.subtitle", &[&"TEST\\user"]);
        assert!(texts.contains(&subtitle), "{texts:?}");
    }

    #[test]
    fn opening_the_context_menu_page_reads_the_store_unless_a_job_runs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.menu_store_error = Some(corrupt("context-menu-backups.json", "sentinel"));
        app.open_page(ActiveTab::ContextMenu);
        assert_eq!(app.active_tab, ActiveTab::ContextMenu);
        assert_eq!(app.menu_store_error, None);

        app.open_page(ActiveTab::Home);
        let release = hold_job(&mut app);
        app.menu_store_error = Some(corrupt("context-menu-backups.json", "sentinel"));
        app.open_page(ActiveTab::ContextMenu);
        assert!(app.menu_store_error.is_some(), "reloaded under a job");
        drop(release);
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn an_open_menu_closes_when_a_home_tile_opens_another_page() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.refresh_tweak_statuses();
        app.active_tab = ActiveTab::Tweaks;
        app.tweak_desired.insert("show_hidden", true);
        app.start_apply_tweaks(&ctx);
        run_until_idle(&mut app, &ctx);

        // Open the row's `…` menu on Explorer & Taskbar.
        let restore = app.tr.t("tweaks.restore_original");
        let menu = row_widget(&mut app, &ctx, "show_hidden", kit::OVERFLOW_BUTTON).expect("…");
        click_at(&mut app, &ctx, menu);
        let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
        assert!(texts.contains(&restore), "the menu did not open");

        // Home, without the nav click (which closes popups by itself).
        app.active_tab = ActiveTab::Home;
        frame(&mut app, &ctx, tall_input(vec![]));
        frame(&mut app, &ctx, tall_input(vec![]));

        // Home's tiles repeat the nav's labels; the sidebar is leftmost.
        for key in ["nav.start", "nav.explorer"] {
            let out = frame(&mut app, &ctx, tall_input(vec![]));
            let tile = text_centers(&out, &app.tr.t(key))
                .into_iter()
                .find(|c| c.x > 220.0)
                .unwrap_or_else(|| panic!("no {key} tile"));
            click_at(&mut app, &ctx, tile);
            for _ in 0..3 {
                let texts = drawn_texts(&frame(&mut app, &ctx, tall_input(vec![])));
                assert!(
                    !texts.contains(&restore),
                    "the menu is open again after the {key} tile"
                );
            }
            if key == "nav.start" {
                assert_eq!(app.active_tab, ActiveTab::StartPrivacy);
                app.active_tab = ActiveTab::Home;
            }
        }
        assert_eq!(app.active_tab, ActiveTab::Tweaks);
    }

    #[test]
    fn changing_page_during_a_job_keeps_progress_and_stores() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        // Control: idle, opening the pages reloads the stores.
        app.backup_store_error = Some(corrupt("backups.json", "sentinel"));
        app.tweak_store_error = Some(corrupt("tweaks-backups.json", "sentinel"));
        app.open_page(ActiveTab::History);
        app.open_page(ActiveTab::Tweaks);
        assert_eq!(app.backup_store_error, None);
        assert_eq!(app.tweak_store_error, None);

        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                release_rx.recv().unwrap();
                Ok(PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));
        app.backup_store_error = Some(corrupt("backups.json", "sentinel"));
        app.tweak_store_error = Some(corrupt("tweaks-backups.json", "sentinel"));

        app.open_page(ActiveTab::History);
        app.open_page(ActiveTab::Tweaks);
        assert_eq!(
            app.backup_store_error,
            Some(corrupt("backups.json", "sentinel"))
        );
        assert_eq!(
            app.tweak_store_error,
            Some(corrupt("tweaks-backups.json", "sentinel"))
        );
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn other_pages_still_show_the_shell_progress_bar() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                release_rx.recv().unwrap();
                Ok(PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));

        let busy_text = app.tr.t("job.build_pak");
        for tab in ActiveTab::ALL {
            app.active_tab = tab;
            let out = frame(&mut app, &ctx, RawInput::default());
            // Exactly one bar: the shell's, or the page's own.
            assert_eq!(
                drawn_texts(&out)
                    .iter()
                    .filter(|t| **t == busy_text)
                    .count(),
                1,
                "{tab:?} lost the progress bar, or drew two"
            );
        }
        // Without a rule selected, the page's bar still shows the job.
        app.active_tab = ActiveTab::Associations;
        app.config.extensions.clear();
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(drawn_texts(&out).contains(&busy_text));
        assert!(ActiveTab::Associations.owns_action_bar());
        assert!(!ActiveTab::Home.owns_action_bar());
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn pending_tweak_changes_show_as_a_badge() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.refresh_tweak_statuses();
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(!drawn_texts(&out).contains(&"1".to_string()));

        app.tweak_desired.insert("show_extensions", true);
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(drawn_texts(&out).contains(&"1".to_string()));
    }

    #[test]
    fn ctrl_f_does_nothing_on_pages_without_a_search_box() {
        for tab in [ActiveTab::Home, ActiveTab::History, ActiveTab::Settings] {
            assert!(super::super::polish::search_box_id(tab).is_none());
        }
    }

    #[test]
    fn a_corrupt_store_reads_in_french() {
        use crate::i18n::messages::load_error;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let config_path = fx.dir.join("config.json");
        std::fs::write(&config_path, "{ broken").unwrap();
        std::fs::write(fx.dir.join("backups.json"), "{ broken").unwrap();
        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        let (config, config_error) = WinTuneApp::load_or_init_config(&config_path);
        let mut app = fx.app_with(&ctx, config);
        app.set_language("fr");
        let tr = app.tr;

        // What startup does, in that order.
        app.adopt_config_error(config_error);
        let config_error = app.config_error.clone().expect("config error");
        assert_eq!(
            app.last_notice(),
            tr.t_args(
                "notice.config_unreadable",
                &[&load_error(tr, &config_error)]
            )
        );
        app.reload_backup_store();
        let backups = app.backup_store_error.clone().expect("backups error");
        assert_eq!(app.last_notice(), load_error(tr, &backups));
        app.reload_tweak_store();
        let tweaks = app.tweak_store_error.clone().expect("tweaks error");
        assert_eq!(app.last_notice(), load_error(tr, &tweaks));

        let mut drawn = Vec::new();
        for tab in [ActiveTab::History, ActiveTab::Settings, ActiveTab::Tweaks] {
            app.open_page(tab);
            for _ in 0..3 {
                drawn.extend(drawn_texts(&frame(&mut app, &ctx, RawInput::default())));
            }
        }

        for (banner, cause) in [
            (
                tr.t_args("originals.unreadable", &[&load_error(tr, &backups)]),
                load_error(tr, &backups),
            ),
            (
                tr.t_args("originals.unreadable", &[&load_error(tr, &tweaks)]),
                load_error(tr, &tweaks),
            ),
            (
                tr.t_args(
                    "settings.config_unreadable",
                    &[&load_error(tr, &config_error)],
                ),
                load_error(tr, &config_error),
            ),
            (
                tr.t_args("tweaks.store_unreadable", &[&load_error(tr, &tweaks)]),
                load_error(tr, &tweaks),
            ),
        ] {
            assert!(banner.contains(&cause));
            assert!(drawn.contains(&banner), "{banner:?} not drawn: {drawn:?}");
        }
        for english in ["It was left untouched", "is unreadable", "Cannot read"] {
            assert!(
                !drawn.iter().any(|t| t.contains(english)),
                "English {english:?} drawn: {drawn:?}"
            );
        }
        // Writing stays blocked, and config.json is left as it was.
        assert!(app.tweak_apply_disabled_reason().is_some());
        assert!(app.apply_disabled_reason().is_some());
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), "{ broken");
    }

    #[test]
    fn shell_pages_draw_no_english_only_text_in_french() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.set_language("fr");
        app.record_history(
            "File associations",
            "Apply",
            &crate::associations::OperationReport {
                succeeded: vec![".a".into()],
                ..Default::default()
            },
        );
        let id = crate::tweaks::catalog()
            .iter()
            .find(|t| WinTuneApp::tweak_groups_of(ActiveTab::StartPrivacy).contains(&t.group))
            .unwrap()
            .id;
        app.tweak_statuses.insert(
            id,
            crate::tweaks::TweakStatus::Custom(crate::tweaks::CustomReason::Unreadable("x".into())),
        );
        // A toast from a translated key, then the dialog open over everything.
        app.request_remove_rule(0);
        app.resolve_confirm(&ctx, false);
        app.request_remove_rule(0);
        // A job in flight: its progress bar must be French too.
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |sink| {
                sink.report(1, 3, "Compressing icons");
                release_rx.recv().unwrap();
                Ok(PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.job.as_ref().unwrap().progress().total == 0 {
            assert!(std::time::Instant::now() < deadline, "no progress reported");
            frame(&mut app, &ctx, RawInput::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let en = crate::i18n::keys_of("en").unwrap();
        let fr = crate::i18n::keys_of("fr").unwrap();
        let english_only: Vec<(&String, &String)> = en
            .iter()
            // The language picker shows each language by its own name.
            .filter(|(k, _)| k.as_str() != "language.name")
            .filter(|(k, v)| v.chars().count() >= 4 && fr.get(*k).is_some_and(|f| f != *v))
            .collect();
        assert!(!english_only.is_empty());

        for tab in [ActiveTab::Home, ActiveTab::Settings, ActiveTab::History] {
            app.active_tab = tab;
            let mut texts = Vec::new();
            for _ in 0..3 {
                let out = frame(&mut app, &ctx, RawInput::default());
                texts.extend(drawn_texts(&out));
            }
            for (key, value) in &english_only {
                assert!(
                    !texts.iter().any(|t| t == *value),
                    "{tab:?} draws English-only text {value:?} (key {key})"
                );
            }
            // The engine's English label never reaches the bar.
            assert!(
                !texts.iter().any(|t| t.contains("Compressing")),
                "{tab:?} shows the engine's English progress label"
            );
            for drawn in [
                app.tr.t("job.build_pak"),
                app.tr.t_args("job.progress", &[&1, &3]),
                app.tr.t("notice.cancelled"),
            ] {
                assert!(
                    texts.contains(&drawn),
                    "{tab:?} did not draw {drawn:?}: {texts:?}"
                );
            }
        }
        app.pending_confirm = None;
        // A busy page is disabled, so let the job finish before clicking.
        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);

        // The associations page and the icon picker, in a tall window so the
        // detail's Advanced options fit.
        use crate::associations::{DriftReason, RuleStatus};
        app.config.extensions = vec![rule(".a"), rule(".b"), rule(".x y")];
        app.rule_statuses.clear();
        app.rule_statuses.insert(
            ".a".into(),
            RuleStatus::Drifted(vec![DriftReason::ProgIdMissing {
                prog_id: "CustomAFile".into(),
            }]),
        );
        app.rule_statuses.insert(".b".into(), RuleStatus::Applied);
        app.selected_ext_index = 0;
        app.active_tab = ActiveTab::Associations;
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        // Open Advanced options by clicking its header.
        let advanced = app.tr.t("assoc.advanced");
        // The page's entry animation moves the header, so settle first.
        for _ in 0..4 {
            let mut settle = tall_input(vec![]);
            settle.time = Some(ctx.input(|i| i.time) + 1.0);
            frame(&mut app, &ctx, settle);
        }
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let pos = text_center(&out, &advanced).expect("Advanced options header not drawn");
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
        ] {
            frame(&mut app, &ctx, tall_input(events));
        }
        // Let the opening animation finish.
        let mut later = tall_input(vec![]);
        later.time = Some(ctx.input(|i| i.time) + 1.0);
        frame(&mut app, &ctx, later);
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        app.open_icon_picker(0);
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        // The page, its opened Advanced options and the picker were drawn.
        for key in ["assoc.preserve", "assoc.field.description", "picker.title"] {
            let drawn = app.tr.t(key);
            assert!(texts.contains(&drawn), "{key} ({drawn:?}) not drawn");
        }
        for (key, value) in &english_only {
            assert!(
                !texts.iter().any(|t| t == *value),
                "associations page or icon picker draws English-only text {value:?} (key {key})"
            );
        }

        // The two tweak pages: one applied tweak with a backup (and its
        // opened `…` menu), one tweak in a Custom state, a pending change
        // for the bar, then a Windows 10 frame.
        let advanced = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
        let mut pending_tweaks = Vec::new();
        for tab in [ActiveTab::Tweaks, ActiveTab::StartPrivacy] {
            let tweak = crate::tweaks::catalog()
                .iter()
                .find(|t| {
                    WinTuneApp::tweak_groups_of(tab).contains(&t.group)
                        && t.os != crate::tweaks::OsRequirement::Windows11
                        && t.id != "show_extensions"
                })
                .unwrap();
            app.active_tab = tab;
            app.tweak_desired.insert(tweak.id, true);
            app.start_apply_tweaks(&ctx);
            run_until_idle(&mut app, &ctx);
            assert!(
                app.tweak_store.has_backup("TEST\\user", tweak.id),
                "{tab:?}: no backup for {}",
                tweak.id
            );
            // A new pending change: the opposite of what was applied.
            app.tweak_desired.insert(tweak.id, false);
            pending_tweaks.push((tab, tweak.id));
        }
        let (key, _) = fx
            .sandbox
            .root
            .create_subkey(fx.hkcu(advanced))
            .expect("create sandbox key");
        key.set_value("HideFileExt", &7u32).expect("write value");
        app.refresh_tweak_statuses();
        // The Unreadable reason replaced by the refresh: draw it too.
        app.tweak_statuses.insert(
            id,
            crate::tweaks::TweakStatus::Custom(crate::tweaks::CustomReason::Unreadable("x".into())),
        );

        let mut texts = Vec::new();
        let applied_tweaks = pending_tweaks.clone();
        for (tab, tweak) in pending_tweaks {
            app.active_tab = tab;
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            // Open the row's `…` menu; it opens the frame after the click.
            let menu = row_widget(&mut app, &ctx, tweak, kit::OVERFLOW_BUTTON)
                .unwrap_or_else(|| panic!("{tab:?}: no … on row {tweak}"));
            let button = |pressed| Event::PointerButton {
                pos: menu,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            for events in [
                vec![Event::PointerMoved(menu)],
                vec![button(true)],
                vec![button(false)],
            ] {
                frame(&mut app, &ctx, tall_input(events));
            }
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            // Close the menu again before the next page.
            frame(
                &mut app,
                &ctx,
                tall_input(vec![key_press(egui::Key::Escape)]),
            );
        }
        app.env.is_windows_11 = false;
        for tab in [ActiveTab::Tweaks, ActiveTab::StartPrivacy] {
            app.active_tab = tab;
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
        }
        // The pages, the pending bar, the Custom pill, the menu and the
        // refresh labels were drawn.
        let positive = [
            app.tr.t_args("tweaks.pending_n", &[&1]),
            app.tr.t("tweaks.pill.custom"),
            app.tr.t("tweaks.restore_original"),
        ];
        for drawn in positive {
            assert!(texts.contains(&drawn), "{drawn:?} not drawn on tweak pages");
        }
        assert!(
            [
                "refresh.live",
                "refresh.new_windows",
                "refresh.explorer_restart"
            ]
            .iter()
            .any(|k| texts.contains(&app.tr.t(k))),
            "no refresh label drawn"
        );
        for (key, value) in &english_only {
            assert!(
                !texts.iter().any(|t| t == *value),
                "a tweak page draws English-only text {value:?} (key {key})"
            );
        }

        // The Icons page, with icons and with a search that matches nothing.
        app.env.is_windows_11 = true;
        app.active_tab = ActiveTab::Gallery;
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        app.gallery_search_query = "zzz".into();
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        app.gallery_search_query.clear();
        for drawn in [
            app.tr.t("icons.build"),
            app.tr.t_args("icons.no_match", &[&"zzz"]),
        ] {
            assert!(texts.contains(&drawn), "{drawn:?} not drawn on Icons");
        }
        for (key, value) in &english_only {
            assert!(
                !texts.iter().any(|t| t == *value),
                "the Icons page draws English-only text {value:?} (key {key})"
            );
        }

        // The Context menu page: one applied entry (so History gets a saved
        // original), one entry with a problem, the Windows 11 banner and each
        // selected entry's detail.
        {
            use crate::context_menu::{EntryProblem, MenuEntry, Place, ProgramChoice};
            let menu_entry = |id: &str, label: &str, program, place| MenuEntry {
                id: id.into(),
                label: label.into(),
                program,
                places: std::collections::BTreeSet::from([place]),
                icon: None,
            };
            *app.menu_entries_mut() = vec![menu_entry(
                "e1",
                "Open one",
                ProgramChoice::VsCode,
                Place::Folder,
            )];
            app.start_menu_apply(&ctx);
            run_until_idle(&mut app, &ctx);
            assert!(
                app.menu_store.backups_of("TEST\\user").is_some(),
                "no context menu snapshot: {}",
                app.last_notice()
            );
            app.menu_entries_mut().push(menu_entry(
                "e2",
                "Shell on files",
                ProgramChoice::Terminal,
                Place::Files,
            ));
            app.refresh_menu_statuses();
            app.env.is_windows_11 = true;
            app.active_tab = ActiveTab::ContextMenu;
            let mut texts = Vec::new();
            for selected in ["e1", "e2"] {
                app.selected_menu_entry = Some(selected.into());
                for _ in 0..3 {
                    texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
                }
            }
            app.selected_menu_entry = None;
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            for drawn in [
                app.tr.t("menu.win11_banner"),
                app.tr.t("menu.use_classic"),
                app.tr.t("menu.status.applied"),
                crate::i18n::messages::entry_problem(app.tr, &EntryProblem::ShellOnFiles),
                "Open one".to_string(),
                "Shell on files".to_string(),
            ] {
                assert!(
                    texts.contains(&drawn),
                    "{drawn:?} not drawn on Context menu: {texts:?}"
                );
            }
            for (key, value) in &english_only {
                assert!(
                    !texts.iter().any(|t| t == *value),
                    "the Context menu page draws English-only text {value:?} (key {key})"
                );
            }

            // Existing entries: a catalog row with its "also affects" line, a
            // third-party verb hidden through an apply (for History), another
            // to rename, and a handler blocked for every account.
            use super::super::menu_existing_state::PendingExisting;
            use crate::context_menu::{Locked, BLOCKED_KEY};
            const GIVE_ACCESS: &str = "{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}";
            const SEVEN_ZIP: &str = "{23170F69-40C1-278A-1000-000100020000}";
            // The entry with a problem would block every Apply.
            app.menu_entries_mut().retain(|e| e.id != "e2");
            app.refresh_menu_statuses();
            fx.seed_machine_clsid(GIVE_ACCESS);
            fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
            fx.seed_machine_verb("Directory", "OtherVerb", "Other thing");
            let machine = fx.machine_key();
            let (k, _) = machine
                .create_subkey(r"Software\Classes\*\shellex\ContextMenuHandlers\7-Zip")
                .unwrap();
            k.set_value("", &SEVEN_ZIP).unwrap();
            let server = format!(r"Software\Classes\CLSID\{SEVEN_ZIP}\InprocServer32");
            let (k, _) = machine.create_subkey(server).unwrap();
            k.set_value("", &r"C:\Apps\7-zip.dll").unwrap();
            let (k, _) = machine.create_subkey(BLOCKED_KEY).unwrap();
            k.set_value(SEVEN_ZIP, &"").unwrap();
            scan_now(&mut app, &ctx);
            let hide = PendingExisting {
                hidden: Some(true),
                label: None,
            };
            let scan = app.menu_scan.clone().expect("scan");
            let verb = |label: &str| {
                scan.others
                    .iter()
                    .find(|e| e.label == label)
                    .unwrap_or_else(|| panic!("{label} not scanned: {:?}", scan.others))
                    .clone()
            };
            let (hidden_verb, renamed_verb) = (verb("Open with App"), verb("Other thing"));
            let locked = scan
                .others
                .iter()
                .find(|e| e.locked == Some(Locked::MachineWide))
                .expect("a locked row")
                .clone();
            app.menu_pending
                .insert(hidden_verb.id.clone(), hide.clone());
            app.start_menu_apply(&ctx);
            assert!(app.pending_confirm.is_none(), "a verb asked first");
            run_until_idle(&mut app, &ctx);
            run_until_scanned(&mut app, &ctx);
            assert!(
                app.menu_scan
                    .as_ref()
                    .unwrap()
                    .find(&hidden_verb.id)
                    .is_some_and(|e| e.has_snapshot),
                "no snapshot of the hidden verb: {}",
                app.last_notice()
            );

            app.menu_pending.insert("win:give_access".into(), hide);
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            // The rename dialog over the page.
            app.menu_rename = Some((renamed_verb.id.clone(), "Other thing".into()));
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            app.menu_rename = None;
            // The confirmation, with the Sharing tab side effect.
            app.start_menu_apply(&ctx);
            assert!(app.pending_confirm.is_some(), "no confirmation");
            let message = app.pending_confirm.as_ref().unwrap().message.clone();
            let mut dialog = Vec::new();
            for _ in 0..3 {
                dialog.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            // Only the dialog draws its title and its message (the entry's
            // name with the side effect).
            assert!(
                dialog.contains(&app.tr.t("menu.apply_title")),
                "confirmation title not drawn: {dialog:?}"
            );
            let first = message.lines().last().unwrap().to_string();
            assert!(
                first.contains(&app.tr.t("menu.also.sharing_tab")),
                "{message:?}"
            );
            assert!(
                dialog.iter().any(|t| t.contains(&first)),
                "confirmation message not drawn: {dialog:?}"
            );
            texts.extend(dialog);
            app.resolve_confirm(&ctx, false);
            // The locked row's toggle is disabled; its tooltip gives the
            // reason in French.
            let locked_title = app.existing_name(&locked);
            frame(&mut app, &ctx, tall_input(vec![]));
            let out = frame(&mut app, &ctx, tall_input(vec![]));
            let at = text_center(&out, &locked_title).expect("locked row not drawn");
            let toggle = ctx
                .viewport_for(egui::ViewportId::ROOT, |v| {
                    v.prev_pass
                        .widgets
                        .layers()
                        .flat_map(|(_, widgets)| widgets.iter())
                        .filter(|w| w.rect.size() == egui::vec2(40.0, 20.0))
                        .filter(|w| w.rect.center().x > at.x)
                        .filter(|w| {
                            w.rect.center().y > at.y - 8.0 && w.rect.center().y - at.y < 48.0
                        })
                        .map(|w| (w.rect.center(), w.enabled))
                        .min_by(|a, b| (a.0.y - at.y).total_cmp(&(b.0.y - at.y)))
                })
                .expect("locked toggle not found");
            let before = app.menu_pending.clone();
            click_at(&mut app, &ctx, toggle.0);
            assert_eq!(app.menu_pending, before, "a locked toggle changed");
            let reason = crate::i18n::messages::locked(app.tr, &Locked::MachineWide);
            assert!(
                !english_only.iter().any(|(_, v)| **v == reason),
                "English lock reason"
            );
            texts.extend(hover_texts(&mut app, &ctx, toggle.0));
            let item = crate::context_menu::find_catalog("give_access").expect("catalog item");
            let mut positive = vec![
                app.tr.t("menu.windows"),
                app.tr.t("menu.others"),
                app.tr.t("menu.win.give_access"),
                app.tr.t(item.description_key),
                app.tr.t("menu.also.sharing_tab"),
                app.tr.t("menu.restart_hint"),
                app.tr.t("menu.pill.will_hide"),
                app.tr.t("tweaks.discard"),
                app.tr.t("tweaks.pill.unofficial"),
                app.tr.t("menu.rename_title"),
                app.tr.t("menu.rename_ok"),
                app.tr.t_args(
                    "menu.added_by",
                    &[&hidden_verb.origin.clone().unwrap_or_default()],
                ),
                "Other thing".to_string(),
                app.existing_name(&locked),
                reason.clone(),
            ];
            positive.extend(item.also_affects_key.map(|key| app.tr.t(key)));
            for drawn in positive {
                assert!(
                    texts.iter().any(|t| t.contains(&drawn)),
                    "{drawn:?} not drawn on the existing entries: {texts:?}"
                );
            }
            for (key, value) in &english_only {
                assert!(
                    !texts.iter().any(|t| t == *value),
                    "the existing entries draw English-only text {value:?} (key {key})"
                );
            }
        }

        // The Startup apps page: a normal item, one that is off, a locked
        // 32-bit all-users item, a missing program and a pending change (so
        // Discard shows).
        {
            use crate::startup::{StartupLock, StartupState, RUN32_KEY};
            let exe = std::env::current_exe().expect("current exe");
            fx.seed_run(false, "Normal app", &format!("\"{}\"", exe.display()));
            fx.seed_run(false, "Off app", &format!("\"{}\"", exe.display()));
            fx.seed_run(false, "Lost app", r#""C:\Nowhere\lost.exe""#);
            fx.seed_run_in(
                true,
                RUN32_KEY,
                "Locked app",
                &format!("\"{}\"", exe.display()),
            );
            app.request_startup_scan();
            run_until_startup_scanned(&mut app, &ctx);
            app.startup_pending.insert("run:off app".into(), false);
            app.startup_pending.insert("run:normal app".into(), false);
            app.start_startup_apply(&ctx);
            run_until_idle(&mut app, &ctx);
            run_until_startup_scanned(&mut app, &ctx);
            assert_eq!(
                app.startup_scan
                    .as_ref()
                    .and_then(|s| s.find("run:off app"))
                    .map(|i| i.state),
                Some(StartupState::Off),
                "{}",
                app.last_notice()
            );
            // One off app, one wish to turn it on, one to turn another off.
            app.startup_pending.insert("run:off app".into(), true);
            app.startup_pending.insert("run:lost app".into(), false);
            app.active_tab = ActiveTab::Startup;
            let mut texts = Vec::new();
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
            }
            let locked_reason =
                crate::i18n::messages::startup_lock(app.tr, &StartupLock::AllUsersOnly);
            let out = frame(&mut app, &ctx, tall_input(vec![]));
            let at = text_center(&out, "Locked app").expect("locked row not drawn");
            let toggle = ctx
                .viewport_for(egui::ViewportId::ROOT, |v| {
                    v.prev_pass
                        .widgets
                        .layers()
                        .flat_map(|(_, widgets)| widgets.iter())
                        .filter(|w| w.rect.size() == egui::vec2(40.0, 20.0))
                        .filter(|w| w.rect.center().x > at.x)
                        .filter(|w| {
                            w.rect.center().y > at.y - 8.0 && w.rect.center().y - at.y < 48.0
                        })
                        .map(|w| w.rect.center())
                        .min_by(|a, b| (a.y - at.y).total_cmp(&(b.y - at.y)))
                })
                .expect("locked toggle not found");
            texts.extend(hover_texts(&mut app, &ctx, toggle));
            let mut positive = vec![
                app.tr.t("nav.startup"),
                app.tr.t("startup.subtitle"),
                app.tr.t("startup.next_sign_in"),
                app.tr.t("tweaks.pill.unofficial"),
                app.tr.t("startup.pill.all_users"),
                app.tr.t("startup.pill.not_found"),
                app.tr.t("startup.pill.will_turn_on"),
                app.tr.t("startup.pill.will_turn_off"),
                app.tr.t_args("startup.pending", &[&2]),
                app.tr.t("tweaks.discard"),
                crate::i18n::messages::startup_state(app.tr, StartupState::Off),
                "Normal app".to_string(),
                "Locked app".to_string(),
                locked_reason.clone(),
            ];
            positive.retain(|t| !t.is_empty());
            for drawn in positive {
                assert!(
                    texts.iter().any(|t| t.contains(&drawn)),
                    "{drawn:?} not drawn on Startup apps: {texts:?}"
                );
            }
            assert!(
                !english_only.iter().any(|(_, v)| **v == locked_reason),
                "English lock reason"
            );
            for (key, value) in &english_only {
                assert!(
                    !texts.iter().any(|t| t == *value),
                    "the Startup apps page draws English-only text {value:?} (key {key})"
                );
            }
            app.startup_pending.clear();
        }

        // The Environment variables page: a Path with a missing folder, a
        // duplicate and a %VAR% entry; an expandable and a non-text
        // variable; every kind of pending edit (so Discard and each pill
        // show); the system section open; both dialogs; the Apply
        // confirmation; then the states without data.
        {
            use super::super::environment_state::{EnvDialog, VarKey};
            use crate::environment::{EnvProblem, EnvScan, VarEdit};
            let d = fx.dir.display().to_string();
            let tools = format!(r"{d}\tools");
            std::fs::create_dir_all(&tools).unwrap();
            fx.seed_env(false, "TOOLS", 1, &tools);
            fx.seed_env(
                false,
                "Path",
                2,
                &format!(r"{tools};%TOOLS%;C:\Nowhere\x;{d}\moved"),
            );
            fx.seed_env(false, "EXPAND", 2, r"%TOOLS%\x");
            fx.seed_env(false, "RENAMEME", 1, "1");
            fx.seed_env(false, "DELETEME", 1, "1");
            fx.seed_env(false, "CHANGEME", 1, "1");
            fx.seed_env_raw(
                "DWORDVAR",
                winreg::RegValue {
                    bytes: vec![5, 0, 0, 0],
                    vtype: winreg::enums::RegType::REG_DWORD,
                },
            );
            fx.seed_env(true, "JAVA_HOME", 1, r"C:\Java");
            fx.seed_env(true, "Path", 2, r"C:\Windows");
            app.request_env_scan();
            run_until_env_scanned(&mut app, &ctx);
            {
                let edit = app.env_path_edit_mut().expect("a text Path");
                edit.remove(2);
                edit.move_up(2);
                edit.add(format!(r"{d}\new"));
            }
            let var_edit = |original: &str, wanted: Option<(&str, &str)>| VarEdit {
                original: Some(original.into()),
                wanted: wanted.map(|(n, v)| (n.into(), v.into())),
            };
            app.env_pending.vars.insert(
                VarKey::Existing("renameme".into()),
                var_edit("RENAMEME", Some(("RENAMED", "1"))),
            );
            app.env_pending.vars.insert(
                VarKey::Existing("deleteme".into()),
                var_edit("DELETEME", None),
            );
            app.env_pending.vars.insert(
                VarKey::Existing("changeme".into()),
                var_edit("CHANGEME", Some(("CHANGEME", "2"))),
            );
            app.env_pending.vars.insert(
                VarKey::New("added".into()),
                VarEdit {
                    original: None,
                    wanted: Some(("ADDED".into(), "1".into())),
                },
            );
            app.active_tab = ActiveTab::Environment;
            let mut texts = Vec::new();
            let draw = |app: &mut WinTuneApp, texts: &mut Vec<String>| {
                for _ in 0..3 {
                    texts.extend(drawn_texts(&frame(app, &ctx, tall_input(vec![]))));
                }
            };
            draw(&mut app, &mut texts);
            // Open the system section.
            let out = frame(&mut app, &ctx, tall_input(vec![]));
            let title = text_center(&out, &app.tr.t("env.system.title")).expect("system title");
            click_at(&mut app, &ctx, title);
            draw(&mut app, &mut texts);
            // The dialogs, with a problem line each.
            app.env_dialog = Some(EnvDialog::Variable {
                key: None,
                name: "A=B".into(),
                value: "v".into(),
            });
            draw(&mut app, &mut texts);
            app.env_dialog = Some(EnvDialog::Entry {
                index: Some(0),
                text: "a;b".into(),
            });
            draw(&mut app, &mut texts);
            app.env_dialog = None;
            // The Apply confirmation.
            app.start_env_apply(&ctx);
            assert!(app.pending_confirm.is_some(), "{}", app.last_notice());
            draw(&mut app, &mut texts);
            app.resolve_confirm(&ctx, false);
            // No data: no Path and no variable, a search nothing matches,
            // an unreadable user and system key, the first scan not done.
            app.discard_env_changes();
            app.env_scan = Some(EnvScan::default());
            draw(&mut app, &mut texts);
            app.env_search = "nothing like it".into();
            draw(&mut app, &mut texts);
            app.env_search.clear();
            app.env_scan = Some(EnvScan {
                user_error: Some("boom".into()),
                system_error: Some("denied".into()),
                ..Default::default()
            });
            draw(&mut app, &mut texts);
            app.env_scan = None;
            draw(&mut app, &mut texts);
            let mut positive = vec![
                app.tr.t("nav.environment"),
                app.tr.t("env.subtitle"),
                app.tr.t("env.note"),
                app.tr.t("env.search"),
                app.tr.t("env.path.title"),
                app.tr.t("env.path.clean"),
                app.tr.t("env.path.add_hint"),
                app.tr.t("env.path.add"),
                app.tr.t("env.browse"),
                app.tr.t("env.pill.not_found"),
                app.tr.t("env.pill.duplicate"),
                app.tr.t("env.pill.will_add"),
                app.tr.t("env.pill.moved"),
                app.tr.t("env.pill.will_remove"),
                app.tr.t("env.pill.uses_refs"),
                app.tr.t("env.pill.read_only"),
                app.tr.t("env.pill.will_change"),
                app.tr.t_args("env.pill.will_rename", &[&"RENAMED"]),
                app.tr.t("env.pill.will_delete"),
                app.tr.t("env.vars.title"),
                app.tr.t("env.vars.add"),
                app.tr.t("env.vars.empty"),
                app.tr.t("env.system.title"),
                app.tr.t("env.system.path"),
                app.tr.t("env.var_title_add"),
                app.tr.t("env.entry_title"),
                app.tr.t("env.name"),
                app.tr.t("env.value"),
                app.tr.t("env.dialog_ok"),
                app.tr.t_args("env.pending", &[&5]),
                app.tr.t("env.up_to_date"),
                app.tr.t("env.path.empty"),
                app.tr.t("env.no_match"),
                app.tr.t("env.scanning"),
                app.tr.t_args("env.unreadable", &[&"boom"]),
                app.tr.t_args("env.system.unreadable", &[&"denied"]),
                app.tr.t("tweaks.discard"),
                app.tr.t("env.apply_title"),
                crate::i18n::messages::env_problem(app.tr, &EnvProblem::NameHasEquals),
                crate::i18n::messages::env_problem(app.tr, &EnvProblem::EntryHasSemicolon),
            ];
            positive.retain(|t| !t.is_empty());
            for drawn in positive {
                assert!(
                    texts.iter().any(|t| t.contains(&drawn)),
                    "{drawn:?} not drawn on Environment variables: {texts:?}"
                );
            }
            for (key, value) in &english_only {
                assert!(
                    !texts.iter().any(|t| t == *value),
                    "the Environment variables page draws English-only text {value:?} (key {key})"
                );
            }
            app.env_scan_needed = false;
            app.active_tab = ActiveTab::Home;
        }

        // The Mouse & keyboard page: a pending speed, precision toggle,
        // trails on, wheel one screen and swap (so Discard and every pill
        // show); Filter Keys on; an unreadable setting; the Pointer speed `…`
        // menu open (with a saved original); the double-click target lit;
        // the swap confirmation; then a search with no match, no scan yet,
        // and a scan whose account is wrong, then unknown.
        {
            use super::super::mouse_keyboard_tab::{click_test_step, ClickTest};
            use crate::i18n::messages::{mk_problem, mk_section, mk_title};
            use crate::mouse_keyboard::SettingId::*;
            use crate::mouse_keyboard::SettingValue::*;
            use crate::mouse_keyboard::{AccountCheck, MkScan, Section, Wanted, WHEEL_PAGESCROLL};
            fx.fake_input().set_filter_keys(true);
            fx.fake_input().fail_get(HoverTime);
            app.request_mk_scan();
            run_until_mk_scanned(&mut app, &ctx);
            app.mk_store
                .record_first("TEST\\user", "pointer_speed", "x", Number(10));
            for (id, wanted) in [
                (PointerSpeed, Wanted::Value(Number(14))),
                (PointerPrecision, Wanted::Toggle(false)),
                (PointerTrails, Wanted::Value(Number(7))),
                (WheelLines, Wanted::Value(Number(WHEEL_PAGESCROLL))),
                (SwapButtons, Wanted::Value(Flag(true))),
            ] {
                app.set_mk_wanted(id, wanted);
            }
            assert_eq!(app.mk_pending_count(), 5);
            app.active_tab = ActiveTab::MouseKeyboard;
            let mut texts = Vec::new();
            let draw = |app: &mut WinTuneApp, texts: &mut Vec<String>| {
                for _ in 0..3 {
                    texts.extend(drawn_texts(&frame(app, &ctx, tall_input(vec![]))));
                }
            };
            draw(&mut app, &mut texts);
            // The Pointer speed `…` menu.
            let speed = mk_title(app.tr, PointerSpeed);
            let menu = widget_right_of(&mut app, &ctx, &speed, kit::OVERFLOW_BUTTON)
                .expect("no … on the Pointer speed row");
            click_at(&mut app, &ctx, menu);
            draw(&mut app, &mut texts);
            frame(
                &mut app,
                &ctx,
                tall_input(vec![key_press(egui::Key::Escape)]),
            );
            // The double-click target, lit.
            let mut lit = ClickTest::default();
            click_test_step(&mut lit, 0.0, 500);
            click_test_step(&mut lit, 0.1, 500);
            app.mk_click_test = lit;
            draw(&mut app, &mut texts);
            // The swap confirmation.
            app.start_mk_apply(&ctx);
            assert!(app.pending_confirm.is_some(), "{}", app.last_notice());
            draw(&mut app, &mut texts);
            app.resolve_confirm(&ctx, false);
            // A search nothing matches, then no scan yet.
            app.mk_search = "nothing like it".into();
            draw(&mut app, &mut texts);
            app.mk_search.clear();
            let scan = app.mk_scan.take().expect("scanned");
            draw(&mut app, &mut texts);
            // Another account, then an account check that failed.
            let wrong = AccountCheck::Different {
                process: "PC\\Admin".into(),
                session: "PC\\Ana".into(),
            };
            let unknown = AccountCheck::Unknown("no session".into());
            for account in [wrong.clone(), unknown.clone()] {
                app.mk_scan = Some(MkScan {
                    account,
                    ..scan.clone()
                });
                draw(&mut app, &mut texts);
            }
            let mut positive = vec![
                app.tr.t("nav.mouse_keyboard"),
                app.tr.t("mk.subtitle"),
                app.tr.t("mk.note"),
                app.tr.t("mk.search"),
                app.tr.t("mk.scanning"),
                app.tr.t("mk.no_match"),
                app.tr.t("mk.pill.not_applied"),
                app.tr.t("mk.windows_default"),
                app.tr.t("tweaks.restore_original"),
                app.tr.t("mk.wheel.lines"),
                app.tr.t("mk.value.one_screen"),
                app.tr.t("mk.click_test.title"),
                app.tr.t("mk.click_test.desc"),
                app.tr.t("mk.click_test.ok"),
                app.tr.t("mk.key_test.title"),
                app.tr.t("mk.key_test.note"),
                app.tr.t("mk.key_test.hint"),
                app.tr.t("mk.filter_keys"),
                app.tr.t_args("mk.pending", &[&5]),
                app.tr.t("mk.apply_title"),
                app.tr.t("tweaks.discard"),
                mk_title(app.tr, HoverTime),
                mk_problem(app.tr, &wrong.problem().unwrap()),
                mk_problem(app.tr, &unknown.problem().unwrap()),
            ];
            positive.extend(Section::ALL.map(|s| mk_section(app.tr, s)));
            positive.retain(|t| !t.is_empty());
            for drawn in positive {
                assert!(
                    texts.iter().any(|t| t.contains(&drawn)),
                    "{drawn:?} not drawn on Mouse & keyboard: {texts:?}"
                );
            }
            for (key, value) in &english_only {
                assert!(
                    !texts.iter().any(|t| t == *value),
                    "the Mouse & keyboard page draws English-only text {value:?} (key {key})"
                );
            }
            app.mk_scan = Some(scan);
            app.discard_mk_changes();
            app.mk_scan_needed = false;
            app.active_tab = ActiveTab::Home;
        }

        // The History page: saved originals in every group, one opened
        // operation with an error and a `.reg` path, then a broken store.
        app.config.extensions = vec![rule(".a"), rule(".b")];
        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);
        assert!(app.backup_store.find(".a").is_some(), "no rule snapshot");
        let mut entry = crate::history::HistoryEntry::from_report(
            "File associations",
            "Apply",
            &crate::associations::OperationReport {
                succeeded: vec![".a".into()],
                errors: vec![".b: failed".into()],
                ..Default::default()
            },
        );
        entry.reg_backup = Some("C:\\backups\\x.reg".into());
        let entry_time = entry.time.clone();
        let mut menu_entry = crate::history::HistoryEntry::from_report(
            "Context menu",
            "Apply",
            &crate::associations::OperationReport {
                succeeded: vec!["Open one".into()],
                ..Default::default()
            },
        );
        menu_entry.time = "2000-01-01 00:00:00".into();
        let mut startup_entry = crate::history::HistoryEntry::from_report(
            "Startup apps",
            "Apply",
            &crate::associations::OperationReport {
                succeeded: vec!["Off app".into()],
                ..Default::default()
            },
        );
        startup_entry.time = "2000-01-02 00:00:00".into();
        let mut env_entry = crate::history::HistoryEntry::from_report(
            "Environment variables",
            "Apply",
            &crate::associations::OperationReport {
                succeeded: vec!["FRVAR".into()],
                ..Default::default()
            },
        );
        env_entry.time = "2000-01-03 00:00:00".into();
        let mut mk_entry = crate::history::HistoryEntry::from_report(
            "Mouse & keyboard",
            "Apply",
            &crate::associations::OperationReport {
                succeeded: vec!["Pointer speed".into()],
                ..Default::default()
            },
        );
        mk_entry.time = "2000-01-04 00:00:00".into();
        app.history = vec![entry, menu_entry, startup_entry, env_entry, mk_entry];
        app.open_page(ActiveTab::History);
        // A changed mouse setting's original (`open_page` reloaded the
        // store from disk).
        app.mk_store.record_first(
            "TEST\\user",
            "pointer_speed",
            "2026-01-02 03:04:05",
            crate::mouse_keyboard::SettingValue::Number(10),
        );
        // Same for an environment variable's original.
        app.env_store
            .snapshots
            .users
            .entry("TEST\\user".to_string())
            .or_default()
            .insert(
                crate::environment::env_slot("FRVAR").id(),
                crate::backups::Backup {
                    timestamp: "2026-01-02 03:04:05".into(),
                    snapshot: crate::backups::Snapshot::Value(Default::default()),
                },
            );
        // `open_page` reloaded the stores from disk, so put the unknown
        // tweak back in memory.
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
        for _ in 0..4 {
            let mut settle = tall_input(vec![]);
            settle.time = Some(ctx.input(|i| i.time) + 1.0);
            frame(&mut app, &ctx, settle);
        }
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let title = drawn_texts(&out)
            .into_iter()
            .find(|t| t.starts_with(&entry_time))
            .expect("operation row not drawn");
        let pos = text_center(&out, &title).expect("operation title");
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
        ] {
            frame(&mut app, &ctx, tall_input(events));
        }
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        let all_groups = texts.clone();
        // One store unreadable: the banner replaces its groups.
        std::fs::write(fx.dir.join("tweaks-backups.json"), "{ broken").unwrap();
        app.reload_tweak_store();
        let error = app.tweak_store_error.clone().expect("store error");
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        for drawn in [
            app.tr.t("history.operations"),
            app.tr.t("originals.title"),
            app.tr.t_args("originals.restore_all", &[&1]),
            app.tr.t_args(
                "originals.unreadable",
                &[&crate::i18n::messages::load_error(app.tr, &error)],
            ),
            app.tr.t("history.copy_reg"),
            ".b: failed".to_string(),
            "C:\\backups\\x.reg".to_string(),
        ] {
            assert!(texts.contains(&drawn), "{drawn:?} not drawn on History");
        }
        // The module names are also the filter chips, so prove each group
        // by one of its own rows.
        let mut rows = vec![
            ".a".to_string(),
            "gone_tweak".to_string(),
            app.tr.t("originals.other"),
            "Open one".to_string(),
            // A hidden third-party verb, by name.
            "Open with App".to_string(),
            // A turned-off startup app.
            "Off app".to_string(),
            // A changed environment variable.
            "FRVAR".to_string(),
            // A changed mouse setting, by its French title.
            crate::i18n::messages::mk_title(app.tr, crate::mouse_keyboard::SettingId::PointerSpeed),
        ];
        rows.extend(
            applied_tweaks
                .iter()
                .map(|(_, id)| crate::i18n::messages::tweak_title(app.tr, id)),
        );
        for drawn in rows {
            assert!(
                all_groups.contains(&drawn),
                "{drawn:?} (saved original) not drawn"
            );
        }
        for (key, value) in &english_only {
            assert!(
                !texts.iter().any(|t| t == *value),
                "the History page draws English-only text {value:?} (key {key})"
            );
        }
        // The Startup apps operation, by its French module name.
        let module = app.tr.t("history.module.startup");
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("2000-01-02") && t.contains(&module)),
            "no Startup apps operation: {texts:?}"
        );
        // The Environment variables operation, by its French module name.
        let module = app.tr.t("history.module.environment");
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("2000-01-03") && t.contains(&module)),
            "no Environment variables operation: {texts:?}"
        );

        // The Mouse & keyboard operation, by its French module name.
        let module = app.tr.t("history.module.mouse_keyboard");
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("2000-01-04") && t.contains(&module)),
            "no Mouse & keyboard operation: {texts:?}"
        );
        let original = crate::i18n::messages::mk_value(
            app.tr,
            Some(crate::mouse_keyboard::SettingId::PointerSpeed),
            &crate::mouse_keyboard::SettingValue::Number(10),
        );
        let desc = app
            .tr
            .t_args("mk.original_desc", &[&original, &"2026-01-02 03:04:05"]);
        assert!(texts.contains(&desc), "{desc:?} not drawn on History");

        // Home: the Startup apps tile, with the counts of the last scan.
        app.active_tab = ActiveTab::Home;
        let mut home = Vec::new();
        for _ in 0..3 {
            home.extend(drawn_texts(&frame(&mut app, &ctx, tall_input(vec![]))));
        }
        let items = app.startup_scan.as_ref().expect("startup scan").items.len();
        let off = app.startup_scan.as_ref().map_or(0, |s| {
            s.items
                .iter()
                .filter(|i| i.state != crate::startup::StartupState::On)
                .count()
        });
        let (vars, folders) = app
            .env_scan
            .as_ref()
            .map_or((0, 0), |s| (s.user.len(), s.user_path_entries().len()));
        for drawn in [
            app.tr.t("nav.startup"),
            app.tr.t_args("home.startup_status", &[&items, &off]),
            app.tr.t("nav.environment"),
            app.tr.t_args("home.environment_status", &[&vars, &folders]),
            app.tr.t("nav.mouse_keyboard"),
            app.tr.t_args("home.mouse_keyboard_status_one", &[&1]),
        ] {
            assert!(
                home.contains(&drawn),
                "{drawn:?} not drawn on Home: {home:?}"
            );
        }
        for (key, value) in &english_only {
            assert!(
                !home.iter().any(|t| t == *value),
                "Home draws English-only text {value:?} (key {key})"
            );
        }
    }

    fn key_press(key: egui::Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }
    }

    fn input_with(events: Vec<Event>) -> RawInput {
        RawInput {
            events,
            ..Default::default()
        }
    }

    fn app_with_dialog_on_settings(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app_with_rules(ctx, &[".a", ".b"]);
        app.open_page(ActiveTab::Settings);
        app.request_remove_rule(0);
        frame(&mut app, ctx, RawInput::default());
        frame(&mut app, ctx, RawInput::default());
        app
    }

    fn rule_names(app: &WinTuneApp) -> Vec<String> {
        app.config
            .extensions
            .iter()
            .map(|r| r.extension.clone())
            .collect()
    }

    #[test]
    fn arrow_keys_cannot_move_focus_from_the_dialog_to_the_nav() {
        use egui::Key;
        for same_frame in [true, false] {
            let fx = TempIcons::new(&["foo"]);
            let ctx = egui::Context::default();
            let mut app = app_with_dialog_on_settings(&fx, &ctx);

            frame(&mut app, &ctx, input_with(vec![key_press(Key::ArrowLeft)]));
            frame(&mut app, &ctx, input_with(vec![key_press(Key::ArrowLeft)]));
            if same_frame {
                frame(
                    &mut app,
                    &ctx,
                    input_with(vec![key_press(Key::ArrowUp), key_press(Key::Enter)]),
                );
            } else {
                frame(&mut app, &ctx, input_with(vec![key_press(Key::ArrowUp)]));
                frame(&mut app, &ctx, input_with(vec![key_press(Key::Enter)]));
            }
            frame(&mut app, &ctx, RawInput::default());

            assert_eq!(
                app.active_tab,
                ActiveTab::Settings,
                "same_frame={same_frame}"
            );
            // Enter acted on Cancel: the dialog closed and the rule is kept.
            assert!(app.pending_confirm.is_none(), "same_frame={same_frame}");
            assert_eq!(rule_names(&app), [".a", ".b"], "same_frame={same_frame}");
        }
    }

    #[test]
    fn escape_cancels_the_dialog_and_keeps_the_rule() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = app_with_dialog_on_settings(&fx, &ctx);

        frame(
            &mut app,
            &ctx,
            input_with(vec![key_press(egui::Key::Escape)]),
        );

        assert!(app.pending_confirm.is_none());
        assert_eq!(rule_names(&app), [".a", ".b"]);
        assert_eq!(app.last_notice(), app.tr.t("notice.cancelled"));
    }

    #[test]
    fn the_caption_close_button_sends_a_close_request() {
        use egui::{Modifiers, Pos2, Rect, ViewportCommand};
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 760.0));
        let at = egui::pos2(1180.0 - 20.0, kit::CAPTION_HEIGHT / 2.0);
        let input = |events| RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        };
        frame(&mut app, &ctx, input(vec![Event::PointerMoved(at)]));
        frame(&mut app, &ctx, input(vec![button(true)]));
        let out = frame(&mut app, &ctx, input(vec![button(false)]));
        assert!(
            commands(&out).contains(&ViewportCommand::Close),
            "commands: {:?}",
            commands(&out)
        );
    }
}
