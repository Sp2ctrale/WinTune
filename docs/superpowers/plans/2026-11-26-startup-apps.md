# Startup apps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Startup apps" page that lists the programs started at sign-in (the three `Run` keys and the two Startup folders) and turns each one off or on for this user, through the same per-user `StartupApproved` switch as Task Manager. Changes are snapshotted first and can be undone from History.

**Architecture:** A new `src/startup/` engine is built like `context_menu/existing.rs` + `engine.rs`. `mod.rs` holds the sources, the switch format and the per-source flag table. `scan.rs` is a read-only scan; it gives one row per `StartupApproved` value. `engine.rs` runs one Apply pass (validate → snapshot → save store + `.reg` → write) and a Restore built on `backups::restore_ids`. The UI copies the context menu's "existing entries" pattern: a background scan job, pending choices until Apply, one `RunningJob` variant, a Saved originals group, a Home tile. The `Slot` id of the approved value is the join key between row, pending choice, store and History.

**Tech Stack:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59 (existing features only: `GetSystemTimeAsFileTime` is in `Win32_System_SystemInformation`, `SetFileAttributesW` in `Win32_Storage_FileSystem`), serde. No new crates.

**Spec:** `docs/superpowers/specs/2026-11-26-startup-apps-design.md` (binding). Windows evidence is in its section 3. The owner's real-machine checklist is its section 7.

## Global Constraints

- **Writes:** WinTune writes only values under `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{Run,Run32,StartupFolder}`. **Nothing is written under HKLM.** It never edits or deletes a `Run` value, and never deletes, moves, renames or edits a file in a Startup folder.
- **Switch format (spec 3.2):** `REG_BINARY`. The first 4 bytes are a little-endian DWORD flag, followed by a FILETIME. A flag of 2 or 6 means on; 3 or 7 means off. No value means on. Turn off = `03 00 00 00` + current FILETIME (12 bytes). Turn on = the snapshot's original put back exactly if it was on (absent, 2 or 6), else `02 00 00 00` + 8 zero bytes.
- **Safety model (ARCHITECTURE.md):** validate (any problem refuses the whole Apply, nothing written), snapshot on first touch (`record_first`), save the store **and** export a `.reg` **before** any write, report every failure. An unreadable `startup-backups.json` blocks Apply and Restore. After Apply, a slot back at its original (same presence, same type and bytes) is forgotten.
- **Per-user switch flags (spec 3.3):** one constant table, `startup::PER_USER_SWITCH_WORKS`, holds the flags. Initial values: `MachineRun` true, `CommonFolder` true, `MachineRun32` false. Per-user sources are always true. The owner's checklist outcome must be a one-line change to that table, and no test may pin the all-users values.
- **Reads:** Run values are read raw and unexpanded (`REG_SZ` or `REG_EXPAND_SZ`). They are expanded only to find the program file. The 32-bit key is read by path, `SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run`, never through `KEY_WOW64_32KEY`.
- **Tests:** tests touch only `HKCU\Software\WinTuneTest` (`regsnap::test_support::Sandbox`, user side `{sandbox}\HKCU`, machine side `{sandbox}\HKLM`) and temp folders. Never launch `WinTune.exe`. UI tests live in `src/ui/*.rs` and use `TempIcons`.
- **Texts:** engine states and lock reasons the page shows are enums, translated in `src/i18n/messages.rs`. Every UI text is a literal key (`tr.t("startup.x")`) present in `i18n/strings.resx` and `i18n/strings.fr.resx`. `OperationReport` detail lines stay English sentences, as in the other engines.
- **History:** the module label is `Startup apps`; the actions are `Apply` and `Restore`. The store is `startup-backups.json` in `models::app_data_dir()`.
- **Page:** kit only, with no colors, sizes or fonts set by hand. Nav entry "Startup apps" goes in the Customize group, after Context menu.
- **Editing files:** Bash heredocs mangle backslashes on this machine, so write and edit files with the Write and Edit tools only.
- **Commits:** the commit messages carry no attribution lines.
- **CI:** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` and `cargo build --release --locked` must pass after every task. `rtk bypass cargo test --locked` shows the full result lines.

## Review Focus

1. **A fresh account has no `StartupApproved\<sub>` key.** Turn off must create the key. Turn on and Restore must delete only the value. (Task 3: `turning_off_writes_three_and_the_time_and_keeps_the_original`.)
2. **Names match case-insensitively, as the registry and NTFS do.** Run `OneDrive` with approved `onedrive`, and `Discord.lnk` beside `discord.LNK`, must each give one row with the right state. (Task 2: `names_match_case_insensitively`.)
3. **Commands Windows accepts but a naive parser misreads.** These are unquoted paths with spaces plus arguments, `%SystemRoot%` in a `REG_EXPAND_SZ`, and bare `ms-teams.exe`. Each must give the right program and no false "Program not found". (Task 2: `commands_windows_accepts_are_parsed_without_false_alarms`.)
4. **The registry changes between the scan and Apply.** If the switch is now an unknown shape, or HKLM now turns the item off, the whole Apply is refused, nothing is written, and the choice stays pending. (Task 3: `the_apply_is_refused_when_the_registry_changed_since_the_scan`; Task 4: `a_refused_startup_apply_keeps_the_choice`.)
5. **Task Manager turns the item back on after WinTune turned it off.** The row reads On and keeps its snapshot. Turning it off again keeps the first original. Restore puts back the exact first bytes. (Task 3: `turned_back_on_elsewhere_keeps_the_first_original`.)

---

## File Structure

- Create `src/startup/mod.rs`: key constants, `Source`, `Approved`, `PER_USER_SWITCH_WORKS`, `Switch` + `read_switch`, `off_value` / `on_value` / `now_filetime`, `StartupTarget`, re-exports.
- Create `src/startup/scan.rs`: `StartupItem`, `StartupScan`, `StartupState`, `StartupLock`, `Origin`, `ScanOptions`, `scan`.
- Create `src/startup/engine.rs`: the store helpers, `StartupChange`, `StartupOptions`, `apply_startup`, `restore_startup`.
- Create `src/ui/startup_state.rs`: the scan job, the pending choices, `startup_change`, `startup_slot_name`.
- Create `src/ui/startup_tab.rs`: the page.
- Modify:
  - `src/lib.rs`;
  - `src/context_menu/mod.rs` and `src/context_menu/origin.rs` (visibility only);
  - in `src/ui/`: `mod.rs`, `jobs.rs`, `confirm.rs`, `originals.rs`, `history_panel.rs`, `home.rs`, `shell.rs`, `polish.rs`, `kit/icons.rs` and `test_support.rs`;
  - `src/i18n/messages.rs` and `src/i18n/tests.rs`;
  - both `.resx` files;
  - `README.md`, `ARCHITECTURE.md` and `docs/registry-reference.md`.

---

### Task 1: Sources, the switch format and the flag table

**Files:**
- Create: `src/startup/mod.rs`
- Modify: `src/lib.rs` (`pub mod startup;`)

**Interfaces — Produces:**
- `pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";`
- `pub const RUN32_KEY: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";`
- `pub const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";`
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub enum Source { UserRun, MachineRun, MachineRun32, UserFolder, CommonFolder }` with:
  - `pub const ALL: [Source; 5]`;
  - `pub fn all_users(self) -> bool`: true for `MachineRun`, `MachineRun32` and `CommonFolder`;
  - `pub fn approved(self) -> Approved`: the user and machine Run sources map to `Run`, `MachineRun32` to `Run32`, and both folders to `StartupFolder`;
  - `pub fn per_user_switch_works(self) -> bool`: a lookup in the table below.
- The table, a doc comment above it saying that the pre-merge checklist in `docs/registry-reference.md` decides each all-users value, and that a failed check sets it to `false`:
  ```rust
  pub const PER_USER_SWITCH_WORKS: [(Source, bool); 5] = [
      (Source::UserRun, true),
      (Source::MachineRun, true),
      (Source::MachineRun32, false),
      (Source::UserFolder, true),
      (Source::CommonFolder, true),
  ];
  ```
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub enum Approved { Run, Run32, StartupFolder }` with:
  - `pub fn key(self) -> String`: `APPROVED_KEY\Run`, `\Run32` or `\StartupFolder`;
  - `pub fn id_prefix(self) -> &'static str`: `"run"`, `"run32"` or `"folder"`;
  - `pub fn slot(self, name: &str) -> Slot`: `Slot::Value { path: self.key(), name }`;
  - `pub fn row_id(self, name: &str) -> String`: `"{prefix}:{name lowercase}"`.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Switch { Absent, On(u32), Off(u32), Unknown }` with `pub fn is_off(self) -> bool`.
- `pub fn read_switch(v: &ValueSnapshot) -> Switch`, with these rules:
  - `!exists` gives `Absent`;
  - a `kind` other than 3 (`REG_BINARY`), or data shorter than 4 bytes, gives `Unknown`;
  - a flag of 2 or 6 gives `On`, a flag of 3 or 7 gives `Off`, any other flag gives `Unknown`.
- `pub fn off_value(filetime: u64) -> ValueSnapshot` (exists, kind 3, `03 00 00 00` + `filetime` LE) and `pub fn on_value() -> ValueSnapshot` (`02 00 00 00` + 8 zeros).
- `pub fn now_filetime() -> u64`: `GetSystemTimeAsFileTime`.
- `pub struct StartupTarget { pub user: RegTarget, pub machine: RegTarget, pub user_folder: Option<PathBuf>, pub common_folder: Option<PathBuf> }`. Writes only ever use `user`.
  - `pub fn current_user() -> Self`: HKCU/HKLM.
  - The folders: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup` and `%ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp`. A missing variable gives `None`.

- [ ] **Step 1: Write the failing tests** in `src/startup/mod.rs`:
  - `switch_reads_every_known_shape`, a table test:
    - absent → `Absent`;
    - `REG_BINARY` `02`+8 zeros → `On(2)`; `06…` → `On(6)`;
    - `03`+time → `Off(3)`; `07…` → `Off(7)`;
    - `03 00 00 00` with 4 bytes only → `Off(3)`;
    - 3 bytes → `Unknown`;
    - `REG_DWORD` with data `03 00 00 00` → `Unknown`;
    - flags 0, 1 and 4 → `Unknown`.
  - `off_and_on_values_are_twelve_bytes`: `off_value(0x01DB_1234_5678_9ABC)` has data `[3,0,0,0, 0xBC,0x9A,0x78,0x56,0x34,0x12,0xDB,0x01]` and kind 3; `on_value().data == [2,0,0,0,0,0,0,0,0,0,0,0]`.
  - `every_source_has_one_flag_and_per_user_sources_work`: each `Source::ALL` element appears exactly once in `PER_USER_SWITCH_WORKS`, and `UserRun` and `UserFolder` are true. Do not assert the all-users values.
  - `sources_map_to_their_approved_value`:
    - the `approved()` table;
    - `Approved::Run32.key() == format!(r"{APPROVED_KEY}\Run32")`;
    - `Approved::StartupFolder.row_id("Discord.LNK") == "folder:discord.lnk"`;
    - `Approved::Run.slot("X").id()` ends with `\\X`, with two backslashes.
  - `now_filetime_is_after_2020`: `> 132_223_104_000_000_000`.
- [ ] **Step 2: Run** `cargo test --locked startup::` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** the items above in `src/startup/mod.rs`, then add `pub mod startup;` to `src/lib.rs`.
- [ ] **Step 4: Run** `cargo test --locked startup::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(startup): sources, StartupApproved switch format and per-source flags`.

### Task 2: The read-only scan

**Files:**
- Create: `src/startup/scan.rs`
- Modify:
  - `src/startup/mod.rs` (`mod scan; pub use scan::*`);
  - `src/context_menu/mod.rs` (`pub(crate) mod origin;`);
  - `src/context_menu/origin.rs` (`expand_env` and `command_program` become `pub(crate)`, unchanged otherwise).

**Interfaces — Consumes:** Task 1; `backups::UserBackups::find_id`; `regsnap::snapshot_value`; `context_menu::fileinfo::decode_utf16le`; `context_menu::origin::command_program`.

**Interfaces — Produces:**
- `#[derive(Debug, Clone, PartialEq, Eq)] pub struct Origin { pub source: Source, pub detail: String }`. `detail` is the Run command as stored (unexpanded), or the file's full path.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum StartupState { On, Off, OffElsewhere }`.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum StartupLock { Unreadable(String), UnknownShape, OffForEveryone, AllUsersOnly }`.
- `#[derive(Debug, Clone, PartialEq)] pub struct StartupItem`, with these fields:
  - `pub id: String`;
  - `pub approved: Approved`;
  - `pub name: String`: the approved value name, spelled as in the first origin;
  - `pub display_name: String`;
  - `pub program: String`;
  - `pub origins: Vec<Origin>`, in `Source` order;
  - `pub program_missing: bool`;
  - `pub state: StartupState`;
  - `pub locked: Option<StartupLock>`;
  - `pub has_snapshot: bool`.

  It has `pub fn slot(&self) -> Slot` (`approved.slot(&name)`), `pub fn all_users(&self) -> bool` (any origin) and `pub fn sources(&self) -> Vec<Source>`.
- `#[derive(Debug, Clone, Default, PartialEq)] pub struct StartupScan { pub items: Vec<StartupItem> }` with `pub fn find(&self, id: &str) -> Option<&StartupItem>` and `pub fn by_slot(&self, slot_id: &str) -> Option<&StartupItem>` (case-insensitive).
- `pub struct ScanOptions<'a> { pub user: &'a str, pub product_name: &'a dyn Fn(&Path) -> Option<String>, pub switch_works: fn(Source) -> bool }`.
- `pub fn scan(target: &StartupTarget, store: &UserBackups, opts: &ScanOptions) -> StartupScan`.

Rules the scan follows (each one is pinned by a test below):
- **Run sources:**
  - `UserRun` = `user` + `RUN_KEY`; `MachineRun` = `machine` + `RUN_KEY`; `MachineRun32` = `machine` + `RUN32_KEY`.
  - Only named values of type `REG_SZ` (1) or `REG_EXPAND_SZ` (2) count. They are decoded raw with `decode_utf16le`, never expanded.
  - The default value and other types are skipped.
  - A missing or unreadable Run key gives no rows.
- **Folder sources:**
  - `read_dir` on each `Some` folder; a missing folder gives no rows.
  - Only regular files are listed. Subfolders, `desktop.ini` (case-insensitive) and files with `FILE_ATTRIBUTE_HIDDEN` (0x2) or `FILE_ATTRIBUTE_SYSTEM` (0x4) are skipped (`MetadataExt::file_attributes`).
  - Name = the file name with its extension; display name = the file stem.
- **One row per approved value:**
  - Rows are grouped by `(source.approved(), name lowercase)`, so `UserRun` + `MachineRun` sharing a name, or the two folders sharing a file name, make one row with two origins.
  - The row's id is `row_id`; its `display_name` is the value name, or the file stem.
  - Approved values with no item (orphans) are never listed.
- **Program:**
  - **Run:** `command_program(detail)` gives a path. `program` = `product_name(path)`, else the path's file stem. `program_missing` = the path contains `\` or `/` and does not exist; a bare name such as `ms-teams.exe` is never flagged.
  - **Folder:** a `.lnk` gives its file stem (the target is not resolved). Any other file gives `product_name(file)`, else the stem, with `program_missing` false.
  - **Pair rows:** the program comes from the first origin; `program_missing` is true if any origin's program is missing.
- **State:**
  - `user_sw` = `read_switch` of `snapshot_value(user, approved.key(), name)`; `machine_sw` is the same read on `machine`.
  - `has_snapshot` = `store.find_id(opts.user, &slot.id()).is_some()`.
  - The item is off when `user_sw.is_off() || machine_sw.is_off()`. Off with a snapshot gives `Off`, off without one gives `OffElsewhere`, and anything else gives `On`.
- **Lock, first match wins:**
  1. either read fails: `Unreadable(error text)`;
  2. either switch is `Unknown`: `UnknownShape`;
  3. `machine_sw.is_off()`: `OffForEveryone`;
  4. any origin source with `!(opts.switch_works)(source)`: `AllUsersOnly`.
- **Order:** rows are sorted by `display_name` lowercase, then by `id`.

Test setup: a sandbox `StartupTarget` whose `user`/`machine` bases are `{sandbox}\HKCU` and `{sandbox}\HKLM`, with folders `<tmp>\user` and `<tmp>\common` (created). `product_name` = `|_| None` unless stated; `switch_works` = `|s| s != Source::MachineRun32`.

- [ ] **Step 1: Write the failing tests** in `src/startup/scan.rs`:
  - `run_values_become_rows_with_their_command_and_program`:
    - Setup: user Run `Tool` = `"<tmp>\Tool\tool.exe" --min` (the file exists), and `product_name` returns `Some("Tool Pro")` for that path.
    - Expect one item: `id == "run:tool"`, `display_name == "Tool"`, `program == "Tool Pro"`, `origins == [Origin { source: UserRun, detail: <the exact command> }]`, `state == On`, `locked == None`, `!program_missing`, `!all_users()`.
  - `commands_windows_accepts_are_parsed_without_false_alarms`:
    - `Parsec` = `<tmp>\Par sec\parsecd.exe app_silent=1`, unquoted (the file exists) → program `parsecd`, not missing.
    - `Sec` as `REG_EXPAND_SZ` `%SystemRoot%\System32\cmd.exe` → `detail` still contains `%SystemRoot%`, program `cmd`, not missing.
    - `Teams` = `ms-teams.exe msteams:system-initiated` → program `ms-teams`, not missing.
    - `Gone` = `"<tmp>\gone\app.exe" -x` → `program_missing`.
  - `a_value_in_both_run_keys_is_one_row_for_everyone`: user Run `X` and machine Run `X` give one item with origins `[UserRun, MachineRun]`, `all_users()`, and `locked == None`.
  - `names_match_case_insensitively`:
    - user Run `OneDrive` plus user approved `Run\onedrive` = `03`+time → state `OffElsewhere`, `name == "OneDrive"`;
    - `Discord.lnk` in `user` and `discord.LNK` in `common` → one `folder:discord.lnk` item with two origins.
  - `states_follow_the_switch_and_the_store`:
    - absent → `On`; `02` → `On`; `06` → `On`;
    - `03` with no store entry → `OffElsewhere`;
    - `03` with a store entry for `slot().id()` under `"TEST\\user"` → `Off`, `has_snapshot`;
    - `02` with a store entry → `On`, `has_snapshot`.
  - `locks_say_why`:
    - user approved value of type `REG_DWORD` → `UnknownShape`;
    - flag `01` → `UnknownShape`;
    - machine approved `Run\M` = `03`+time for a machine Run item `M` → `OffForEveryone` and state not `On`;
    - a machine Run32 item → `AllUsersOnly`;
    - a machine Run item with no approved value → `None`.
  - `run32_items_use_the_run32_switch`: machine `RUN32_KEY` value `Old` → `id == "run32:old"`, `approved == Run32`. An approved `Run\Old` = `03` does not make it off; an approved `Run32\Old` = `03` does.
  - `startup_folders_skip_hidden_system_and_desktop_ini`:
    - Setup: `user` holds `desktop.ini`, `hidden.lnk` (made hidden with `SetFileAttributesW`), a subfolder `sub`, `App.lnk` and `script.bat`.
    - Expect display names `["App", "script"]`. `App`'s detail is the full path of `App.lnk` and its program is `App`.
    - `user_folder: None` and a non-existent `common_folder` → no panic, no extra rows.
  - `orphan_switches_and_odd_run_values_are_ignored`: approved `Run\Ghost` = `02` with no Run value, a Run value of type `REG_DWORD`, and a default value `""` → no rows.
  - `rows_are_sorted_by_name`: `b`, `A` and `c` → `["A", "b", "c"]`.
- [ ] **Step 2: Run** `cargo test --locked startup::scan` — Expected: FAIL.
- [ ] **Step 3: Implement** `scan.rs` and the two visibility changes in `context_menu`.
- [ ] **Step 4: Run** `cargo test --locked startup` and `cargo test --locked context_menu` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(startup): read-only scan of Run keys and Startup folders`.

### Task 3: Apply and restore

**Files:**
- Create: `src/startup/engine.rs`
- Modify: `src/startup/mod.rs` (`mod engine; pub use engine::*`)

**Interfaces — Consumes:**
- Tasks 1 and 2.
- From `backups`: `take_snapshot`, `restore_snapshot`, `save_then_export`, `restore_ids`, `UserBackups::{find_id, record_first, forget, backups_of}`.
- `regsnap::{snapshot_value, restore_value}`.
- `associations::OperationReport`.

**Interfaces — Produces:**
- `pub type StartupBackupStore = backups::UserBackups;`
- `pub fn default_startup_store_path() -> PathBuf`: `app_data_dir().join("startup-backups.json")`.
- `pub fn load_startup_store(path: &Path) -> Result<StartupBackupStore, crate::persist::LoadError>` and `pub fn save_startup_store(store: &StartupBackupStore, path: &Path) -> Result<(), String>`. Both work like `load_menu_store` and `save_menu_store`.
- `#[derive(Debug, Clone, PartialEq)] pub struct StartupChange { pub approved: Approved, pub name: String, pub on: bool, pub sources: Vec<Source> }`.
  - `pub fn slot(&self) -> Slot`.
  - `pub fn from_item(item: &StartupItem, on: bool) -> Self`.
- `pub struct StartupOptions<'a>` with these fields:
  - `pub user: String`;
  - `pub store_path: PathBuf`;
  - `pub reg_backup_dir: PathBuf`;
  - `pub progress: &'a dyn Fn(usize, usize, &str)`;
  - `pub switch_works: fn(Source) -> bool`;
  - `pub now: u64`: the FILETIME a turn-off writes.
- `pub fn apply_startup(target: &StartupTarget, store: &mut StartupBackupStore, changes: &[StartupChange], opts: &StartupOptions) -> OperationReport`.
- `pub fn restore_startup(target: &StartupTarget, store: &mut StartupBackupStore, ids: &[String], opts: &StartupOptions) -> OperationReport`. It calls `restore_ids` with `restore_snapshot` on `target.user`, `slot_of` = `Ok(Slot::from_id(id))`, and `name_of` = the value name taken from the id.

Algorithm of `apply_startup` (one pass, the context menu's safety order):
1. **Validate every change, writing nothing.** Each problem below adds one `"{name}: …"` line to `report.errors`; if any line exists, return.
   - an empty `name`;
   - a source with `!(opts.switch_works)(s)`: "turned on for every account; WinTune cannot change it for your account alone";
   - the user switch cannot be read, or is `Unknown`;
   - the machine switch cannot be read, is `Unknown`, or is off: "turned off for every account; WinTune changes only yours".
   - Two changes on the same slot id (case-insensitive): only the first counts.
2. **Snapshot each slot** on `target.user` with `take_snapshot`. Keep a stored spelling via `find_id`, then call `record_first`. A failed snapshot adds an error and skips that change. Return if nothing was snapshotted.
3. **Save the store and export.** Call `save_then_export(store, &opts.store_path, &target.user, &taken, &opts.reg_backup_dir, "startup-before-apply")`. On `Err`, push the error and return. On `Ok`, set `reg_backup_file`.
4. **Write.**
   - Off: `restore_value(user root, user.path(approved.key()), name, &off_value(opts.now))`. This creates the key when it is missing.
   - On: read the recorded original. If `read_switch(original)` is `Absent` or `On(_)`, put it back with `restore_snapshot`. Otherwise write `on_value()`.
   - Each success pushes `name` to `succeeded`; each failure pushes `"{name}: {e}"`.
5. **Forget slots back at their original.** Re-read each written slot. If it matches its recorded original (`exists` is equal, and when it exists `kind` and `data` are too), `forget` it. If anything was forgotten, save the store; a failed save gives `Cannot save snapshots: {e}`.

Test setup: the same sandbox target as Task 2; `now = 0x01DB_0000_0000_0001`; the store and `.reg` files go in a temp folder.

- [ ] **Step 1: Write the failing tests** in `src/startup/engine.rs`:
  - `turning_off_writes_three_and_the_time_and_keeps_the_original`:
    - Setup: user Run `App`, and no `StartupApproved` key at all.
    - Apply off. The user value `StartupApproved\Run\App` is then `REG_BINARY` `03 00 00 00` + `now` LE (12 bytes).
    - The store holds `Approved::Run.slot("App").id()` with `Snapshot::Value { exists: false, .. }`.
    - One `.reg` file exists in the reg folder, and the Run value reads back the same string.
  - `turning_on_again_deletes_the_value_and_forgets_the_snapshot`: after the test above, apply on → the value is absent and the store file holds no entry for the user.
  - `turning_on_an_item_off_elsewhere_writes_two_and_restore_puts_the_original_back`:
    - Setup: an approved value `03`+`T0`.
    - Apply on → `02`+8 zeros, and the snapshot is kept.
    - `restore_startup([slot id])` → exactly `03`+`T0`, and the slot is forgotten.
  - `an_original_six_comes_back_exactly`: original `06`+zeros → off → on → exactly the original bytes, and the slot is forgotten.
  - `turned_back_on_elsewhere_keeps_the_first_original`:
    - Setup: user Run `App`, no switch. Apply off.
    - The test writes `02`+zeros itself, playing Task Manager. Apply off again.
    - The stored snapshot is still `exists: false`. `restore_startup` → the value is absent.
  - `the_apply_is_refused_when_the_registry_changed_since_the_scan`:
    - Setup: user approved `Run\Bad` as a `REG_DWORD`. Apply off for `Bad` and for a valid `Good`.
    - `errors` is non-empty. Neither value is written, no store file and no `.reg` file are created, and `succeeded` is empty.
  - `machine_off_or_all_users_only_refuses`:
    - machine approved `Run\M` = `03` → refused;
    - a change with `sources: vec![MachineRun32]` and `switch_works = |s| s != MachineRun32` → refused.
  - `only_hkcu_is_written`:
    - Setup: a machine Run item `M`, and the file `App.lnk` in the common folder.
    - Apply off for both → the machine approved key has no values, and the machine Run value is unchanged.
    - `App.lnk` still exists with the same bytes, and the user value `StartupApproved\StartupFolder\App.lnk` is `03`+`now`.
  - `the_store_round_trips`: a missing file loads as the default store; save then load gives the same store.
  - `restore_without_a_snapshot_reports_it`: `restore_startup` on an unknown id → one error containing `no original state recorded`.
- [ ] **Step 2: Run** `cargo test --locked startup::engine` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked startup` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(startup): turn startup apps off and on through StartupApproved, with snapshots`.

### Task 4: App wiring: scan job, pending choices, apply and restore jobs

**Files:**
- Create: `src/ui/startup_state.rs`
- Modify:
  - `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/confirm.rs`, `src/ui/test_support.rs`;
  - `src/i18n/messages.rs`, `src/i18n/tests.rs`;
  - both `.resx` files.

**Interfaces — Consumes:** Tasks 1-3; `tasks::BackgroundJob`; `busy_or_blocked`; `refuse_if_busy`; `record_history`; `show_report`.

**Interfaces — Produces:**
- **`AppEnv` gains:**
  - `startup_target: Arc<dyn Fn() -> StartupTarget + Send + Sync>`;
  - `startup_store_path: PathBuf`;
  - `startup_switch_works: fn(Source) -> bool`.
  - Production, in `system()`: `StartupTarget::current_user`, `default_startup_store_path()` and `Source::per_user_switch_works`.
  - `TempIcons::env()`: the sandbox `{sb}\HKCU` / `{sb}\HKLM`, folders `dir/startup-user` and `dir/startup-common` (created in `TempIcons::new`), `dir/startup-backups.json` and `|s| s != Source::MachineRun32`.
- **`TempIcons` helpers:**
  - `seed_run(&self, machine: bool, name: &str, command: &str)`: writes a `REG_SZ` under the sandbox side's `RUN_KEY`;
  - `startup_folder(&self, common: bool) -> PathBuf`;
  - `user_switch(&self, name: &str) -> Option<Vec<u8>>`: the data of the user `StartupApproved\Run` value;
  - `run_until_startup_scanned(app, ctx)`.
- **`WinTuneApp` fields:**
  - `startup_store: StartupBackupStore`;
  - `startup_store_error: Option<LoadError>`;
  - `startup_scan: Option<StartupScan>`: `None` until the first scan ends;
  - `startup_scan_job: Option<BackgroundJob<StartupScan>>`;
  - `startup_scan_needed: bool`;
  - `startup_pending: BTreeMap<String, bool>`: the wanted "on" state, by `StartupItem::id`;
  - `startup_search: String`.
- **`startup_state.rs`:**
  - `pub(super) fn startup_change(item: &StartupItem, wanted_on: bool) -> Option<StartupChange>`: `None` when the item is locked or `wanted_on == (state == On)`.
  - `reload_startup_store(&mut self)`, shaped like `reload_menu_store`.
  - `request_startup_scan(&mut self)` and `pump_startup_scan(&mut self, ctx)`, shaped exactly like `request_menu_scan` / `pump_menu_scan`:
    - the scan uses the store clone, `env.user_label`, `env.product_name` and `env.startup_switch_works`;
    - when it finishes, pending ids whose `startup_change` is `None` are pruned;
    - a panicked scan shows the toast `startup.scan_failed`;
    - a scan never starts while `is_busy()`.
  - `startup_changes(&self) -> Vec<StartupChange>`, `startup_pending_count(&self) -> usize` and `discard_startup_changes(&mut self)` (clears `startup_pending` only).
  - `pub(super) fn startup_slot_name(&self, slot_id: &str) -> String`: the scanned item's `display_name` via `by_slot`, else the value name after the slot id's `\\` separator.
  - Calls: `new()` calls `reload_startup_store()` and `request_startup_scan()`; `ui()` calls `pump_startup_scan(ctx)` after `pump_menu_scan`.
- **`jobs.rs`:**
  - `StartupOutcome { report, store: StartupBackupStore, restored: bool }`.
  - `RunningJob::Startup { job: BackgroundJob<StartupOutcome>, restore: bool }`, added to every arm:
    - `title_key`: `job.apply_startup` "Applying startup changes…" / `job.restore_startup` "Restoring startup apps…";
    - `name_key`: `job.name.apply_startup` / `job.name.restore_startup`;
    - `history_label`: `("Startup apps", "Apply" | "Restore")`.
  - `startup_blocked_reason(&self) -> Option<String>`: an unreadable store gives `tweaks.blocked`.
  - `start_startup_apply(ctx)`: `refuse_if_busy` → the blocked reason (error toast) → no changes, do nothing → spawn `apply_startup` with `now: now_filetime()` taken in the job. No confirmation: nothing is deleted, and History can undo it.
  - `start_startup_restore(ctx, ids: Vec<String>)`: blocked only by the store reason.
  - `finish_startup_job`:
    - it replaces `startup_store`, and clears `startup_pending` only on a clean apply (no errors);
    - `show_report` uses `ReportKind::AppliedStartup` (`report.applied_startup` "{0} startup change(s) applied") or `ReportKind::RestoredStartup` (`report.restored_startup` "{0} startup app(s) restored");
    - `record_history("Startup apps", "Apply" | "Restore", ..)`.
  - `poll_job` ends with `request_startup_scan()` too. Its panic branch also calls `reload_startup_store()`.
- **`confirm.rs`:**
  - `ConfirmAction::RestoreStartup(Vec<String>)` (slot ids), and `request_restore_startup(&mut self, ids: Vec<String>)`. It is shaped like `request_restore_menu_keys`: names come from `startup_slot_name`, with at most `MAX_NAMED` names, else `startup.restore_many` "{0} apps".
  - Keys: `startup.restore_title` "Restore original startup settings?" and `startup.restore_message` "{0} will get back the on/off state they had before WinTune changed them.".
  - `resolve_confirm` runs `start_startup_restore`.
- **`messages.rs`:**
  - `startup_state(tr, StartupState)`:
    - `startup.state.on` "On" / "Activée";
    - `startup.state.off` "Turned off by WinTune" / "Désactivée par WinTune";
    - `startup.state.off_elsewhere` "Turned off by another program" / "Désactivée par un autre programme".
  - `startup_lock(tr, &StartupLock)`:
    - `startup.locked.unreadable` "Cannot read this app's switch: {0}";
    - `startup.locked.unknown` "Its switch is stored in a way WinTune does not know, so WinTune leaves it alone.";
    - `startup.locked.machine` "Turned off for every account on this PC. WinTune changes only your account.";
    - `startup.locked.all_users` "Turned on for every account. WinTune cannot turn it off for your account alone yet.".
  - `startup_source(tr, Source)`: `startup.source.user_run` "Run key (you)", `.machine_run` "Run key (every account)", `.machine_run32` "Run key, 32-bit (every account)", `.user_folder` "Startup folder (you)", `.common_folder` "Startup folder (every account)".
  - `history_module` maps `"Startup apps"` to `history.module.startup` "Startup apps" / "Applications au démarrage".
  - Write the French values for every key.

- [ ] **Step 1: Write the failing tests:**
  - `startup_state.rs`:
    - `the_startup_scan_runs_in_background_and_waits_for_jobs`: seed a user Run item; `hold_job`; `request_startup_scan` → no scan job while busy. Release, then `run_until_idle` and `run_until_startup_scanned` → `startup_scan` has 1 item.
    - `startup_change_is_none_for_locked_or_unchanged`: built on a hand-made `StartupItem`.
  - `jobs.rs`:
    - `a_startup_apply_turns_off_logs_and_clears_pending`:
      - seed `App`, scan, then `startup_pending.insert("run:app", false)`, `start_startup_apply` and `run_until_idle`;
      - `user_switch("App")` starts with `[3,0,0,0]` and is 12 bytes long;
      - the last history entry is `"Startup apps"` / `"Apply"`;
      - `startup_pending` is empty;
      - after the rescan the item is `Off`.
    - `a_refused_startup_apply_keeps_the_choice`: scan and set a pending off. Then overwrite the user switch with a `REG_DWORD` and apply → an error toast, the choice is still pending, and nothing is written.
    - `an_unreadable_startup_store_blocks_apply_and_restore`: `startup_store_error = Some(corrupt(..))` → `start_startup_apply` and `start_startup_restore` start no job.
  - `confirm.rs`: `restoring_a_startup_app_asks_first_and_restores`:
    - after the apply above, call `request_restore_startup(vec![slot id])`;
    - `pending_confirm` is `RestoreStartup`, and its message contains `App`;
    - `resolve_confirm(&ctx, true)` and `run_until_idle` → the switch is absent, and history ends with `"Restore"`.
  - `src/i18n/tests.rs`: `every_startup_message_translates_in_every_shipped_language`, shaped like the context menu test. Every `StartupState`, every `StartupLock`, every `Source` and `history_module(tr, "Startup apps")` give non-empty text, and none is the raw key.
- [ ] **Step 2: Run** `cargo test --locked ui::` and `cargo test --locked i18n` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.** Add every key to both `.resx` files.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(startup): scan job, pending choices, apply and restore jobs`.

### Task 5: The Startup apps page

**Files:**
- Create: `src/ui/startup_tab.rs`
- Modify:
  - `src/ui/mod.rs`: `ActiveTab::Startup`, `ALL: [ActiveTab; 9]` (after `ContextMenu`), `history_module` → `Some("Startup apps")`, `owns_action_bar`, the tab dispatch;
  - `src/ui/shell.rs`:
    - the nav item after Context menu: `nav.startup` "Startup apps" / "Applications au démarrage";
    - `open_page`: when not busy, reload the startup store; on page change, request a startup scan;
    - the French test;
  - `src/ui/polish.rs`: `search_box_id(ActiveTab::Startup)` → `Some(egui::Id::new("search_startup"))`;
  - `src/ui/kit/icons.rs`: `Icon::Startup` = `'\u{E7E8}'` (PowerButton, present in both fonts), and `ALL: [Icon; 21]`;
  - both `.resx` files.

**Interfaces — Consumes:**
- From Task 4: the fields, `startup_change`, `request_restore_startup`, `start_startup_apply`, `discard_startup_changes`, `startup_pending_count`, `startup_blocked_reason` and `messages::{startup_state, startup_lock, startup_source}`.
- Kit: `page_header`, `section`, `SettingRow`, `pill`, `toggle_enabled`, `overflow_menu`, `empty_state`, `page_action_bar`.

**Interfaces — Produces:** `pub(super) fn show_startup_tab(&mut self, ctx: &egui::Context)`.

Content (spec 5.3):
- **Header:** `kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |ui| …)`:
  - `name` = `nav.startup`;
  - subtitle `startup.subtitle` "Programs that start when you sign in to Windows." / "Programmes lancés à l'ouverture de session Windows.";
  - in the actions slot, the `tweaks.pill.unofficial` pill (`PillKind::Warning`) with the hover text `startup.unofficial_hint` "Uses the same switch as Task Manager, which Microsoft does not document." / "Utilise le même interrupteur que le Gestionnaire des tâches, que Microsoft ne documente pas.".
  - Under the header, one secondary line: `startup.next_sign_in` "Changes take effect the next time you sign in." / "Les changements prennent effet à la prochaine ouverture de session.".
- **Search box** (`search_startup`, focused by Ctrl+F through `focus_search_if_requested`): it filters on `display_name` or `program`, case-insensitively.
- **Rows, one list sorted as scanned:** one `SettingRow` per item.
  - `title` = `display_name`; `description` = `program`; `hover` = one line per origin, `"{startup_source}: {detail}"`.
  - Pills:
    - `startup.pill.all_users` "All users" / "Tous les comptes" (Neutral) when `all_users()` and only one origin;
    - `startup.pill.both` "For you and every account" / "Pour vous et tous les comptes" (Neutral) when there are two origins;
    - `startup.pill.not_found` "Program not found" / "Programme introuvable" (Warning) when `program_missing`;
    - the `startup_state` text (Neutral) when the state is not `On`, which says who turned it off (spec 2: "who turned it off");
    - when pending differs from the current state, `startup.pill.will_turn_off` "Will be turned off" / "Sera désactivée" or `startup.pill.will_turn_on` "Will be turned on" / "Sera activée" (Accent).
  - **Toggle:** on = `startup_pending.get(id)` else `state == On`. It is disabled with `messages::startup_lock` when locked, else with `busy_or_blocked(&self.startup_store_error)`. A change back to the current state removes the pending entry; any other change inserts it.
  - **`…` menu:** `tweaks.restore_original` when `has_snapshot` → `request_restore_startup(vec![item.slot().id()])`. Like the menu rows, the action is applied after drawing.
- **Empty and loading states:**
  - `startup.scanning` "Reading startup apps…" while `startup_scan` is `None`; rows stay shown during later rescans;
  - an empty scan shows `empty_state(Icon::Startup, "startup.empty")` "No program starts when you sign in.";
  - a search with no match shows `startup.no_match` "No app matches your search.".
- **Action bar** (`page_action_bar`):
  - status `startup.pending` "{0} not applied" when `startup_pending_count() > 0`, else `startup.up_to_date` "All changes applied";
  - Discard (`tweaks.discard`) is shown when `startup_pending` is non-empty;
  - Apply (`tweaks.apply`) is disabled with `startup_blocked_reason`, the busy reason, or `startup.nothing_to_apply` "Nothing to apply" when the count is 0.

- [ ] **Step 1: Write the failing headless tests** in `startup_tab.rs`. Seed through `TempIcons` and finish the scan first.
  - `the_page_lists_items_with_their_pills`: a user Run item whose program is missing and a machine Run item → drawn texts contain both display names, "All users" and "Program not found".
  - `toggling_marks_pending_and_discard_clears_it`: click the toggle (`row_widget`) → `startup_pending == {"run:app": false}`, and the status shows "1 not applied". Click Discard → empty.
  - `apply_from_the_page_turns_the_app_off`: toggle, click Apply, `run_until_idle` → `user_switch("App")` flag 3. After the rescan the toggle reads off.
  - `a_locked_row_has_a_disabled_toggle_with_the_reason`: a machine Run32 item → the hover text of its toggle equals `messages::startup_lock(tr, &StartupLock::AllUsersOnly)`, and a click changes nothing.
  - `ctrl_f_focuses_the_startup_search` and `search_filters_by_program`.
  - `the_page_shows_the_loading_line_until_the_first_scan`.
  - `opening_the_page_requests_a_scan`: `open_page(ActiveTab::Startup)` from Home → `startup_scan_needed`.
  - `src/ui/shell.rs`: extend `shell_pages_draw_no_english_only_text_in_french` to draw the Startup page with a normal item, a locked Run32 item, a "Program not found" item and a pending change (so Discard shows).
- [ ] **Step 2: Run** `cargo test --locked startup_tab` — Expected: FAIL.
- [ ] **Step 3: Implement.** Fix any non-exhaustive `match` on `ActiveTab` or `Icon`. Home's `tile_title` may fall through `_` until Task 6.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS, including `every_tab_renders_without_panicking` over the 9 tabs.
- [ ] **Step 5: Commit** — `feat(startup): Startup apps page with toggles, search and action bar`.

### Task 6: History and Home

**Files:**
- Modify:
  - `src/ui/originals.rs`, `src/ui/history_panel.rs`, `src/ui/home.rs`;
  - `src/ui/shell.rs`: `open_page(History)` also reloads the startup store; the French test;
  - both `.resx` files.

**Interfaces — Consumes:** Task 4 (`startup_store`, `startup_store_error`, `startup_slot_name`, `request_restore_startup`); Task 5 (`ActiveTab::Startup`, `Icon::Startup`).

**Interfaces — Produces:**
- **Saved originals:**
  - `OriginalsModule::Startup`, ordered after `ContextMenu`, titled `history.module.startup`;
  - `originals_of(Startup)` gives one row per `startup_store` backup of `env.user_label`: `key` = slot id, `title` = `startup_slot_name(key)`, `description` = `originals.saved_on` with the timestamp;
  - `RestoreRequest::Startup(Vec<String>)` goes to `request_restore_startup`;
  - an unreadable store shows the `originals.unreadable` banner in place of the group, as the menu does.
- **History filter:** `HistoryFilter::Startup` matches `"Startup apps"`, with a chip labelled `history.module.startup`, placed after Context menu.
- **Home tile:** `ModuleSummary { page: ActiveTab::Startup, icon: Icon::Startup, status, drift: 0 }`, after the Context menu tile.
  - `status` = `home.startup_status` "{0} apps, {1} turned off" / "{0} application(s), {1} désactivée(s)".
  - `{0}` = the item count and `{1}` = the items whose state is not `On`. Both are 0 until the first scan.
  - The `tile_title` arm is `ActiveTab::Startup => tr.t("nav.startup")`.

- [ ] **Step 1: Write the failing tests:**
  - `originals.rs`: `a_turned_off_app_is_listed_in_saved_originals_and_restores`:
    - apply off on the seeded `App`;
    - the Startup apps group shows a row titled `App`;
    - clicking its Restore and confirming, then `run_until_idle` → the switch is absent and the group is gone.
  - `originals.rs`: `an_unreadable_startup_store_shows_a_banner`.
  - `history_panel.rs`: `the_startup_filter_shows_only_startup_operations`: one "Startup apps" entry and one "Context menu" entry → the Startup chip lists only the first.
  - `home.rs`: `the_startup_tile_counts_apps_and_turned_off`: two items, one with switch `03` → "2 apps, 1 turned off"; clicking the tile opens `ActiveTab::Startup`.
  - `shell.rs`: in the French test, History shows a startup original and a "Startup apps" operation, and Home shows the tile.
- [ ] **Step 2: Run** `cargo test --locked ui::` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(startup): Saved originals, History filter and Home tile`.

### Task 7: Docs, drift test and the owner's checklist

**Files:**
- Modify: `README.md`, `ARCHITECTURE.md`, `docs/registry-reference.md`, `src/startup/mod.rs` (the test).

**Ruling on the drift test: add it.** The startup locations are a handful of constants, and the reference doc is where the owner's checklist lives. A test that fails when they drift apart costs a few lines.

- [ ] **Step 1: Write the failing drift test** `the_registry_reference_names_every_startup_location` in `src/startup/mod.rs`. With `include_str!("../../docs/registry-reference.md")`, assert that the doc contains:
  - `RUN_KEY` and `RUN32_KEY`;
  - each `Approved::key()` for `Run`, `Run32` and `StartupFolder`;
  - `startup-backups.json`;
  - `PER_USER_SWITCH_WORKS`;
  - the heading text `Pre-merge checklist (Startup apps)`.

  Run `cargo test --locked the_registry_reference_names_every_startup_location` — Expected: FAIL.
- [ ] **Step 2: `docs/registry-reference.md`.** Add a `## Startup apps` section before `## Sources`, with these parts:
  - **What WinTune reads:** a table of the five sources, giving for each its key or folder, the matching `StartupApproved` sub-key and whether it covers all users. It also notes that RunOnce, packaged startup tasks, scheduled tasks and services are not listed (spec 2).
  - **What WinTune writes:**
    - only HKCU `StartupApproved\{Run,Run32,StartupFolder}\<name>`, `REG_BINARY`, 12 bytes;
    - off = `03 00 00 00` + FILETIME; on = the original put back, or `02` + zeros; Restore = the exact original or deletion;
    - never a Run value, a file, or HKLM.
  - **Evidence:**
    - spec 3.2 and 3.4 (OBSERVED on Windows 11 26300), with the shapes seen;
    - the Learn startup-apps page (https://learn.microsoft.com/windows/compatibility/startup-apps) for the sources;
    - a note that the format is undocumented, which is why the page carries the Unofficial pill.
  - **Per-user switch flags:** the current `PER_USER_SWITCH_WORKS` values, and the rule that a failed checklist item 2 or 3 sets the matching entry to `false` in `src/startup/mod.rs`, a one-line change.
  - **`#### Pre-merge checklist (Startup apps)`:** spec section 7 items 1-5 as checkboxes. The owner records each result there before merging.
- [ ] **Step 3: README and ARCHITECTURE.**
  - **README:**
    - the feature bullet;
    - a `## Startup apps` section: what is listed, turning apps off and on per account, "takes effect at next sign-in", the Unofficial pill, locked rows and why, "Program not found";
    - the Safety and undo paragraph (Saved originals "Startup apps");
    - the Data files table (`startup-backups.json`);
    - the Code layout table (`src/startup/`).
  - **ARCHITECTURE:**
    - the store table;
    - a `## Startup apps engine` section: sources, one row per approved value, switch format, the lock order, the apply algorithm, the flag table;
    - the UI paragraph listing the modules and Home tiles.
- [ ] **Step 4: Run** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `rtk bypass cargo test --locked` and `cargo build --release --locked` — Expected: all clean, with the drift test passing.
- [ ] **Step 5: Commit** — `docs(startup): README, ARCHITECTURE, registry reference and pre-merge checklist`.

## Rulings the spec left open

Each ruling below gives the decision, then what it costs if it is wrong.

1. **An HKLM `StartupApproved` value that is off, unreadable or of unknown shape locks the row, whatever the row's sources.** If wrong: a rare per-user item that shares its name with an HKLM-disabled value is locked for no reason.
2. **The item counts as off when either the HKCU or the HKLM switch is off.** If wrong: when an HKCU `02` actually overrides an HKLM `03`, a row reads off (and locked) while it still runs.
3. **"Program not found" is shown only for a command whose program has a folder part.** Bare names such as `ms-teams.exe` are never flagged. If wrong: a leftover bare-name item gets no pill.
4. **The program falls back to the file stem, not the full file name** (the context menu does the same; spec 5.1's "Discord" example implies a stem). If wrong: only the wording changes.
5. **The Startup folders list every regular file**, not just `.lnk`, except `desktop.ini`, hidden and system files. A non-`.lnk` file gets its product name. If wrong: a stray file shows up as a row, as it may in Task Manager.
6. **Run values of a non-string type, the default value, and unreadable Run keys give no rows**, with no error shown. If wrong: an item is missing with no explanation (unlikely, since users can read Run keys).
7. **The folder paths come from `%APPDATA%` and `%ProgramData%`, not `SHGetKnownFolderPath`.** A missing variable skips that folder. If wrong: on a broken environment, folder items are missing.
8. **The flag table is injected as `fn(Source) -> bool`** into the scan, the engine and `AppEnv`, so tests do not change when the owner flips a flag. If wrong: nothing breaks.
9. **`origin::expand_env` and `command_program` become `pub(crate)` in place** instead of moving to a shared module. If wrong: a later refactor moves them.
10. **Apply asks no confirmation** (it is non-destructive and History can undo it); Restore asks, as in the other modules. If wrong: one more dialog to add.
11. **Pair rows:** the name and program come from the per-user origin, and the row is "missing" if either program is missing. If wrong: only the display changes.
12. **The Home tile's "turned off" count includes both Off and Off elsewhere.** If wrong: the number differs from "turned off by WinTune".
13. **`OperationReport` detail lines stay English sentences**, as in every other engine. Only the states and locks the page shows are enums. If wrong: French users read English detail lines, as they already do for the other modules.
14. **The loading line shows only before the first scan; rows stay visible during rescans.** If wrong: nothing breaks.

## Not in this PR

- Packaged (Store/MSIX) startup tasks, RunOnce, scheduled tasks, services and the "Startup impact" figure (spec 2, non-goals).
- Resolving a `.lnk` target for its product name (spec 5.1 call).
- Sections in the list (spec 5.1 call).
