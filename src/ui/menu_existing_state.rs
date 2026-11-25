//! Entries already in the right-click menu: the background scan that lists
//! them, and the hide / rename changes picked but not applied yet. Apply
//! itself is `jobs::start_menu_apply`, one job with the list's entries.

use super::{kit, WinTuneApp};
use crate::context_menu::{
    find_catalog, rename_problem, scan, ExistingChange, ExistingEntry, ExistingState, Mechanism,
    Rename, Scan, ScanOptions,
};
use crate::i18n::messages;
use crate::tasks::{BackgroundJob, JobPoll};
use eframe::egui;

pub(super) type ScanJob = BackgroundJob<Scan>;

/// What the user changed on one existing entry. `None` keeps the current
/// state.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct PendingExisting {
    /// The wanted hidden state.
    pub hidden: Option<bool>,
    /// `Some(Some(text))` renames, `Some(None)` puts the original name back.
    pub label: Option<Option<String>>,
}

/// The change `pending` asks of `entry`, or `None` when it asks for the
/// current state or the row is locked (the engine would refuse the whole
/// Apply for it). A row that cannot be renamed keeps its name; typing the
/// original name back is a Reset name.
pub(super) fn existing_change(
    entry: &ExistingEntry,
    pending: &PendingExisting,
) -> Option<ExistingChange> {
    if entry.locked.is_some() {
        return None;
    }
    let hidden_now = entry.state != ExistingState::Shown;
    let hidden = pending.hidden.unwrap_or(hidden_now);
    let at_original = entry
        .original_label
        .as_deref()
        .is_none_or(|original| original == entry.label);
    let rename = match &pending.label {
        _ if !entry.renamable() => Rename::Keep,
        None => Rename::Keep,
        Some(Some(text)) if text.trim() == entry.label => Rename::Keep,
        Some(Some(text)) if entry.original_label.as_deref() == Some(text.trim()) => {
            Rename::Original
        }
        Some(Some(text)) => Rename::To(text.trim().to_string()),
        Some(None) if at_original => Rename::Keep,
        Some(None) => Rename::Original,
    };
    (hidden != hidden_now || rename != Rename::Keep).then(|| ExistingChange {
        mechanism: entry.mechanism.clone(),
        hidden,
        rename,
    })
}

impl WinTuneApp {
    /// Schedules a scan; it starts as soon as no registry job is running.
    pub(super) fn request_menu_scan(&mut self) {
        self.menu_scan_needed = true;
    }

    /// Called every frame: collects a finished scan and starts a pending one.
    /// The last result stays shown while the next scan runs.
    pub(super) fn pump_menu_scan(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.menu_scan_job.as_mut() {
            match job.poll() {
                JobPoll::Running => return,
                JobPoll::Finished(scan) => self.menu_scan = Some(scan),
                JobPoll::Panicked(msg) => {
                    let text = self.tr.t_args("menu.scan_failed", &[&msg]);
                    self.notify(kit::ToastKind::Error, text);
                }
            }
            self.menu_scan_job = None;
        }

        // Reading while a job writes would show transient states.
        if !self.menu_scan_needed || self.is_busy() {
            return;
        }
        self.menu_scan_needed = false;

        let env = self.env.clone();
        // Empty when the store is unreadable: rows show no snapshot, and
        // Apply stays blocked by `menu_blocked_reason`.
        let store = self.menu_store.clone();
        let repaint_ctx = ctx.clone();
        self.menu_scan_job = Some(BackgroundJob::spawn(
            "Reading the right-click menu",
            move || repaint_ctx.request_repaint(),
            move |_| {
                let opts = ScanOptions {
                    user: &env.user_label,
                    windows_dir: env.windows_dir.clone(),
                    is_windows_11: env.is_windows_11,
                    indirect: &env.resolve_indirect,
                    product_name: &env.product_name,
                };
                scan(&(env.menu_target)(), &store, &opts)
            },
        ));
    }

    /// The changes Apply writes for the pending hides and renames: one per
    /// pending id the last scan still lists and whose wish differs from the
    /// current state.
    pub(super) fn menu_existing_changes(&self) -> Vec<ExistingChange> {
        self.pending_existing()
            .into_iter()
            .map(|(_, change)| change)
            .collect()
    }

    /// Each pending change with the row it applies to.
    fn pending_existing(&self) -> Vec<(&ExistingEntry, ExistingChange)> {
        let Some(scan) = &self.menu_scan else {
            return Vec::new();
        };
        self.menu_pending
            .iter()
            .filter_map(|(id, pending)| {
                let entry = scan.find(id)?;
                Some((entry, existing_change(entry, pending)?))
            })
            .collect()
    }

    /// "Discard": forgets the pending hides and renames (the list of added
    /// entries and config.json are left alone).
    pub(super) fn discard_menu_changes(&mut self) {
        self.menu_pending.clear();
    }

    /// How the UI names an existing row: a Windows entry by its translated
    /// catalog name, any other by its label.
    pub(super) fn existing_name(&self, entry: &ExistingEntry) -> String {
        match entry.catalog.and_then(find_catalog) {
            Some(item) => self.tr.t(item.name_key),
            None => entry.label.clone(),
        }
    }

    /// "Name: problem" for each pending new name that cannot be applied.
    pub(super) fn menu_rename_problems(&self) -> Vec<String> {
        let tr = self.tr;
        self.pending_existing()
            .into_iter()
            .filter_map(|(entry, change)| {
                let Rename::To(label) = &change.rename else {
                    return None;
                };
                let problem = messages::rename_problem(tr, &rename_problem(label)?);
                Some(tr.t_args("menu.problem_line", &[&self.existing_name(entry), &problem]))
            })
            .collect()
    }

    /// What the pending hides also do beyond the listed places: the catalog
    /// side effect of a Windows entry, or, for another program's shell
    /// extension, that it disappears everywhere. Empty: Apply asks nothing.
    pub(super) fn menu_apply_warnings(&self) -> Vec<String> {
        let tr = self.tr;
        self.pending_existing()
            .into_iter()
            .filter(|(_, change)| change.hidden && matches!(change.mechanism, Mechanism::Clsid(_)))
            .filter_map(|(entry, _)| match entry.catalog.and_then(find_catalog) {
                Some(item) => item
                    .also_affects_key
                    .map(|key| tr.t_args("menu.problem_line", &[&tr.t(item.name_key), &tr.t(key)])),
                None => Some(tr.t_args("menu.also.handler", &[&entry.label])),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::context_menu::{ExistingState, Mechanism, Rename};
    use egui::RawInput;

    #[test]
    fn the_scan_runs_in_background_and_fills_menu_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        assert!(app.menu_scan.is_none());

        app.request_menu_scan();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.menu_scan_job.is_some() || app.menu_scan.is_some());
        run_until_scanned(&mut app, &ctx);

        let scan = app.menu_scan.as_ref().expect("scan result");
        assert_eq!(scan.others.len(), 1, "{:?}", scan.others);
        assert_eq!(scan.others[0].label, "Open with App");
        assert_eq!(scan.others[0].state, ExistingState::Shown);
    }

    #[test]
    fn the_scan_waits_for_a_running_job() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let release = hold_job(&mut app);

        app.request_menu_scan();
        frame(&mut app, &ctx, RawInput::default());
        assert!(app.menu_scan_job.is_none(), "scan started during a job");
        assert!(app.menu_scan_needed);

        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        run_until_scanned(&mut app, &ctx);
        assert_eq!(app.menu_scan.as_ref().map(|s| s.others.len()), Some(1));
    }

    #[test]
    fn a_finished_job_asks_for_a_new_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        let release = hold_job(&mut app);
        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
        assert!(app.menu_scan_needed || app.menu_scan_job.is_some());
        run_until_scanned(&mut app, &ctx);
        assert!(app.menu_scan.is_some());
    }

    #[test]
    fn opening_the_context_menu_page_asks_for_a_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(super::super::ActiveTab::ContextMenu);
        assert!(app.menu_scan_needed);
    }

    #[test]
    fn a_panicking_scan_is_a_toast() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.menu_scan_job = Some(BackgroundJob::spawn(
            "Reading the right-click menu",
            || {},
            |_| -> Scan { panic!("boom") },
        ));
        run_until_scanned(&mut app, &ctx);
        let shown = app.last_notice();
        assert!(
            shown.starts_with("Could not read the right-click menu: "),
            "{shown}"
        );
        assert!(app.menu_scan.is_none());
    }

    /// One seeded third-party verb, scanned.
    fn scanned_verb(fx: &TempIcons, ctx: &egui::Context) -> (WinTuneApp, String) {
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = fx.app(ctx);
        scan_now(&mut app, ctx);
        let id = app.menu_scan.as_ref().unwrap().others[0].id.clone();
        (app, id)
    }

    fn pending(hidden: Option<bool>, label: Option<Option<&str>>) -> PendingExisting {
        PendingExisting {
            hidden,
            label: label.map(|l| l.map(str::to_string)),
        }
    }

    #[test]
    fn pending_changes_map_to_existing_changes() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let (mut app, id) = scanned_verb(&fx, &ctx);
        let mechanism = app.menu_scan.as_ref().unwrap().others[0].mechanism.clone();
        assert!(matches!(mechanism, Mechanism::Verbs(_)));
        let change = |hidden, rename| ExistingChange {
            mechanism: mechanism.clone(),
            hidden,
            rename,
        };

        app.menu_pending
            .insert(id.clone(), pending(Some(true), None));
        assert_eq!(app.menu_existing_changes(), [change(true, Rename::Keep)]);

        app.menu_pending
            .insert(id.clone(), pending(None, Some(Some(" Mine "))));
        assert_eq!(
            app.menu_existing_changes(),
            [change(false, Rename::To("Mine".into()))]
        );

        // Equal to the current state: nothing to write.
        for same in [
            pending(Some(false), None),
            pending(None, Some(Some("Open with App"))),
            pending(Some(false), Some(None)),
            PendingExisting::default(),
        ] {
            app.menu_pending.insert(id.clone(), same.clone());
            assert!(app.menu_existing_changes().is_empty(), "{same:?}");
        }

        // An id the scan no longer has is skipped.
        app.menu_pending.clear();
        app.menu_pending
            .insert("verb:gone|x".into(), pending(Some(true), None));
        assert!(app.menu_existing_changes().is_empty());
    }

    #[test]
    fn reset_name_is_sent_only_when_the_name_differs_from_the_original() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        // Renamed in the user copy, by someone else.
        let copy = fx.hkcu(r"Software\Classes\Directory\shell\AppVerb");
        let (k, _) = fx.sandbox.root.create_subkey(&copy).unwrap();
        k.set_value("MUIVerb", &"Renamed").unwrap();
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        let entry = app.menu_scan.as_ref().unwrap().others[0].clone();
        assert_eq!(entry.label, "Renamed");

        app.menu_pending
            .insert(entry.id.clone(), pending(None, Some(None)));
        assert_eq!(
            app.menu_existing_changes(),
            [ExistingChange {
                mechanism: entry.mechanism.clone(),
                hidden: false,
                rename: Rename::Original,
            }]
        );
    }

    #[test]
    fn typing_the_original_name_back_is_a_reset_name() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let copy = fx.hkcu(r"Software\Classes\Directory\shell\AppVerb");
        let (k, _) = fx.sandbox.root.create_subkey(&copy).unwrap();
        k.set_value("MUIVerb", &"Renamed").unwrap();
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        let entry = app.menu_scan.as_ref().unwrap().others[0].clone();
        assert_eq!(entry.original_label.as_deref(), Some("Open with App"));

        app.menu_pending.insert(
            entry.id.clone(),
            pending(None, Some(Some(" Open with App "))),
        );
        assert_eq!(
            app.menu_existing_changes(),
            [ExistingChange {
                mechanism: entry.mechanism.clone(),
                hidden: false,
                rename: Rename::Original,
            }]
        );

        // Already at its original name: typing it is no change at all.
        k.delete_value("MUIVerb").unwrap();
        scan_now(&mut app, &ctx);
        assert_eq!(
            app.menu_scan.as_ref().unwrap().others[0].label,
            "Open with App"
        );
        assert!(app.menu_existing_changes().is_empty());
    }

    #[test]
    fn a_locked_row_is_skipped_and_the_rest_of_the_apply_goes_through() {
        use crate::context_menu::{
            entry_key, Locked, MenuEntry, Place, ProgramChoice, BLOCKED_KEY,
        };
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        // Another program's handler, hidden for every account (HKLM Blocked).
        let clsid = "{23170F69-40C1-278A-1000-000100020000}";
        let machine = fx.machine_key();
        let handler = r"Software\Classes\*\shellex\ContextMenuHandlers\7-Zip";
        let (k, _) = machine.create_subkey(handler).unwrap();
        k.set_value("", &clsid).unwrap();
        let server = format!(r"Software\Classes\CLSID\{clsid}\InprocServer32");
        let (k, _) = machine.create_subkey(server).unwrap();
        k.set_value("", &r"C:\Apps\7-zip.dll").unwrap();
        let (k, _) = machine.create_subkey(BLOCKED_KEY).unwrap();
        k.set_value(clsid, &"").unwrap();

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
        let mut app = fx.app_with(&ctx, config);
        scan_now(&mut app, &ctx);
        let id = format!("clsid:{clsid}");
        let row = app.menu_scan.as_ref().unwrap().find(&id).expect("row");
        assert_eq!(row.locked, Some(Locked::MachineWide));
        // Pending show on the locked row (it may have locked after the click).
        app.menu_pending.insert(id, pending(Some(false), None));
        assert!(app.menu_existing_changes().is_empty());
        assert!(app.menu_apply_warnings().is_empty());

        app.start_menu_apply(&ctx);
        assert!(app.is_busy(), "{}", app.last_notice());
        run_until_idle(&mut app, &ctx);

        assert!(fx.key_exists(&fx.hkcu(&entry_key(Place::Folder, "e1"))));
        let last = app.history.first().expect("history entry");
        assert!(last.errors.is_empty(), "{:?}", last.errors);
        assert!(!fx.key_exists(&fx.hkcu(BLOCKED_KEY)));
    }

    #[test]
    fn a_catalog_row_is_never_renamed() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_clsid("{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}");
        let mut app = fx.app(&ctx);
        scan_now(&mut app, &ctx);
        assert!(app
            .menu_scan
            .as_ref()
            .unwrap()
            .find("win:give_access")
            .is_some());
        app.menu_pending
            .insert("win:give_access".into(), pending(None, Some(Some("X"))));
        assert!(app.menu_existing_changes().is_empty());
    }

    #[test]
    fn discard_clears_pending_but_not_the_list() {
        use crate::context_menu::{MenuEntry, Place, ProgramChoice};
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
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
        let mut app = fx.app_with(&ctx, config);
        app.menu_pending
            .insert("verb:x|y".into(), pending(Some(true), None));

        app.discard_menu_changes();

        assert!(app.menu_pending.is_empty());
        assert_eq!(app.menu_entries().len(), 1);
        assert!(!app.env.config_path.exists(), "config.json written");
    }

    #[test]
    fn a_bad_new_name_blocks_apply_with_the_entry_named() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let (mut app, id) = scanned_verb(&fx, &ctx);
        app.menu_pending.insert(id, pending(None, Some(Some("@x"))));

        app.start_menu_apply(&ctx);

        assert!(!app.is_busy());
        let shown = app.last_notice();
        assert!(shown.contains("Open with App"), "{shown}");
        assert!(shown.contains("The label cannot start with @."), "{shown}");
    }
}
