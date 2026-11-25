# Context menu module, PR 1: engine and "My entries"

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the disabled "Context menu" page into a working module where a user adds their own right-click entries (label, program, places, icon), applies them per user with a snapshot first, and restores them from History.

**Architecture:** A new engine `src/context_menu/` on `regsnap`, shaped like `src/tweaks.rs`: an injectable HKCU target, a per-user snapshot store (`context-menu-backups.json`), a status read, an apply that makes the registry match the user's list, and a restore. The UI adds a kit page (`src/ui/context_menu_tab.rs`) that edits the list in `config.json` (autosaved), shows each entry's live state, and applies through one background job. PR 2 (hide/rename existing entries, curated list, scan) is out of scope.

**Tech Stack:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59, rfd 0.15, serde.

**Spec:** `docs/superpowers/specs/2026-11-25-context-menu-design.md` (sections 6.1, 6.2, 6.6, 6.7 and 7 item 1 are this PR).

## Global Constraints

- Safety model (ARCHITECTURE.md): validate, snapshot on first touch, save the store **and** write a `.reg` export **before** any registry write; report every failure; an unreadable store blocks writing.
- Everything is per user: written under `HKCU\Software\Classes`; the entry list and the snapshots are keyed by `AppEnv::user_label`.
- Added entry keys are named `WinTune.<id>` under `<class>\shell\`. WinTune never touches a `shell` key whose name does not start with `WinTune.` in this PR.
- Places and placeholders: Folder → `Directory`, `%1`; Background → `Directory\Background`, `%V`; Drive → `Drive`, `%1`; Files → `*`, `%1`.
- Label: 1 to 80 characters after trimming. At least one place.
- No free arguments, no admin, no extended (Shift) entries, no submenus.
- Every UI text is a translation key used as a literal, present in `i18n/strings.resx` and `i18n/strings.fr.resx`; engine problems are enums translated in `i18n/messages.rs`.
- History stores the English module label `Context menu` and the actions `Apply` / `Restore`.
- Tests never touch the real HKCU outside the `HKCU\Software\WinTuneTest` sandbox (Task 1's probe is a script, not a test).
- Kit only on the page: no colors, sizes or fonts set by hand.
- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --locked` pass after every task.

## Review Focus

1. A folder, file or drive path with spaces, an apostrophe or accents reaches the program as one argument (command strings quote every placeholder; PowerShell uses Windows' own `-literalPath '%V'` form).
2. A drive root (`C:\`) is passed without the trailing `\"` turning into an escaped quote (Drive uses `"%1."`, pinned by a test).
3. An entry whose places shrink, or an entry deleted from the list, leaves no `WinTune.*` key behind after Apply (Apply removes orphans, snapshotting them first).
4. A second account (different `user_label`) sees neither the first account's entries nor its snapshots.
5. A program that disappears after the entry was added (uninstalled VS Code, moved custom exe) blocks Apply with a translated reason instead of writing a broken command.

---

## File Structure

- Create `scripts/context-menu-probe.ps1`: Task 1's real-machine probe (rerunnable on Windows 10).
- Create `src/context_menu/mod.rs`: types (`Place`, `ProgramChoice`, `MenuEntry`, `ContextMenuConfig`, `EntryProblem`, `EntryStatus`), validation, re-exports.
- Create `src/context_menu/programs.rs`: `Programs` detection and command lines.
- Create `src/context_menu/engine.rs`: target, store, status, apply, restore.
- Create `src/ui/context_menu_tab.rs`: the page.
- Modify: `src/lib.rs`, `src/models.rs` (`AppConfig.context_menu`), `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/confirm.rs`, `src/ui/shell.rs`, `src/ui/home.rs`, `src/ui/polish.rs`, `src/ui/originals.rs`, `src/ui/history_panel.rs`, `src/ui/test_support.rs`, `src/i18n/messages.rs`, both `.resx`, `docs/registry-reference.md`, `README.md`, `ARCHITECTURE.md`.

---

### Task 1: Real-machine probe

**Files:**
- Create: `scripts/context-menu-probe.ps1`
- Modify: `docs/registry-reference.md` (new section "Context menu")

The probe runs unelevated in Windows PowerShell 5.1 on this machine. It writes only under `HKCU\Software\Classes` and `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked`, records what it found before, and puts everything back in a `finally` block. It reads menus with `(New-Object -ComObject Shell.Application).NameSpace(<parent>).ParseName(<leaf>).Verbs()`, which lists classic verbs and shellex handlers (not modern entries) in a fresh read.

- [ ] **Step 1: Write the probe** with these checks, each printing `PASS`/`FAIL` and the evidence:
  1. Add `HKCU\Software\Classes\Directory\shell\WinTuneProbe.a` with `MUIVerb` = `WinTune probe é'`, `Icon` = `"C:\Windows\System32\notepad.exe"`, `command` default = `"C:\Windows\System32\notepad.exe" "%1"`; the verb shows on a temp folder named `probe 'é x`. Same for `Drive` on `C:\` and `*` on a temp file.
  2. Pick the first HKLM `Directory\shell` verb that has no `Extended` value and shows in `Verbs()` (on this machine `AnyCode`). Create the HKCU copy of its key with only `ProgrammaticAccessOnly` = `""`: does it disappear from `Verbs()`? Then add `MUIVerb` = `Probe renamed` to that copy without `ProgrammaticAccessOnly`: does the label change, and does the verb still have its command (`$verb.DoIt()` is **not** called; check that `HKCR\Directory\shell\<verb>\command` still reads the HKLM value)?
  3. Add `{596AB062-B4D2-4215-9F74-E9109B0A8153}` (Restore previous versions) to the HKCU `Blocked` key: does its verb disappear from `Verbs()` of a folder?
- [ ] **Step 2: Run it** — `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/context-menu-probe.ps1`. Expected: a PASS/FAIL line per check, and afterwards `reg query HKCU\Software\Classes\Directory\shell` lists no `WinTuneProbe` key and `Blocked` is as before.
- [ ] **Step 3: Record the results** in `docs/registry-reference.md` under a new "Context menu" section: the four key paths and placeholders, `MUIVerb`, `Icon`, `ProgrammaticAccessOnly` (Microsoft Learn, *Creating Shortcut Menu Handlers*), `Blocked` (undocumented), each check's result with the Windows build (`[Environment]::OSVersion`), and a "Needs the owner's eyes" list: the Windows 11 "Show more options" placement, modern entries (Open in Terminal) under `Blocked`, side effects on Properties tabs. **If check 1 fails, stop and report: the rest of the plan depends on it.** Checks 2 and 3 only feed PR 2 and spec section 8.
- [ ] **Step 4: Commit** — `git add scripts/context-menu-probe.ps1 docs/registry-reference.md` then `git commit -m "docs(context-menu): real-machine probe of registry verbs and results"`.

### Task 2: Types, programs and validation

**Files:**
- Create: `src/context_menu/mod.rs`, `src/context_menu/programs.rs`
- Modify: `src/lib.rs` (`pub mod context_menu;`), `src/models.rs` (`AppConfig`)

**Interfaces — Produces:**
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub enum Place { Folder, Background, Drive, Files }` with `pub const ALL: [Place; 4]`, `pub fn class_path(self) -> &'static str` and `pub fn placeholder(self) -> &'static str` (values in Global Constraints).
- `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(tag = "kind", content = "path")] pub enum ProgramChoice { VsCode, Terminal, PowerShell, Cmd, Custom(String) }`.
- `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] pub struct MenuEntry { pub id: String, pub label: String, pub program: ProgramChoice, pub places: BTreeSet<Place>, pub icon: Option<String> }` — `icon: None` means the program's own icon.
- `#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] pub struct ContextMenuConfig { pub users: BTreeMap<String, Vec<MenuEntry>> }` with `pub fn entries_of(&self, user: &str) -> &[MenuEntry]` and `pub fn entries_mut(&mut self, user: &str) -> &mut Vec<MenuEntry>`.
- `AppConfig` gains `#[serde(default)] pub context_menu: ContextMenuConfig`.
- `pub fn new_entry_id(existing: &[MenuEntry]) -> String`: the lowest unused `e1`, `e2`, …
- `pub fn key_name(id: &str) -> String` → `WinTune.<id>`; `pub const KEY_PREFIX: &str = "WinTune.";`.
- `#[derive(Debug, Clone, Default)] pub struct Programs { pub vscode: Option<PathBuf>, pub terminal: Option<PathBuf>, pub powershell: Option<PathBuf>, pub cmd: Option<PathBuf> }` with `pub fn detect() -> Self` (VS Code via `detector::resolve_vscode_executable(None)`; Terminal `%LOCALAPPDATA%\Microsoft\WindowsApps\wt.exe`; PowerShell `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`; cmd `%SystemRoot%\System32\cmd.exe`; each kept only if the file exists) and `pub fn resolve(&self, choice: &ProgramChoice) -> Option<PathBuf>` (Custom: the path if the file exists).
- `pub fn command_line(choice: &ProgramChoice, exe: &Path, place: Place) -> String`, `pub fn icon_value(entry: &MenuEntry, exe: &Path) -> String` (the custom icon if set, else the exe; both written as `"<path>"`).
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum EntryProblem { EmptyLabel, LabelTooLong, NoPlace, ProgramNotFound, IconNotFound, DuplicateLabel(Place) }` and `pub fn entry_problems(entries: &[MenuEntry], programs: &Programs) -> Vec<(String, EntryProblem)>` (entry id, problem).

- [ ] **Step 1: Write the failing tests** in `src/context_menu/mod.rs` and `programs.rs`:

```rust
#[test]
fn command_lines_quote_the_placeholder_for_each_place() {
    let exe = Path::new(r"C:\Apps\My App\app.exe");
    let custom = ProgramChoice::Custom(exe.display().to_string());
    assert_eq!(command_line(&custom, exe, Place::Folder), r#""C:\Apps\My App\app.exe" "%1""#);
    assert_eq!(command_line(&custom, exe, Place::Background), r#""C:\Apps\My App\app.exe" "%V""#);
    assert_eq!(command_line(&custom, exe, Place::Drive), r#""C:\Apps\My App\app.exe" "%1.""#);
    assert_eq!(command_line(&custom, exe, Place::Files), r#""C:\Apps\My App\app.exe" "%1""#);
    let wt = Path::new(r"C:\Users\u\AppData\Local\Microsoft\WindowsApps\wt.exe");
    assert_eq!(command_line(&ProgramChoice::Terminal, wt, Place::Background),
        r#""C:\Users\u\AppData\Local\Microsoft\WindowsApps\wt.exe" -d "%V""#);
    let ps = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    assert_eq!(command_line(&ProgramChoice::PowerShell, ps, Place::Folder),
        r#""C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -noexit -command Set-Location -literalPath '%V'"#);
    let cmd = Path::new(r"C:\Windows\System32\cmd.exe");
    assert_eq!(command_line(&ProgramChoice::Cmd, cmd, Place::Drive),
        r#""C:\Windows\System32\cmd.exe" /s /k pushd "%V""#);
}

#[test]
fn entry_problems_cover_label_places_program_icon_and_duplicates() { /* one entry per problem:
   label "  " -> EmptyLabel; 81 chars -> LabelTooLong; 80 chars -> none;
   places empty -> NoPlace; Custom("C:\\missing.exe") -> ProgramNotFound;
   icon Some(missing) -> IconNotFound; two entries labelled "Open" both on Folder
   -> DuplicateLabel(Folder) for the second only; "Open" on Folder + "Open" on Files -> none */ }

#[test]
fn new_entry_ids_fill_the_lowest_gap() { /* [] -> "e1"; [e1, e3] -> "e2" */ }

#[test]
fn an_old_config_without_context_menu_loads_with_an_empty_list() { /* serde_json of an AppConfig
   JSON without the field -> context_menu == ContextMenuConfig::default() */ }

#[test]
fn entries_are_kept_per_user() { /* entries_mut("A").push(..); entries_of("B").is_empty() */ }
```

Fill the four commented bodies with exactly the assertions their comments list; `Programs` in tests points at temp files created by the test.
- [ ] **Step 2: Run** `cargo test --locked context_menu` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** the Interfaces above. Label length counts `char`s after `trim()`.
- [ ] **Step 4: Run** `cargo test --locked context_menu` — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(context-menu): entry types, program detection, command lines and validation"`.

### Task 3: Engine — status, apply, restore

**Files:**
- Create: `src/context_menu/engine.rs`
- Modify: `src/context_menu/mod.rs` (re-exports)

**Interfaces — Consumes:** Task 2. `regsnap::{snapshot_key, restore_key, delete_tree, export_reg_full, unique_reg_file, write_reg_file, RegistryKeySnapshot}`, `associations::OperationReport`, `clock`, `persist`.

**Interfaces — Produces:**
- `pub struct MenuTarget { pub root: RegKey, pub root_name: String, pub base: String }` with `pub fn current_user() -> Self` (HKCU, empty base) and `pub(crate) fn path(&self, rel: &str) -> String` (same rule as `TweakTarget::path`). Every relative path starts at `Software\Classes\`.
- `pub fn entry_key(place: Place, id: &str) -> String` → `Software\Classes\<class_path>\shell\WinTune.<id>`.
- `MenuBackup { timestamp: String, snapshot: RegistryKeySnapshot }`, `#[derive(Default, Serialize, Deserialize, Clone, PartialEq, Debug)] MenuBackupStore { users: BTreeMap<String, BTreeMap<String, MenuBackup>> }` (inner key = `entry_key` path) with `backups_of(&self, user) -> Option<&BTreeMap<..>>`; `default_menu_store_path()` → `app_data_dir().join("context-menu-backups.json")`; `load_menu_store(path) -> Result<MenuBackupStore, persist::LoadError>`; `save_menu_store(store, path) -> Result<(), String>`.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum EntryStatus { Applied, NotApplied, Changed(Vec<Place>) }` and `pub fn entry_status(target: &MenuTarget, entry: &MenuEntry, programs: &Programs) -> EntryStatus`: per chosen place, compares `MUIVerb`, `Icon` and `command` default with what Apply would write; all chosen places absent → `NotApplied`; all equal → `Applied`; otherwise `Changed(places that differ)`. A program that no longer resolves → `Changed(all chosen places)` if any key exists, else `NotApplied`.
- `pub fn orphan_keys(target: &MenuTarget, entries: &[MenuEntry]) -> Vec<String>`: existing `WinTune.*` keys under the four `shell` keys that belong to no (entry, chosen place).
- `pub struct MenuOptions<'a> { pub user: String, pub store_path: PathBuf, pub reg_backup_dir: PathBuf, pub progress: &'a dyn Fn(usize, usize, &str) }`.
- `pub fn apply_entries(target: &MenuTarget, store: &mut MenuBackupStore, entries: &[MenuEntry], programs: &Programs, opts: &MenuOptions) -> OperationReport`: makes the registry match `entries`. Phase 1: refuse the whole apply if `entry_problems` is non-empty (one error per problem, nothing written); list keys to write (each entry × chosen place) and keys to delete (`orphan_keys`); snapshot each on first touch into `store.users[user]`. Phase 2: save store, write `<stamp>-context-menu-before-apply.reg`; on failure, nothing written. Phase 3: delete-then-write each entry key (`MUIVerb`, `Icon`, `command` default as REG_SZ; the verb key's default value is not set), delete orphans; `succeeded` gets the entry label per entry fully written and the key path per orphan removed.
- `pub fn restore_keys(target: &MenuTarget, store: &mut MenuBackupStore, keys: &[String], opts: &MenuOptions) -> OperationReport`: like `tweaks::restore_tweaks` (restore the snapshot, forget it on success, keep it on failure, save the store).

- [ ] **Step 1: Write the failing tests** (sandbox `Env` like `tweaks::tests::Env`, `MenuTarget` based at the sandbox path):
  - `apply_writes_one_key_per_place_with_label_icon_and_command`: entry `e1` "Open with App" on Folder + Background with a temp `app.exe` → `Software\Classes\Directory\shell\WinTune.e1` has `MUIVerb` "Open with App", `Icon` `"<exe>"`, `command` default `"<exe>" "%1"`; the Background key has `"%V"`; no `Drive` or `*` key; `entry_status` == `Applied`; report has a `.reg` file and no errors.
  - `status_is_not_applied_before_apply_and_changed_after_an_outside_edit`: before apply `NotApplied`; after apply, overwrite the Folder `MUIVerb` → `Changed(vec![Place::Folder])`.
  - `shrinking_places_or_deleting_an_entry_removes_its_keys_after_apply`: apply on Folder+Files, then apply with Folder only → Files key gone; apply with `[]` → Folder key gone; `orphan_keys` empty.
  - `apply_never_touches_keys_without_the_prefix`: a pre-existing `Directory\shell\Other` survives every apply.
  - `a_problem_blocks_the_whole_apply`: one valid + one empty-label entry → errors non-empty, no key written, store file not created.
  - `restore_puts_back_the_state_before_the_first_apply`: pre-existing `Directory\shell\WinTune.e1` with `MUIVerb` "Before" → apply → restore that key → `MUIVerb` is "Before" and the snapshot is forgotten; restoring an absent-originally key deletes it.
  - `snapshots_are_per_user`: apply as user "A" then as "B" on the same keys → `store.users["A"]` holds the original state, `store.users["B"]` holds A's written state; `backups_of("C")` is `None`.
  - `an_unwritable_store_blocks_apply`: `store_path` inside a missing read-only location (a path whose parent is a file) → error "Nothing applied…", no key written.
- [ ] **Step 2: Run** `cargo test --locked context_menu::engine` — Expected: FAIL.
- [ ] **Step 3: Implement** the Interfaces above, mirroring `tweaks::apply_tweaks` / `restore_tweaks` for phases, progress and error wording.
- [ ] **Step 4: Run** `cargo test --locked context_menu` — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(context-menu): engine with per-user snapshots, apply and restore"`.

### Task 4: App wiring — environment, store, job, navigation, Home

**Files:** `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/shell.rs`, `src/ui/home.rs`, `src/ui/polish.rs`, `src/ui/test_support.rs`, `src/i18n/messages.rs`, both `.resx`.

**Interfaces — Consumes:** Tasks 2-3.

**Interfaces — Produces:**
- `AppEnv` gains `menu_target: Arc<dyn Fn() -> MenuTarget + Send + Sync>`, `menu_store_path: PathBuf`, `programs: Programs` (`AppEnv::system` uses `Programs::detect()`; `TempIcons::env` uses `{sandbox}\HKCU` as base, `dir/context-menu-backups.json`, and a `Programs` whose four paths are temp files created by the fixture).
- `WinTuneApp` gains `menu_store: MenuBackupStore`, `menu_store_error: Option<LoadError>`, `menu_statuses: HashMap<String, EntryStatus>` (by entry id), `menu_orphans: Vec<String>` (`orphan_keys` of the current list, refreshed with the statuses), `selected_menu_entry: Option<String>`; methods `reload_menu_store(&mut self)` and `refresh_menu_statuses(&mut self)` (synchronous, like `refresh_tweak_statuses`; never while `is_busy()`), both called at startup, from `open_page`, and after every finished job; `menu_entries(&self) -> &[MenuEntry]` and `menu_entries_mut(&mut self) -> &mut Vec<MenuEntry>` (for `env.user_label`, the latter marks autosave dirty the same way rule edits do).
- `RunningJob::ContextMenu { job: BackgroundJob<MenuOutcome>, restore: bool }`, `MenuOutcome { report, store, restored }`; `title_key` `job.apply_menu` / `job.restore_menu`, `name_key` `job.name.apply_menu` / `job.name.restore_menu`, `history_label` `("Context menu", "Apply" | "Restore")`; `start_menu_apply(&mut self, ctx)` and `start_menu_restore(&mut self, ctx, keys: Vec<String>)`, both refusing via `refuse_if_busy`, when `menu_store_error` or `config_error` is set, and (apply) when `entry_problems` is non-empty. Finishing: replace `menu_store`, `(env.notify_shell)()`, `show_report(ReportKind::AppliedMenu | RestoredMenu)`, `record_history("Context menu", …)`, `refresh_menu_statuses()`; a panic also reloads the menu store.
- `ActiveTab::ContextMenu` is a real page: in `ALL` (8 entries, after `StartPrivacy`), `history_module` → `Some("Context menu")`, `owns_action_bar` → true, routed to `show_context_menu_tab` (Task 5; until then a stub drawing the header only). The nav entry and Home tile are enabled; the `nav.coming_soon` key and the two "cannot be opened" tests are removed. `polish::search_box_id` stays `None` for it.
- Home: `ModuleSummary` for the page with `status = tr.t_args("home.menu_status", &[&count, &changed])` ("{0} entries, {1} changed" / "{0} entrée(s), {1} modifiée(s)"), `drift = changed` (entries with `EntryStatus::Changed`).
- `messages::history_module` maps `"Context menu"` to `history.module.context_menu`; `messages::entry_problem(tr, &EntryProblem) -> String` and `messages::entry_status(tr, &EntryStatus) -> String` (Applied "Applied" / "Appliquée", NotApplied "Not applied" / "Non appliquée", Changed "Out of date" / "Pas à jour"; the Changed pill's hover is `menu.changed_hint` "The registry differs from this entry (edited here or by another program). Apply to update it."). An edit on the page calls `refresh_menu_statuses()` so the pill follows the edit; these reads are a few registry values per entry.

- [ ] **Step 1: Write the failing tests:**
  - `shell.rs`: `the_context_menu_page_opens_from_the_nav` (click the nav label → `active_tab == ContextMenu`); replaces the two removed tests.
  - `home.rs`: `the_context_menu_tile_counts_entries_and_changes` (two entries applied in the sandbox, one edited outside → tile text contains the `home.menu_status` text for 2 and 1, and `drift_target()` is `Some(ContextMenu)` when no other module drifts).
  - `jobs.rs`: `a_menu_apply_runs_in_background_and_is_logged` (two entries, `start_menu_apply`, `run_until_idle` → keys exist in the sandbox, last history entry module "Context menu" action "Apply", `menu_statuses` all `Applied`); `menu_apply_is_refused_while_the_store_is_unreadable` (corrupt `context-menu-backups.json` → no key written, a toast).
  - `mod.rs`: `another_user_sees_an_empty_list` (config with entries for `TEST\user`; app with `user_label` "OTHER\user" → `menu_entries()` is empty).
- [ ] **Step 2: Run** `cargo test --locked ui::` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement** the Interfaces above; add every new key to both `.resx`.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS (the walk over `ActiveTab::ALL` in existing tests now covers the page).
- [ ] **Step 5: Commit** — `git commit -m "feat(context-menu): app wiring, background job, navigation and Home tile"`.

### Task 5: The page — "My entries"

**Files:** Create `src/ui/context_menu_tab.rs`; modify `src/ui/mod.rs`, `src/ui/confirm.rs`, both `.resx`.

**Interfaces — Consumes:** Task 4's app fields and methods; kit (`page_header`, `banner`, `section_with_action`, `SettingRow`, `pill`, `overflow_menu`, `empty_state`, `primary_button`, `secondary_button`, `page_action_bar`).

**Interfaces — Produces:** `fn show_context_menu_tab(&mut self, ctx: &egui::Context)`; `ConfirmAction::RemoveMenuEntry { id: String }` (removes the entry from the list only; the message says its keys go at the next Apply and its original stays in History).

Content, top to bottom (spec 6.7, "My entries" part only):
- Header: breadcrumb `WinTune › Context menu`, title `nav.context_menu`, subtitle `menu.subtitle` = "For {0}. Right-click entries you add, per account." with `env.user_label`.
- When `env.is_windows_11`: an Info banner `menu.win11_banner` ("On Windows 11 these entries appear under "Show more options".") with action `menu.use_classic` that calls `open_page(ActiveTab::Tweaks)`.
- `menu_store_error` → the error banner from `messages::load_error`, as on the tweak pages.
- Section `menu.my_entries` with action `menu.add` (adds `MenuEntry { id: new_entry_id, label: tr.t("menu.new_label") /* "New entry" */, program: ProgramChoice::VsCode if detected else Custom(""), places: {Folder}, icon: None }` and selects it). Empty list → `empty_state` with `menu.empty` and the same action.
- One row per entry: label, the places joined (`menu.place.folder|background|drive|files`), a pill for its state (Applied / Not applied / Changed, using `messages::entry_status`), the first problem in red under it if any; click selects; `…` menu with `menu.remove`.
- Detail of the selected entry, under the list: Label (text field, max 80 enforced by validation, not by truncating input); Program (`egui::ComboBox` with VS Code, Windows Terminal, PowerShell, Command Prompt, Other program…; undetected presets disabled with `menu.program_missing` on hover; "Other program…" shows the path field and a Browse button using `rfd::FileDialog` filtered to `exe`); Appears on (four checkboxes in `Place::ALL` order); Icon (`menu.icon_default` "Program's icon" or the path, with Change… (`rfd`, filters `ico`, `exe`, `dll`) and Reset).
- Action bar (`page_action_bar`): text `menu.pending` = "{0} not applied" (entries not `Applied` plus `menu_orphans.len()`) when that count is non-zero, else `menu.up_to_date`; Apply disabled with the reason (busy, store or config error, the first problem); no Discard in this PR.

- [ ] **Step 1: Write the failing headless tests** (in `context_menu_tab.rs`, with `TempIcons`):
  - `adding_an_entry_selects_it_and_autosaves` → list has 1 entry labelled with `menu.new_label`, selected, config saved after the autosave delay helper the other tests use.
  - `apply_from_the_page_writes_the_entry` → edit label to "Open with App", tick Background, click Apply → sandbox keys exist for Folder and Background, the row pill reads Applied.
  - `apply_is_disabled_with_the_problem_as_reason` → empty label → Apply disabled, hover text is `messages::entry_problem(EmptyLabel)`.
  - `remove_asks_first_and_apply_then_deletes_the_keys` → applied entry, `…` → Remove → `pending_confirm` is `RemoveMenuEntry`, confirm → list empty, keys still there, Apply → keys gone.
  - `the_windows_11_banner_opens_the_explorer_page` → click `menu.use_classic` → `active_tab == Tweaks`; with `is_windows_11 = false` the banner is not drawn.
- [ ] **Step 2: Run** `cargo test --locked context_menu_tab` — Expected: FAIL.
- [ ] **Step 3: Implement** the page as described.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(context-menu): My entries page with editor, states and apply"`.

### Task 6: History — operations filter and saved originals

**Files:** `src/ui/originals.rs`, `src/ui/history_panel.rs`, `src/ui/confirm.rs`, both `.resx`.

**Interfaces — Produces:** `HistoryFilter::ContextMenu` (matches `"Context menu"`, chip `history.module.context_menu`, after Start); `OriginalsModule::ContextMenu` placed after `Start` and before `Other`, title `history.module.context_menu`; its rows are this user's `menu_store` snapshots: title = the label of the listed entry whose key it is, else the key path; description `originals.saved_on`. `ConfirmAction::RestoreMenuKeys(Vec<String>)` → `start_menu_restore`. An unreadable `context-menu-backups.json` replaces only this group with its banner and disables its buttons, like the tweak groups.

- [ ] **Step 1: Write the failing tests** in `originals.rs`: `the_context_menu_group_lists_and_restores_after_confirm` (apply one entry, open History → group title and the entry label drawn; click Restore original → confirm pending; confirm → key gone, snapshot forgotten, history logs "Context menu"/"Restore"); `a_broken_menu_store_blocks_only_its_group`; in `history_panel.rs`: `the_context_menu_chip_filters_operations`.
- [ ] **Step 2: Run** `cargo test --locked originals` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(context-menu): History filter and saved originals group"`.

### Task 7: French check and docs

**Files:** `src/ui/shell.rs` (French test), `README.md`, `ARCHITECTURE.md`, `docs/registry-reference.md`.

- [ ] **Step 1: Extend `shell_pages_draw_no_english_only_text_in_french`** to draw the Context menu page with one applied entry, one entry with a problem, the Windows 11 banner and a selected entry's detail, and History with the Context menu group. Run `cargo test --locked shell_pages_draw_no_english_only_text_in_french` — Expected: PASS (add any missing French value until it does).
- [ ] **Step 2: Docs.** README: the module in the page list and a short "Context menu" section (what it adds, per account, "Show more options" on Windows 11, restore from History). ARCHITECTURE: the engine, the store file, the page (remove "shown disabled (coming soon)"). Registry reference: the written values per place.
- [ ] **Step 3: Run** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked` — Expected: all clean.
- [ ] **Step 4: Commit** — `git commit -m "docs(context-menu): README, architecture and French check"`.
