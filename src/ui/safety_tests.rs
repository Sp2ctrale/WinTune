//! UI safety fixes found in review: unreadable config.json, "Custom" tweaks,
//! keyboard behind the modal dialog, failed jobs in the history.

use super::jobs::RunningJob;
use super::test_support::*;
use super::WinTuneApp;
use crate::models::AppConfig;
use crate::tasks::BackgroundJob;
use crate::tweaks::TweakStatus;
use eframe::egui;

const ADVANCED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";

#[test]
fn an_unreadable_config_loads_defaults_and_says_so() {
    let fx = TempIcons::new(&["foo"]);
    let path = fx.dir.join("config.json");
    std::fs::write(&path, "{ broken").unwrap();

    let (config, error) = WinTuneApp::load_or_init_config(&path);

    assert!(error.is_some(), "a broken config.json must be reported");
    assert_eq!(
        config.extensions.len(),
        AppConfig::default().extensions.len()
    );
}

#[test]
fn an_unreadable_config_is_never_overwritten() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let path = fx.dir.join("config.json");
    std::fs::write(&path, "{ broken").unwrap();
    let (config, error) = WinTuneApp::load_or_init_config(&path);
    let mut app = fx.app_with(&ctx, config);
    app.config_error = error;

    app.save_config();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
    assert!(
        app.status_message.contains("not saved"),
        "{}",
        app.status_message
    );
}

#[test]
fn a_saved_config_reloads_and_leaves_no_temp_file() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo", ".bar"]);

    app.save_config();

    let (config, error) = WinTuneApp::load_or_init_config(&fx.dir.join("config.json"));
    assert!(error.is_none(), "{:?}", error);
    assert_eq!(config.extensions.len(), 2);
    assert!(!fx.dir.join("config.json.tmp").exists());
}

#[test]
fn a_tweak_in_a_custom_state_can_be_turned_off() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let (k, _) = fx.sandbox.root.create_subkey(fx.hkcu(ADVANCED)).unwrap();
    k.set_value("Hidden", &0u32).unwrap(); // neither on (1) nor off (2)
    let mut app = fx.app_with_rules(&ctx, &[".foo"]);
    app.refresh_tweak_statuses();
    assert!(matches!(
        app.tweak_statuses.get("show_hidden"),
        Some(TweakStatus::Custom(_))
    ));

    app.tweak_desired.insert("show_hidden", false);

    assert_eq!(
        app.pending_tweak_changes(),
        vec![("show_hidden".to_string(), false)]
    );
    app.start_apply_tweaks(&ctx);
    run_until_idle(&mut app, &ctx);
    let hidden: u32 = fx
        .sandbox
        .root
        .open_subkey(fx.hkcu(ADVANCED))
        .unwrap()
        .get_value("Hidden")
        .unwrap();
    assert_eq!(hidden, 2, "Windows default written");
}

#[test]
fn no_job_starts_while_a_confirmation_is_open() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo"]);
    app.request_remove_rule(0);
    assert!(app.pending_confirm.is_some());

    // E.g. Tab to "Apply" behind the dialog, then Space.
    let rules = app.config.extensions.clone();
    app.start_apply(&ctx, rules);
    app.start_build_pak(&ctx);

    assert!(!app.is_busy(), "a job started behind the open dialog");
    assert!(
        app.status_message.contains("dialog"),
        "{}",
        app.status_message
    );
    assert!(!fx.key_exists(&fx.classes(".foo")));
}

#[test]
fn a_registry_job_that_panics_is_logged_in_the_history() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo"]);
    app.job = Some(RunningJob::Tweaks(
        BackgroundJob::spawn(
            "Applying tweaks",
            || {},
            |_| -> super::jobs::TweakOutcome { panic!("boom") },
        ),
        "Explorer & Taskbar",
    ));

    run_until_idle(&mut app, &ctx);

    let entry = app.history.first().expect("panicked job not logged");
    assert_eq!(entry.module, "Explorer & Taskbar");
    assert!(
        entry.errors.iter().any(|e| e.contains("boom")),
        "{:?}",
        entry.errors
    );
}
