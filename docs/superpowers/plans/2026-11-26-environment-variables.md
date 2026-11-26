# Environment variables Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An "Environment variables" page that lists the user's variables (`HKCU\Environment`), edits them, and edits the user's `Path` as a list. The page flags missing and duplicate folders and shows the system variables read-only. Every write is snapshotted first, can be undone from History, and is followed by one `WM_SETTINGCHANGE` "Environment" broadcast.

**Architecture:** A new `src/environment/` engine, built like `src/startup/`:
- `path.rs`: the pure `Path` list logic. It splits and joins the list, expands entries for display, flags missing and duplicate folders, and keeps a pending list edit.
- `rules.rs`: the pure variable rules. It holds the type rule (spec 5.2), validation (spec 5.3), warnings, and turns pending edits into `EnvChange`s.
- `scan.rs`: a read-only scan of the user and system keys.
- `engine.rs`: one Apply pass (validate → snapshot → save the store + `.reg` → write → forget → broadcast) and a Restore built on `backups::restore_ids`. Restore keeps a rename's two slots together.

The UI copies the Startup apps pattern: a background scan job, pending edits until Apply, one `RunningJob` variant, a Saved originals group and a Home tile. The slot id `backups::Slot::Value { path: "Environment", name }` is the join key between row, pending change, store and History.

**Tech Stack:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59 (existing features only: `SendMessageTimeoutW` is already used by `tweaks::notify_explorer_settings`), rfd 0.15 (`pick_folder`, already a dependency), serde. No new crate.

**Spec:** `docs/superpowers/specs/2026-11-26-environment-variables-design.md` (binding). Windows evidence: its section 3, from `recon-envvars.md`. The owner's real-machine checklist: its section 7.

## Global Constraints

- **Writes:** WinTune writes only values of `HKCU\Environment`. **Nothing is ever written under HKLM, and nothing under `HKCU\Volatile Environment`**, which is never read either. The system key `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment` is read only.
- **Raw values:** values are read raw, with their registry type kept (`REG_SZ` = 1, `REG_EXPAND_SZ` = 2). They are decoded with `context_menu::fileinfo::decode_utf16le` and **never expanded on write**. Expansion is used only to display `Path` entries and to check whether a folder exists. Values of any other type are shown read-only and never written.
- **Types (spec 5.2):** an existing value keeps its type, except that a `REG_SZ` whose new text has a `%name%` reference becomes `REG_EXPAND_SZ`. A new variable is `REG_EXPAND_SZ` if its text has a reference, else `REG_SZ`. A new `Path` is always `REG_EXPAND_SZ`. A reference is `%`, then one or more characters other than `%` and `=`, then `%`.
- **Validation (spec 5.3):** any problem refuses the whole Apply, and nothing is written.
  - Names are trimmed, not empty, free of `=` and NUL, at most 255 characters, and unique ignoring case after the change.
  - A value is not empty, at most 32,767 characters, and free of NUL.
  - A `Path` entry containing `;` is refused. An empty entry is dropped on join.
  - Lengths count UTF-16 units.
- **Safety model (ARCHITECTURE.md):**
  1. Validate.
  2. Snapshot on first touch (`record_first`).
  3. Save `environment-backups.json` **and** export a `.reg` **before** any write.
  4. Report every failure.
  5. Forget a slot that is back at its original (same presence, type, bytes and name spelling).
  6. Broadcast once if anything was written.

  An unreadable store blocks Apply and Restore.
- **Broadcast:** `WM_SETTINGCHANGE` with `lParam` "Environment", sent through `SendMessageTimeoutW(HWND_BROADCAST, …, SMTO_ABORTIFHUNG, 5000)`. It is injected through `AppEnv::notify_environment` and `EnvOptions::broadcast`. **Tests never send the real broadcast:** they inject a counter.
- **Tests:** tests touch only `HKCU\Software\WinTuneTest` (`regsnap::test_support::Sandbox`: user side `{sandbox}\HKCU`, machine side `{sandbox}\HKLM`) and temp folders. Never launch `WinTune.exe`. UI tests live in `src/ui/*.rs` and use `TempIcons`.
- **Texts:**
  - Engine problems and warnings the page shows are enums (`EnvProblem`, `EnvWarning`), translated in `src/i18n/messages.rs`.
  - Every UI text is a literal key (`tr.t("env.x")`) present in both `i18n/strings.resx` and `i18n/strings.fr.resx`. French follows the file's existing punctuation.
  - `OperationReport` detail lines stay English sentences, as in the other engines.
- **History:** the module label is `Environment variables`; the actions are `Apply` and `Restore`. The store is `environment-backups.json` in `models::app_data_dir()`.
- **Page:** kit only, with no colors, sizes or fonts set by hand. The nav entry "Environment variables" goes in the Customize group, after Startup apps.
- **Editing files:** bash heredocs mangle backslashes on this machine, so write and edit files with the Write and Edit tools only.
- **Commits:** the commit messages carry no attribution lines.
- **CI:** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` and `cargo build --release --locked` must pass after every task. `rtk bypass cargo test --locked` shows the full result lines.

## Review Focus

1. **A `Path` with empty entries, a trailing `;` or whitespace-only entries.** Nobody's `Path` may be rewritten unless they edited it. When it is edited, every entry they did not touch keeps its exact text, including `%VAR%`, trailing backslashes and quotes. (Task 1: `split_and_join_keep_entries_verbatim`; Task 5: `an_untouched_path_is_never_written`.)
2. **Name case.** `PATH` versus `Path`, and renaming `foo` to `Foo`. Registry value names are case-insensitive and keep their first spelling, so a case-only rename must really change the stored spelling, and Restore must bring the old spelling back. (Task 4: `a_case_only_rename_changes_the_spelling_and_restore_brings_it_back`.)
3. **Texts with a `%`.** A `REG_SZ` value with a single `%` ("50% done") stays `REG_SZ`. An installer's `REG_SZ` `Path` with `%VAR%` entries becomes `REG_EXPAND_SZ` only when edited. An unedited value never changes type. (Task 2: `kind_rules_follow_spec_5_2`; Task 4: `editing_keeps_the_type_and_the_unexpanded_text`.)
4. **Another program rewrites `Path` between the scan and Apply**, as an installer adds its folder. The confirmation says so and names the folders that will be removed. Apply writes the wanted list. A later Restore names what it removes and adds back. (Task 5: `a_path_changed_elsewhere_is_named_in_the_confirmation`, `restoring_path_names_removed_and_added_back_entries`.)
5. **Self-referencing or unknown references** (`A=%B%`, `B=%A%`, `%NOPE%\bin`). Expansion must end, and an entry it cannot expand is never flagged "Folder not found". (Task 1: `expansion_stops_on_cycles`, `analyze_flags_missing_and_later_duplicates`.)

---

## File Structure

- **Create `src/environment/`:**
  - `mod.rs`: the key constants, `REG_SZ_KIND` and `REG_EXPAND_SZ_KIND`, `env_slot`, `is_path_name`, `encode_text`, `process_var`, `EnvTarget`, the re-exports and the drift test;
  - `path.rs`: `Resolver`, `Expanded`, split/join, `has_reference`, `folder_key`, `checkable`, `EntryInfo`, `analyze_entries`, `without_missing_and_duplicates`, `removed_entries`, `PathEntry`, `PathEdit`, `EntryMark`;
  - `rules.rs`: `EnvVar`, `EnvChange`, `kind_to_write`, `EnvProblem` and its checks, `names_after`, `validate_changes`, `EnvWarning`, `is_protected`, `change_warnings`, `VarEdit`, `var_change`, `path_change`;
  - `scan.rs`: `read_vars`, `EnvScan`, `scan_environment`;
  - `engine.rs`: `EnvBackupStore`, the store helpers, `EnvOptions`, `apply_environment`, `restore_environment`, `broadcast_environment`.
- **Create in `src/ui/`:**
  - `environment_state.rs`: `EnvPending`, `VarKey`, `EnvDialog`, the scan job, the pending changes, the resolver and the folder cache;
  - `environment_tab.rs`: the page and its dialog.
- **Modify:**
  - `src/lib.rs`;
  - `src/backups.rs`: `save_then_export` takes `&impl Serialize`;
  - in `src/ui/`: `mod.rs`, `jobs.rs`, `confirm.rs`, `originals.rs`, `history_panel.rs`, `home.rs`, `shell.rs`, `polish.rs`, `test_support.rs`, `kit/icons.rs` and `kit/tests.rs`;
  - `src/i18n/messages.rs` and `src/i18n/tests.rs`;
  - both `.resx` files;
  - `README.md`, `ARCHITECTURE.md` and `docs/registry-reference.md`.

---

### Task 1: The Path list (pure)

**Files:**
- Create: `src/environment/mod.rs`, `src/environment/path.rs`
- Modify: `src/lib.rs` (`pub mod environment;`)

**Interfaces — Produces (`mod.rs`):**
- `pub const USER_ENV_KEY: &str = "Environment";`
- `pub const SYSTEM_ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";`
- `pub const VOLATILE_ENV_KEY: &str = "Volatile Environment";`, with a doc comment: never read, never written; named only for the guard test.
- `pub const PATH_NAME: &str = "Path";` and `pub const PATH_LIMIT: usize = 8191;`.
- `pub const REG_SZ_KIND: u32 = 1;` and `pub const REG_EXPAND_SZ_KIND: u32 = 2;`
- `pub fn is_path_name(name: &str) -> bool`: `eq_ignore_ascii_case("Path")`.
- `pub fn env_slot(name: &str) -> backups::Slot`: `Slot::Value { path: USER_ENV_KEY, name }`.
- `pub fn encode_text(text: &str) -> Vec<u8>`: UTF-16LE plus a NUL terminator.
- `pub fn process_var(name: &str) -> Option<String>`: `std::env::var(name).ok()`. It is the production fallback for per-sign-in variables such as `USERPROFILE`, which live in `Volatile Environment`.
- `mod path; pub use path::*;`

**Interfaces — Produces (`path.rs`):**
- `#[derive(Debug, Clone, PartialEq)] pub struct Expanded { pub text: String, pub complete: bool }`. `complete` is false when a reference could not be resolved.
- `pub struct Resolver`, with:
  - `pub fn new(user: Vec<(String, String, bool)>, system: Vec<(String, String, bool)>, process: fn(&str) -> Option<String>) -> Self`. Each tuple is `(name, raw value, expandable)`, where `expandable` means the value is `REG_EXPAND_SZ`;
  - `pub fn get(&self, name: &str) -> Option<String>`;
  - `pub fn expand(&self, text: &str) -> Expanded`.

  Lookup order: user, then system, then `process`, ignoring ASCII case. An expandable value is itself expanded, at most 8 levels deep; past that, the reference stays as written and `complete` is false. An unknown name stays as written, and `complete` is false.
- `pub fn has_reference(text: &str) -> bool`: true when the text holds a reference as defined by spec 5.2.
- `pub fn split_entries(raw: &str) -> Vec<String>`: split on `;`. Entries that are empty or whitespace-only are dropped; every other entry is kept verbatim (not trimmed, quotes and trailing `\` kept). A quoted entry that contains a `;` is split like any other; joining puts it back unchanged.
- `pub fn join_entries(entries: &[String]) -> String`: drop entries that are empty or whitespace-only, join with `;`, and add no trailing `;`.
- `pub fn folder_key(expanded: &str) -> String`: strip one pair of surrounding `"` and any trailing `\` or `/`, then lowercase.
- `pub fn checkable(expanded: &str) -> bool`: an absolute local path (ASCII letter, `:`, then `\` or `/`), after stripping quotes. UNC paths and relative entries give false.
- `#[derive(Debug, Clone, PartialEq)] pub struct EntryInfo { pub text: String, pub expanded: String, pub complete: bool, pub missing: bool, pub duplicate: bool }`.
- `pub fn analyze_entries(entries: &[String], system: &[String], resolver: &Resolver, exists: &dyn Fn(&str) -> bool) -> Vec<EntryInfo>`:
  - `missing` = `complete && checkable(expanded) && !exists(expanded)`;
  - `duplicate` = the entry's `folder_key(expanded)` equals that of an earlier user entry, or of any system entry (expanded with the same resolver). Only the later copy is flagged;
  - `exists` is called only for complete, checkable entries.
- `pub fn without_missing_and_duplicates(infos: &[EntryInfo]) -> Vec<String>`: the texts of the entries that are neither missing nor duplicate, in order.
- `pub fn removed_entries(from: &[String], to: &[String]) -> Vec<String>`: the multiset difference `from − to`, matched by exact text, in `from` order. The restore diff is `removed = removed_entries(current, original)` and `added_back = removed_entries(original, current)`.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum EntryMark { Same, Added, Moved }`.
- `#[derive(Debug, Clone, PartialEq)] pub struct PathEntry { pub text: String, pub origin: Option<usize>, pub moved: bool }`. `origin` is the entry's index in the list the edit started from; `None` for a new or edited entry.
- `#[derive(Debug, Clone, PartialEq)] pub struct PathEdit { pub started_from: Option<String>, pub entries: Vec<PathEntry> }`, with these methods:
  - `pub fn start(current_raw: Option<&str>) -> Self`: the split entries, with `origin: Some(i)` and `moved: false`;
  - `pub fn texts(&self) -> Vec<String>` and `pub fn original(&self) -> Vec<String>` (`split_entries` of `started_from`);
  - `pub fn move_up(&mut self, i: usize)` and `pub fn move_down(&mut self, i: usize)`: they swap, and set `moved = true` on the entry the user moved only. At the edges they do nothing;
  - `pub fn edit(&mut self, i: usize, text: String)`: the entry gets `origin: None` and `moved: false`;
  - `pub fn remove(&mut self, i: usize)` and `pub fn add(&mut self, text: String)` (appended, with `origin: None`);
  - `pub fn removed(&self) -> Vec<String>`: the original entries whose index is no entry's `origin`, in original order;
  - `pub fn mark(&self, i: usize) -> EntryMark`: `Added` if `origin` is `None`, `Moved` if `moved`, else `Same`;
  - `pub fn is_unchanged_from(&self, current_raw: Option<&str>) -> bool`: `texts() == split_entries(current_raw.unwrap_or(""))`.

- [ ] **Step 1: Write the failing tests** in `src/environment/path.rs`. A test resolver is built from literal tuples, with `process` = `|n| (n == "PROC").then(|| "P".into())`.
  - `split_and_join_keep_entries_verbatim`:
    - `split_entries(r#"C:\a;;%X%\b\; ;"C:\q""#) == [r"C:\a", r"%X%\b\", r#""C:\q""#]`;
    - `split_entries("") == []`; `split_entries("A;") == ["A"]`;
    - `join_entries` of those three gives `r#"C:\a;%X%\b\;"C:\q""#`;
    - `join_entries(&[])` is `""`;
    - for a clean raw `r"C:\a;%LOCALAPPDATA%\agy\bin;D:\x\"`, `join(split(raw)) == raw`;
    - `r#""C:\a;b";D:\c"#` splits into three entries and joins back to the same text.
  - `references_follow_spec_5_2`:
    - true: `"%A%"`, `r"%A%\b"`, `"100% and 5%"` (the spec's accepted cost);
    - false: `"50% off"`, `"%%"`, `"%a=b%"`, `""`.
  - `expansion_uses_user_then_system_then_process`: user `A`=`u`, system `A`=`s` and `B`=`sb` → `expand("%A%;%b%;%PROC%")` gives `"u;sb;P"`, complete.
  - `expansion_keeps_unknown_names_and_marks_incomplete`: `%NOPE%\x` stays as written with `complete: false`; `"50%"` is unchanged and complete.
  - `nested_expandable_values_are_expanded`: user `BASE`=`C:\b`, and `TOOLS` (expandable) = `%BASE%\tools` → `%TOOLS%\bin` expands to `C:\b\tools\bin`. A non-expandable `Z`=`%BASE%` gives `get("Z") == Some("%BASE%")`.
  - `expansion_stops_on_cycles`: expandable `X`=`%Y%` and `Y`=`%X%` → `expand("%X%")` returns (the test finishes), with `complete: false`.
  - `folder_keys_ignore_case_quotes_and_trailing_backslash`: `folder_key(r"C:\Tools\") == folder_key(r#""c:\tools""#)`.
  - `checkable_paths_are_absolute_local`:
    - true: `C:\x` and `c:/x`;
    - false: `\\srv\share`, `bin`, `%X%\y`.
  - `analyze_flags_missing_and_later_duplicates`:
    - Setup: user `T`=`C:\tools`; `exists` true only for `c:\tools`; entries `[C:\tools, %T%\, C:\gone, %NOPE%\x, C:\SYS\]`; system `[c:\sys]`.
    - Expect `duplicate` = `[F, T, F, F, T]` and `missing` = `[F, F, T, F, F]`. `exists` is never called for `%NOPE%\x`.
  - `remove_missing_and_duplicates_keeps_first_copies`: on the infos above → `[C:\tools, %NOPE%\x]`.
  - `removed_entries_is_a_multiset_difference_in_order`: `removed_entries([A,B,A,C],[A,C]) == [B,A]`.
  - `path_edit_moves_edits_removes_and_adds`:
    - `start(Some("A;B;C"))`; `move_up(1)` → texts `[B,A,C]`, `mark(0) == Moved`, `mark(1) == Same`;
    - `remove(2)` → `removed() == [C]`;
    - `edit(0, "B2")` → `mark(0) == Added`, and `removed()` contains `B`;
    - `add("D")` → the last mark is `Added`;
    - `move_up(0)` and `move_down(last)` change nothing;
    - `PathEdit::start(None)` has no entries and `original() == []`;
    - `is_unchanged_from(Some("A;B;"))` is true for `start(Some("A;B"))`.
- [ ] **Step 2: Run** `cargo test --locked environment::` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `mod.rs` (the items above) and `path.rs`, then add `pub mod environment;` to `src/lib.rs`.
- [ ] **Step 4: Run** `cargo test --locked environment::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(environment): Path list logic: split, join, expansion, missing and duplicate folders`.

### Task 2: Variable rules (pure)

**Files:**
- Create: `src/environment/rules.rs`
- Modify: `src/environment/mod.rs` (`mod rules; pub use rules::*;`)

**Interfaces — Consumes:** Task 1.

**Interfaces — Produces:**
- `#[derive(Debug, Clone, PartialEq)] pub struct EnvVar { pub name: String, pub kind: u32, pub value: Option<String>, pub has_snapshot: bool }`. `value` is `None` when the kind is not 1 or 2. It has these methods:
  - `pub fn is_text(&self) -> bool`;
  - `pub fn is_path(&self) -> bool`;
  - `pub fn uses_references(&self) -> bool`: `kind == REG_EXPAND_SZ_KIND`;
  - `pub fn slot(&self) -> Slot`: `env_slot(&name)`.
- `#[derive(Debug, Clone, PartialEq)] pub enum EnvChange { Add { name: String, value: String }, Edit { name: String, value: String }, Delete { name: String }, Rename { from: String, to: String, value: String } }`, with:
  - `pub fn slot_names(&self) -> Vec<&str>`: `[from, to]` for a rename, or only `[from]` when `from` and `to` differ only in ASCII case;
  - `pub fn display_name(&self) -> String`: the name, or `"{from} → {to}"`.
- `pub fn kind_to_write(existing: Option<u32>, name: &str, value: &str) -> u32`:
  - `Some(1)` with a reference gives 2;
  - `Some(k)` gives `k`;
  - `None` with `is_path_name` gives 2;
  - `None` with a reference gives 2;
  - anything else gives 1.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum EnvProblem`, with these variants:
  - `EmptyName`, `NameHasEquals`, `NameHasNul`, `NameTooLong`, `NameTaken(String)`, `PathName`;
  - `EmptyValue`, `ValueTooLong`, `ValueHasNul`;
  - `EmptyEntry`, `EntryHasSemicolon`;
  - `Gone(String)`, `NotText(String)`, `Twice(String)`.
- **Checks:**
  - `pub fn name_problem(name: &str) -> Option<EnvProblem>` checks the trimmed name, with 255 UTF-16 units at most.
  - `pub fn value_problem(value: &str) -> Option<EnvProblem>`: 32,767 UTF-16 units at most.
  - `pub fn entry_problem(entry: &str) -> Option<EnvProblem>`: empty or whitespace-only gives `EmptyEntry`; a `;` gives `EntryHasSemicolon`; a NUL gives `ValueHasNul`.
- `pub fn names_after(current: &[EnvVar], changes: &[EnvChange]) -> Vec<String>`: the current names, minus deleted and renamed-from ones, plus added and renamed-to ones.
- `pub fn validate_changes(current: &[EnvVar], changes: &[EnvChange]) -> Vec<(String, EnvProblem)>`. The `String` is the variable the problem is about. Current names match ignoring ASCII case. The rules are:
  - `Add`: `name_problem`, `value_problem`, and `NameTaken` if the name exists.
  - `Edit`: `value_problem`; `Gone` if the variable is missing; `NotText` if it is not text.
  - `Delete`: `Gone` if the variable is missing; `NotText` if it is not text.
  - `Rename`: `name_problem(to)`, `value_problem`, `Gone` or `NotText` for `from`, and `NameTaken` if `to` exists and is not `from` ignoring case.
  - Two changes touching the same slot name ignoring case give `Twice`.
  - A duplicate (ignoring case) in `names_after` gives `NameTaken`.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum EnvWarning { Removes(String), PathTooLong(usize), PathChangedElsewhere }`.
- `pub fn is_protected(name: &str) -> bool`: `Path`, `TEMP`, `TMP`, or a name starting with `OneDrive`, all ignoring ASCII case.
- `pub fn change_warnings(changes: &[EnvChange], merged_path_len: Option<usize>) -> Vec<EnvWarning>`:
  - `Removes(name)` for a `Delete` of a protected name, or a `Rename` whose `from` is protected;
  - `PathTooLong(n)` when `merged_path_len == Some(n)` with `n > PATH_LIMIT`.
  - `PathChangedElsewhere` is added by the UI.
- `#[derive(Debug, Clone, PartialEq)] pub struct VarEdit { pub original: Option<String>, pub wanted: Option<(String, String)> }`. `original` is the variable's current name; `None` for a new variable. `wanted` is `None` for a delete.
- `pub fn var_change(edit: &VarEdit, current: Option<&EnvVar>) -> Option<EnvChange>`:
  - a new variable with `wanted` gives `Add`;
  - an existing `original` whose `current` is `None` (gone) or not text gives `None`;
  - `wanted: None` gives `Delete { current.name }`;
  - the same exact name: `None` when the value is unchanged, else `Edit { current.name, value }`;
  - another name, including a case-only change, gives `Rename { from: current.name, to, value }`.
- `pub fn path_change(edit: &PathEdit, current: Option<&EnvVar>) -> Option<EnvChange>`:
  - `current` not text gives `None`;
  - `edit.is_unchanged_from(current value)` gives `None`;
  - no entries left gives `Delete { current.name }` if a `Path` exists, else `None`;
  - otherwise `Edit { current.name, join_entries(texts) }`, or `Add { PATH_NAME, … }` when there is no `Path`.

- [ ] **Step 1: Write the failing tests** in `src/environment/rules.rs`:
  - `kind_rules_follow_spec_5_2`:
    - `(Some(1), "X", "50% done")` gives 1; `(Some(1), "X", r"%TEMP%\x")` gives 2;
    - `(Some(2), "X", "plain")` gives 2; `(Some(1), "Path", r"C:\a;%X%\b")` gives 2; `(Some(1), "Path", r"C:\a")` gives 1;
    - `(None, "path", r"C:\a")` gives 2; `(None, "X", "%Y%")` gives 2; `(None, "X", "y")` gives 1.
  - `names_are_checked`:
    - `"  "` gives `EmptyName`; `"A=B"` gives `NameHasEquals`; `"A\0"` gives `NameHasNul`;
    - 256 × `"a"` gives `NameTooLong`; 255 × `"é"` gives `None`; `" Foo "` gives `None`.
  - `values_are_checked`:
    - `""` gives `EmptyValue`; 32,767 × `"a"` gives `None`; 32,768 × `"a"` gives `ValueTooLong`;
    - 16,384 × `"𝄞"` gives `ValueTooLong` (two units each); `"a\0b"` gives `ValueHasNul`.
  - `entries_are_checked`: `""` and `"  "` give `EmptyEntry`; `r"C:\a;b"` gives `EntryHasSemicolon`; `r"C:\a"` gives `None`.
  - `var_edits_become_changes`:
    - new `("FOO","1")` → `Add`;
    - an edit of `FOO` with a new value → `Edit`; with the same value → `None`;
    - `FOO` to `BAR` → `Rename`; `foo` to `Foo` → `Rename { from: "foo", to: "Foo", .. }`, with `slot_names() == ["foo"]`;
    - a delete → `Delete`; a non-text current (kind 4) → `None`; a gone current → `None`.
  - `path_edits_become_changes`:
    - unchanged → `None`;
    - a move → `Edit { "Path", "B;A" }`; the current name `PATH` is kept;
    - every entry removed → `Delete`;
    - no current `Path` plus `add("C:\x")` → `Add { "Path", r"C:\x" }`;
    - a current kind of 4 → `None`.
  - `validation_refuses_what_spec_5_3_refuses`, against current `[Path(2), TEMP(2), x(1), D(4)]`:
    - `Add X` → `NameTaken`; `Add Y` + `Add y` → `NameTaken`;
    - `Rename x→TEMP` → `NameTaken`; `Rename x→X` → no problem;
    - `Edit GONE` → `Gone("GONE")`; `Edit D` → `NotText("D")`;
    - `Add "A=B"` → `NameHasEquals`; `Edit x ""` → `EmptyValue`;
    - `Edit x` + `Delete X` → `Twice`; `Delete x` + `Add X` → no problem.
  - `warnings_name_protected_variables_and_long_paths`:
    - `Delete TEMP` → `[Removes("TEMP")]`; `Rename OneDriveCommercial→X` → `Removes`;
    - `Edit TEMP` → `[]`; `Delete FOO` → `[]`;
    - `Some(8191)` → no `PathTooLong`; `Some(8192)` → `PathTooLong(8192)`;
    - `is_protected`: true for `tmp` and `onedrive`; false for `PATHEXT`.
- [ ] **Step 2: Run** `cargo test --locked environment::rules` — Expected: FAIL.
- [ ] **Step 3: Implement** `rules.rs`.
- [ ] **Step 4: Run** `cargo test --locked environment::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(environment): type rule, validation, warnings and pending edits as changes`.

### Task 3: The read-only scan

**Files:**
- Create: `src/environment/scan.rs`
- Modify: `src/environment/mod.rs` (`mod scan; pub use scan::*;`, `EnvTarget`)

**Interfaces — Consumes:** Tasks 1-2; `backups::{RegTarget, UserBackups::find_id}`; `context_menu::fileinfo::decode_utf16le`.

**Interfaces — Produces:**
- In `mod.rs`: `pub struct EnvTarget { pub user: RegTarget, pub machine: RegTarget }`, with `pub fn current_user() -> Self` (HKCU and HKLM). Writes only ever use `user`; `machine` is only read.
- `pub fn read_vars(t: &RegTarget, rel: &str) -> Result<Vec<EnvVar>, String>`:
  - a missing key gives `Ok(vec![])`;
  - the default (unnamed) value is skipped;
  - kinds 1 and 2 are decoded raw; other kinds give `value: None`;
  - `has_snapshot` is false;
  - the result is sorted by lowercase name.
- `#[derive(Debug, Clone, Default, PartialEq)] pub struct EnvScan`, with these fields:
  - `pub user: Vec<EnvVar>` and `pub system: Vec<EnvVar>`;
  - `pub user_error: Option<String>` and `pub system_error: Option<String>`;
  - `pub folders: HashMap<String, bool>`: folder existence keyed by expanded entry text.

  It has these methods:
  - `pub fn user_var(&self, name: &str) -> Option<&EnvVar>` (ignoring ASCII case);
  - `pub fn user_path(&self) -> Option<&EnvVar>`;
  - `pub fn user_path_entries(&self) -> Vec<String>` and `pub fn system_path_entries(&self) -> Vec<String>`: `split_entries` of a text `Path`, else empty;
  - `pub fn resolver(&self, process: fn(&str) -> Option<String>) -> Resolver`: text variables only, expandable = kind 2.
- `pub fn scan_environment(target: &EnvTarget, snapshots: &UserBackups, user: &str, process: fn(&str) -> Option<String>) -> EnvScan`:
  - It reads `user` + `USER_ENV_KEY` and `machine` + `SYSTEM_ENV_KEY`. It never opens `VOLATILE_ENV_KEY`.
  - `has_snapshot` = `snapshots.find_id(user, &env_slot(name).id()).is_some()`.
  - `folders`: for every user and system `Path` entry, expanded with the scan's resolver, it holds `Path::new(e).is_dir()` when the expansion is complete and `checkable`. Other entries are not in the map.

Test setup: a sandbox `EnvTarget` whose `user` and `machine` bases are `{sandbox}\HKCU` and `{sandbox}\HKLM`, with `process` = `|_| None`, and a temp folder.

- [ ] **Step 1: Write the failing tests** in `src/environment/scan.rs`:
  - `user_variables_are_read_raw_with_their_type`:
    - Setup: `REG_EXPAND_SZ` `X` = `%USERPROFILE%\x`; `REG_SZ` `b` = `y`; `REG_DWORD` `D`; a default value; and `PATH` (`REG_EXPAND_SZ`).
    - `X.value == Some("%USERPROFILE%\x")`, `X.kind == 2`; `D.value == None`, `D.kind == 4`.
    - The names are `["b", "D", "PATH", "X"]`, and `user_path()` finds `PATH`.
  - `system_variables_are_read_from_the_machine_side`: machine `SYSTEM_ENV_KEY` `Path` = `C:\s1;C:\s2` → `system_path_entries() == [C:\s1, C:\s2]`, and `user` is empty.
  - `a_missing_key_gives_no_variables_and_no_error`.
  - `has_snapshot_follows_the_store`: a store entry for `env_slot("Foo").id()` and a variable `FOO` → `has_snapshot`.
  - `folders_are_checked_for_expanded_path_entries`:
    - Setup: user `TOOLS` = `<tmp>\tools` (created); user `Path` = `%TOOLS%;<tmp>\missing;\\server\share;%NOPE%\x`.
    - `folders` holds `<tmp>\tools` → true and `<tmp>\missing` → false, and has no other key.
  - `volatile_environment_is_never_read`: a value under `{HKCU}\Volatile Environment` → it is absent from `user`.
- [ ] **Step 2: Run** `cargo test --locked environment::scan` — Expected: FAIL.
- [ ] **Step 3: Implement** `scan.rs` and `EnvTarget`.
- [ ] **Step 4: Run** `cargo test --locked environment::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(environment): read-only scan of user and system variables`.

### Task 4: Apply, restore, rename pairs and the broadcast

**Files:**
- Create: `src/environment/engine.rs`
- Modify:
  - `src/environment/mod.rs` (`mod engine; pub use engine::*;`);
  - `src/backups.rs`: `save_then_export` takes `store: &impl serde::Serialize` instead of `&UserBackups`. The callers are unchanged.

**Interfaces — Consumes:**
- Tasks 1-3.
- From `backups`: `take_snapshot`, `save_then_export`, `restore_ids` and `UserBackups::{find_id, record_first, forget, backups_of}`.
- `regsnap::{snapshot_value, restore_value, ValueSnapshot}`.
- `associations::OperationReport`.

**Interfaces — Produces:**
- The store:
  ```rust
  #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
  pub struct EnvBackupStore {
      #[serde(flatten)]
      pub snapshots: UserBackups,
      /// Per user: slot ids restored together (a rename's two slots).
      #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
      pub renames: BTreeMap<String, Vec<Vec<String>>>,
  }
  ```
  - `pub fn link(&mut self, user: &str, a: &str, b: &str)`: merges any groups holding `a` or `b` into one.
  - `pub fn with_partners(&self, user: &str, ids: &[String]) -> Vec<String>`: the ids plus every member of their groups, matched ignoring ASCII case, without duplicates, in the input order first.
  - `pub fn prune(&mut self, user: &str) -> bool`: drops ids with no snapshot left and groups with fewer than two ids; it says whether anything changed.
- `pub fn default_env_store_path() -> PathBuf`: `app_data_dir().join("environment-backups.json")`. `load_env_store` and `save_env_store` work like `load_startup_store` and `save_startup_store`.
- `pub struct EnvOptions<'a> { pub user: String, pub store_path: PathBuf, pub reg_backup_dir: PathBuf, pub progress: &'a dyn Fn(usize, usize, &str), pub broadcast: &'a dyn Fn() }`.
- `pub fn apply_environment(target: &EnvTarget, store: &mut EnvBackupStore, changes: &[EnvChange], opts: &EnvOptions) -> OperationReport`.
- `pub fn restore_environment(target: &EnvTarget, store: &mut EnvBackupStore, ids: &[String], opts: &EnvOptions) -> OperationReport`.
- `pub fn broadcast_environment()`: `SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment" as wide, SMTO_ABORTIFHUNG, 5000, &mut result)`, with a `// SAFETY:` comment like `notify_explorer_settings`. No test calls it.

**Private helpers:**
- `write_exact(t, name, kind, data)`: find the stored spelling of `name` ignoring ASCII case. If it differs from `name`, delete that value first; then `restore_value` with `exists: true`. This is the only way to change a name's case.
- `restore_exact(t, slot, snapshot)`: `write_exact` when the snapshot exists, else `restore_value` with `exists: false`.
- `stored_spelling(t, name) -> Result<Option<String>, String>`.

**Algorithm of `apply_environment` (the safety order):**
1. **Validate.** `current = read_vars(&target.user, USER_ENV_KEY)`; a read error adds one error line and returns. Each `(name, problem)` from `validate_changes(&current, changes)` adds `"{name}: {english}"`. The English text comes from a private `fn english(&EnvProblem) -> String`. Any line returns with nothing written.
2. **Snapshot.** For every slot name of every change, the slot is `env_slot(stored spelling, or the change's name when it is new)`. `take_snapshot` it, keep the stored id spelling via `find_id`, and call `record_first`. For a rename whose two slots differ, call `store.link(user, from_id, to_id)`. A failed snapshot adds an error and returns.
3. **Save the store and export:** `save_then_export(&*store, &opts.store_path, &target.user, &taken, &opts.reg_backup_dir, "environment-before-apply")`. On `Err`, push the error and return.
4. **Write**, in change order. Each change pushes `display_name()` to `succeeded` or `"{name}: {e}"` to `errors`.
   - `Add` and `Edit`: `write_exact(name, kind_to_write(existing kind, name, value), encode_text(value))`.
   - `Delete`: `restore_value` with `exists: false`.
   - `Rename`, case-only: `write_exact(to, …)` on the single slot.
   - `Rename`, otherwise: write `to` first, using `kind_to_write(Some(from kind), to, value)`, then delete `from`. A failed write skips the delete.
5. **Forget** each touched slot that is back at its original: same `exists`, and when it exists the same `kind`, `data` and exact stored spelling (the slot name in the id). Then `prune`. If anything changed, `save_env_store`; a failure gives `Cannot save snapshots: {e}`.
6. **Broadcast:** call `(opts.broadcast)()` once if `succeeded` is non-empty.

**`restore_environment`:**
1. `all = store.with_partners(user, ids)`. Order them so that slots whose recorded snapshot `exists` come first: the old name comes back before the new one goes.
2. Call `backups::restore_ids(&|slot, s| restore_exact(&target.user, slot, s), &mut store.snapshots, user, &all, &opts.store_path, opts.progress, &|id| Ok(Slot::from_id(id)), &|id| value name)`.
3. `prune`, then **always** `save_env_store` when anything succeeded. `restore_ids` saved only the `users` part, without `renames`.
4. Broadcast once if anything succeeded.

Test setup: the sandbox target of Task 3; the store and `.reg` files go in a temp folder; `broadcast` = a closure counting into a `Cell<usize>`; and `USER` = `"TEST\\user"`.

- [ ] **Step 1: Write the failing tests** in `src/environment/engine.rs`:
  - `adding_a_variable_writes_sz_or_expand_sz_and_keeps_the_original`:
    - Apply `Add FOO=bar` and `Add BAR=%FOO%\x` → `FOO` has kind 1 and data `encode_text("bar")`; `BAR` has kind 2 and text `%FOO%\x`.
    - The store holds both slots with `exists: false`; one `.reg` file exists; broadcasts == 1.
  - `editing_keeps_the_type_and_the_unexpanded_text`:
    - `REG_EXPAND_SZ` `X=%USERPROFILE%\a` edited to `%USERPROFILE%\b` → kind 2 with exactly that text;
    - `REG_SZ` `Y="50% done"` edited to `"60% done"` → kind 1;
    - `REG_SZ` `Z` edited to `%TEMP%\z` → kind 2.
  - `deleting_and_restoring_brings_the_exact_bytes_back`:
    - Setup: `REG_EXPAND_SZ` `Odd` with raw data that has no NUL terminator.
    - Delete → gone. `restore_environment([slot id])` → the same kind and bytes, the slot is forgotten, and broadcasts == 2.
  - `a_rename_writes_the_new_name_first_and_restores_both_from_either_half`:
    - `OLD=v` (kind 2), apply `Rename OLD→NEW v2` → `NEW` has kind 2 and text `v2`; `OLD` is gone.
    - `renames[USER]` holds one group with both slot ids.
    - `restore_environment([NEW id])` → `OLD` is back with its exact bytes, `NEW` is gone, and `store == EnvBackupStore::default()`.
  - `a_case_only_rename_changes_the_spelling_and_restore_brings_it_back`:
    - `foo=1`, apply `Rename foo→Foo 1` → `enum_values` names include `Foo` and not `foo`. Exactly one slot is recorded, and it is kept, because the spelling differs.
    - Restore → the name is spelled `foo` again.
  - `editing_back_to_the_original_forgets_the_snapshot`: `X=a`, apply `Edit X=b`, then `Edit X=a` → `store.snapshots.users` is empty.
  - `the_apply_is_refused_when_the_registry_changed_since_the_scan`:
    - Apply `Edit GONE=1` plus a valid `Add NEW=1` → `errors` non-empty, `succeeded` empty.
    - `NEW` is absent; there is no store file and no `.reg` file; broadcasts == 0.
  - `invalid_changes_are_refused_whole`. Each of these alone is refused, with nothing written:
    - `Add "A=B"`; `Add x` + `Add X`; `Rename a→b` where `b` exists; `Edit` of a `REG_DWORD`.
  - `only_hkcu_environment_is_written`:
    - Setup: machine `SYSTEM_ENV_KEY` `Path` = `C:\s`, `{HKCU}\Volatile Environment\USERNAME` = `u`, and user `X`.
    - Apply `Add`, `Edit`, `Delete` and `Rename`, then restore everything.
    - The machine key holds exactly its one value, unchanged. The Volatile key holds exactly `USERNAME` = `u`.
  - `the_store_round_trips_with_its_rename_groups`: a missing file loads the default; a store with snapshots and a group saves then loads equal. `{"users":{}}` loads with empty `renames`.
  - `store_groups_link_find_partners_and_prune`:
    - `link(a,b)` and `link(b,c)` → one group `{a,b,c}`;
    - `with_partners(["B"])` contains `a`, `b` and `c`;
    - after `forget` of `a` and `b`, `prune` → no group left.
  - `restore_without_a_snapshot_reports_it`: an unknown id → one error containing `no original state recorded`, and broadcasts == 0.
- [ ] **Step 2: Run** `cargo test --locked environment::engine` — Expected: FAIL.
- [ ] **Step 3: Implement** `engine.rs` and the `save_then_export` signature change.
- [ ] **Step 4: Run** `cargo test --locked environment` and `cargo test --locked backups` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(environment): apply and restore with snapshots, rename pairs and one broadcast`.

### Task 5: App wiring: scan job, pending edits, jobs, confirmations and messages

**Files:**
- Create: `src/ui/environment_state.rs`
- Modify:
  - `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/confirm.rs`, `src/ui/test_support.rs`;
  - `src/i18n/messages.rs`, `src/i18n/tests.rs`;
  - both `.resx` files.

**Interfaces — Consumes:** Tasks 1-4; `tasks::BackgroundJob`; `refuse_if_busy`; `busy_or_blocked`; `record_history`; `show_report`; `confirm::named_list` and `MAX_NAMED`.

**Interfaces — Produces:**
- **`AppEnv` gains:**
  - `env_target: Arc<dyn Fn() -> EnvTarget + Send + Sync>`;
  - `env_store_path: PathBuf`;
  - `notify_environment: Arc<dyn Fn() + Send + Sync>`;
  - `process_var: fn(&str) -> Option<String>`.
  - Production, in `system()`: `EnvTarget::current_user`, `default_env_store_path()`, `Arc::new(broadcast_environment)` and `environment::process_var`.
- **`TempIcons`:**
  - a new field `broadcasts: Arc<AtomicUsize>`;
  - in `env()`: the sandbox `{sb}\HKCU` / `{sb}\HKLM`, `dir/environment-backups.json`, `notify_environment` incrementing `broadcasts`, and `process_var: |_| None`.

  Helpers. They must cover every key a later task's tests seed:
  - `seed_env(&self, machine: bool, name: &str, kind: u32, value: &str)`: a text value under `{sb}\HKCU\Environment`, or under `{sb}\HKLM\` + `SYSTEM_ENV_KEY`;
  - `seed_env_raw(&self, name: &str, value: winreg::RegValue)`: any type, user side;
  - `seed_volatile(&self, name: &str, value: &str)`: under `{sb}\HKCU\Volatile Environment`;
  - `user_env(&self, name: &str) -> Option<(u32, String)>`: kind and decoded text;
  - `user_env_names(&self) -> Vec<String>`: the stored spellings;
  - `broadcast_count(&self) -> usize`;
  - `run_until_env_scanned(app, ctx)`, like `run_until_startup_scanned`.
- **`WinTuneApp` fields:**
  - `env_store: EnvBackupStore` and `env_store_error: Option<LoadError>`;
  - `env_scan: Option<EnvScan>`, `env_scan_job: Option<BackgroundJob<EnvScan>>` and `env_scan_needed: bool`;
  - `env_pending: EnvPending`;
  - `env_folders: HashMap<String, bool>`: the folder cache, replaced by `scan.folders` at each scan;
  - `env_search: String`;
  - `env_dialog: Option<EnvDialog>`;
  - `env_new_entry: String`.
- **`environment_state.rs`:**
  - `#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)] pub(super) enum VarKey { Existing(String), New(String) }`, with lowercase names.
  - `#[derive(Debug, Clone, Default, PartialEq)] pub(super) struct EnvPending { pub(super) path: Option<PathEdit>, pub(super) vars: BTreeMap<VarKey, VarEdit> }`.
  - `#[derive(Debug, Clone, PartialEq)] pub(super) enum EnvDialog { Variable { key: Option<VarKey>, name: String, value: String }, Entry { index: Option<usize>, text: String } }`, used by Task 6.
  - `reload_env_store`, `request_env_scan` and `pump_env_scan(ctx)`, shaped like the Startup ones:
    - the scan uses `env_store.snapshots`, `env.user_label` and `env.process_var`;
    - a panicked scan shows the toast `env.scan_failed`;
    - a scan never starts while `is_busy()`.
  - When a scan finishes, `env_folders = scan.folders.clone()`, and:
    - the `Path` edit is dropped when `path_change(edit, scan.user_path())` is `None`. Otherwise it is **kept as the full wanted list**, `started_from` included;
    - each var edit is dropped when `var_change(edit, current)` is `None`. `current` is `scan.user_var(original)` for `Existing`, and `None` for `New`.
  - `env_changes(&self) -> Vec<EnvChange>`: the `Path` change first, then the var changes in key order. It is empty before the first scan. `env_pending_count(&self) -> usize` = its length (a `Path` edit counts as one). `discard_env_changes(&mut self)` resets `env_pending`.
  - `env_path_edit_mut(&mut self) -> Option<&mut PathEdit>`: it starts the edit from the scanned user `Path` (`None` before the first scan, or when the `Path` is not text).
  - `env_path_changed_elsewhere(&self) -> bool`: an edit exists and its `started_from` differs from the scanned user `Path` value.
  - `env_resolver(&self) -> Resolver`: the scanned user variables with the pending var edits applied (adds, edits and renames; deletes removed), the system variables, and `env.process_var`.
  - `env_path_infos(&mut self) -> Vec<EntryInfo>`: `analyze_entries` of the wanted list (the edit's `texts()`, else the scanned entries), against `system_path_entries()` with `env_resolver()`. `exists` reads `env_folders`. A missing key is checked once with `Path::is_dir` on the UI thread and cached.
  - `env_slot_name(&self, slot_id: &str) -> String`: the value name inside the slot id.
  - Calls: `new()` calls `reload_env_store()` and `request_env_scan()`; `ui()` calls `pump_env_scan(ctx)` after `pump_startup_scan`.
- **`jobs.rs`:**
  - `EnvOutcome { report, store: EnvBackupStore, restored: bool }`.
  - `RunningJob::Environment { job, restore }`, added to every arm:
    - `title_key`: `job.apply_environment` / `job.restore_environment`;
    - `name_key`: `job.name.apply_environment` / `job.name.restore_environment`;
    - `history_label`: `("Environment variables", "Apply" | "Restore")`.
  - The keys test near line 1281 lists the four new keys.
  - `env_blocked_reason(&self) -> Option<String>`: the `tweaks.blocked` text of an unreadable store.
  - `start_env_apply(ctx)`: `refuse_if_busy` → the blocked reason (error toast) → no changes, do nothing → `lines = env_apply_lines(&changes)`. Non-empty lines → `request_apply_environment(lines)`; otherwise `run_env_apply(ctx)`.
  - `env_apply_lines(&self, changes: &[EnvChange]) -> Vec<String>`, in this order:
    1. `messages::env_warning` of each `change_warnings(changes, merged)`, then `PathChangedElsewhere` when `env_path_changed_elsewhere()`.
       - `merged` is `Some` only when a change writes `Path`.
       - It is the UTF-16 length of the expanded system `Path`, plus `;`, plus the expanded new user `Path`. The system part and its `;` are left out when the system `Path` is empty.
    2. `env.confirm.deleted` for each `Delete` that is not `Path`.
    3. `env.confirm.path_removed` with `removed_entries(scanned user entries, wanted texts)`, joined by `\n`, when that list is non-empty.
  - `run_env_apply(ctx)`: busy and blocked re-checked, then spawn `apply_environment` with `env_changes()`.
  - `start_env_restore(ctx, ids)`: blocked only by the store reason; spawns `restore_environment`.
  - Both jobs pass `EnvOptions { broadcast: &*env.notify_environment, .. }`.
  - `finish_env_job`:
    - it replaces `env_store`, and resets `env_pending` only on a clean apply;
    - `show_report` uses `ReportKind::AppliedEnvironment` (`report.applied_environment`) or `ReportKind::RestoredEnvironment` (`report.restored_environment`);
    - it calls `record_history("Environment variables", …)`.
  - `poll_job` also ends with `request_env_scan()`; its panic branch also calls `reload_env_store()`.
- **`confirm.rs`:**
  - `ConfirmAction::ApplyEnvironment` and `ConfirmAction::RestoreEnvironment(Vec<String>)`.
  - `request_apply_environment(&mut self, lines: &[String])`: `env.apply_title`, `env.apply_message` with `{0}` = the lines joined by `\n\n`, and the confirm label `tweaks.apply`.
  - `request_restore_environment(&mut self, ids: Vec<String>)`: empty ids do nothing.
    - `all = env_store.with_partners(user, &ids)`, named with `named_list(…, "env.restore_many")`.
    - When `all` holds the `Path` slot (`is_path_name` of its value name), the message adds the restore diff. `original` = `split_entries` of the recorded snapshot when it exists and is text, else empty; `current` = `scanned user_path_entries()`. It appends `env.confirm.path_removed` with `removed_entries(current, original)` and `env.confirm.path_added` with `removed_entries(original, current)`, each only when non-empty.
    - The action is `RestoreEnvironment(all)`.
  - `resolve_confirm`: `ApplyEnvironment` → `run_env_apply`; `RestoreEnvironment(ids)` → `start_env_restore`.
  - `handle_shortcuts` (`polish.rs`) also returns early while `env_dialog.is_some()`.
- **`messages.rs`:** `env_problem(tr, &EnvProblem) -> String`, `env_warning(tr, &EnvWarning) -> String`, and `history_module` maps `"Environment variables"` to `history.module.environment`.

**Keys added in this task (EN / FR):**

| Key | English | French |
|---|---|---|
| `job.apply_environment` | Applying environment changes… | Application des changements de variables… |
| `job.restore_environment` | Restoring environment variables… | Restauration des variables d'environnement… |
| `job.name.apply_environment` | Applying environment changes | L'application des changements de variables |
| `job.name.restore_environment` | Restoring environment variables | La restauration des variables d'environnement |
| `report.applied_environment` | {0} environment change(s) applied | {0} changement(s) de variables appliqué(s) |
| `report.restored_environment` | {0} variable(s) restored | {0} variable(s) restaurée(s) |
| `history.module.environment` | Environment variables | Variables d'environnement |
| `env.scan_failed` | Could not read the environment variables: {0} | Impossible de lire les variables d'environnement : {0} |
| `env.apply_title` | Apply these changes? | Appliquer ces changements ? |
| `env.apply_message` | {0} | {0} |
| `env.restore_title` | Restore original variables? | Restaurer les variables d'origine ? |
| `env.restore_message` | {0} will get back the value they had before WinTune changed them. | {0} retrouveront la valeur qu'elles avaient avant que WinTune les modifie. |
| `env.restore_many` | {0} variables | {0} variables |
| `env.confirm.deleted` | {0} will be deleted. | {0} sera supprimée. |
| `env.confirm.path_removed` | Removed from Path:\n{0} | Retirés de Path :\n{0} |
| `env.confirm.path_added` | Added back to Path:\n{0} | Remis dans Path :\n{0} |
| `env.warning.removes` | {0} is used by Windows and many programs. Deleting or renaming it can break them. | {0} est utilisée par Windows et de nombreux programmes. La supprimer ou la renommer peut les casser. |
| `env.warning.path_too_long` | With the system Path, your Path will be {0} characters long. Command Prompt cannot use a Path over 8,191 characters. | Avec le Path du système, votre Path fera {0} caractères. L'invite de commandes ne peut pas utiliser un Path de plus de 8 191 caractères. |
| `env.warning.path_changed` | Path was changed by another program since you started editing. Apply writes your list, so folders that program added will be removed. | Path a été modifié par un autre programme depuis le début de vos changements. Appliquer écrit votre liste : les dossiers ajoutés par ce programme seront retirés. |
| `env.problem.empty_name` | Enter a name. | Saisissez un nom. |
| `env.problem.name_equals` | A name cannot contain =. | Un nom ne peut pas contenir =. |
| `env.problem.name_nul` | A name cannot contain a null character. | Un nom ne peut pas contenir de caractère nul. |
| `env.problem.name_too_long` | A name can have at most 255 characters. | Un nom compte au plus 255 caractères. |
| `env.problem.name_taken` | You already have a variable named {0}. | Vous avez déjà une variable nommée {0}. |
| `env.problem.path_name` | Edit Path in the Your Path section. | Modifiez Path dans la section Votre Path. |
| `env.problem.empty_value` | Enter a value. To remove a variable, delete it. | Saisissez une valeur. Pour retirer une variable, supprimez-la. |
| `env.problem.value_too_long` | A value can have at most 32,767 characters. | Une valeur compte au plus 32 767 caractères. |
| `env.problem.value_nul` | A value cannot contain a null character. | Une valeur ne peut pas contenir de caractère nul. |
| `env.problem.empty_entry` | Enter a folder. | Saisissez un dossier. |
| `env.problem.entry_semicolon` | A folder cannot contain ;. | Un dossier ne peut pas contenir ;. |
| `env.problem.gone` | {0} no longer exists. | {0} n'existe plus. |
| `env.problem.not_text` | {0} is not text, so WinTune does not change it. | {0} n'est pas du texte : WinTune ne la modifie pas. |
| `env.problem.twice` | {0} is changed twice in one Apply. | {0} est modifiée deux fois dans la même application. |

- [ ] **Step 1: Write the failing tests:**
  - `environment_state.rs`:
    - `the_environment_scan_runs_in_background_and_waits_for_jobs`: like the Startup test, with `seed_env(false, "FOO", 1, "x")`.
    - `a_scan_prunes_matched_edits_and_keeps_the_path_list`:
      - Seed `Path` = `A;B`. Start an edit and `move_down(0)`. The test writes `B;A` itself, then scans → `env_pending.path == None`.
      - Seed `A;B` again, start an edit and `move_down(0)`, and the test writes `A;B;C` → after the scan the edit is kept, `env_path_changed_elsewhere()` is true, and `env_changes()` holds `Edit Path = "B;A"`.
    - `env_changes_combine_path_and_variables`: a `Path` add plus a rename of `FOO` to `BAR` → `[Edit { "Path", … }, Rename { "FOO", "BAR", … }]`.
    - `the_resolver_uses_pending_values`:
      - Setup: `TOOLS` = `<dir>\one`, `Path` = `%TOOLS%\bin`, and a pending edit of `TOOLS` to `<dir>\two`.
      - `env_path_infos()[0].expanded == <dir>\two\bin`.
  - `jobs.rs`:
    - `an_environment_apply_writes_broadcasts_once_logs_and_clears_pending`:
      - pending `New("foo")` → `("FOO","1")`; `start_env_apply`, then `run_until_idle`;
      - `user_env("FOO") == Some((1, "1"))`; `broadcast_count() == 1`;
      - the last history entry is `"Environment variables"` / `"Apply"`; `env_pending` is empty.
    - `an_untouched_path_is_never_written`:
      - Seed `Path` = `A;;B;` as `REG_SZ`, plus `X` = `1`. Apply an edit of `X` only.
      - The raw `Path` is still exactly `A;;B;`, of kind 1.
    - `a_refused_environment_apply_keeps_the_choices`:
      - Scan, then set a pending edit of `X`. The test deletes `X` from the registry, then apply.
      - **Right after `run_until_idle` (before the next scan prunes)**: the toast is an error, `env_pending.vars` still holds `Existing("x")`, `broadcast_count() == 0`, and no store file exists.
    - `an_unreadable_environment_store_blocks_apply_and_restore`:
      - **First set a real pending change and assert `env_pending_count() == 1`.** Then set `env_store_error = Some(corrupt(..))`.
      - `start_env_apply` and `start_env_restore` start no job, and both show the blocked toast.
    - `a_panicking_environment_job_is_logged_and_reloads_the_store`, like the Startup one.
  - `confirm.rs`:
    - `deleting_a_protected_variable_asks_first`:
      - pending delete of `TEMP` → `start_env_apply`;
      - `pending_confirm.action == ApplyEnvironment`, and the message contains `messages::env_warning(tr, &Removes("TEMP"))`;
      - `resolve_confirm(true)` and `run_until_idle` → `TEMP` is gone.
    - `a_plain_edit_applies_without_asking`: an edit of `FOO` → no confirm, and a job runs.
    - `a_path_changed_elsewhere_is_named_in_the_confirmation`:
      - Edit `Path` `A;B` to `B;A`. The test writes `A;B;C`, then scans.
      - `start_env_apply` → the message contains the `PathChangedElsewhere` text and `C` under `env.confirm.path_removed`.
    - `restoring_path_names_removed_and_added_back_entries`:
      - Seed `Path` = `A;B`. Apply `A;C`. The test then writes `A;C;D`, and the app rescans.
      - `request_restore_environment([path slot id])` → the message contains `C` and `D` under the removed text, and `B` under the added-back text.
      - Resolve → `Path` is exactly `A;B`, with its original kind.
    - `restoring_one_half_of_a_rename_restores_both`:
      - Apply a rename of `OLD` to `NEW`. `request_restore_environment([NEW slot id])` → the action holds both ids, and the message names `OLD` and `NEW`.
      - Resolve → `OLD` is back and `NEW` is gone.
  - `src/i18n/tests.rs`: `every_environment_message_translates_in_every_shipped_language`, shaped like the Startup test:
    - every `EnvProblem` variant (the `String` ones with `"X"`, asserting the text contains `X`) and every `EnvWarning` (`Removes("X")`, `PathTooLong(9000)` containing `9000`);
    - `history_module(tr, "Environment variables")` **must equal `tr.t("history.module.environment")`** and must not be the key: an unmapped label passes through unchanged, so "non-empty and not the key" alone proves nothing;
    - every text is non-empty, unique, has no `{`, and does not start with `env.` or `history.`.
- [ ] **Step 2: Run** `cargo test --locked ui::` and `cargo test --locked i18n` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.** Add every key above to both `.resx` files.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(environment): scan job, pending edits, apply and restore jobs and confirmations`.

### Task 6: The Environment variables page

**Files:**
- Create: `src/ui/environment_tab.rs`
- Modify:
  - `src/ui/mod.rs`: `ActiveTab::Environment`; `ALL: [ActiveTab; 10]`, after `Startup`; `history_module` → `Some("Environment variables")`; `owns_action_bar`; the tab dispatch; `ui()` calls `show_env_dialog(ctx)` after `show_menu_rename_dialog`.
  - `src/ui/shell.rs`:
    - the nav item after Startup apps;
    - `open_page`: when not busy, reload the env store; on page change, request an env scan;
    - the French test.
  - `src/ui/polish.rs`: `search_box_id(ActiveTab::Environment)` → `Some(egui::Id::new("search_environment"))`.
  - `src/ui/kit/icons.rs`: `Icon::Environment` = `'\u{E943}'` (Code), and `ALL: [Icon; 22]`. If the font test says the glyph is missing, use `'\u{E756}'` (CommandPrompt).
  - **`src/ui/kit/tests.rs`**: `icon_ordinal` gets `Environment => 21`, and `assert_eq!(Icon::ALL.len(), 22)`. A new Icon variant fails here otherwise.
  - `src/ui/home.rs`: the `tile_title` arm `ActiveTab::Environment => tr.t("nav.environment")`, needed now that `_` falls back to the context menu. The tile itself comes in Task 7.
  - both `.resx` files.

**Interfaces — Consumes:** Task 5 (fields, `env_path_edit_mut`, `env_path_infos`, `env_changes`, `env_pending_count`, `discard_env_changes`, `start_env_apply`, `env_blocked_reason`, `request_restore_environment`, `EnvDialog`, `VarKey`); Tasks 1-2 (`entry_problem`, `name_problem`, `value_problem`, `names_after`, `without_missing_and_duplicates`, `EntryMark`); kit (`page_header`, `section_with_action`, `SettingRow`, `pill`, `overflow_menu`, `disclosure`, `empty_state`, `banner`, `dialog`, `page_action_bar`); `rfd::FileDialog::new().pick_folder()`, as in `settings.rs`.

**Interfaces — Produces:** `pub(super) fn show_environment_tab(&mut self, ctx: &egui::Context)` and `pub(super) fn show_env_dialog(&mut self, ctx: &egui::Context)`.

Content (spec 5.5):
- **Header:** `page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |_| {})`, then one secondary line, `env.note`. Below it, the search box `search_environment`: Ctrl+F focuses it, and it filters every section case-insensitively.
  - Path rows match on the entry text or its expanded form.
  - Variable rows match on name or value.
- **Banner:** a `user_error` shows `env.unreadable` as an error banner in place of the sections. The loading line `env.scanning` shows while `env_scan` is `None`.
- **Your Path** (`section_with_action`, title `env.path.title`). Actions, right to left:
  - `env.path.clean`: disabled with `env.path.clean_none` when no entry is missing or a duplicate. It sets the wanted list to `without_missing_and_duplicates`, which turns those entries into pending removals;
  - `tweaks.restore_original`, when the store holds the `Path` slot (`find_id`) → `request_restore_environment`.

  Rows:
  - **One `SettingRow` per wanted entry, in order.** `title` = the text; `hover` = the expanded form when it differs. Pills:
    - `env.pill.not_found` (Warning);
    - `env.pill.duplicate` (Warning);
    - `env.pill.will_add` (Accent) for `EntryMark::Added`;
    - `env.pill.moved` (Accent) for `Moved`.
  - **The row's `…` menu** (`overflow_menu`) holds `env.move_up`, `env.move_down`, `env.edit` and `env.remove`. Move up is disabled on the first row and Move down on the last, with the hint `env.no_move`. Every item is disabled with the busy reason while a job runs. As on the Startup rows, actions are applied after drawing.
  - **Then one row per `removed()` entry**, with pill `env.pill.will_remove` (Accent) and no menu.
  - **With no user `Path` and no edit:** `empty_state(Icon::Environment, "env.path.empty")`.
  - **Last row: the add line.** A `TextEdit` bound to `env_new_entry` (hint `env.path.add_hint`), `env.browse`, and `env.path.add`.
    - Browse calls `rfd::FileDialog::new().pick_folder()` and puts the folder in the field.
    - Add is disabled with `messages::env_problem(entry_problem(..))`. On click it calls `add(text)` on `env_path_edit_mut()` (starting an edit, even from no `Path`) and clears the field.
- **Your variables** (`section_with_action`, title `env.vars.title`, action `env.vars.add` → `EnvDialog::Variable { key: None, .. }`).
  - **Rows:** every scanned user variable except `Path`, plus pending `New` ones, sorted by lowercase name.
    - `title` = the current name, or the wanted name of a new variable.
    - `description` = the value, cut to 80 characters plus `…`; `hover` = the full value.
  - **Pills:**
    - `env.pill.uses_refs` "Uses %…%" (Neutral) when `uses_references()`;
    - `env.pill.read_only` (Neutral) when not text;
    - pending: `env.pill.will_add` (new), `env.pill.will_change` (edit), `env.pill.will_rename` with `{0}` = the new name (rename), or `env.pill.will_delete` (delete).
  - **`…` menu:**
    - `env.edit` opens `EnvDialog::Variable` with the wanted or current name and value;
    - `env.delete` sets `wanted: None`, or drops a pending `New`;
    - `tweaks.restore_original`, when `has_snapshot` → `request_restore_environment(vec![var.slot().id()])`.

    On a non-text variable, Edit and Delete are disabled with `env.not_text`.
  - **When there is no variable at all:** `env.vars.empty`.
- **For every account (read only):** `kit::disclosure(ui, "env_system", &tr.t("env.system.title"), false, …)`.
  - Its rows are the system variables except `Path` (name, shortened value, full on hover, no menu), then a `env.system.path` row followed by one row per system `Path` entry, with its expanded form on hover. A `system_error` shows `env.system.unreadable`.
- **Search:** `env.no_match` when the search hides every row of every section.
- **Dialog** (`show_env_dialog`), built with `kit::dialog` like the rename dialog. Escape cancels; Enter in the last field accepts when allowed.
  - **`Variable`:**
    - title `env.var_title_add` or `env.var_title_edit`; fields labelled `env.name` and `env.value`;
    - the problem line is the first of: `name_problem`; `PathName` when `is_path_name(name)`; `NameTaken` when the trimmed name is in `names_after(scan.user, env_changes() without this key's own change)`, ignoring case; `value_problem`;
    - OK (`env.dialog_ok`) stores the trimmed name: `VarEdit { original, wanted: Some((name, value)) }` under `Existing(original lowercase)`, or under `New(name lowercase)` for an added variable. An add that is renamed in the dialog moves its key.
  - **`Entry`:** title `env.entry_title`, one field, the problem `entry_problem`; OK calls `edit(i, text)`.
- **Action bar** (`page_action_bar`):
  - status `env.pending` when `env_pending_count() > 0`, else `env.up_to_date`;
  - Discard (`tweaks.discard`) when `env_pending != EnvPending::default()`;
  - Apply (`tweaks.apply`) → `start_env_apply`. It is disabled with `env_blocked_reason`, the busy reason, or `env.nothing_to_apply`.

**Keys added in this task (EN / FR):**

| Key | English | French |
|---|---|---|
| `nav.environment` | Environment variables | Variables d'environnement |
| `env.subtitle` | Your variables and the folders Windows searches for programs. | Vos variables et les dossiers où Windows cherche les programmes. |
| `env.note` | New programs see your changes. Windows that are already open, such as terminals, keep the old values until you restart them. | Les nouveaux programmes voient vos changements. Les fenêtres déjà ouvertes, comme les terminaux, gardent les anciennes valeurs jusqu'à leur redémarrage. |
| `env.search` | Search variables and folders | Rechercher des variables et des dossiers |
| `env.scanning` | Reading your variables… | Lecture de vos variables… |
| `env.unreadable` | Cannot read your variables: {0} | Impossible de lire vos variables : {0} |
| `env.no_match` | Nothing matches your search. | Rien ne correspond à votre recherche. |
| `env.path.title` | Your Path | Votre Path |
| `env.path.empty` | You have no Path of your own yet. Add a first folder below. | Vous n'avez pas encore de Path à vous. Ajoutez un premier dossier ci-dessous. |
| `env.path.clean` | Remove missing and duplicates | Retirer les introuvables et les doublons |
| `env.path.clean_none` | No folder is missing or a duplicate. | Aucun dossier introuvable ni en double. |
| `env.path.add_hint` | Folder to add, for example C:\Tools\bin | Dossier à ajouter, par exemple C:\Tools\bin |
| `env.path.add` | Add to Path | Ajouter à Path |
| `env.browse` | Browse… | Parcourir… |
| `env.pill.not_found` | Folder not found | Dossier introuvable |
| `env.pill.duplicate` | Duplicate | En double |
| `env.pill.will_add` | Will be added | Sera ajouté |
| `env.pill.moved` | Moved | Déplacé |
| `env.pill.will_remove` | Will be removed | Sera retiré |
| `env.pill.uses_refs` | Uses %…% | Utilise %…% |
| `env.pill.read_only` | Read only | Lecture seule |
| `env.pill.will_change` | Will be changed | Sera modifiée |
| `env.pill.will_rename` | Will be renamed {0} | Sera renommée {0} |
| `env.pill.will_delete` | Will be deleted | Sera supprimée |
| `env.move_up` | Move up | Monter |
| `env.move_down` | Move down | Descendre |
| `env.no_move` | Already at the end of the list. | Déjà au bout de la liste. |
| `env.edit` | Edit | Modifier |
| `env.remove` | Remove | Retirer |
| `env.delete` | Delete | Supprimer |
| `env.not_text` | This value is not text. WinTune shows it but does not change it. | Cette valeur n'est pas du texte. WinTune l'affiche mais ne la modifie pas. |
| `env.vars.title` | Your variables | Vos variables |
| `env.vars.add` | Add a variable | Ajouter une variable |
| `env.vars.empty` | You have no other variable. | Vous n'avez pas d'autre variable. |
| `env.system.title` | For every account (read only) | Pour tous les comptes (lecture seule) |
| `env.system.path` | Path (every account) | Path (tous les comptes) |
| `env.system.unreadable` | Cannot read the system variables: {0} | Impossible de lire les variables du système : {0} |
| `env.var_title_add` | Add a variable | Ajouter une variable |
| `env.var_title_edit` | Edit variable | Modifier la variable |
| `env.entry_title` | Edit folder | Modifier le dossier |
| `env.name` | Name | Nom |
| `env.value` | Value | Valeur |
| `env.dialog_ok` | OK | OK |
| `env.pending` | {0} changes not applied | {0} changement(s) non appliqué(s) |
| `env.up_to_date` | All changes applied | Tous les changements sont appliqués |
| `env.nothing_to_apply` | Nothing to apply | Rien à appliquer |

- [ ] **Step 1: Write the failing headless tests** in `environment_tab.rs`. Seed through `TempIcons`, finish the scan, and set `active_tab`. Use real temp folders for the entries that must exist.
  - `the_path_section_lists_entries_with_their_pills`:
    - Setup: `TOOLS` = `<dir>\tools` (created); `Path` = `<dir>\tools;%TOOLS%;C:\Nowhere\x;%UNKNOWN%\y`.
    - All four texts are drawn. "Duplicate" is drawn once and "Folder not found" exactly once.
    - Hovering the `%TOOLS%` row shows `<dir>\tools`.
  - `a_duplicate_of_the_system_path_is_flagged`: system `Path` = `<dir>\tools`; user `Path` = `<dir>\TOOLS\` → "Duplicate".
  - `moving_an_entry_marks_it_and_apply_writes_the_new_order`:
    - Setup: `TOOLS` = `<dir>`; `Path` = `<dir>\a;%TOOLS%\b` (kind 2). Open the second row's `…`, click "Move up" → "Moved" is drawn.
    - Click Apply, then `run_until_idle` → `user_env("Path") == Some((2, "%TOOLS%\b;<dir>\a"))`, with `%TOOLS%` unexpanded.
  - `remove_missing_and_duplicates_makes_pending_removals`:
    - Setup: the same `TOOLS` and `Path` as the first test.
    - Click the button → "Will be removed" is drawn twice, and the status shows `env.pending` with 1.
    - Apply → a confirm opens (removals). Accept → the registry `Path` is exactly `<dir>\tools;%UNKNOWN%\y`.
  - `adding_a_folder_from_the_field`:
    - `env_new_entry = "C:\x;y"` → the Add button is disabled, and its hover shows the `EntryHasSemicolon` text.
    - `"<dir>\new"` → click Add → "Will be added" is drawn and the field is empty.
  - `no_user_path_offers_the_first_folder`: the `env.path.empty` text is drawn. Add a folder and Apply → `user_env("Path")` has kind 2.
  - `the_variable_dialog_adds_and_refuses_bad_names`:
    - Click "Add a variable" → `env_dialog` is `Variable { key: None, .. }`.
    - Set the name to `A=B` → the `NameHasEquals` text is drawn and OK is disabled.
    - Set it to `FOO`, where `FOO` exists → `NameTaken` text; set it to `path` → `PathName` text.
    - Set it to ` NEW ` with value `%FOO%\x`, then OK → `env_pending.vars[New("new")]` wants `("NEW", …)`.
    - Apply → `user_env("NEW") == Some((2, "%FOO%\x"))`.
  - `a_non_text_variable_is_read_only`: a `REG_DWORD` `D` → "Read only" is drawn, and its menu's Edit and Delete are disabled with `env.not_text`.
  - `the_system_section_is_collapsed_and_read_only`:
    - A system `JAVA_HOME` is not drawn at first.
    - Click `env.system.title` → it is drawn, and no `…` button sits on its row.
  - `ctrl_f_focuses_the_environment_search` and `search_filters_path_entries_and_variables`.
  - `the_page_shows_the_loading_line_until_the_first_scan`.
  - `opening_the_page_requests_a_scan`: `open_page(ActiveTab::Environment)` from Home → `env_scan_needed`.
  - `src/ui/shell.rs`: extend `shell_pages_draw_no_english_only_text_in_french` to draw the Environment page with these states:
    - a `Path` with a missing entry, a duplicate and a `%VAR%` entry;
    - a `REG_EXPAND_SZ` variable and a non-text one;
    - a pending move, a pending removal, a pending rename and a pending delete, so that Discard and every pending pill show;
    - the system section opened;
    - the Variable dialog and then the Entry dialog drawn;
    - the Apply confirmation drawn.

    Add the new keys to its `positive` list.
- [ ] **Step 2: Run** `cargo test --locked environment_tab` — Expected: FAIL.
- [ ] **Step 3: Implement.** Fix every non-exhaustive `match` on `ActiveTab` or `Icon`.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS, including `every_tab_renders_without_panicking` over the 10 tabs and the kit icon tests.
- [ ] **Step 5: Commit** — `feat(environment): Environment variables page with the Path list editor`.

### Task 7: History and Home

**Files:**
- Modify:
  - `src/ui/originals.rs`, `src/ui/history_panel.rs`, `src/ui/home.rs`;
  - `src/ui/shell.rs`: `open_page(History)` also reloads the env store; the French test;
  - both `.resx` files.

**Interfaces — Consumes:** Task 5 (`env_store`, `env_store_error`, `env_slot_name`, `request_restore_environment`); Task 6 (`ActiveTab::Environment`, `Icon::Environment`).

**Interfaces — Produces:**
- **Saved originals:**
  - `OriginalsModule::Environment`, after `Startup`, titled `history.module.environment`;
  - `originals_of(Environment)` gives one row per `env_store.snapshots` backup of `env.user_label`: `key` = slot id, `title` = `env_slot_name(key)`, `description` = `originals.saved_on`;
  - `RestoreRequest::Environment(Vec<String>)` goes to `request_restore_environment`; "Restore all" passes every key;
  - an unreadable store shows the `originals.unreadable` banner in place of the group.
- **History filter:** `HistoryFilter::Environment` matches `"Environment variables"`. Its chip is labelled `history.module.environment` and sits after Startup apps.
- **Home tile:** `ModuleSummary { page: ActiveTab::Environment, icon: Icon::Environment, status, drift: 0 }`, after the Startup tile.
  - `status` = `home.environment_status` "{0} variables, {1} folders in Path" / "{0} variable(s), {1} dossier(s) dans Path".
  - `{0}` = the number of scanned user variables (`Path` included); `{1}` = `user_path_entries().len()`. Both are 0 until the first scan.

- [ ] **Step 1: Write the failing tests:**
  - `originals.rs`: `a_changed_variable_is_listed_in_saved_originals_and_restores`:
    - apply an edit of `FOO`;
    - the Environment variables group shows a row titled `FOO`;
    - click its Restore and confirm, then `run_until_idle` → `FOO` has its old value, the group is gone, and `broadcast_count() == 2`.
  - `originals.rs`: `a_renamed_pair_restores_together_from_history`: rename `OLD` to `NEW`, then click Restore on the `OLD` row → the confirm action holds both ids.
  - `originals.rs`: `an_unreadable_environment_store_shows_a_banner`.
  - `history_panel.rs`: `the_environment_filter_shows_only_environment_operations`.
  - `home.rs`: `the_environment_tile_counts_variables_and_path_folders`:
    - before a scan, the tile shows `home.environment_status` with `0, 0`;
    - with `FOO`, `BAR` and `Path` = `A;B` scanned → `3, 2`;
    - clicking the tile opens `ActiveTab::Environment`.
  - `shell.rs`: in the French test, History shows an environment original and an "Environment variables" operation, and Home shows the tile.
- [ ] **Step 2: Run** `cargo test --locked ui::` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(environment): Saved originals, History filter and Home tile`.

### Task 8: Docs, drift test and the owner's checklist

**Files:**
- Modify: `README.md`, `ARCHITECTURE.md`, `docs/registry-reference.md`, `src/environment/mod.rs` (the test).

- [ ] **Step 1: Write the failing drift test** `the_registry_reference_names_every_environment_location` in `src/environment/mod.rs`.
  - With `include_str!("../../docs/registry-reference.md")`, assert that the doc contains each of these **between backticks, in its terminated form**: `` `HKCU\Environment` ``, `` `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment` `` and `` `HKCU\Volatile Environment` ``. Build each from the constants, for example ``format!("`HKCU\\{USER_ENV_KEY}`")``.
  - A bare `Environment` would also match the `lParam` text and every other key, so the test would prove nothing. The closing backtick also keeps `HKCU\Environment` from matching inside a longer path.
  - It also asserts `` `environment-backups.json` ``, `` `WM_SETTINGCHANGE` `` and the heading text `Pre-merge checklist (Environment variables)`.

  Run `cargo test --locked the_registry_reference_names_every_environment_location` — Expected: FAIL.
- [ ] **Step 2: `docs/registry-reference.md`.** Add a `## Environment variables` section before `## Sources`, with these parts:
  - **What WinTune reads:**
    - `HKCU\Environment` (raw, with its type);
    - `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment`, read only;
    - `HKCU\Volatile Environment`, never read or written;
    - the process environment, for display expansion only.
  - **What WinTune writes:**
    - only values of `HKCU\Environment`, as `REG_SZ` or `REG_EXPAND_SZ` by the spec 5.2 rule;
    - a rename is a write of the new name and a delete of the old one;
    - a case-only rename is a delete then a write;
    - then one `WM_SETTINGCHANGE` with "Environment" (`SMTO_ABORTIFHUNG`, 5 s);
    - never HKLM, never `setx`.
  - **Evidence,** with tags as in spec section 3:
    - DOCUMENTED: the locations, the types, the broadcast, the 32,767 limit, the `=` rule and the 8,191 command-line limit, with the Learn URLs from `recon-envvars.md`;
    - FOLKLORE: the merge order, case-insensitive names and the `SMTO_ABORTIFHUNG` recipe;
    - OBSERVED: the shapes seen on Windows 11 26300 (spec 3).
  - **`#### Pre-merge checklist (Environment variables)`:** spec section 7, items 1-5, as checkboxes. The owner records each result there before merging.
- [ ] **Step 3: README and ARCHITECTURE.**
  - **README:**
    - the feature bullet;
    - a `## Environment variables` section: the Path list, the missing and duplicate pills, "Remove missing and duplicates", the system variables read only, "new programs see the change, open terminals do not", and why `setx` is not used;
    - the Safety and undo paragraph (Saved originals "Environment variables", a rename restored as one);
    - the Data files table (`environment-backups.json`);
    - the Code layout table (`src/environment/`);
    - the Roadmap, if it lists this module.
  - **ARCHITECTURE:**
    - the store table row: snapshots keyed by user, then by slot id (`Environment` value), plus `renames` groups;
    - a `## Environment variables engine` section: the pure `path.rs` and `rules.rs`, the scan, the apply order, the rename pairs, the case-only rename, and the injected broadcast;
    - the UI paragraph (modules, Home tiles).
- [ ] **Step 4: Run** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `rtk bypass cargo test --locked` and `cargo build --release --locked` — Expected: all clean, with the drift test passing.
- [ ] **Step 5: Commit** — `docs(environment): README, ARCHITECTURE, registry reference and pre-merge checklist`.

## Rulings the spec left open

Each ruling below gives the decision, then what it costs if it is wrong.

1. **Apply asks first only when it deletes a variable, removes a `Path` entry, or carries a warning.** A plain add or edit applies at once (History undoes it). If wrong: one dialog more or less.
2. **Rename pairs are stored as `renames` groups next to `users` in `environment-backups.json`** (`#[serde(flatten)]`). For that, `backups::save_then_export` now takes `&impl Serialize`. `restore_ids` saves only `users`, so the engine saves the full store again right after. If wrong: if WinTune dies between those two saves, the other pairs restore one half at a time.
3. **A case-only rename, and a restore of an old spelling, delete the value and then write it.** That is the only way to change a stored name's case. "Back at original" includes the spelling. If wrong: a crash between the delete and the write loses that one variable until Restore or the `.reg` puts it back.
4. **`split_entries` drops empty and whitespace-only entries and keeps all others verbatim**, splitting naively on `;`, as Windows' dialog does. If wrong: a quoted entry containing `;` shows as two rows. Its text is still written back unchanged unless the user edits one half.
5. **Only absolute local paths (`X:\…`) are checked for existence.** UNC and relative entries are never flagged. If wrong: a dead network folder gets no "Folder not found".
6. **A small expander (`Resolver`) is used, not `ExpandEnvironmentStringsW`.** It looks up user variables (with pending edits), then system ones, then the process environment, at most 8 levels deep. It expands entries whatever `Path`'s type. If wrong: on exotic setups, the displayed expansion differs from what Windows builds. This is display only.
7. **Lengths count UTF-16 units** (255, 32,767 and 8,191). The 8,191 warning measures the expanded system `Path` + `;` + the expanded new user `Path`, and only when the Apply writes `Path`. If wrong: the warning fires a few characters early or late.
8. **Every change made against a variable that has vanished since the scan refuses the whole Apply (`Gone`).** This covers an edit, a delete and a rename. The next scan drops those edits. If wrong: the user clicks Apply twice.
9. **A `Path` list edit counts as one change** in "N changes not applied". If wrong: only the number differs.
10. **The Move up, Move down, Edit and Remove actions of a `Path` row sit in its `…` menu**, the kit's row pattern. If wrong: moving an entry far takes two clicks per step.
11. **The variable dialog refuses the name `Path`**, so `Path` is edited only in its own section and cannot be renamed from WinTune. If wrong: renaming `Path` needs another dialog path.
12. **Every change, deletes included, counts in `succeeded`**, and the toast says "N environment change(s) applied". The report lines stay English. If wrong: the wording changes.
13. **Browse fills the Add field and does not add on its own.** If wrong: one more click.
14. **"Moved" is the entry the user moved**, as tracked by `PathEdit`, not a list comparison: swapping two neighbours marks only the one the user moved. If wrong: the marks differ.
15. **Removed entries are listed after the `Path` rows with "Will be removed"**, and only Discard brings them back. If wrong: no per-row undo.
16. **The type rule applies to `Path` like any value:** an edited `REG_SZ` `Path` stays `REG_SZ` unless the joined text has a reference. If wrong: a `REG_SZ` `Path` without references is not upgraded. Windows reads it the same way.
17. **The restore diff and the confirmation's removed list compare entries as written** (exact text, as multisets). If wrong: an entry rewritten only by case or by a trailing `\` shows as removed plus added back.
18. **An unreadable `HKCU\Environment` shows a banner in place of the sections.** If wrong: nothing breaks, since nothing can be edited.
19. **The Home tile counts `Path` among the variables.** If wrong: the number differs by one.
20. **The Edit dialog trims the name and keeps the value verbatim.** If wrong: a value with deliberate leading spaces is kept, which is what the user typed.
21. **`EnvProblem::Twice` and `Gone` are engine-side refusals the page cannot normally produce.** They are still enums with keys, so a refused Apply's toast never shows a raw key. If wrong: a few unused translations.

## Not in this PR

- Editing system variables (HKLM), and `Volatile Environment` (spec 2, non-goals).
- Import or export of variable sets (spec 2).
- Changing the environment of programs already running (Windows cannot).
