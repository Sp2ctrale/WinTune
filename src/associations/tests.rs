//! Sandbox tests of the association engine (`HKCU\Software\WinTuneTest`).

use super::*;
use crate::models::{BackupStore, DoubleClickMode, ExtensionConfig, RegistryKeySnapshot};
use crate::regsnap::snapshot_key;
use crate::regsnap::test_support::Sandbox;
use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

const HIVE_A: &str = "S-1-5-21-1000";
const HIVE_B: &str = "S-1-5-21-2000";
const USER_CHOICE_REL: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.foo";

struct Env {
    sb: Sandbox,
    dir: PathBuf,
}

impl Env {
    fn new() -> Self {
        let sb = Sandbox::new();
        let dir = std::env::temp_dir().join(format!(
            "wintune-assoc-{}",
            sb.path.rsplit('\\').next().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self { sb, dir }
    }

    fn target(&self, hives: &[&str]) -> AssocTarget {
        AssocTarget {
            machine_root: RegKey::predef(HKEY_CURRENT_USER),
            machine_root_name: "HKEY_CURRENT_USER".into(),
            machine_classes: self.sb.key("Machine\\Classes"),
            users_root: RegKey::predef(HKEY_CURRENT_USER),
            users_root_name: "HKEY_CURRENT_USER".into(),
            user_hives: hives
                .iter()
                .map(|h| self.sb.key(&format!("Users\\{}", h)))
                .collect(),
        }
    }

    fn classes(&self, rel: &str) -> String {
        self.sb.key(&format!("Machine\\Classes\\{}", rel))
    }

    fn user(&self, hive: &str, rel: &str) -> String {
        self.sb.key(&format!("Users\\{}\\{}", hive, rel))
    }

    fn snap(&self, path: &str) -> RegistryKeySnapshot {
        snapshot_key(&self.sb.root, path).unwrap()
    }

    fn exists(&self, path: &str) -> bool {
        self.sb.root.open_subkey(path).is_ok()
    }

    fn default_of(&self, path: &str) -> String {
        self.sb
            .root
            .open_subkey(path)
            .unwrap()
            .get_value("")
            .unwrap()
    }

    fn opts<'a>(
        &self,
        resolve_icon: &'a dyn Fn(&ExtensionConfig) -> Result<String, String>,
    ) -> ApplyOptions<'a> {
        ApplyOptions {
            editor_command: "\"C:\\Editor\\editor.exe\" \"%1\"".into(),
            editor_label: "VS Code".into(),
            wsl_available: false,
            resolve_icon,
            backup_store_path: self.dir.join("backups.json"),
            reg_backup_dir: self.dir.join("reg-backups"),
            progress: &no_progress,
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn icon_ok(_: &ExtensionConfig) -> Result<String, String> {
    Ok("C:\\ProgramData\\ico\\foo.ico".into())
}

fn no_progress(_: usize, _: usize, _: &str) {}

fn with_ext(ext: &str) -> ExtensionConfig {
    let mut c = foo_cfg(&format!("Custom{}File", ext.trim_start_matches('.')));
    c.extension = ext.into();
    c
}

fn assert_progress_ends_complete(calls: &[(usize, usize, String)], total: usize) {
    assert!(!calls.is_empty(), "no progress reported");
    assert!(calls.iter().all(|c| c.1 == total), "{:?}", calls);
    assert!(calls.windows(2).all(|w| w[0].0 <= w[1].0), "{:?}", calls);
    assert_eq!(calls.last().unwrap().0, total, "{:?}", calls);
}

#[test]
fn apply_reports_progress_through_snapshot_and_write_phases() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let calls = std::cell::RefCell::new(Vec::new());
    let record = |d: usize, t: usize, l: &str| calls.borrow_mut().push((d, t, l.to_string()));
    let mut opts = env.opts(&icon_ok);
    opts.progress = &record;

    apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[with_ext(".foo"), with_ext(".bar")],
        &opts,
    );

    let calls = calls.into_inner();
    assert_progress_ends_complete(&calls, 4);
    assert!(calls.iter().any(|c| c.2.contains(".bar")), "{:?}", calls);
}

#[test]
fn rollback_reports_progress_per_extension() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[with_ext(".foo"), with_ext(".bar")],
        &env.opts(&icon_ok),
    );
    let calls = std::cell::RefCell::new(Vec::new());
    let record = |d: usize, t: usize, l: &str| calls.borrow_mut().push((d, t, l.to_string()));

    rollback_extensions(
        &target,
        &mut store,
        &[".foo".into(), ".bar".into()],
        &record,
    );

    assert_progress_ends_complete(&calls.into_inner(), 2);
}

fn foo_cfg(prog_id: &str) -> ExtensionConfig {
    ExtensionConfig {
        enabled: true,
        extension: ".foo".into(),
        prog_id: prog_id.into(),
        description: "Foo File".into(),
        icon_name: "foo".into(),
        reset_existing: true,
        reset_user_choice: true,
        mode: DoubleClickMode::OpenInEditor,
        launcher_command: None,
        elevated_command: None,
        perceived_type: "text".into(),
        content_type: "text/plain".into(),
        preserve_existing_properties: true,
        extra_verbs: Vec::new(),
    }
}

/// A realistic "before WinTune" state for `.foo`.
fn seed_original_state(env: &Env) {
    let root = &env.sb.root;
    let (ext, _) = root.create_subkey(env.classes(".foo")).unwrap();
    ext.set_value("", &"Old.File").unwrap();
    ext.set_value("Content Type", &"text/x-foo").unwrap();
    let (owp, _) = ext.create_subkey("OpenWithProgids").unwrap();
    owp.set_raw_value(
        "Other.App",
        &winreg::RegValue {
            bytes: Vec::new(),
            vtype: REG_NONE,
        },
    )
    .unwrap();
    let (old, _) = root
        .create_subkey(env.classes("Old.File\\shell\\open\\command"))
        .unwrap();
    old.set_value("", &"old.exe \"%1\"").unwrap();
    let (uc, _) = root
        .create_subkey(env.user(HIVE_A, &format!("{}\\UserChoice", USER_CHOICE_REL)))
        .unwrap();
    uc.set_value("ProgId", &"Old.File").unwrap();
    let (ucls, _) = root
        .create_subkey(env.user(HIVE_A, "Software\\Classes\\.foo"))
        .unwrap();
    ucls.set_value("", &"User.Foo").unwrap();
}

#[test]
fn apply_registers_progid_and_points_extension_to_it() {
    let env = Env::new();
    let mut store = BackupStore::default();

    let report = apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.succeeded, vec![".foo".to_string()]);
    assert_eq!(env.default_of(&env.classes(".foo")), "CustomFooFile");
    assert_eq!(env.default_of(&env.classes("CustomFooFile")), "Foo File");
    assert_eq!(
        env.default_of(&env.classes("CustomFooFile\\DefaultIcon")),
        "C:\\ProgramData\\ico\\foo.ico"
    );
    let cmd = env.snap(&env.classes("CustomFooFile\\shell\\open\\command"));
    assert_eq!(cmd.values.get("").map(|v| v.0), Some(REG_EXPAND_SZ as u32));
    assert_eq!(
        cmd.default_value.as_deref(),
        Some("\"C:\\Editor\\editor.exe\" \"%1\"")
    );
}

#[test]
fn apply_cleans_user_overrides_and_keeps_previous_progid_in_open_with() {
    let env = Env::new();
    seed_original_state(&env);
    let mut store = BackupStore::default();

    apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    assert!(!env.exists(&env.user(HIVE_A, USER_CHOICE_REL)));
    assert!(!env.exists(&env.user(HIVE_A, "Software\\Classes\\.foo")));
    let owp = env.snap(&env.classes(".foo\\OpenWithProgids"));
    assert!(owp.values.contains_key("Old.File"));
    assert!(owp.values.contains_key("Other.App"));
}

#[test]
fn rollback_restores_machine_and_user_keys_exactly() {
    let env = Env::new();
    seed_original_state(&env);
    let before_ext = env.snap(&env.classes(".foo"));
    let before_old = env.snap(&env.classes("Old.File"));
    let before_uc = env.snap(&env.user(HIVE_A, USER_CHOICE_REL));
    let before_ucls = env.snap(&env.user(HIVE_A, "Software\\Classes\\.foo"));
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(env.snap(&env.classes(".foo")), before_ext);
    assert_eq!(env.snap(&env.classes("Old.File")), before_old);
    assert_eq!(env.snap(&env.user(HIVE_A, USER_CHOICE_REL)), before_uc);
    assert_eq!(
        env.snap(&env.user(HIVE_A, "Software\\Classes\\.foo")),
        before_ucls
    );
    assert!(!env.exists(&env.classes("CustomFooFile")));
    assert!(!store.backups.contains_key(".foo"));
}

#[test]
fn renaming_the_progid_removes_the_previously_written_one() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("RenamedFooFile")],
        &env.opts(&icon_ok),
    );

    assert!(!env.exists(&env.classes("CustomFooFile")));
    assert_eq!(env.default_of(&env.classes(".foo")), "RenamedFooFile");
}

#[test]
fn rollback_after_progid_rename_leaves_no_orphan_progid() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("RenamedFooFile")],
        &env.opts(&icon_ok),
    );

    rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

    assert!(!env.exists(&env.classes("CustomFooFile")));
    assert!(!env.exists(&env.classes("RenamedFooFile")));
    assert!(!env.exists(&env.classes(".foo")));
}

#[test]
fn reusing_an_existing_progid_restores_it_on_rollback() {
    let env = Env::new();
    seed_original_state(&env);
    let before_old = env.snap(&env.classes("Old.File"));
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("Old.File")],
        &env.opts(&icon_ok),
    );
    assert_eq!(env.default_of(&env.classes("Old.File")), "Foo File");

    rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

    assert_eq!(env.snap(&env.classes("Old.File")), before_old);
}

#[test]
fn user_hive_seen_only_on_a_later_apply_is_snapshotted_before_cleanup() {
    let env = Env::new();
    let mut store = BackupStore::default();
    apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    let (uc, _) = env
        .sb
        .root
        .create_subkey(env.user(HIVE_B, &format!("{}\\UserChoice", USER_CHOICE_REL)))
        .unwrap();
    uc.set_value("ProgId", &"B.File").unwrap();
    let before_b = env.snap(&env.user(HIVE_B, USER_CHOICE_REL));
    let both = env.target(&[HIVE_A, HIVE_B]);

    apply_extensions(
        &both,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    assert!(!env.exists(&env.user(HIVE_B, USER_CHOICE_REL)));
    rollback_extensions(&both, &mut store, &[".foo".to_string()], &no_progress);

    assert_eq!(env.snap(&env.user(HIVE_B, USER_CHOICE_REL)), before_b);
}

#[test]
fn missing_icon_skips_the_extension_and_reports_it() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let no_icon = |_: &ExtensionConfig| -> Result<String, String> { Err("icon not found".into()) };

    let report = apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&no_icon),
    );

    assert!(report.succeeded.is_empty());
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains(".foo") && e.contains("icon not found")));
    assert!(!env.exists(&env.classes(".foo")));
    assert!(!env.exists(&env.classes("CustomFooFile")));
}

#[test]
fn invalid_extension_or_progid_is_rejected_without_writing() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let mut bad_ext = foo_cfg("CustomFooFile");
    bad_ext.extension = "foo\\..\\bar".into();
    let mut bad_prog = foo_cfg("");
    bad_prog.extension = ".bar".into();

    let report = apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[bad_ext, bad_prog],
        &env.opts(&icon_ok),
    );

    assert_eq!(report.errors.len(), 2, "{:?}", report.errors);
    assert!(!env.exists(&env.classes(".bar")));
    assert!(store.backups.is_empty());
}

fn status_ctx() -> StatusContext {
    StatusContext {
        editor_command: "\"C:\\Editor\\editor.exe\" \"%1\"".into(),
        wsl_available: false,
        icon_dir: "C:\\ProgramData\\ico".into(),
    }
}

/// Applies `.foo` (→ CustomFooFile) in the sandbox and returns its status context.
fn applied_foo(env: &Env, store: &mut BackupStore) -> AssocTarget {
    let target = env.target(&[HIVE_A]);
    let report = apply_extensions(
        &target,
        store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    target
}

fn reasons(status: RuleStatus) -> Vec<String> {
    match status {
        RuleStatus::Drifted(r) => r,
        other => panic!("expected Drifted, got {:?}", other),
    }
}

#[test]
fn a_rule_never_applied_is_not_applied() {
    let env = Env::new();
    seed_original_state(&env);

    let status = rule_status(
        &env.target(&[HIVE_A]),
        &foo_cfg("CustomFooFile"),
        None,
        &status_ctx(),
    );

    assert_eq!(status, RuleStatus::NotApplied);
}

#[test]
fn a_freshly_applied_rule_is_applied() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let target = applied_foo(&env, &mut store);

    let status = rule_status(
        &target,
        &foo_cfg("CustomFooFile"),
        store.backups.get(".foo"),
        &status_ctx(),
    );

    assert_eq!(status, RuleStatus::Applied);
}

#[test]
fn an_extension_repointed_by_another_app_has_drifted() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let target = applied_foo(&env, &mut store);
    let (k, _) = env.sb.root.create_subkey(env.classes(".foo")).unwrap();
    k.set_value("", &"Other.App").unwrap();

    let r = reasons(rule_status(
        &target,
        &foo_cfg("CustomFooFile"),
        store.backups.get(".foo"),
        &status_ctx(),
    ));

    assert!(r.iter().any(|m| m.contains("Other.App")), "{:?}", r);
}

#[test]
fn an_explorer_user_choice_override_has_drifted() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let target = applied_foo(&env, &mut store);
    let (uc, _) = env
        .sb
        .root
        .create_subkey(env.user(HIVE_A, &format!("{}\\UserChoice", USER_CHOICE_REL)))
        .unwrap();
    uc.set_value("ProgId", &"Other.App").unwrap();

    let r = reasons(rule_status(
        &target,
        &foo_cfg("CustomFooFile"),
        store.backups.get(".foo"),
        &status_ctx(),
    ));

    assert!(
        r.iter()
            .any(|m| m.contains("UserChoice") && m.contains("Other.App")),
        "{:?}",
        r
    );
}

#[test]
fn a_deleted_progid_has_drifted() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let target = applied_foo(&env, &mut store);
    env.sb
        .root
        .delete_subkey_all(env.classes("CustomFooFile"))
        .unwrap();

    let r = reasons(rule_status(
        &target,
        &foo_cfg("CustomFooFile"),
        store.backups.get(".foo"),
        &status_ctx(),
    ));

    assert!(r.iter().any(|m| m.contains("CustomFooFile")), "{:?}", r);
}

#[test]
fn a_rule_edited_since_apply_has_drifted() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let target = applied_foo(&env, &mut store);
    let mut edited = foo_cfg("CustomFooFile");
    edited.description = "Renamed".into();
    let mut ctx = status_ctx();
    ctx.editor_command = "\"C:\\Other\\other.exe\" \"%1\"".into();

    let r = reasons(rule_status(
        &target,
        &edited,
        store.backups.get(".foo"),
        &ctx,
    ));

    assert!(r.iter().any(|m| m.contains("description")), "{:?}", r);
    assert!(r.iter().any(|m| m.contains("command")), "{:?}", r);
}

#[test]
fn computing_the_status_writes_nothing() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let target = applied_foo(&env, &mut store);
    let before = env.snap(&env.sb.path);

    let _ = rule_status(
        &target,
        &foo_cfg("CustomFooFile"),
        store.backups.get(".foo"),
        &status_ctx(),
    );
    let _ = rule_status(&target, &with_ext(".bar"), None, &status_ctx());

    assert_eq!(env.snap(&env.sb.path), before);
}

#[test]
fn find_rule_problems_reports_invalid_values_with_their_index() {
    let mut bad = foo_cfg("CustomFooFile");
    bad.extension = ".a b".into();
    let rules = [with_ext(".ok"), bad];

    let problems = find_rule_problems(&rules);

    assert_eq!(problems.len(), 1, "{:?}", problems);
    assert_eq!(problems[0].0, 1);
    assert!(problems[0].1.contains(".a b"), "{:?}", problems);
}

#[test]
fn find_rule_problems_flags_every_duplicate_case_insensitively() {
    let rules = [with_ext(".foo"), with_ext(".bar"), with_ext(".FOO")];

    let problems = find_rule_problems(&rules);

    let indexes: Vec<usize> = problems.iter().map(|p| p.0).collect();
    assert_eq!(indexes, vec![0, 2], "{:?}", problems);
    assert!(problems.iter().all(|p| p.1.contains("more than once")));
}

#[test]
fn find_rule_problems_is_empty_for_the_default_rules() {
    assert!(find_rule_problems(&crate::models::default_extensions()).is_empty());
}

// ---- Review fixes -------------------------------------------------

#[test]
fn system_progids_and_executable_extensions_are_rejected() {
    for pid in [
        "CLSID",
        "clsid",
        "Wow6432Node",
        "Interface",
        "Directory",
        "Folder",
        "exefile",
        "LNKFILE",
    ] {
        assert!(
            validate_extension(&foo_cfg(pid)).is_err(),
            "ProgID {:?} accepted",
            pid
        );
    }
    for ext in [".exe", ".LNK", ".scr", ".dll"] {
        let mut c = foo_cfg("CustomFooFile");
        c.extension = ext.into();
        assert!(
            validate_extension(&c).is_err(),
            "extension {:?} accepted",
            ext
        );
    }
}

#[test]
fn an_existing_progid_of_another_app_is_overwritten_not_deleted() {
    let env = Env::new();
    seed_original_state(&env);
    let (print, _) = env
        .sb
        .root
        .create_subkey(env.classes("Old.File\\shell\\print\\command"))
        .unwrap();
    print.set_value("", &"print.exe \"%1\"").unwrap();
    let before = env.snap(&env.classes("Old.File"));
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();

    let report = apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("Old.File")],
        &env.opts(&icon_ok),
    );

    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(
        env.exists(&env.classes("Old.File\\shell\\print\\command")),
        "foreign subkey deleted"
    );
    assert_eq!(env.default_of(&env.classes("Old.File")), "Foo File");
    rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);
    assert_eq!(env.snap(&env.classes("Old.File")), before);
}

#[test]
fn a_progid_created_by_wintune_is_rewritten_from_scratch() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    let mut cfg = foo_cfg("CustomFooFile");
    cfg.extra_verbs.push(crate::models::CustomContextCommand {
        verb_key: "old_verb".into(),
        menu_label: "Old".into(),
        command: "old.exe".into(),
    });
    apply_extensions(&target, &mut store, &[cfg], &env.opts(&icon_ok));

    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    assert!(!env.exists(&env.classes("CustomFooFile\\shell\\old_verb")));
}

#[test]
fn rules_sharing_a_progid_are_flagged() {
    let mut yml = with_ext(".yml");
    yml.prog_id = "CustomYamlFile".into();
    let mut yaml = with_ext(".yaml");
    yaml.prog_id = "customyamlfile".into();

    let problems = find_rule_problems(&[yml, with_ext(".ok"), yaml]);

    let indexes: Vec<usize> = problems.iter().map(|p| p.0).collect();
    assert_eq!(indexes, vec![0, 2], "{:?}", problems);
    assert!(
        problems.iter().all(|p| p.1.contains("ProgID")),
        "{:?}",
        problems
    );
}

#[test]
fn changing_only_the_case_of_an_extension_reuses_its_snapshot() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    let mut upper = foo_cfg("CustomFooFile");
    upper.extension = ".FOO".into();

    apply_extensions(&target, &mut store, &[upper], &env.opts(&icon_ok));

    assert_eq!(store.backups.len(), 1, "{:?}", store.backups.keys());
    let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(!env.exists(&env.classes(".foo")));
    assert!(!env.exists(&env.classes("CustomFooFile")));
}

#[test]
fn a_progid_already_recorded_by_another_extension_keeps_its_true_original() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    let mut foo = foo_cfg("CustomSharedFile");
    foo.extension = ".foo".into();
    apply_extensions(&target, &mut store, &[foo], &env.opts(&icon_ok));
    let mut bar = foo_cfg("CustomSharedFile");
    bar.extension = ".bar".into();

    apply_extensions(&target, &mut store, &[bar], &env.opts(&icon_ok));

    let original = &store.backups[".bar"].prog_id_snapshots["CustomSharedFile"];
    assert!(
        !original.exists,
        "recorded WinTune's own key as the original"
    );
}

#[test]
fn rollback_keeps_the_snapshot_of_a_signed_out_user_until_they_are_back() {
    let env = Env::new();
    let (uc, _) = env
        .sb
        .root
        .create_subkey(env.user(HIVE_B, &format!("{}\\UserChoice", USER_CHOICE_REL)))
        .unwrap();
    uc.set_value("ProgId", &"B.File").unwrap();
    let before_b = env.snap(&env.user(HIVE_B, USER_CHOICE_REL));
    let target = env.target(&[HIVE_B]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    // B signs out: the hive is no longer loaded.
    env.sb
        .root
        .delete_subkey_all(env.sb.key(&format!("Users\\{}", HIVE_B)))
        .unwrap();

    let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

    assert!(!report.warnings.is_empty());
    assert!(!env.exists(&env.classes(".foo")), "machine part restored");
    let kept = store
        .backups
        .get(".foo")
        .expect("snapshot of the signed-out user dropped");
    assert_eq!(kept.user_overrides.len(), 1);

    // B signs back in: the next rollback finishes the job.
    env.sb
        .root
        .create_subkey(env.sb.key(&format!("Users\\{}", HIVE_B)))
        .unwrap();
    let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(env.snap(&env.user(HIVE_B, USER_CHOICE_REL)), before_b);
    assert!(store.backups.is_empty());
}

#[test]
fn validate_extension_accepts_normal_and_rejects_dangerous_values() {
    assert!(validate_extension(&foo_cfg("CustomFooFile")).is_ok());
    for ext in ["", ".", "foo", ".a\\b", ".a b"] {
        let mut c = foo_cfg("CustomFooFile");
        c.extension = ext.into();
        assert!(
            validate_extension(&c).is_err(),
            "extension {:?} accepted",
            ext
        );
    }
    for pid in ["", "  ", "A\\B", ".foo"] {
        assert!(
            validate_extension(&foo_cfg(pid)).is_err(),
            "progid {:?} accepted",
            pid
        );
    }
}

#[test]
fn disabled_extensions_are_ignored() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let mut cfg = foo_cfg("CustomFooFile");
    cfg.enabled = false;

    let report = apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[cfg],
        &env.opts(&icon_ok),
    );

    assert!(report.succeeded.is_empty() && report.errors.is_empty());
    assert!(!env.exists(&env.classes(".foo")));
}

#[test]
fn script_types_get_an_edit_verb_named_after_the_configured_editor() {
    let env = Env::new();
    let mut store = BackupStore::default();
    let mut cfg = foo_cfg("CustomFooFile");
    cfg.mode = DoubleClickMode::ExecuteScript;
    cfg.launcher_command = Some("\"%1\" %*".into());
    let mut opts = env.opts(&icon_ok);
    opts.editor_label = "Notepad++".into();

    apply_extensions(&env.target(&[HIVE_A]), &mut store, &[cfg], &opts);

    assert_eq!(
        env.default_of(&env.classes("CustomFooFile\\shell\\open\\command")),
        "\"%1\" %*"
    );
    assert_eq!(
        env.default_of(&env.classes("CustomFooFile\\shell\\edit")),
        "Edit with Notepad++"
    );
}

#[test]
fn apply_writes_a_reg_safety_net_of_the_previous_state() {
    let env = Env::new();
    seed_original_state(&env);
    let mut store = BackupStore::default();

    let report = apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    let file = report.reg_backup_file.expect("no .reg written");
    let bytes = std::fs::read(&file).unwrap();
    let words: Vec<u16> = bytes[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    let text = String::from_utf16(&words).unwrap();
    assert!(
        text.contains(&format!("[-HKEY_CURRENT_USER\\{}]", env.classes(".foo"))),
        "{text}"
    );
    assert!(text.contains("@=\"Old.File\""), "{text}");
    assert!(
        text.contains(&format!(
            "[-HKEY_CURRENT_USER\\{}]",
            env.classes("CustomFooFile")
        )),
        "{text}"
    );
}

#[test]
fn apply_persists_backups_with_timestamps() {
    let env = Env::new();
    let mut store = BackupStore::default();

    apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );

    let on_disk = load_backup_store(&env.dir.join("backups.json")).unwrap();
    let b = on_disk.backups.get(".foo").expect("backup not persisted");
    assert_eq!(b.custom_prog_id, "CustomFooFile");
    assert!(b.timestamp.starts_with("20"), "timestamp: {}", b.timestamp);
    assert!(b.last_applied.is_some());
}

#[test]
fn failed_rollback_keeps_the_backup() {
    let env = Env::new();
    let target = env.target(&[HIVE_A]);
    let mut store = BackupStore::default();
    apply_extensions(
        &target,
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    // Corrupt the snapshot so restoring it must fail (key name > 255 chars).
    let bad = RegistryKeySnapshot {
        exists: true,
        ..Default::default()
    };
    let b = store.backups.get_mut(".foo").unwrap();
    b.hklm_extension_key.exists = true;
    b.hklm_extension_key.subkeys.insert("x".repeat(300), bad);

    let report = rollback_extensions(&target, &mut store, &[".foo".to_string()], &no_progress);

    assert!(!report.errors.is_empty());
    assert!(store.backups.contains_key(".foo"));
}

#[test]
fn legacy_backup_without_progid_snapshots_still_removes_the_custom_progid() {
    let env = Env::new();
    env.sb
        .root
        .create_subkey(env.classes("CustomFooFile\\DefaultIcon"))
        .unwrap();
    let legacy = r#"{"backups":{".foo":{
        "extension":".foo","custom_prog_id":"CustomFooFile",
        "icon_installed_path":"C:\\ico\\foo.ico","timestamp":"Initial Snapshot",
        "hklm_extension_key":{"exists":false,"default_value":null,"values":{},"subkeys":{}},
        "hklm_previous_prog_id":null,"user_overrides":[]}}}"#;
    let mut store: BackupStore = serde_json::from_str(legacy).unwrap();

    let report = rollback_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[".foo".to_string()],
        &no_progress,
    );

    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(!env.exists(&env.classes("CustomFooFile")));
}

#[test]
fn missing_backup_file_loads_as_empty_store() {
    let env = Env::new();
    assert!(load_backup_store(&env.dir.join("none.json"))
        .unwrap()
        .backups
        .is_empty());
}

#[test]
fn corrupt_backup_file_is_an_error_not_an_empty_store() {
    // Returning an empty store would make the next save erase every snapshot.
    let env = Env::new();
    let path = env.dir.join("backups.json");
    std::fs::write(&path, "{ not json").unwrap();
    assert!(load_backup_store(&path).is_err());
}

#[test]
fn save_then_load_roundtrips() {
    let env = Env::new();
    let path = env.dir.join("sub\\backups.json");
    let mut store = BackupStore::default();
    apply_extensions(
        &env.target(&[HIVE_A]),
        &mut store,
        &[foo_cfg("CustomFooFile")],
        &env.opts(&icon_ok),
    );
    save_backup_store(&store, &path).unwrap();
    let back = load_backup_store(&path).unwrap();
    assert_eq!(back.backups.keys().collect::<Vec<_>>(), vec![".foo"]);
}
