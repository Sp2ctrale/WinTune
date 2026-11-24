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
fn a_saved_config_reloads_and_leaves_no_temp_file() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo", ".bar"]);

    app.config.code_executable = r"C:\Other\editor.exe".to_string();
    app.flush_config();

    let (config, error) = WinTuneApp::load_or_init_config(&fx.dir.join("config.json"));
    assert!(error.is_none(), "{:?}", error);
    assert_eq!(config.extensions.len(), 2);
    assert_eq!(config.code_executable, r"C:\Other\editor.exe");
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
        app.last_notice().contains("dialog"),
        "{}",
        app.last_notice()
    );
    assert!(!fx.key_exists(&fx.classes(".foo")));
}

#[test]
fn a_registry_job_that_panics_is_logged_in_the_history() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo"]);
    app.job = Some(RunningJob::Tweaks {
        job: BackgroundJob::spawn(
            "Applying tweaks",
            || {},
            |_| -> super::jobs::TweakOutcome { panic!("boom") },
        ),
        module: "Explorer & Taskbar",
        restore: false,
    });

    run_until_idle(&mut app, &ctx);

    let entry = app.history.first().expect("panicked job not logged");
    assert_eq!(entry.module, "Explorer & Taskbar");
    assert!(
        entry.errors.iter().any(|e| e.contains("boom")),
        "{:?}",
        entry.errors
    );
}

#[test]
fn a_tweak_restore_job_is_titled_and_logged_as_restore() {
    let fx = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut app = fx.app_with_rules(&ctx, &[".foo"]);

    app.start_restore_tweaks(&ctx, vec!["show_hidden".to_string()]);

    let job = app.job.as_ref().expect("the restore job did not start");
    assert_eq!(job.title_key(), "job.restore_tweaks");
    assert_eq!(job.name_key(), "job.name.restore_tweaks");
    assert_eq!(job.history_label(), Some(("Explorer & Taskbar", "Restore")));
    run_until_idle(&mut app, &ctx);
}

#[test]
fn history_module_labels_are_the_stored_ones() {
    use super::ActiveTab;
    assert_eq!(
        ActiveTab::Associations.history_module(),
        Some("File associations")
    );
    assert_eq!(
        ActiveTab::Tweaks.history_module(),
        Some("Explorer & Taskbar")
    );
    assert_eq!(
        ActiveTab::StartPrivacy.history_module(),
        Some("Start & Privacy")
    );
    assert_eq!(ActiveTab::Home.history_module(), None);
}
