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
        if page == ActiveTab::ContextMenu {
            // Shown in the navigation but not built yet: never openable.
            return;
        }
        if page == ActiveTab::Rollback && !self.is_busy() {
            self.reload_backup_store();
        }
        if page == ActiveTab::Associations && self.active_tab != page {
            self.request_status_refresh();
        }
        if matches!(page, ActiveTab::Tweaks | ActiveTab::StartPrivacy) && !self.is_busy() {
            self.reload_tweak_store();
            self.refresh_tweak_statuses();
        }
        if self.active_tab != page {
            self.focus_search = false;
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
        let icons = tr.t("nav.icons");
        let history = tr.t("nav.history");
        let rollback = tr.t("nav.rollback");
        let settings = tr.t("nav.settings");
        let customize = tr.t("nav.group.customize");
        let tools = tr.t("nav.group.tools");
        let coming_soon = tr.t("nav.coming_soon");

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
                    kit::NavItem {
                        enabled: false,
                        disabled_hint: Some(&coming_soon),
                        ..item(
                            ActiveTab::ContextMenu,
                            kit::Icon::ContextMenu,
                            &context_menu,
                        )
                    },
                ],
            },
            kit::NavGroup {
                title: Some(&tools),
                items: vec![
                    item(ActiveTab::Gallery, kit::Icon::Icons, &icons),
                    item(ActiveTab::History, kit::Icon::History, &history),
                    item(ActiveTab::Rollback, kit::Icon::Restore, &rollback),
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

    const NAV_LABELS: [&str; 9] = [
        "Home",
        "File associations",
        "Explorer & Taskbar",
        "Start & Privacy",
        "Context menu",
        "Icons",
        "History",
        "Rollback",
        "Settings",
    ];

    fn click_at(app: &mut WinTuneApp, ctx: &egui::Context, pos: egui::Pos2) {
        let ev = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![Event::PointerMoved(pos)],
            vec![ev(true)],
            vec![ev(false)],
        ] {
            frame(
                app,
                ctx,
                RawInput {
                    events,
                    ..Default::default()
                },
            );
        }
    }

    #[test]
    fn the_app_opens_on_home() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        assert_eq!(app.active_tab, ActiveTab::Home);
    }

    #[test]
    fn the_nav_lists_pages_in_spec_order() {
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
    }

    #[test]
    fn context_menu_cannot_be_opened() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let out = frame(&mut app, &ctx, RawInput::default());

        // Control: a live row does open.
        let history = text_center(&out, "History").expect("History row");
        click_at(&mut app, &ctx, history);
        assert_eq!(app.active_tab, ActiveTab::History);

        app.active_tab = ActiveTab::Home;
        let out = frame(&mut app, &ctx, RawInput::default());
        let disabled = text_center(&out, "Context menu").expect("Context menu row");
        click_at(&mut app, &ctx, disabled);
        assert_eq!(app.active_tab, ActiveTab::Home);
    }

    #[test]
    fn open_page_never_opens_the_context_menu() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::History);
        app.open_page(ActiveTab::ContextMenu);
        assert_eq!(app.active_tab, ActiveTab::History);
    }

    #[test]
    fn changing_page_during_a_job_keeps_progress_and_stores() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);

        // Control: idle, opening the pages reloads the stores.
        app.backup_store_error = Some("sentinel".into());
        app.tweak_store_error = Some("sentinel".into());
        app.open_page(ActiveTab::Rollback);
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
        app.backup_store_error = Some("sentinel".into());
        app.tweak_store_error = Some("sentinel".into());

        app.open_page(ActiveTab::Rollback);
        app.open_page(ActiveTab::Tweaks);
        assert_eq!(app.backup_store_error.as_deref(), Some("sentinel"));
        assert_eq!(app.tweak_store_error.as_deref(), Some("sentinel"));
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
        let tall = |events: Vec<Event>| RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1180.0, 2400.0),
            )),
            events,
            ..Default::default()
        };
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall(vec![]))));
        }
        // Open Advanced options by clicking its header.
        let advanced = app.tr.t("assoc.advanced");
        // The page's entry animation moves the header, so settle first.
        for _ in 0..4 {
            let mut settle = tall(vec![]);
            settle.time = Some(ctx.input(|i| i.time) + 1.0);
            frame(&mut app, &ctx, settle);
        }
        let out = frame(&mut app, &ctx, tall(vec![]));
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
            frame(&mut app, &ctx, tall(events));
        }
        // Let the opening animation finish.
        let mut later = tall(vec![]);
        later.time = Some(ctx.input(|i| i.time) + 1.0);
        frame(&mut app, &ctx, later);
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall(vec![]))));
        }
        app.open_icon_picker(0);
        for _ in 0..3 {
            texts.extend(drawn_texts(&frame(&mut app, &ctx, tall(vec![]))));
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

        // Center of the widget of exactly `size` on the row of tweak `tweak`.
        let widget_on_row = |app: &mut WinTuneApp, tweak: &str, size: egui::Vec2| {
            let out = frame(app, &ctx, tall(vec![]));
            let description = crate::i18n::messages::tweak_description(app.tr, tweak);
            let at = text_center(&out, &description)?;
            ctx.viewport_for(egui::ViewportId::ROOT, |v| {
                v.prev_pass
                    .widgets
                    .layers()
                    .flat_map(|(_, widgets)| widgets.iter())
                    .filter(|w| w.rect.size() == size && w.rect.center().x > at.x)
                    .map(|w| w.rect.center())
                    .filter(|c| (c.y - at.y).abs() < 24.0)
                    .min_by(|a, b| (a.y - at.y).abs().total_cmp(&(b.y - at.y).abs()))
            })
        };
        let mut texts = Vec::new();
        for (tab, tweak) in pending_tweaks {
            app.active_tab = tab;
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall(vec![]))));
            }
            // Open the row's `…` menu; it opens the frame after the click.
            let menu = widget_on_row(&mut app, tweak, kit::OVERFLOW_BUTTON)
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
                frame(&mut app, &ctx, tall(events));
            }
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall(vec![]))));
            }
            // Close the menu again before the next page.
            frame(&mut app, &ctx, tall(vec![key_press(egui::Key::Escape)]));
        }
        app.env.is_windows_11 = false;
        for tab in [ActiveTab::Tweaks, ActiveTab::StartPrivacy] {
            app.active_tab = tab;
            for _ in 0..3 {
                texts.extend(drawn_texts(&frame(&mut app, &ctx, tall(vec![]))));
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
