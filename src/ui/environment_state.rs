//! Environment variables: the background scan, and the edits picked but not
//! applied yet (the `Path` list as a whole, and one entry per variable).
//! Apply itself is `jobs::start_env_apply`, one job with the pending changes.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use super::{kit, WinTuneApp};
use crate::backups::Slot;
use crate::environment::{
    analyze_entries, kind_to_write, load_env_store, path_change, resolver_vars, scan_environment,
    unquote, var_change, EntryInfo, EnvBackupStore, EnvChange, EnvScan, PathEdit, Resolver,
    VarEdit, REG_EXPAND_SZ_KIND,
};
use crate::i18n::messages;
use crate::tasks::{BackgroundJob, JobPoll};
use eframe::egui;

pub(super) type EnvScanJob = BackgroundJob<EnvScan>;

/// A variable being edited, by its lowercase name: the name it has now, or
/// the name chosen for a new variable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)] // `New` is built by the page (Task 6); remove then.
pub(super) enum VarKey {
    Existing(String),
    New(String),
}

/// What the user changed and has not applied yet.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct EnvPending {
    /// The whole wanted `Path` list.
    pub(super) path: Option<PathEdit>,
    pub(super) vars: BTreeMap<VarKey, VarEdit>,
}

/// The dialog open on the page, if any.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // The page (Task 6) uses it; remove then.
pub(super) enum EnvDialog {
    Variable {
        key: Option<VarKey>,
        name: String,
        value: String,
    },
    Entry {
        index: Option<usize>,
        text: String,
    },
}

impl WinTuneApp {
    pub(super) fn reload_env_store(&mut self) {
        match load_env_store(&self.env.env_store_path) {
            Ok(store) => {
                self.env_store = store;
                self.env_store_error = None;
            }
            Err(e) => {
                self.env_store = EnvBackupStore::default();
                let text = messages::load_error(self.tr, &e);
                self.notify(kit::ToastKind::Error, text);
                self.env_store_error = Some(e);
            }
        }
    }

    /// Schedules a scan; it starts as soon as no registry job is running.
    pub(super) fn request_env_scan(&mut self) {
        self.env_scan_needed = true;
    }

    /// Called every frame: collects a finished scan and starts a pending one.
    /// The last result stays shown while the next scan runs.
    pub(super) fn pump_env_scan(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.env_scan_job.as_mut() {
            match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(scan) => {
                    self.prune_env_edits(&scan);
                    self.env_folders = scan.folders.clone();
                    self.env_scan = Some(scan);
                }
                JobPoll::Panicked(msg) => {
                    let text = self.tr.t_args("env.scan_failed", &[&msg]);
                    self.notify(kit::ToastKind::Error, text);
                }
            }
            self.env_scan_job = None;
        }

        // Reading while a job writes would show transient states.
        if !self.env_scan_needed || self.is_busy() {
            return;
        }
        self.env_scan_needed = false;

        let env = self.env.clone();
        // Empty when the store is unreadable: rows show no snapshot, and
        // Apply stays blocked by `env_blocked_reason`.
        let snapshots = self.env_store.snapshots.clone();
        let repaint_ctx = ctx.clone();
        self.env_scan_job = Some(BackgroundJob::spawn(
            "Reading the environment variables",
            move || repaint_ctx.request_repaint(),
            move |_| {
                scan_environment(
                    &(env.env_target)(),
                    &snapshots,
                    &env.user_label,
                    env.process_var,
                )
            },
        ));
    }

    /// Drops the edits the registry already matches (applied by a partial
    /// Apply, or made elsewhere). A kept `Path` edit stays the full wanted
    /// list, whatever the registry holds now. An unreadable scan prunes
    /// nothing: it lists no variables, so every edit would look done.
    fn prune_env_edits(&mut self, scan: &EnvScan) {
        if scan.user_error.is_some() {
            return;
        }
        if let Some(edit) = &self.env_pending.path {
            if path_change(edit, scan.user_path()).is_none() {
                self.env_pending.path = None;
            }
        }
        self.env_pending.vars.retain(|key, edit| {
            let current = match key {
                VarKey::Existing(name) => scan.user_var(name),
                VarKey::New(_) => None,
            };
            var_change(edit, current).is_some()
        });
    }

    /// The changes Apply writes: the `Path` change, then one per variable
    /// edit the last scan still shows as different. Empty before a scan.
    pub(super) fn env_changes(&self) -> Vec<EnvChange> {
        let Some(scan) = &self.env_scan else {
            return Vec::new();
        };
        let mut changes: Vec<EnvChange> = self
            .env_pending
            .path
            .iter()
            .filter_map(|edit| path_change(edit, scan.user_path()))
            .collect();
        for (key, edit) in &self.env_pending.vars {
            let current = match key {
                VarKey::Existing(name) => scan.user_var(name),
                VarKey::New(_) => None,
            };
            changes.extend(var_change(edit, current));
        }
        changes
    }

    #[allow(dead_code)] // The page (Task 6) uses it; remove then.
    pub(super) fn env_pending_count(&self) -> usize {
        self.env_changes().len()
    }

    /// "Discard": forgets every pending edit.
    #[allow(dead_code)] // The page (Task 6) uses it; remove then.
    pub(super) fn discard_env_changes(&mut self) {
        self.env_pending = EnvPending::default();
    }

    /// The `Path` edit, started from the scanned value on first use. `None`
    /// before the first scan, or when the `Path` is not text.
    #[allow(dead_code)] // The page (Task 6) uses it; remove then.
    pub(super) fn env_path_edit_mut(&mut self) -> Option<&mut PathEdit> {
        if self.env_pending.path.is_none() {
            let current = self.env_scan.as_ref()?.user_path();
            if current.is_some_and(|c| !c.is_text()) {
                return None;
            }
            let raw = current.and_then(|c| c.value.as_deref());
            self.env_pending.path = Some(PathEdit::start(raw));
        }
        self.env_pending.path.as_mut()
    }

    /// The registry's `Path` is not what the edit started from: another
    /// program changed it meanwhile.
    pub(super) fn env_path_changed_elsewhere(&self) -> bool {
        let (Some(edit), Some(scan)) = (&self.env_pending.path, &self.env_scan) else {
            return false;
        };
        let now = scan.user_path().and_then(|v| v.value.as_deref());
        edit.started_from.as_deref() != now
    }

    /// Resolves `%name%` as Windows will after Apply: the scanned user
    /// variables with the pending edits, then the system ones.
    pub(super) fn env_resolver(&self) -> Resolver {
        let process = self.env.process_var;
        let Some(scan) = &self.env_scan else {
            return Resolver::new(Vec::new(), Vec::new(), process);
        };
        let mut user = resolver_vars(&scan.user);
        for edit in self.env_pending.vars.values() {
            let kind = edit
                .original
                .as_deref()
                .and_then(|name| scan.user_var(name))
                .map(|v| v.kind);
            if let Some(name) = &edit.original {
                user.retain(|(n, _, _)| !n.eq_ignore_ascii_case(name));
            }
            if let Some((name, value)) = &edit.wanted {
                let expandable = kind_to_write(kind, name, value) == REG_EXPAND_SZ_KIND;
                user.push((name.clone(), value.clone(), expandable));
            }
        }
        Resolver::new(user, resolver_vars(&scan.system), process)
    }

    /// The wanted `Path` entries (the edit, else the scanned ones) with their
    /// expansion and flags. A folder the scan did not check is checked here
    /// once and remembered.
    #[allow(dead_code)] // The page (Task 6) uses it; remove then.
    pub(super) fn env_path_infos(&mut self) -> Vec<EntryInfo> {
        let Some(scan) = &self.env_scan else {
            return Vec::new();
        };
        let wanted = match &self.env_pending.path {
            Some(edit) => edit.texts(),
            None => scan.user_path_entries(),
        };
        let system = scan.system_path_entries();
        let resolver = self.env_resolver();
        let folders = &self.env_folders;
        let learned: RefCell<Vec<(String, bool)>> = RefCell::new(Vec::new());
        let infos = analyze_entries(&wanted, &system, &resolver, &|key| {
            if let Some(exists) = folders.get(key) {
                return *exists;
            }
            let exists = Path::new(unquote(key)).is_dir();
            learned.borrow_mut().push((key.to_string(), exists));
            exists
        });
        self.env_folders.extend(learned.into_inner());
        infos
    }

    /// How the UI names a variable slot (as in History's saved originals):
    /// the value name inside the slot id.
    pub(super) fn env_slot_name(&self, slot_id: &str) -> String {
        match Slot::from_id(slot_id) {
            Slot::Value { name, .. } => name,
            Slot::Key(path) => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use egui::RawInput;

    fn scanned(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_env_scan();
        run_until_env_scanned(&mut app, ctx);
        app
    }

    fn rescan(app: &mut WinTuneApp, ctx: &egui::Context) {
        app.request_env_scan();
        run_until_env_scanned(app, ctx);
    }

    #[test]
    fn the_environment_scan_runs_in_background_and_waits_for_jobs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        fx.seed_env(false, "FOO", 1, "x");
        assert!(app.env_scan.is_none());
        let release = hold_job(&mut app);

        app.request_env_scan();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.env_scan_job.is_none(), "scan started during a job");
        assert!(app.env_scan_needed);

        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        run_until_env_scanned(&mut app, &ctx);
        let scan = app.env_scan.as_ref().expect("scanned");
        assert_eq!(
            scan.user_var("FOO").and_then(|v| v.value.as_deref()),
            Some("x")
        );
    }

    #[test]
    fn a_panicking_environment_scan_is_a_toast() {
        use crate::tasks::BackgroundJob;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.env_scan_job = Some(BackgroundJob::spawn(
            "Reading the environment variables",
            || {},
            |_| -> crate::environment::EnvScan { panic!("boom") },
        ));
        run_until_env_scanned(&mut app, &ctx);
        assert!(
            app.last_notice()
                .starts_with("Could not read the environment variables: "),
            "{}",
            app.last_notice()
        );
        assert!(app.env_scan.is_none());
    }

    #[test]
    fn a_scan_prunes_matched_edits_and_keeps_the_path_list() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A;B");
        let mut app = scanned(&fx, &ctx);
        app.env_path_edit_mut().unwrap().move_down(0);
        assert_eq!(app.env_pending_count(), 1);
        // Something else wrote the very list the user wanted.
        fx.seed_env(false, "Path", 2, "B;A");
        rescan(&mut app, &ctx);
        assert_eq!(app.env_pending.path, None);

        fx.seed_env(false, "Path", 2, "A;B");
        rescan(&mut app, &ctx);
        app.env_path_edit_mut().unwrap().move_down(0);
        assert!(!app.env_path_changed_elsewhere());
        // Another program added a folder meanwhile.
        fx.seed_env(false, "Path", 2, "A;B;C");
        rescan(&mut app, &ctx);
        assert!(app.env_pending.path.is_some(), "the edit was dropped");
        assert!(app.env_path_changed_elsewhere());
        assert_eq!(
            app.env_changes(),
            [EnvChange::Edit {
                name: "Path".into(),
                value: "B;A".into()
            }]
        );
    }

    #[test]
    fn a_scan_prunes_variable_edits_the_registry_already_matches() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "FOO", 1, "1");
        let mut app = scanned(&fx, &ctx);
        app.env_pending.vars.insert(
            VarKey::Existing("foo".into()),
            VarEdit {
                original: Some("FOO".into()),
                wanted: Some(("FOO".into(), "2".into())),
            },
        );
        app.env_pending.vars.insert(
            VarKey::New("bar".into()),
            VarEdit {
                original: None,
                wanted: Some(("BAR".into(), "1".into())),
            },
        );
        assert_eq!(app.env_pending_count(), 2);
        fx.seed_env(false, "FOO", 1, "2");
        rescan(&mut app, &ctx);
        assert_eq!(app.env_pending_count(), 1);
        assert!(!app
            .env_pending
            .vars
            .contains_key(&VarKey::Existing("foo".into())));
        app.discard_env_changes();
        assert_eq!(app.env_pending, EnvPending::default());
    }

    #[test]
    fn env_changes_combine_path_and_variables() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Path", 2, "A");
        fx.seed_env(false, "FOO", 1, "1");
        let mut app = scanned(&fx, &ctx);
        assert!(app.env_changes().is_empty());
        app.env_pending.vars.insert(
            VarKey::Existing("foo".into()),
            VarEdit {
                original: Some("FOO".into()),
                wanted: Some(("BAR".into(), "1".into())),
            },
        );
        app.env_path_edit_mut().unwrap().add("B".into());
        assert_eq!(
            app.env_changes(),
            [
                EnvChange::Edit {
                    name: "Path".into(),
                    value: "A;B".into()
                },
                EnvChange::Rename {
                    from: "FOO".into(),
                    to: "BAR".into(),
                    value: "1".into()
                }
            ]
        );
        assert_eq!(app.env_pending_count(), 2);
    }

    #[test]
    fn there_are_no_changes_before_the_first_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        assert!(app.env_path_edit_mut().is_none());
        app.env_pending.vars.insert(
            VarKey::New("bar".into()),
            VarEdit {
                original: None,
                wanted: Some(("BAR".into(), "1".into())),
            },
        );
        assert!(app.env_changes().is_empty());
        assert_eq!(app.env_pending_count(), 0);
    }

    #[test]
    fn the_resolver_uses_pending_values() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let dir = fx.dir.display().to_string();
        fx.seed_env(false, "TOOLS", 1, &format!(r"{dir}\one"));
        fx.seed_env(false, "Path", 2, r"%TOOLS%\bin");
        let mut app = scanned(&fx, &ctx);
        assert_eq!(app.env_path_infos()[0].expanded, format!(r"{dir}\one\bin"));
        app.env_pending.vars.insert(
            VarKey::Existing("tools".into()),
            VarEdit {
                original: Some("TOOLS".into()),
                wanted: Some(("TOOLS".into(), format!(r"{dir}\two"))),
            },
        );
        assert_eq!(app.env_path_infos()[0].expanded, format!(r"{dir}\two\bin"));
    }

    #[test]
    fn a_folder_missing_from_the_scan_is_checked_once_and_cached() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let present = fx.dir.display().to_string();
        let absent = fx.dir.join("nope").display().to_string();
        fx.seed_env(false, "Path", 2, &present);
        let mut app = scanned(&fx, &ctx);
        app.env_path_edit_mut().unwrap().add(absent.clone());
        let infos = app.env_path_infos();
        assert!(!infos[0].missing);
        assert!(infos[1].missing);
        assert_eq!(app.env_folders.get(&absent), Some(&false));
        // A scan replaces the cache with what it found.
        rescan(&mut app, &ctx);
        assert_eq!(app.env_folders.get(&absent), None);
        assert_eq!(app.env_folders.get(&present), Some(&true));
    }

    #[test]
    fn a_slot_is_named_by_its_value_name() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let app = fx.app(&ctx);
        let id = crate::environment::env_slot("Foo").id();
        assert_eq!(app.env_slot_name(&id), "Foo");
    }
}
