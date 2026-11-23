//! The shell around the pages: grouped navigation, page switching with its
//! safety guards, and the Settings placeholder (Task 6 replaces it).

use super::{kit, ActiveTab, WinTuneApp, APP_VERSION};
use eframe::egui::{self, Frame, Margin};

impl WinTuneApp {
    /// Switches page. Stores are never reloaded under a running job.
    pub(super) fn open_page(&mut self, page: ActiveTab) {
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
            self.open_page(selected);
        }
    }

    /// Central panel with the kit's page margins.
    pub(super) fn page_panel(ctx: &egui::Context, body: impl FnOnce(&mut egui::Ui)) {
        egui::CentralPanel::default()
            .frame(Frame::central_panel(&ctx.style()).inner_margin(Margin::same(kit::space::XL)))
            .show(ctx, body);
    }

    pub(super) fn show_settings_placeholder(&mut self, ctx: &egui::Context) {
        let text = self.tr.t("settings.placeholder");
        Self::page_panel(ctx, |ui| {
            kit::empty_state(ui, kit::Icon::Settings, &text, None);
        });
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

        let busy_text = app.tr.t("job.build_pak");
        for tab in ActiveTab::ALL {
            app.active_tab = tab;
            let out = frame(&mut app, &ctx, RawInput::default());
            assert!(
                drawn_texts(&out).contains(&busy_text),
                "{tab:?} lost the progress bar"
            );
        }
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
}
