# Context menu module, PR 2: managing existing entries

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user hide well-known Windows right-click entries from a curated list, hide or rename entries other programs installed (found by a registry scan), all per user, snapshotted first and restorable from History.

**Architecture:** The snapshot/restore plumbing that `tweaks.rs` and `context_menu/engine.rs` both copy moves into a shared `src/backups.rs` (target, per-user store, slot snapshot/restore, the "save store then `.reg`" safety net, the restore loop). The context menu engine gains a read-only scan of HKCU and HKLM `Classes` (`existing.rs`), a static Windows catalog (`catalog.rs`), and one `apply_menu` that writes added entries and hide/rename changes in a single snapshot → persist → write pass. The page gets two new sections (Windows, Other programs) with toggles held as pending changes until Apply; the scan runs on a background job.

**Tech Stack:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59 (adds the `Win32_Storage_FileSystem` feature for version info), serde.

**Spec:** `docs/superpowers/specs/2026-11-25-context-menu-design.md` (sections 6.1, 6.3, 6.4, 6.5, 6.6, 6.7 and 7 item 2 are this PR). Real-machine results so far: `docs/registry-reference.md` § Context menu.

## Global Constraints

- Safety model (ARCHITECTURE.md): validate, snapshot on first touch, save the store **and** write a `.reg` export **before** any registry write; report every failure; an unreadable store blocks writing.
- Everything is per user: writes go only under `HKCU\Software\Classes\<class>\shell\<verb>` (a per-user copy of the verb key) and `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked`. **Nothing is written under HKLM.** HKLM is read only (scan).
- WinTune never deletes or renames another program's key. On an existing verb it only sets or removes `ProgrammaticAccessOnly` (REG_SZ `""`) and `MUIVerb` on the HKCU copy; on a handler it only sets or removes a `Blocked` value named `{CLSID}` (REG_SZ `""`).
- Hide on a static verb = `ProgrammaticAccessOnly` on the HKCU copy (documented; probe 2a PASS). Rename = `MUIVerb` on the HKCU copy (probe 2b PASS). Hide on a handler or modern entry = `Blocked` (undocumented → "Unofficial" pill on the row). Handlers and modern entries cannot be renamed.
- A rename label follows the entry label rules: 1 to 80 characters after trimming, no leading `@`, `&` written as `&&`.
- `context-menu-backups.json` written by PR 1 (bare key snapshots) must still load and restore; `tweaks-backups.json` must stay byte-compatible.
- Every UI text is a translation key used as a literal, present in `i18n/strings.resx` and `i18n/strings.fr.resx`; engine problems are enums translated in `i18n/messages.rs`.
- History keeps the English module label `Context menu` and the actions `Apply` / `Restore`.
- Tests never touch the real HKCU/HKLM outside the `HKCU\Software\WinTuneTest` sandbox; the scan's "machine" root is a sandbox subkey in tests.
- Kit only on the page: no colors, sizes or fonts set by hand.
- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --locked` pass after every task.

## Review Focus

1. **Show after hide leaves no trace.** Hiding an HKLM-only verb creates an HKCU copy; showing it again (or Restore original) must delete that copy, not leave an empty key that shadows the HKLM verb. (Task 4: `showing_again_deletes_the_copy_wintune_created`.)
2. **PR 1 stores keep working.** A `context-menu-backups.json` holding bare `RegistryKeySnapshot`s loads, lists in History and restores. (Task 1: `a_pr1_menu_store_still_loads_and_restores`.)
3. **Windows' own handlers are never offered as "Other programs".** "New", "Open with", Defender's scan entry, BitLocker verbs: hiding them by mistake would break Explorer. Anything whose file lives under the Windows folder is skipped unless it is in the catalog. (Task 3: `windows_own_entries_are_not_listed`.)
4. **A machine-wide hide can't be undone per user.** A CLSID in HKLM `Blocked` is shown "Hidden for every account" with a disabled toggle, and `apply_menu` refuses a change on it. (Tasks 3 and 4.)
5. **The scan never blocks the UI or runs under a write.** It runs on a background job, never while `is_busy()`, and the sections show a loading line meanwhile. (Task 5: `the_scan_waits_for_a_running_job`.)

---

## File Structure

- Create `src/backups.rs`: `RegTarget`, `Slot`, `Snapshot`, `Backup`, `UserBackups`, `take_snapshot`, `restore_snapshot`, `save_then_export`, `restore_ids`.
- Modify `src/tweaks.rs`: `TweakTarget`, `TweakSnapshot`, `TweakBackup`, `TweakBackupStore` become aliases of the shared types; `apply_tweaks` / `restore_tweaks` use the shared helpers.
- Modify `src/context_menu/engine.rs`: `MenuTarget { user: RegTarget, machine: RegTarget }`; `MenuBackup`/`MenuBackupStore` become aliases; `apply_menu`; `restore_keys` takes slot ids.
- Create `src/context_menu/catalog.rs`: the curated Windows list.
- Create `src/context_menu/existing.rs`: `Mechanism`, `ExistingEntry`, `scan`, `ExistingChange`.
- Create `src/context_menu/fileinfo.rs`: `resolve_indirect` (SHLoadIndirectString) and `product_name` (version info). Production only, injected into the scan.
- Create `src/ui/context_menu_existing.rs`: the Windows and Other programs sections, rename, pending changes.
- Modify: `Cargo.toml`, `src/lib.rs`, `src/context_menu/mod.rs`, `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/context_menu_tab.rs`, `src/ui/confirm.rs`, `src/ui/originals.rs`, `src/ui/home.rs`, `src/ui/polish.rs`, `src/ui/shell.rs` (French test), `src/ui/test_support.rs`, `src/i18n/messages.rs`, both `.resx`, `docs/registry-reference.md`, `README.md`, `ARCHITECTURE.md`.

---

### Task 1: Shared backup helpers

**Files:** Create `src/backups.rs`; modify `src/lib.rs`, `src/tweaks.rs`, `src/context_menu/engine.rs`, `src/context_menu/mod.rs`, every `MenuTarget { .. }` construction (`src/ui/mod.rs`, `src/ui/test_support.rs`, engine tests).

**Interfaces — Produces:**
- `pub struct RegTarget { pub root: RegKey, pub root_name: String, pub base: String }` with `pub fn current_user() -> Self`, `pub fn local_machine() -> Self` (HKLM, `"HKEY_LOCAL_MACHINE"`, empty base) and `pub fn path(&self, rel: &str) -> String` (same rule as today's `TweakTarget::path`). `pub type TweakTarget = RegTarget;`.
- `#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)] pub enum Slot { Key(String), Value { path: String, name: String } }` with `pub fn id(&self) -> String` (Key → the path; Value → `"{path}\\\\{name}"`, i.e. two backslashes, which never occur in a key path) and `pub fn from_id(id: &str) -> Slot` (split at the first `\\\\`).
- `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(tag = "kind", content = "data")] pub enum Snapshot { Value(ValueSnapshot), Key(RegistryKeySnapshot) }` — exactly today's `TweakSnapshot` (variant order and serde attributes unchanged). `pub type TweakSnapshot = Snapshot;`.
- `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct Backup { pub timestamp: String, #[serde(deserialize_with = "snapshot_or_bare_key")] pub snapshot: Snapshot }`; the deserializer accepts the tagged form or a bare `RegistryKeySnapshot` (PR 1 menu store) as `Snapshot::Key` (an `#[serde(untagged)]` helper enum: `Tagged(Snapshot)` first, `Bare(RegistryKeySnapshot)` second). `pub type TweakBackup = Backup; pub type MenuBackup = Backup;`.
- `#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] pub struct UserBackups { pub users: BTreeMap<String, BTreeMap<String, Backup>> }` with `backups_of(&self, user) -> Option<&BTreeMap<String, Backup>>`, `has_backup(&self, user, id) -> bool`, `record_first(&mut self, user: &str, id: &str, timestamp: &str, snapshot: Snapshot)` (keeps an existing entry), `forget(&mut self, user, id)` (drops the user when empty). `pub type TweakBackupStore = UserBackups; pub type MenuBackupStore = UserBackups;`.
- `pub fn take_snapshot(t: &RegTarget, slot: &Slot) -> Result<Snapshot, String>` and `pub fn restore_snapshot(t: &RegTarget, slot: &Slot, s: &Snapshot) -> Result<(), String>` (kind mismatch → `Err("the recorded snapshot does not match …")`), over `regsnap::{snapshot_key, snapshot_value, restore_key, restore_value}` with `t.path(..)`.
- `pub fn save_then_export(store: &UserBackups, store_path: &Path, t: &RegTarget, taken: &[(Slot, Snapshot)], reg_dir: &Path, stem: &str) -> Result<PathBuf, String>`: saves the store, then writes `<file_stamp>-<stem>.reg` with every key and value; errors are the existing sentences `Nothing applied: cannot save snapshots ({e})` / `Nothing applied: cannot write .reg backup ({e})`.
- `pub fn restore_ids(t: &RegTarget, store: &mut UserBackups, user: &str, ids: &[String], store_path: &Path, progress: &dyn Fn(usize, usize, &str), slot_of: &dyn Fn(&str) -> Result<Slot, String>, name_of: &dyn Fn(&str) -> String) -> OperationReport`: the loop both `restore_tweaks` and `restore_keys` run today (same messages: `no original state recorded for {user}`, `(snapshot kept, restore can be retried)`, `Cannot save snapshots: {e}`, final `Restore finished`).
- `MenuTarget` becomes `pub struct MenuTarget { pub user: RegTarget, pub machine: RegTarget }` with `current_user()` = `{ RegTarget::current_user(), RegTarget::local_machine() }`. Writes only ever use `target.user`.
- `restore_keys(target, store, ids, opts)` takes slot ids (`Slot::from_id`), so PR 1's key paths keep working unchanged.

- [ ] **Step 1: Write the failing tests** in `src/backups.rs`:
  - `slot_ids_round_trip`: `Slot::Key(r"Software\Classes\Directory\shell\A")` and `Slot::Value { path: BLOCKED.into(), name: "{ABC}".into() }` → `from_id(id())` equals the slot.
  - `tweak_store_json_is_unchanged`: a `UserBackups` with one `Snapshot::Value` backup serializes to the same JSON string as a fixture copied from today's `TweakBackupStore` output (write the fixture by hand in the test: `{"users":{"U":{"t":{"timestamp":"x","snapshot":{"kind":"Value","data":{…}}}}}}`).
  - `a_pr1_menu_store_still_loads_and_restores`: JSON with a bare `RegistryKeySnapshot` under `users.U["Software\\Classes\\Directory\\shell\\WinTune.e1"].snapshot` loads as `Snapshot::Key`; `restore_ids` on that id in a sandbox target recreates the key from the snapshot.
  - `record_first_keeps_the_oldest_snapshot` and `forget_drops_an_empty_user`.
- [ ] **Step 2: Run** `cargo test --locked backups` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `src/backups.rs`, then switch `tweaks.rs` and `engine.rs` to it (aliases above; `apply_tweaks` and `apply_entries` call `take_snapshot`, `record_first`, `save_then_export`; `restore_tweaks` and `restore_keys` call `restore_ids` with `slot_of` = the tweak's action slot / `Slot::from_id`). Delete the copies they replace.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS, with every existing tweak and menu test unchanged except `MenuTarget` construction.
- [ ] **Step 5: Commit** — `refactor: share snapshot store, safety net and restore loop between tweaks and context menu`.

### Task 2: The curated Windows catalog

**Files:** Create `src/context_menu/catalog.rs`; modify `src/context_menu/mod.rs`, `docs/registry-reference.md`, both `.resx`.

**Interfaces — Consumes:** `Place`, `tweaks::OsRequirement`.

**Interfaces — Produces:**
- `pub enum CatalogMechanism { Clsid(&'static str), Verbs(&'static [(&'static str, &'static str)]) }` (class path, verb name).
- `pub enum Presence { ClsidRegistered, TerminalInstalled, VerbExists }`.
- `pub struct CatalogItem { pub id: &'static str, pub name_key: &'static str, pub description_key: &'static str, pub also_affects_key: Option<&'static str>, pub mechanism: CatalogMechanism, pub places: &'static [Place], pub os: OsRequirement, pub presence: Presence }`; `pub fn catalog() -> &'static [CatalogItem]`; `pub fn find_catalog(id: &str) -> Option<&'static CatalogItem>`; `pub fn catalog_clsid(clsid: &str) -> Option<&'static CatalogItem>` (case-insensitive).

The catalog, from the read-only check on this machine (Windows 11 26300; CLSIDs read from `HKCR\CLSID`, places from the `shellex` keys that reference them, Terminal from its AppxManifest):

| id | Mechanism | Places | OS | Presence | Also affects |
| --- | --- | --- | --- | --- | --- |
| `open_in_terminal` | Clsid `{9F156763-7844-4DC4-B2B1-901F640F5155}` | Folder, Background | Any | TerminalInstalled | — |
| `share` | Clsid `{E2BF9676-5F8F-435C-97EB-11607A5BEDF7}` | Files, Folder | Any | ClsidRegistered | — |
| `give_access` | Clsid `{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}` | Files, Folder, Background, Drive | Any | ClsidRegistered | `menu.also.sharing_tab` "Also removes the Sharing tab from folder and drive Properties." |
| `previous_versions` | Clsid `{596AB062-B4D2-4215-9F74-E9109B0A8153}` | Files, Folder, Drive | Any | ClsidRegistered | `menu.also.previous_versions_tab` "Also removes the Previous Versions tab from Properties." |
| `include_in_library` | Clsid `{3DAD6C5D-2167-4CAE-9914-F99E41C12CFA}` | Folder | Any | ClsidRegistered | — |
| `pin_to_start` | Clsid `{470C0EBD-5D73-4D58-9CED-E91E22E23282}` | Folder | Any | ClsidRegistered | `menu.also.pin_to_start_apps` "Also removes Pin to Start from programs and shortcuts." |
| `send_to` | Clsid `{7BA4C740-9E81-11CF-99D3-00AA004AE837}` | Files, Folder | Any | ClsidRegistered | — |
| `cast_to_device` | Clsid `{7AD84985-87B4-4A16-BE58-8B72A5B390F7}` | Files | Any | ClsidRegistered | — |
| `scan_with_defender` | Clsid `{09A47860-11B0-4DA5-AFA5-26D86198A780}` | Files, Folder, Drive | Any | ClsidRegistered | — |
| `pin_to_quick_access` | Verbs `[("Folder","pintohome"), ("Drive","pintohome")]` | Folder, Drive | Any | VerbExists | — |
| `add_to_favorites` | Verbs `[("*","pintohomefile")]` | Files | Windows11 | VerbExists | — |

Names (en / fr): Open in Terminal / Ouvrir dans le Terminal; Share / Partager; Give access to / Donner l'accès à; Restore previous versions / Restaurer les versions précédentes; Include in library / Inclure dans la bibliothèque; Pin to Start / Épingler au menu Démarrer; Send to / Envoyer vers; Cast to device / Caster sur un appareil; Scan with Microsoft Defender / Analyser avec Microsoft Defender; Pin to Quick access / Épingler à Accès rapide; Add to Favorites / Ajouter aux favoris. Each gets a one-line description key `menu.win.<id>.desc`.

- [ ] **Step 1: Write the failing tests** in `catalog.rs`: `catalog_ids_are_unique_and_translated` (every `name_key`, `description_key`, `also_affects_key` exists in both `.resx`, via the existing resx loader used by `i18n::tests`); `clsids_are_braced_uppercase_guids` (regex `^\{[0-9A-F-]{36}\}$`); `catalog_clsid_matches_case_insensitively`.
- [ ] **Step 2: Run** `cargo test --locked catalog` — Expected: FAIL.
- [ ] **Step 3: Implement** the catalog and keys. In `docs/registry-reference.md` § Context menu add "Windows entries WinTune can hide": the table above with the evidence column (CLSID friendly name from `HKCR\CLSID`, the `shellex` keys found, Terminal manifest `desktop5:Verb Id="OpenTerminalHere"`), and add to "Needs the owner's eyes": each Clsid row hides in the open Explorer (or after restart), the Terminal and Share modern entries, the two Properties tabs, and every row on Windows 10.
- [ ] **Step 4: Run** `cargo test --locked catalog` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(context-menu): curated Windows entries with their CLSIDs and side effects`.

### Task 3: The scan

**Files:** Create `src/context_menu/existing.rs`, `src/context_menu/fileinfo.rs`; modify `Cargo.toml` (`Win32_Storage_FileSystem`), `src/context_menu/mod.rs`.

**Interfaces — Consumes:** Task 1 (`MenuTarget`, `UserBackups`, `Slot`), Task 2 (catalog).

**Interfaces — Produces:**
- `pub const BLOCKED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";`
- `pub const SCAN_CLASSES: [&str; 6] = ["*", "AllFilesystemObjects", "Folder", "Directory", r"Directory\Background", "Drive"];` and `pub fn class_places(class: &str) -> &'static [Place]`: `*` → Files; `AllFilesystemObjects` → Files, Folder; `Folder` → Folder, Drive; `Directory` → Folder; `Directory\Background` → Background; `Drive` → Drive.
- `#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)] pub struct VerbRef { pub class: String, pub verb: String }` with `pub fn user_key(&self) -> String` → `Software\Classes\<class>\shell\<verb>`.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum Mechanism { Verbs(Vec<VerbRef>), Clsid(String) }` with `pub fn slots(&self) -> Vec<Slot>` (each verb's `Slot::Key(user_key)`; Clsid → `Slot::Value { path: BLOCKED_KEY, name: clsid }`) and `pub fn renamable(&self) -> bool` (Verbs only).
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ExistingState { Shown, HiddenByWinTune, HiddenElsewhere }`; `#[derive(Debug, Clone, PartialEq, Eq)] pub enum Locked { MachineWide, Unreadable(String) }`.
- `#[derive(Debug, Clone, PartialEq)] pub struct ExistingEntry { pub id: String, pub label: String, pub original_label: Option<String>, pub catalog: Option<&'static str>, pub mechanism: Mechanism, pub places: BTreeSet<Place>, pub origin: Option<String>, pub state: ExistingState, pub locked: Option<Locked>, pub has_snapshot: bool }`.
- `#[derive(Debug, Clone, Default, PartialEq)] pub struct Scan { pub windows: Vec<ExistingEntry>, pub others: Vec<ExistingEntry> }` with `pub fn find(&self, id: &str) -> Option<&ExistingEntry>` and `pub fn by_slot(&self, slot_id: &str) -> Option<&ExistingEntry>`.
- `pub struct ScanOptions<'a> { pub user: &'a str, pub windows_dir: PathBuf, pub is_windows_11: bool, pub terminal_installed: bool, pub indirect: &'a dyn Fn(&str) -> Option<String>, pub product_name: &'a dyn Fn(&Path) -> Option<String> }`.
- `pub fn scan(target: &MenuTarget, store: &UserBackups, opts: &ScanOptions) -> Scan` — read-only.
- `fileinfo.rs`: `pub fn resolve_indirect(text: &str) -> Option<String>` (`SHLoadIndirectString`, buffer 512) and `pub fn product_name(path: &Path) -> Option<String>` (`GetFileVersionInfoSizeW` / `GetFileVersionInfoW` / `VerQueryValueW` on `\VarFileInfo\Translation`, then `\StringFileInfo\<lang><cp>\ProductName`; `None` on any failure).

Rules the scan follows (the implementer's tests pin each):
- **Merged view.** Per class, verb names are the union of the user and machine `shell` subkeys. A verb's effective values come from the HKCU copy when the value is set there, else from HKLM. Label = `MUIVerb`, else the key's default value, else the verb name; a leading `@` goes through `opts.indirect` (raw text when it returns `None`); accelerators removed (`&&` → `&`, single `&` dropped).
- **Origin file.** Verb: the program of `command`'s default value (first quoted token, or the text up to the first `.exe` inclusive; `%VAR%` expanded) or, without a command, `DelegateExecute`'s CLSID server. Handler: CLSID from the handler key's default value when it is a braced GUID, else from the key name; server = `Software\Classes\CLSID\<clsid>\InprocServer32` default (user then machine), env-expanded. `origin` = `opts.product_name(file)`, else the file stem; `None` when there is no file.
- **Skipped:** `WinTune.*` verbs; canonical verbs `open, opennew, explore, find, print, printto, runas, runasuser, openas, properties` (case-insensitive); everything the catalog covers (by CLSID or `(class, verb)`); anything whose origin file is under `opts.windows_dir` (case-insensitive prefix); a verb hidden by its installer (`ProgrammaticAccessOnly` or `LegacyDisable` effective) with no WinTune snapshot; a handler with no parseable CLSID.
- **Grouping.** Verbs with the same verb name (case-insensitive) and origin file form one row, id `verb:<name lowercase>|<origin path lowercase>`, `places` = union of `class_places`. Handlers with the same CLSID form one row, id `clsid:<CLSID uppercase>`, label = the CLSID's default value when not empty, else the trimmed handler key name. Catalog rows have id `win:<catalog id>`, label from translation (the UI translates `catalog`; `label` holds the English name).
- **State.** Clsid: value present in HKCU `Blocked` → hidden; present in the machine root's `Blocked` → hidden and `locked = MachineWide`. Verbs: hidden when every ref's HKCU copy has `ProgrammaticAccessOnly`. `has_snapshot` = the store has any of the mechanism's slot ids for `opts.user`. Hidden + snapshot → `HiddenByWinTune`; hidden without → `HiddenElsewhere`.
- `original_label` (Verbs only): the label computed from the HKLM key plus the snapshot's `MUIVerb` (if the snapshot holds the HKCU copy's original), i.e. what Reset name returns to.
- A key that exists but can't be opened for reading → the row is listed with `locked = Unreadable(error text)`.
- Catalog rows: listed only when present (`ClsidRegistered`: `Software\Classes\CLSID\<clsid>` in user or machine root; `TerminalInstalled`: `opts.terminal_installed`; `VerbExists`: any verb key in user or machine root) and the OS fits.

- [ ] **Step 1: Write the failing tests** in `existing.rs`, with a sandbox `MenuTarget` whose `user` and `machine` bases are `{sandbox}\HKCU` and `{sandbox}\HKLM`, `windows_dir` a temp `Windows` folder, `indirect` = `|_| None`, `product_name` = `|_| None`:
  - `a_third_party_verb_is_listed_once_with_its_places_and_origin`: machine `Directory\shell\AppVerb` (MUIVerb `Open with &App`, command `"<tmp>\App\app.exe" "%1"`) and `Directory\Background\shell\AppVerb` (same exe, `%V`) → one `others` row, label `Open with App`, places {Folder, Background}, origin `Some("app")`, state Shown, renamable.
  - `a_handler_is_grouped_by_clsid_and_named_from_its_clsid`: `*\shellex\ContextMenuHandlers\ 7-Zip` = `{23170F69-40C1-278A-1000-000100020000}` and the same under `Directory`, CLSID default `7-Zip Shell Extension`, InprocServer32 `<tmp>\7-Zip\7-zip.dll` → one row `clsid:{23170F69-…}`, label `7-Zip Shell Extension`, places {Files, Folder}.
  - `windows_own_entries_are_not_listed`: a handler whose server is `<windows_dir>\System32\shell32.dll` and a verb whose command is under `windows_dir` → `others` empty.
  - `canonical_wintune_and_installer_hidden_verbs_are_skipped`: `open`, `WinTune.e1`, and a verb with HKLM `ProgrammaticAccessOnly` → not listed.
  - `catalog_entries_go_to_windows_only_when_present`: machine `Software\Classes\CLSID\{596AB062-…}` exists and `AllFilesystemObjects\shellex\ContextMenuHandlers\{596AB062-…}` too → `windows` has `win:previous_versions`, `others` doesn't list it; `open_in_terminal` absent with `terminal_installed = false`; `add_to_favorites` absent with `is_windows_11 = false`.
  - `hidden_states_follow_blocked_and_the_store`: CLSID in sandbox HKCU `Blocked` without snapshot → `HiddenElsewhere`; with a store entry for its slot id → `HiddenByWinTune`; CLSID in sandbox HKLM `Blocked` → `locked == Some(MachineWide)`.
  - `an_hkcu_copy_renames_and_hides_a_machine_verb`: HKCU copy with `MUIVerb` `Renamed` → label `Renamed`; add `ProgrammaticAccessOnly` → hidden.
  - `labels_resolve_indirect_strings_and_drop_accelerators`: `indirect` returns `Some("Resolved &Name")` for `@x.dll,-1` → label `Resolved Name`; `A && B` → `A & B`.
- [ ] **Step 2: Run** `cargo test --locked existing` — Expected: FAIL.
- [ ] **Step 3: Implement** `existing.rs` and `fileinfo.rs` (no tests for `fileinfo.rs` beyond `product_name_of_a_file_without_version_info_is_none` on a temp file, and `product_name` of `%SystemRoot%\System32\cmd.exe` being `Some(_)`, marked `#[cfg(windows)]`).
- [ ] **Step 4: Run** `cargo test --locked context_menu` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(context-menu): read-only scan of existing verbs and handlers`.

### Task 4: Apply and restore hide/rename

**Files:** Modify `src/context_menu/engine.rs`, `src/context_menu/existing.rs`, `src/context_menu/mod.rs`.

**Interfaces — Consumes:** Tasks 1-3.

**Interfaces — Produces:**
- `#[derive(Debug, Clone, PartialEq)] pub struct ExistingChange { pub mechanism: Mechanism, pub hidden: bool, pub label: Option<String> }` — the wanted final state; `label: None` means the original name.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum RenameProblem { Empty, TooLong, LeadingAt }` and `pub fn rename_problem(label: &str) -> Option<RenameProblem>` (same limits as `entry_problems`).
- `pub fn apply_menu(target: &MenuTarget, store: &mut UserBackups, entries: &[MenuEntry], changes: &[ExistingChange], programs: &Programs, opts: &MenuOptions) -> OperationReport`. `apply_entries(..)` becomes `apply_menu(.., &[], ..)`.

Algorithm (one pass, the safety order of PR 1):
1. Validate: `entry_problems` as today; a change with a `label` that has a `rename_problem`, or a label on a Clsid change → refuse the whole apply (nothing written). A Clsid change whose value exists in `target.machine`'s `Blocked` → refuse the whole apply with `"{clsid}: hidden for every account; WinTune changes only yours"`.
2. Snapshot every slot touched (entry keys, orphans, `mechanism.slots()` of each change) with `take_snapshot` + `record_first`.
3. `save_then_export(.., stem = "context-menu-before-apply")`.
4. Write entries and orphans as today; then for each change:
   - **Clsid:** `hidden` → set `Blocked\<clsid>` = REG_SZ `""`; shown → delete that value (absent is fine).
   - **Verb ref (HKCU copy `K`):** create `K` if needed; `ProgrammaticAccessOnly` = `""` when hidden, else deleted; `MUIVerb` = `verb_text(label)` when `label` is set, else the snapshot's original `MUIVerb` if the snapshot is a `Key` that held one, else deleted. Then, when the recorded snapshot says `K` did not exist and `K` now has no values and no subkeys, delete `K`.
   - `report.succeeded` gets the change's report name: the label if set, else the first verb name or the CLSID.
- `restore_keys` already restores both kinds through `Slot::from_id` (Task 1).

- [ ] **Step 1: Write the failing tests** in `engine.rs` (sandbox `MenuTarget` with user and machine sides):
  - `hiding_a_machine_verb_writes_only_an_hkcu_copy`: machine `Directory\shell\AppVerb` with command → `apply_menu` hidden → user copy has `ProgrammaticAccessOnly` = `""` and nothing else; machine key unchanged; a `.reg` file exists.
  - `showing_again_deletes_the_copy_wintune_created`: then apply `hidden: false` → user copy key absent.
  - `rename_then_reset_puts_the_original_label_back`: user copy pre-exists with `MUIVerb` `Vendor` (per-user install) → apply `label: Some("Mine & Co")` → `MUIVerb` `Mine && Co` → apply `label: None` → `MUIVerb` `Vendor`.
  - `hide_and_rename_then_restore_is_exact`: hide + rename the machine-only verb, then `restore_keys` with its slot id → user copy absent, snapshot forgotten.
  - `blocking_a_clsid_and_restoring_it`: `apply_menu` Clsid hidden → value present in user `Blocked`; `restore_keys([slot id])` → value absent; with a pre-existing value (hidden elsewhere) → shown deletes it and restore puts it back.
  - `a_machine_wide_block_refuses_the_apply`: CLSID in machine `Blocked` → errors non-empty, nothing written, store file not created.
  - `a_bad_rename_blocks_the_whole_apply`: label `@x` → nothing written; `rename_problem` table: `""`/`"  "` → Empty, 81 chars → TooLong, `"@a"` → LeadingAt, 80 chars → None.
  - `entries_and_changes_share_one_reg_file`: one entry + one change → one `.reg` file in `reg_backup_dir` containing both paths.
- [ ] **Step 2: Run** `cargo test --locked context_menu::engine` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(context-menu): hide and rename existing entries with snapshots`.

### Task 5: App wiring — scan job, pending changes, one apply, History, Home

**Files:** `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/confirm.rs`, `src/ui/originals.rs`, `src/ui/home.rs`, `src/ui/test_support.rs`, `src/i18n/messages.rs`, both `.resx`.

**Interfaces — Consumes:** Tasks 1-4.

**Interfaces — Produces:**
- `AppEnv` gains `windows_dir: PathBuf` (`%SystemRoot%`; `TempIcons::env` uses a temp folder) and `menu_target` builds a `MenuTarget` with a sandbox `machine` side in tests (`{sandbox}\HKLM`). `TempIcons` gets `fn machine_key(&self) -> RegKey` for tests to seed HKLM-like keys.
- `WinTuneApp` gains `menu_scan: Option<Scan>` (None until the first scan ends), `menu_scan_job: Option<BackgroundJob<Scan>>`, `menu_scan_needed: bool`, `menu_pending: BTreeMap<String, PendingExisting>` (by `ExistingEntry::id`) with `#[derive(Debug, Clone, Default, PartialEq)] pub(super) struct PendingExisting { pub hidden: Option<bool>, pub label: Option<Option<String>> }`.
- `request_menu_scan(&mut self)` / `pump_menu_scan(&mut self, ctx)`, shaped exactly like `request_status_refresh` / `pump_status_refresh`: requested at startup, when the Context menu page opens, after every finished job; started only when `!is_busy()`; `ScanOptions` from `env.user_label`, `env.windows_dir`, `env.is_windows_11`, `env.programs.terminal.is_some()`, `fileinfo::resolve_indirect`, `fileinfo::product_name` (tests: `|_| None`). A panicked scan shows the toast `menu.scan_failed` "Could not read the right-click menu: {0}".
- `fn menu_existing_changes(&self) -> Vec<ExistingChange>`: for each pending id still in `menu_scan`, the entry's mechanism with `hidden` = pending or current (`state != Shown`) and `label` = pending or current override (`Some(label)` only when it differs from `original_label`); ids whose pending value equals the current state produce nothing.
- `start_menu_apply` runs `apply_menu(entries, menu_existing_changes())`; it clears `menu_pending` when the job finishes (success or not: the rescan shows the truth). Refused for the reasons PR 1 lists, plus any `rename_problem`.
- `ConfirmAction::ApplyMenu` — used instead of applying directly when a pending change hides a Clsid row whose catalog item has `also_affects_key`, or any `others` Clsid row: message lists each warning (catalog `also_affects` text; for other programs `menu.also.handler` "{0} is hidden everywhere its program adds it, not only in the places listed."), confirm label `menu.apply`.
- `discard_menu_changes(&mut self)` clears `menu_pending` only (never `config.json`).
- History: `menu_key_name` resolves a non-`WinTune.` slot id through `menu_scan.by_slot(id)` (catalog rows use their translated name), else the verb name or CLSID from the id; description is `originals.saved_on` with the timestamp. Restore goes through the existing `RestoreMenuKeys` flow.
- Home: `home.menu_status` becomes "{0} entries, {1} hidden" / "{0} entrée(s), {1} masquée(s)" with hidden = scan rows in `HiddenByWinTune` (0 until the first scan ends); drift unchanged.
- `messages::existing_state(tr, ExistingState) -> String` (Shown "Shown" / "Affichée", HiddenByWinTune "Hidden" / "Masquée", HiddenElsewhere "Hidden by another program" / "Masquée par un autre programme"), `messages::locked(tr, &Locked) -> String` (`menu.locked.machine` "Hidden for every account on this PC. WinTune changes only your account." / "Masquée pour tous les comptes de ce PC. WinTune ne modifie que votre compte."; `menu.locked.unreadable` "Cannot read this entry: {0}"), `messages::rename_problem(tr, &RenameProblem) -> String` (reuse the entry problem texts).

- [ ] **Step 1: Write the failing tests:**
  - `mod.rs`: `the_scan_runs_in_background_and_fills_menu_scan` (seed a machine verb; `request_menu_scan`, run frames until idle → `menu_scan.others.len() == 1`); `the_scan_waits_for_a_running_job` (hold a job with `hold_job`, request a scan → no scan job while busy; release → scan runs).
  - `jobs.rs`: `one_apply_writes_entries_and_hides` (one entry + pending hide on a seeded machine verb → after `run_until_idle` both keys exist, one history line "Context menu"/"Apply", `menu_pending` empty, rescan shows `HiddenByWinTune`); `discard_clears_pending_but_not_the_list`.
  - `confirm.rs` or `jobs.rs`: `hiding_give_access_asks_first_with_its_side_effect` (seed the catalog CLSID; pending hide on `win:give_access`; Apply → `pending_confirm` is `ApplyMenu` and its message contains the `menu.also.sharing_tab` text; nothing written until confirmed).
  - `originals.rs`: `a_hidden_entry_is_listed_by_its_name_and_restores` (after the apply above, the Context menu group shows the scanned label; Restore original → user copy gone).
  - `home.rs`: update `the_context_menu_tile_counts_entries_and_changes` to the new text (2 entries, 1 hidden after a hide).
- [ ] **Step 2: Run** `cargo test --locked ui::` — Expected: new tests FAIL.
- [ ] **Step 3: Implement**; add every key to both `.resx`.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(context-menu): scan job, pending hide/rename changes and one apply`.

### Task 6: The page — Windows and Other programs

**Files:** Create `src/ui/context_menu_existing.rs`; modify `src/ui/context_menu_tab.rs`, `src/ui/mod.rs`, `src/ui/polish.rs`, both `.resx`.

**Interfaces — Consumes:** Task 5's fields and methods; kit `section`, `section_with_action`, `SettingRow`, `pill`, `toggle_enabled`, `overflow_menu`, `empty_state`, `page_action_bar`, `dialog`, `dialog_buttons`.

**Interfaces — Produces:** `fn show_menu_windows_section(&mut self, ui: &mut egui::Ui)` and `fn show_menu_others_section(&mut self, ui: &mut egui::Ui)` called after "My entries"; `polish::search_box_id(ActiveTab::ContextMenu)` → `Some(egui::Id::new("search_menu_others"))`; `menu_rename: Option<(String, String)>` on the app (entry id, text being edited).

Content (spec 6.7):
- **Windows** (`menu.windows`): one `SettingRow` per `menu_scan.windows` row: translated name, places joined (`menu.place.*`), description, the `also_affects` line when set, an `Unofficial` pill (`tweaks.pill.unofficial`, already translated, `PillKind::Warning`) for Clsid rows, and for Clsid rows a secondary line `menu.restart_hint` "May need an Explorer restart to show." / "Peut demander un redémarrage de l'Explorateur." Toggle = shown (on) / hidden (off), reflecting pending over current; disabled with `messages::locked` when `locked`, and while busy. A row whose pending value differs from its current state shows an `PillKind::Accent` pill, like the tweak pages: `menu.pill.will_hide` "Will be hidden" / "Sera masquée", `menu.pill.will_show` "Will be shown" / "Sera affichée", or (label only) `menu.pill.will_rename` "Will be renamed" / "Sera renommée". `…` menu: Restore original when `has_snapshot`.
- **Other programs** (`menu.others`) with a search box (filters on label and origin, case-insensitive): rows show label, places, origin, the `Unofficial` pill for Clsid rows, state pill when `HiddenElsewhere`, toggle as above. `…` menu: Rename… (Verbs only), Reset name (when the current label differs from `original_label` or a rename is pending), Restore original (when `has_snapshot`). Rename… opens a kit `dialog` with a text field prefilled with the current label, the `rename_problem` in red under it, and OK (`menu.rename_ok`, disabled while a problem shows) / Cancel; OK stores `PendingExisting.label`.
- While `menu_scan` is `None` or a scan job runs: both sections show `menu.scanning` "Reading the right-click menu…" instead of rows. An empty Other programs list shows `empty_state` `menu.others_empty` "No other program adds entries here."
- Action bar: the pending count adds `menu_pending` entries that differ from the current state; **Discard** (`tweaks.discard`) shown and enabled when `menu_pending` is non-empty, calls `discard_menu_changes`; Apply as before (or via `ConfirmAction::ApplyMenu`).

- [ ] **Step 1: Write the failing headless tests** in `context_menu_existing.rs` (seeded sandbox, scan finished first):
  - `toggling_a_windows_entry_marks_it_pending_and_discard_clears_it`.
  - `apply_hides_a_third_party_verb_from_the_page` (toggle the seeded verb off, Apply, run until idle → user copy has `ProgrammaticAccessOnly`; after the rescan the toggle reads hidden).
  - `rename_from_the_menu_applies_the_new_label` (`…` → Rename… → type `Mine` → OK → Apply → user copy `MUIVerb` `Mine`).
  - `rename_ok_is_disabled_for_a_leading_at`.
  - `a_machine_wide_block_shows_a_disabled_toggle_with_the_reason` (hover text equals `messages::locked(MachineWide)`).
  - `ctrl_f_focuses_the_other_programs_search` and `search_filters_by_origin`.
  - `sections_show_the_loading_line_until_the_scan_ends`.
- [ ] **Step 2: Run** `cargo test --locked context_menu_existing` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(context-menu): Windows and Other programs sections with hide and rename`.

### Task 7: French check, docs, owner checklist

**Files:** `src/ui/shell.rs` (French test), `README.md`, `ARCHITECTURE.md`, `docs/registry-reference.md`.

- [ ] **Step 1: Extend `shell_pages_draw_no_english_only_text_in_french`** to draw the Context menu page with a seeded catalog row (with its "also affects" line), a third-party verb, a machine-wide blocked handler (disabled toggle), a pending change (Discard visible), the rename dialog open, and History with a hidden entry's original. Run `cargo test --locked shell_pages_draw_no_english_only_text_in_french` — Expected: PASS (add missing French values until it does).
- [ ] **Step 2: Docs.** README § Context menu: the Windows and Other programs sections, hide = per account, Unofficial rows and the restart hint, rename for classic entries only, side-effect warnings. ARCHITECTURE: `src/backups.rs` shared by tweaks and context menu; the scan and its background job; the two new write locations. Registry reference: "What WinTune writes" for hide (`ProgrammaticAccessOnly` on the HKCU copy, deleted again when shown if WinTune created the copy), rename (`MUIVerb`, `&&`), `Blocked` values; and a **pre-merge checklist for PR 2**: on Windows 11 (and Windows 10 if available) hide then show each Windows row and one third-party verb and handler, in the open Explorer and after an Explorer restart; rename a third-party verb; check the Sharing and Previous Versions tabs disappear with their rows and come back on Show; Restore original from History for each; record anything that differs before merging.
- [ ] **Step 3: Run** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked` — Expected: all clean.
- [ ] **Step 4: Commit** — `docs(context-menu): existing entries, catalog evidence and the PR 2 checklist`.

## Not in this PR

- **"Changed outside WinTune" for hidden entries** (spec 6.3, last bullet). The store keeps only the original state, not what WinTune last wrote, so a marker removed by a program's updater can't be told apart from a Show. The row then simply reads "Shown" and can be hidden again. Recording the wanted state per slot would fix it; owner decision.
- Third-party packaged (MSIX) entries and Edit with Photos / Clipchamp (spec non-goal; their CLSIDs were not confirmed).
- Machine-wide (HKLM) hides: the probe showed none are needed (spec section 8 answered).
