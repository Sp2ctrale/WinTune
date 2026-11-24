//! Second review: the two tweak tabs must not act on each other's pending
//! changes, and smaller state leaks (focus flag, history module, hidden
//! config-save errors).

use super::test_support::*;
use super::{ActiveTab, WinTuneApp};
use eframe::egui::{self, Event, Key, Modifiers, RawInput};

const ADVANCED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";

fn dword(fx: &TempIcons, name: &str) -> Option<u32> {
    fx.sandbox
        .root
        .open_subkey(fx.hkcu(ADVANCED))
        .ok()?
        .get_value(name)
        .ok()
}

fn app(fx: &TempIcons, ctx: &egui::Context, tab: ActiveTab) -> WinTuneApp {
    let mut app = fx.app_with_rules(ctx, &[".foo"]);
    app.refresh_tweak_statuses();
    app.active_tab = tab;
    app
}

#[test]
fn apply_only_writes_the_pending_changes_of_the_current_tab() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = app(&fx, &ctx, ActiveTab::Tweaks);
    app.tweak_desired.insert("show_protected_os_files", true); // Explorer tab
    app.active_tab = ActiveTab::StartPrivacy;
    app.tweak_desired.insert("no_app_launch_tracking", true);

    assert_eq!(
        app.pending_tweak_changes_in(WinTuneApp::tweak_groups_of(ActiveTab::StartPrivacy)),
        vec![("no_app_launch_tracking".to_string(), true)]
    );
    app.start_apply_tweaks(&ctx);
    run_until_idle(&mut app, &ctx);

    assert_eq!(dword(&fx, "Start_TrackProgs"), Some(0));
    assert_eq!(
        dword(&fx, "ShowSuperHidden"),
        None,
        "other tab's tick was written"
    );
    assert_eq!(
        app.tweak_desired.get("show_protected_os_files"),
        Some(&true)
    );
}

#[test]
fn discard_only_clears_the_current_tab() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = app(&fx, &ctx, ActiveTab::StartPrivacy);
    app.tweak_desired.insert("show_protected_os_files", true);
    app.tweak_desired.insert("no_app_launch_tracking", true);

    app.discard_tweak_changes(ActiveTab::StartPrivacy);

    assert!(!app.tweak_desired.contains_key("no_app_launch_tracking"));
    assert!(app.tweak_desired.contains_key("show_protected_os_files"));
}

#[test]
fn a_restore_keeps_unrelated_pending_ticks() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = app(&fx, &ctx, ActiveTab::Tweaks);
    app.tweak_desired.insert("show_hidden", true);
    app.start_apply_tweaks(&ctx);
    run_until_idle(&mut app, &ctx);
    app.tweak_desired.insert("start_more_pins", true);

    app.request_restore_tweaks(vec!["show_hidden".into()]);
    app.resolve_confirm(&ctx, true);
    run_until_idle(&mut app, &ctx);

    assert_eq!(dword(&fx, "Hidden"), None, "restored");
    assert_eq!(app.tweak_desired.get("start_more_pins"), Some(&true));
}

#[test]
fn start_and_privacy_operations_are_logged_under_their_own_module() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = app(&fx, &ctx, ActiveTab::StartPrivacy);
    app.tweak_desired.insert("no_recent_documents", true);

    app.start_apply_tweaks(&ctx);
    run_until_idle(&mut app, &ctx);

    assert_eq!(app.history[0].module, "Start & Privacy");
}

#[test]
fn ctrl_f_on_a_tab_without_search_does_not_leak_focus_to_another_tab() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = app(&fx, &ctx, ActiveTab::History);
    frame(&mut app, &ctx, RawInput::default());
    let ctrl_f = RawInput {
        events: vec![Event::Key {
            key: Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
        modifiers: Modifiers::COMMAND,
        ..Default::default()
    };
    frame(&mut app, &ctx, ctrl_f);

    app.active_tab = ActiveTab::Associations;
    frame(&mut app, &ctx, RawInput::default());
    frame(&mut app, &ctx, RawInput::default());

    let id = super::polish::search_box_id(ActiveTab::Associations).unwrap();
    assert!(
        !ctx.memory(|m| m.has_focus(id)),
        "focus leaked to another tab"
    );
}

#[test]
fn a_refused_config_save_is_reported_after_an_apply() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo"]);
    app.config_error = Some(corrupt("config.json", "bad json"));
    let rules = app.config.extensions.clone();

    app.start_apply(&ctx, rules);
    run_until_idle(&mut app, &ctx);

    let shown = app.last_notice();
    assert!(shown.contains("Configuration not saved"), "{}", shown);
}
