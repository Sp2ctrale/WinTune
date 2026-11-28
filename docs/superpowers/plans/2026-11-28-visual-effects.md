# Visual effects Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Visual effects" page (French "Effets visuels") that shows and changes the 18 settings of spec 5.1 in six sections, applies each one live through `SystemParametersInfoW`, checks each write by reading it back, keeps the UI effects master's companions safe, and undoes everything from a row or from History.

**Architecture:** First a small shared layer, `src/sysparams/`, takes what Mouse & keyboard and this module both need: `SettingValue` / `Shape`, `ApiError` and the raw `SystemParametersInfoW` GET/SET executor (with the new `ANIMATIONINFO` family), the account check, the snapshot store type (with an optional `companion_of`) and the `.reg` value export. Mouse & keyboard re-exports the moved names, so its code, tests and store file do not change. Then a new `src/visual_effects/`:
- `catalog.rs`: pure. The 18 ids, sections, controls, page ranges, defaults, gates, the master's companions, the write order, and the problem / failure enums.
- `backend.rs`: the injectable `EffectsApi`, the real `SpiEffects` (one descriptor per id), the test `FakeEffects` (with the two FOLKLORE master behaviours of spec 3 as switches).
- `store.rs`: `visual-effects-backups.json` and the `.reg` of `Control Panel\Desktop` and `\WindowMetrics`.
- `engine.rs`: scan, Apply (account → validate → read, companions included → snapshot → store + `.reg` → write in master order + read back → forget) and Restore.

The UI copies Mouse & keyboard (background scan, pending values until Apply, one `RunningJob` variant, a Saved originals group, a Home tile). `EffectId::key` is the join key between row, pending value, store and History.

**Tech Stack:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52 (read-only `.reg`), windows-sys 0.59, serde. No new crate.

**Spec:** `docs/superpowers/specs/2026-11-28-visual-effects-design.md` (binding, every **(call)** included). Evidence: its section 3, from `recon-visual-effects.md`. The owner's real-machine checklist: its section 7.

## Global Constraints

- **Versions:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59. **No new crate**, no new windows-sys feature (`ANIMATIONINFO` and every `SPI_*` are in `Win32_UI_WindowsAndMessaging`, already on).
- **Writes:** every change is a `SystemParametersInfoW` SET with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`, on a job worker thread. **Never write under `HKCU\Control Panel`** (above all `UserPreferencesMask`), `Explorer\Advanced` or `DWM`. Never restore by replaying registry bytes. A write whose readback differs is never a success.
- **Tests never call a real SET**; they use `FakeEffects` (and `FakeInput` for Mouse & keyboard). Registry tests touch only `HKCU\Software\WinTuneTest` (`regsnap::test_support::Sandbox`); files only in temp folders. **Never launch `WinTune.exe`.**
- **Texts:** every user-visible text is a literal key in both `i18n/strings.resx` and `i18n/strings.fr.resx` (French follows that file's punctuation). Engine reasons are enums (`VeProblem`, `VeFailure`, `VeIssue`) translated in `src/i18n/messages.rs`; `OperationReport` lines written to History stay English.
- **History:** module label `Visual effects`; actions `Apply` / `Restore`; store `visual-effects-backups.json` in `models::app_data_dir()`; `.reg` `reg-backups\<stamp>-visual-effects.reg`.
- **Page:** kit only, no colors, sizes or fonts on the page. Nav "Visual effects" in the Customize group, after Mouse & keyboard.
- **Commands:** run cargo from PowerShell, never from the Bash tool. Write and edit files with Write/Edit, not Bash heredocs (they mangle backslashes).
- **Before the PR (and after every task):** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`. `rtk bypass cargo test --locked` shows the full result lines.
- **Commits:** on `feat/visual-effects-1`, one per task, no attribution lines.

## Review Focus

1. **A live value outside a range or off the step** (contrast 1,250 from the ClearType Text Tuner, type 3, caret 25). Shown as it is ("Unknown (3)"), never written unless that control moves, and Restore puts it back exactly. (Task 3: `restore_sets_the_stored_value_exactly_even_outside_the_page_range`; Task 5: `an_out_of_range_live_value_is_shown_and_never_written_unless_moved`.)
2. **The master changes outside WinTune** (Settings > Accessibility > Animation effects, Performance Options) between the scan and Apply. The companions are read at Apply time, not taken from the scan; a pending value the live one now matches is dropped by the next scan. (Task 3: `companions_are_read_at_apply_time_not_from_the_scan`; Task 4: `a_scan_drops_choices_the_live_value_matches_or_cannot_read`.)
3. **A gate turned off with choices pending under it, including two levels down** (UI effects off while Menu effect is pending "Slide"). The choices it disables are dropped, the hover names the outermost gate that is off, and turning the master back on enables the rows at once. (Task 4: `turning_a_gate_off_drops_the_choices_it_disables`, `turning_the_master_on_in_the_batch_enables_its_rows_at_once`.)
4. **WinTune elevated as another administrator.** Banner instead of sections; Apply and a Restore from History are refused with no GET or SET. (Task 3: `the_wrong_account_refuses_apply_and_restore_before_any_call`; Task 4: `a_restore_from_history_as_the_wrong_account_is_refused`; Task 5: `the_wrong_account_shows_a_banner_instead_of_the_sections`.)
5. **"Restore all" over a group holding the master, its companions and a companion that became its own row.** Each id is written once, in master order, and the group ends empty. (Task 3: `restore_all_writes_each_id_once_in_master_order`.)

---

## File Structure

- **Create `src/sysparams/`:** `mod.rs` (docs, `SettingValue`, `Shape`, re-exports), `spi.rs` (`ApiError`, `SetCall`, the executor), `account.rs` (`AccountCheck`, `compare_accounts`, `session_account_check`), `store.rs` (`SpiBackup`, `SpiBackupStore`, `export_values`).
- **Create `src/visual_effects/`:** `mod.rs` (docs, re-exports, drift test), `catalog.rs`, `backend.rs`, `store.rs`, `engine.rs`.
- **Create in `src/ui/`:** `visual_effects_state.rs`, `visual_effects_tab.rs`.
- **Modify:** `src/lib.rs`; `src/mouse_keyboard/{catalog,backend,store}.rs` (Task 1 only); in `src/ui/`: `mod.rs`, `jobs.rs`, `confirm.rs`, `originals.rs`, `history_panel.rs`, `home.rs`, `shell.rs`, `polish.rs`, `test_support.rs`, `kit/icons.rs`, `kit/tests.rs`; `src/i18n/messages.rs`, `src/i18n/tests.rs`; both `.resx`; `README.md`, `ARCHITECTURE.md`, `docs/registry-reference.md`.

---

### Task 1: The shared `src/sysparams/` layer (Mouse & keyboard unchanged)

**Files:**
- Create: `src/sysparams/{mod,spi,account,store}.rs`
- Modify: `src/lib.rs` (`pub mod sysparams;`), `src/mouse_keyboard/catalog.rs`, `backend.rs`, `store.rs`

**Items that move** (bodies verbatim unless noted), and how `mouse_keyboard` keeps its names:

| Item | From | To | Mouse & keyboard keeps it through |
|---|---|---|---|
| `SettingValue` (serde attributes unchanged, `shape()`), `Shape` | `catalog.rs` | `sysparams/mod.rs` | `pub use crate::sysparams::{SettingValue, Shape};` in `catalog.rs` |
| `ApiError`, `WRONG_SHAPE` (now `pub(crate)`), `last_error`, `spi_get<T>` (now `pub(crate)`) | `backend.rs` | `sysparams/spi.rs` | `pub use crate::sysparams::ApiError;` and `use crate::sysparams::{last_error, spi_get, WRONG_SHAPE};` |
| `SetCall` (gains `Animation`), the SET body of `SpiInput::set` (becomes `spi_set`) | `backend.rs` | `sysparams/spi.rs` | `pub(crate) use crate::sysparams::SetCall;`; `SpiInput::set` = `spi_set(set_call(id, value)?)` |
| `AccountCheck`, `compare_accounts`, `session_account_check`, `process_account`, `wts_string` | `backend.rs` | `sysparams/account.rs` | `pub use crate::sysparams::{compare_accounts, session_account_check, AccountCheck};`. `impl AccountCheck { pub fn problem(&self) -> Option<MkProblem> }` **stays in `mouse_keyboard/backend.rs`** (an inherent impl in another module of the same crate), so `scan.account.problem()` still compiles everywhere |
| `MkBackup`, `MkBackupStore` and their methods | `store.rs` | `sysparams/store.rs` as `SpiBackup`, `SpiBackupStore` | `pub type MkBackup = SpiBackup; pub type MkBackupStore = SpiBackupStore;` |
| The body of `export_control_panel` | `store.rs` | `sysparams/store.rs` as `export_values` | `export_control_panel(source, reg_dir)` = `export_values(source, &CONTROL_PANEL_KEYS, reg_dir, "mouse-keyboard")` |

`CONTROL_PANEL_KEYS`, `default_mk_store_path`, `load_mk_store`, `save_mk_store`, the descriptors, `set_call`, `SpiInput`, `FakeInput` and the `test_support::Fixture` stay in `mouse_keyboard`.

**Interfaces — Produces (`crate::sysparams`):**
- `pub(crate) enum SetCall { Ui { action: u32, ui: u32 }, PvValue { action: u32, pv: usize }, PvArray { action: u32, array: [i32; 3] }, Animation { action: u32, on: bool } }` (derives as today).
- `pub(crate) fn spi_set(call: SetCall) -> Result<(), ApiError>`: always `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`; `Animation` passes a local `ANIMATIONINFO { cbSize: 8, iMinAnimate: on as i32 }` with `uiParam` 8. `FALSE` → `Err(last_error())`. Each `unsafe` has a `// SAFETY:` comment.
- `pub(crate) fn spi_get_animation(action: u32) -> Result<bool, ApiError>`: `ANIMATIONINFO { cbSize: 8, .. }`, `uiParam` 8, `fWinIni` 0; on when `iMinAnimate != 0`.
- `SpiBackup { pub timestamp: String, pub value: SettingValue, #[serde(default, skip_serializing_if = "Option::is_none")] pub companion_of: Option<String> }`.
- `SpiBackupStore { pub users: BTreeMap<String, BTreeMap<String, SpiBackup>> }` with today's `backups_of`, `has_backup`, `original`, `record_first` (sets `companion_of: None`), `forget`, plus:
  - `record_companion(&mut self, user: &str, id: &str, timestamp: &str, value: SettingValue, master: &str)`: inserts with `companion_of: Some(master)` only when `id` has no snapshot (an existing one, own or companion, is kept as it is);
  - `companion_of(&self, user: &str, id: &str) -> Option<&str>`, `clear_companion(&mut self, user: &str, id: &str)`;
  - `companions_of(&self, user: &str, master: &str) -> Vec<String>` (key order);
  - `own_backups(&self, user: &str) -> Vec<(&String, &SpiBackup)>`: entries with `companion_of == None`, key order.
- `pub fn export_values(source: &RegTarget, keys: &[&str], reg_dir: &Path, label: &str) -> Result<PathBuf, String>`: today's behaviour (values directly in each key, missing key skipped, no `[-` line, read only) to `unique_reg_file(reg_dir, "{file_stamp}-{label}")`.

- [ ] **Step 1: Pin today's bytes first.** In `mouse_keyboard/store.rs` add `the_store_file_bytes_do_not_change`: the three-entry store of `the_store_json_has_the_spec_shape`, `save_mk_store` to the fixture path, then `fs::read_to_string == serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(JSON).unwrap()).unwrap()` (JSON = that test's literal) and the file does not contain `companion_of`.
- [ ] **Step 2: Run** `cargo test --locked mouse_keyboard` — Expected: PASS (it pins the current file).
- [ ] **Step 3: Write the failing tests** in `sysparams/store.rs` and `spi.rs`:
  - `a_backup_leaves_companion_of_out_when_empty`: `record_first("U", "menu_fade", "x", Flag(true))` serializes to `{"users":{"U":{"menu_fade":{"timestamp":"x","value":{"kind":"Flag","data":true}}}}}`; `record_companion("U", "cursor_shadow", "x", Flag(false), "ui_effects")` adds `"cursor_shadow":{"timestamp":"x","value":{"kind":"Flag","data":false},"companion_of":"ui_effects"}`; both parse back equal.
  - `record_companion_keeps_an_existing_snapshot`: own `Flag(true)`, then `record_companion(.., Flag(false), "ui_effects")` → value `Flag(true)`, `companion_of == None`.
  - `companions_are_listed_cleared_and_left_out_of_own_backups`: two companions and one own row → `companions_of("U", "ui_effects")` is the two keys in order, `own_backups("U")` is the own row; `clear_companion` moves one to `own_backups`.
  - `export_values_takes_the_key_list_and_the_label`: sandbox `A` with `V1` = "1", `A\Sub` with `V2`, `B` missing; `export_values(.., &["A", r"A\Sub", "B"], dir, "label")` → the file name ends with `-label.reg`, contains `"V1"="1"` and `"V2"`, no `[-`; with `&["A"]` it lacks `V2`.
  - `animation_info_is_eight_bytes`: `size_of::<ANIMATIONINFO>() == 8`.
- [ ] **Step 4: Run** `cargo test --locked sysparams` — Expected: FAIL (module missing).
- [ ] **Step 5: Implement** the move as the table says. No line inside a `#[cfg(test)]` block of `src/mouse_keyboard/` or of `src/ui/` changes, apart from Step 1's new test.
- [ ] **Step 6: Run** `cargo test --locked` — Expected: PASS, every Mouse & keyboard test included, and `the_store_file_bytes_do_not_change` still green. `git diff --stat src/ui` is empty.
- [ ] **Step 7: Commit** — `refactor(sysparams): share the SPI executor, account check, snapshot store and .reg export`.

### Task 2: The visual effects catalog and backend

**Files:**
- Create: `src/visual_effects/{mod,catalog,backend}.rs`
- Modify: `src/lib.rs` (`pub mod visual_effects;`)

**Interfaces — Consumes:** Task 1 (`SettingValue`, `Shape`, `ApiError`, `SetCall`, `spi_get`, `spi_get_animation`, `spi_set`, `WRONG_SHAPE`, `AccountCheck`).

**Interfaces — Produces (`catalog.rs`):**
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub enum EffectId { UiEffects, WindowAnimation, ClientAreaAnimation, ComboboxAnimation, ListboxSmoothScroll, MenuAnimation, MenuFade, SelectionFade, TooltipAnimation, TooltipFade, CursorShadow, DropShadow, FontSmoothing, FontSmoothingType, CleartypeContrast, CaretWidth, FocusBorderWidth, FocusBorderHeight }`, in spec 5.1 table order (`Ord` is table order), with `ALL: [EffectId; 18]`, `key()` (the spec ids), `from_key(&str) -> Option<EffectId>`, `section()`, `shape()` (`Number` for the type, contrast, caret and both borders; `Flag` otherwise), `control()`, `windows_default()`, `in_page_range(&SettingValue) -> bool`, `gate() -> Option<Gate>`.
- `pub enum VeSection { UiEffects, Animations, MenusTooltips, Shadows, Text, CaretFocus }` with `ALL: [VeSection; 6]`.
- `pub enum VeControl { Toggle, FadeSlide, SmoothingType, Slider { min: u32, max: u32, step: u32 } }`: fades `FadeSlide`; type `SmoothingType`; contrast `Slider(1000, 2200, 100)`; caret `Slider(1, 20, 1)`; borders `Slider(1, 10, 1)`; the rest `Toggle`.
- `pub enum Gate { Master, On(EffectId), ClearType }`: `Master` for combo box, list box, menu animation, selection fade, tooltip animation, cursor shadow; `On(MenuAnimation)` for menu fade; `On(TooltipAnimation)` for tooltip fade; `On(FontSmoothing)` for the type; `ClearType` (smoothing on **and** type 2) for contrast; `None` for the others.
- `pub const MASTER_COMPANIONS: [EffectId; 7]`: the six `Gate::Master` rows plus `TooltipFade`, table order (ruling 1).
- `pub const FE_STANDARD: u32 = 1; pub const FE_CLEARTYPE: u32 = 2;`
- `in_page_range`: shape mismatch → false; flags → true; type `1 | 2`; contrast `1000..=2200`; caret `1..=20`; borders `1..=10`. The step is not checked.
- `windows_default`: every flag `Flag(true)` (fades: Fade), type `Number(2)`, contrast `Number(1400)`, caret and borders `Number(1)`.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct VeChange { pub id: EffectId, pub value: SettingValue }` and `pub fn check_change(c: &VeChange) -> Option<VeProblem>` (`OutOfRange(id)` when not `in_page_range`).
- `pub fn write_order(ids: &[EffectId], master_target: Option<bool>) -> Vec<EffectId>`: table order, deduplicated, except `UiEffects` first when `Some(true)` and last when `Some(false)`.
- `VeProblem { OutOfRange(EffectId), UnknownSetting(String), Unreadable { id: EffectId, code: u32 }, WrongAccount { process: String, session: String }, AccountUnknown(String), NotSaved(String), NoOriginal(String) }`; `VeFailure { SetFailed { id, code: u32 }, Mismatch { id, wanted: SettingValue, read: SettingValue }, Unverified { id, code: u32 }, HeldByMaster { id, wanted: SettingValue, read: SettingValue }, StoreNotSaved(String) }`; `VeIssue { Problem(VeProblem), Failure(VeFailure) }` (derives as `MkProblem` / `MkFailure` / `MkIssue`).
- `pub fn account_problem(check: &AccountCheck) -> Option<VeProblem>` (`Different` → `WrongAccount`, `Unknown` → `AccountUnknown`).

**Interfaces — Produces (`backend.rs`):**
- `pub trait EffectsApi: Send + Sync { fn get(&self, id: EffectId) -> Result<SettingValue, ApiError>; fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError>; }`
- `pub(crate) enum VeFamily { Ui, PvValue, Animation }`, `pub(crate) struct VeDescriptor { id, get_action: u32, family: VeFamily, set_action: u32 }`, `pub(crate) const VE_DESCRIPTORS: [VeDescriptor; 18]`, `pub(crate) fn ve_descriptor(id) -> &'static VeDescriptor`, `pub(crate) fn ve_set_call(id, &SettingValue) -> Result<SetCall, ApiError>` (shape mismatch → `WRONG_SHAPE`).
- `pub struct SpiEffects;`: GET `spi_get::<u32>(get_action, ..)` (`Flag` = ≠ 0) or `spi_get_animation`; SET `spi_set(ve_set_call(..)?)`.
- `#[cfg(test)] pub(crate) struct FakeEffects` (map behind a `Mutex`) with `new()` seeded from spec 3 "Seen here" (every flag on except `CursorShadow` off; type 2; contrast 1200; caret 1; borders 1), `value`, `put`, `fail_get`, `fail_set` (code 5), `keep(id, v)`, `calls() -> Vec<VeCall>` (`VeCall { Get(EffectId), Set(EffectId, SettingValue) }`), `sets()`, and `master_mode(MasterMode)` with `pub(crate) enum MasterMode { Gates, Clears, ReadsOff }`:
  - `Gates` (default): every value independent (FOLKLORE "the master only gates");
  - `Clears` (case 1): a SET of `UiEffects` to `Flag(false)` also stores `Flag(false)` for every `MASTER_COMPANIONS` id;
  - `ReadsOff` (case 2): while `UiEffects` holds `Flag(false)`, a GET of a `MASTER_COMPANIONS` id returns `Flag(false)`; its SET still stores the value.

- [ ] **Step 1: Write the failing tests.** `catalog.rs`:
  - `ids_are_the_spec_table_in_order`: `ALL.map(key)` is the 18 spec ids in order; `from_key` round-trips; `from_key("hot_tracking") == None`; sections `[UiEffects ×1, Animations ×4, MenusTooltips ×5, Shadows ×2, Text ×3, CaretFocus ×3]`.
  - `defaults_follow_spec_5_1`: as above, each `in_page_range`.
  - `page_ranges_follow_spec_5_1`: accepted type 1, 2; contrast 1000, 1250, 2200; caret 1, 20; borders 1, 10. Refused type 0, 3; contrast 999, 2201; caret 0, 21; borders 0, 11; `Flag(true)` for caret; `Number(1)` for cursor shadow.
  - `gates_follow_spec_5_1`: the `gate()` of all 18 ids; `MASTER_COMPANIONS == [ComboboxAnimation, ListboxSmoothScroll, MenuAnimation, SelectionFade, TooltipAnimation, TooltipFade, CursorShadow]`.
  - `the_master_is_written_first_going_on_and_last_going_off`: `write_order(&[CursorShadow, UiEffects, MenuAnimation], Some(true)) == [UiEffects, MenuAnimation, CursorShadow]`; with `Some(false)` → `[MenuAnimation, CursorShadow, UiEffects]`; `write_order(&[CaretWidth, FontSmoothingType, FontSmoothing], None) == [FontSmoothing, FontSmoothingType, CaretWidth]`.
  - `check_change_and_account_problem`: contrast 2300 → `OutOfRange(CleartypeContrast)`; `account_problem(&Unknown("x")) == Some(AccountUnknown("x"))`.
  
  `backend.rs`:
  - `descriptors_follow_the_spec_table`: `VE_DESCRIPTORS.map(|d| d.id) == EffectId::ALL` and each `(get_action, family, set_action)`:
    ```text
    ui_effects 0x103E PvValue 0x103F        window_animation 0x0048 Animation 0x0049
    client_area_animation 0x1042 PvValue 0x1043   combobox_animation 0x1004 PvValue 0x1005
    listbox_smooth_scroll 0x1006 PvValue 0x1007   menu_animation 0x1002 PvValue 0x1003
    menu_fade 0x1012 PvValue 0x1013          selection_fade 0x1014 PvValue 0x1015
    tooltip_animation 0x1016 PvValue 0x1017  tooltip_fade 0x1018 PvValue 0x1019
    cursor_shadow 0x101A PvValue 0x101B      drop_shadow 0x1024 PvValue 0x1025
    font_smoothing 0x004A Ui 0x004B          font_smoothing_type 0x200A PvValue 0x200B
    cleartype_contrast 0x200C PvValue 0x200D caret_width 0x2006 PvValue 0x2007
    focus_border_width 0x200E PvValue 0x200F focus_border_height 0x2010 PvValue 0x2011
    ```
    No `set_action` is `0x0025` (drag full windows belongs to Mouse & keyboard).
  - `set_calls_put_the_value_where_each_family_takes_it`: `(FontSmoothing, Flag(true)) == Ui { 0x4B, 1 }`; `(CursorShadow, Flag(true)) == PvValue { 0x101B, 1 }`; `(CleartypeContrast, Number(1200)) == PvValue { 0x200D, 1200 }`; `(WindowAnimation, Flag(false)) == Animation { 0x49, on: false }`; `(CleartypeContrast, Flag(true)) == Err(ApiError { code: 87 })`.
  - `every_real_effect_get_reads_its_shape_in_the_documented_range`: `SpiEffects.get(id)` is `Ok` with the id's shape; type 1 or 2; contrast `1000..=2200`; caret and both borders `>= 1`. **Never calls `set`**; `#[ignore]` with a comment only if CI's session makes it flaky (spec 5.2).
  - `the_fake_models_both_master_cases`: `Gates`: `set(UiEffects, false)` leaves `value(MenuAnimation) == Flag(true)`. `Clears`: same SET → `value(MenuAnimation) == Flag(false)`, `value(DropShadow) == Flag(true)`. `ReadsOff`: after the SET, `get(MenuAnimation) == Ok(Flag(false))` while `value(MenuAnimation) == Flag(true)`; `keep`, `fail_get`, `calls` behave as `FakeInput`'s.
- [ ] **Step 2: Run** `cargo test --locked visual_effects::` — Expected: FAIL.
- [ ] **Step 3: Implement** `catalog.rs`, `backend.rs`, `mod.rs` (`//!` docs with the "never write under `HKCU\Control Panel`" rule; `pub use` of both) and the `lib.rs` line.
- [ ] **Step 4: Run** `cargo test --locked visual_effects::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(visual-effects): catalog, gates, write order and SystemParametersInfo backend`.

### Task 3: Store, `.reg` net, scan, apply and restore with the master's companions

**Files:**
- Create: `src/visual_effects/store.rs`, `src/visual_effects/engine.rs`
- Modify: `src/visual_effects/mod.rs`

**Interfaces — Consumes:** Tasks 1-2; `clock::now_local`; `persist`; `associations::OperationReport`; `backups::RegTarget`.

**Interfaces — Produces (`store.rs`):** `pub type VeBackupStore = SpiBackupStore;`, `default_ve_store_path()` (`app_data_dir().join("visual-effects-backups.json")`), `load_ve_store(&Path) -> Result<VeBackupStore, LoadError>`, `save_ve_store(&VeBackupStore, &Path) -> Result<(), String>`, `pub const DESKTOP_KEYS: [&str; 2] = [r"Control Panel\Desktop", r"Control Panel\Desktop\WindowMetrics"];`, `export_desktop(source: &RegTarget, reg_dir: &Path) -> Result<PathBuf, String>` = `export_values(source, &DESKTOP_KEYS, reg_dir, "visual-effects")`; `#[cfg(test)] test_support::Fixture` like Mouse & keyboard's: sandbox `{sb}\HKCU` with `Control Panel\Desktop` (`UserPreferencesMask` `REG_BINARY 9E 1E 07 80 12 00 00 00`, `CaretWidth` DWORD 1), `Desktop\WindowMetrics` (`MinAnimate` = "1"), `Desktop\MuiCached` (`MachinePreferredUILanguages`), `USER = "TEST\\user"`.

**Interfaces — Produces (`engine.rs`):**
- `pub struct VeScan { pub values: BTreeMap<EffectId, Result<SettingValue, ApiError>>, pub account: AccountCheck }` with `live`, `read_error`, `account_ok`; `pub fn scan_visual_effects(api: &dyn EffectsApi, account: &dyn Fn() -> AccountCheck) -> VeScan` (one GET per id, table order, no SET).
- `pub struct VeOptions<'a> { pub user: String, pub store_path: PathBuf, pub reg_backup_dir: PathBuf, pub reg_source: &'a RegTarget, pub account: &'a dyn Fn() -> AccountCheck, pub progress: &'a dyn Fn(usize, usize, &str) }`.
- `pub struct VeReport { pub report: OperationReport, pub issues: Vec<VeIssue> }`, invariant `report.errors[i] == english_issue(&issues[i])`; `succeeded` holds `EffectId::key` strings.
- `pub fn english_issue(issue: &VeIssue) -> String`: `Mismatch` → `"{key}: Windows kept {read} instead of {wanted}"`; `HeldByMaster` → `"{key}: Windows keeps this effect off while UI effects is off"`; values `"on"` / `"off"` / the number.
- A private `held_or_mismatch(api, id, wanted, read) -> VeFailure`: for a `MASTER_COMPANIONS` id, one GET of `UiEffects`; `Ok(Flag(false))` → `HeldByMaster`, anything else → `Mismatch`. Other ids → `Mismatch`.
- `pub fn apply_visual_effects(api: &dyn EffectsApi, store: &mut VeBackupStore, changes: &[VeChange], opts: &VeOptions) -> VeReport`:
  1. `account_problem(&(opts.account)())` is `Some` → push it, return (no call). Then `check_change` each; any → return.
  2. GET, in table order, every touched id plus, when `UiEffects` is touched, every `MASTER_COMPANIONS` id; any failure → one `Unreadable` per failure, return.
  3. Each touched id: `record_first`, then `clear_companion`. When `UiEffects` is touched, each untouched companion: `record_companion(.., "ui_effects")`. `save_ve_store`, then `export_desktop`; either error → `NotSaved`, return.
  4. For `id in write_order(touched, target of UiEffects)`: SET; `Err` → `SetFailed`, next. GET: `Err` → `Unverified`; differs → `held_or_mismatch`; equal → succeeded.
  5. Forget each touched id whose after value (readback; the step-2 value after `SetFailed`) equals its original, except `HeldByMaster` ids, whose snapshot is kept. When `UiEffects` was touched **and its snapshot was just forgotten**, GET each `companions_of(user, "ui_effects")`: equal to its original → forget; differs or the GET fails → `clear_companion` (kept as its own row). While the master stays changed, its companions are kept (ruling 10). Save when anything changed (`StoreNotSaved`).
- `pub fn restore_visual_effects(api, store, ids: &[String], opts) -> VeReport`: account first. Unknown key → `UnknownSetting`, no snapshot → `NoOriginal` (both skipped). When `ui_effects` is among the ids: GET each `MASTER_COMPANIONS` id with no snapshot (failure → `Unreadable`, return, nothing written), `record_companion`, save the store if any was recorded (`NotSaved`), and add every `companions_of(user, "ui_effects")` to the ids. Then `write_order(ids, stored master value)`: SET the stored value exactly (no range check), GET; equal → forget; differs → `held_or_mismatch`, keep. If the master was forgotten, `clear_companion` on each companion left. Save if changed. No `.reg`.

Test setup: `FakeEffects::new()`, `Fixture`, `account = || AccountCheck::Same`.

- [ ] **Step 1: Write the failing tests.** `store.rs`:
  - `the_reg_net_exports_desktop_and_window_metrics_only`: the file contains `"UserPreferencesMask"=hex:9e,1e,07,80,12,00,00,00`, `"MinAnimate"="1"` and a `Control Panel\Desktop\WindowMetrics]` header; neither `MuiCached` nor `[-`; the name ends with `-visual-effects.reg`; the sandbox is unchanged.
  
  `engine.rs`, basics:
  - `the_scan_reads_every_effect_once`: `fail_get(CaretWidth)` → `read_error(CaretWidth) == Some(ApiError { code: 5 })`, 17 live values, `calls()` 18 `Get`s in table order.
  - `apply_sets_reads_back_and_keeps_the_original`: `CursorShadow` `Flag(true)` → fake on; store `cursor_shadow` = `Flag(false)`, `companion_of == None`; one `.reg`; `calls() == [Get(CursorShadow), Set(CursorShadow, Flag(true)), Get(CursorShadow)]`.
  - `the_whole_apply_is_refused_on_a_bad_value`: contrast 2300 plus caret 2 → `[Problem(OutOfRange(CleartypeContrast))]`, no call, no store file, no `.reg`.
  - `windows_keeping_another_value_with_the_master_on_is_a_plain_mismatch`: `keep(CursorShadow, Flag(false))` → `[Failure(Mismatch { id: CursorShadow, wanted: Flag(true), read: Flag(false) })]`, `report.errors[0]` contains `"Windows kept off instead of on"`.
  - `restore_sets_the_stored_value_exactly_even_outside_the_page_range`: fake type 3 and caret 25; apply type 2 and caret 2; restore both → fake 3 and 25, store empty, no new `.reg`.
  - `the_wrong_account_refuses_apply_and_restore_before_any_call`: `Different { "PC\\Admin", "PC\\Ana" }` → exactly `[Problem(WrongAccount { .. })]` for each, `calls()` empty, no store file.
  - `an_unsavable_store_or_reg_refuses_before_any_write` (as Mouse & keyboard's), `nothing_under_control_panel_is_written` (snapshot the sandbox `Control Panel` before and after an apply of 6 ids and a restore of all → identical).

  Write order and readback while the master is off:
  - `the_master_is_written_first_when_it_goes_on`: `put(UiEffects, Flag(false))`; apply `UiEffects` on, `MenuAnimation` off, `CursorShadow` on → every `Get` precedes the first `Set`; `sets()` ids `[UiEffects, MenuAnimation, CursorShadow]`.
  - `the_master_is_written_last_when_it_goes_off`: apply `UiEffects` off and `DropShadow` off, `CaretWidth` 2 → `sets()` ids `[DropShadow, CaretWidth, UiEffects]`.
  - `a_gated_write_under_a_master_that_is_off_is_held_by_master`: `master_mode(ReadsOff)`, `put(UiEffects, Flag(false))`; apply `CursorShadow` `Flag(true)` → `[Failure(HeldByMaster { id: CursorShadow, wanted: Flag(true), read: Flag(false) })]`; `report.errors[0]` contains `"keeps this effect off while UI effects is off"`; the last call is `Get(UiEffects)`; `store.has_backup(USER, "cursor_shadow")` (kept although the read value equals the original `Flag(false)`).
  - `a_gated_row_written_with_the_master_going_on_reads_back_after_it`: `ReadsOff`, master off; apply `UiEffects` on and `MenuAnimation` on → no issue, `succeeded == ["ui_effects", "menu_animation"]` in write order, and `Get(MenuAnimation)` (the snapshot) comes before `Set(UiEffects, ..)`.

  Companion snapshots (spec 5.3), with the OBSERVED seed:
  - `turning_the_master_off_snapshots_its_companions_first`: apply `UiEffects` `Flag(false)` → the calls before the first `Set` are `Get` of `UiEffects` then the 7 companions in table order; `sets() == [(UiEffects, Flag(false))]`; the store holds `ui_effects` = `Flag(true)` with no mark and each companion with `companion_of == Some("ui_effects")`: `combobox_animation`, `listbox_smooth_scroll`, `menu_animation`, `selection_fade`, `tooltip_animation`, `tooltip_fade` = `Flag(true)`, `cursor_shadow` = `Flag(false)`.
  - `a_failed_companion_get_refuses_the_whole_apply`: `fail_get(CursorShadow)`; apply master off → `[Problem(Unreadable { id: CursorShadow, code: 5 })]`, `sets()` empty, no store file.
  - `a_row_with_its_own_snapshot_keeps_it`: apply `CursorShadow` on, then master off → `cursor_shadow` still `Flag(false)`, `companion_of == None`.
  - `changing_a_companion_directly_clears_its_mark`: apply master off, then `MenuAnimation` off → `menu_animation` original `Flag(true)`, `companion_of == None`.
  - `companions_are_read_at_apply_time_not_from_the_scan`: scan; then `put(MenuAnimation, Flag(false))`; apply master off → `menu_animation` snapshot `Flag(false)`.

  Restore and forget, both FOLKLORE cases:
  - `restoring_the_master_restores_its_companions_master_first` (case 1): `master_mode(Clears)`; apply master off (fake companions now off); restore `["ui_effects"]` → `sets()` == `UiEffects` `Flag(true)`, then the 7 companions in table order with their originals (`CursorShadow` `Flag(false)`); every fake value equals the seed; `store.users` empty.
  - `restoring_a_master_that_was_off_writes_it_last`: `put(UiEffects, Flag(false))`; apply master on; restore `["ui_effects"]` → the 7 companions first, `(UiEffects, Flag(false))` last.
  - `case_2_loses_nothing`: `ReadsOff`; apply master off; a scan reads the 7 companions `Flag(false)`; restore `["ui_effects"]` → every readback matches, store empty, fake values equal the seed.
  - `turning_the_master_back_on_forgets_it_and_its_companions`: `Gates`; apply master off, then master on → `store.users` empty; after the last `Set`, one `Get` per companion.
  - `case_1_a_companion_still_off_becomes_its_own_row`: `Clears`; apply master off, then on → `ui_effects` forgotten; `cursor_shadow` forgotten (reads its original off); the other 6 companions kept as `Flag(true)` with `companion_of == None`.
  - `restore_all_writes_each_id_once_in_master_order`: `Clears`; apply master off, then `CursorShadow` `Flag(true)` (now its own row, original `Flag(false)`); restore every key of the store → each id appears once in `sets()`, `UiEffects` first; store empty.
  - `a_restore_sets_a_gated_id_while_its_gate_is_off`: `put(UiEffects, Flag(false))`; apply `MenuAnimation` off; restore `["menu_animation"]` → `Set(MenuAnimation, Flag(true))` is sent (the engine does not gate).
- [ ] **Step 2: Run** `cargo test --locked visual_effects::` — Expected: FAIL.
- [ ] **Step 3: Implement** `store.rs` and `engine.rs`.
- [ ] **Step 4: Run** `cargo test --locked visual_effects` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(visual-effects): store, .reg net, apply and restore with the master's companions`.

### Task 4: App wiring: scan job, pending values, gates, presets, jobs, confirmation and messages

**Files:**
- Create: `src/ui/visual_effects_state.rs`
- Modify: `src/ui/mod.rs`, `jobs.rs`, `confirm.rs`, `test_support.rs`, `src/i18n/messages.rs`, `src/i18n/tests.rs`, both `.resx`

**Interfaces — Consumes:** Tasks 1-3; the Mouse & keyboard state as the model (`reload_mk_store`, `pump_mk_scan`, `spawn_mk_job`, `finish_mk_job`).

**Interfaces — Produces:**
- **`AppEnv` gains** `effects: Arc<dyn EffectsApi>` (`Arc::new(SpiEffects)` in `system()`) and `ve_store_path: PathBuf` (`default_ve_store_path()`). The account check and the `.reg` source reuse `input_account` and `mk_reg_source` (ruling 6). `ReportKind` gains `AppliedVisualEffects` and `RestoredVisualEffects` (toasts `report.applied_visual_effects` / `report.restored_visual_effects`).
- **`TempIcons`** gains `effects: Arc<FakeEffects>`, `fake_effects(&self) -> &FakeEffects`, `dir/visual-effects-backups.json`, and `run_until_ve_scanned(app, ctx)`.
- **`WinTuneApp` fields:** `ve_store: VeBackupStore`, `ve_store_error: Option<LoadError>`, `ve_scan: Option<VeScan>`, `ve_scan_job: Option<BackgroundJob<VeScan>>`, `ve_scan_needed: bool`, `ve_pending: BTreeMap<EffectId, SettingValue>`, `ve_search: String`. `new()` calls `reload_ve_store()` and `request_ve_scan()`; `ui()` calls `pump_ve_scan(ctx)` after `pump_mk_scan`.
- **`visual_effects_state.rs`:**
  - `reload_ve_store`, `request_ve_scan`, `pump_ve_scan(ctx)` like Mouse & keyboard's (scan never while busy; panic → `ve.scan_failed`); a finished scan drops pending values that are unreadable or equal to the live one.
  - `ve_original(id) -> Option<SettingValue>`; `ve_display(id) -> Option<SettingValue>` (pending, else live; `None` when unread).
  - `ve_gate_reason(&self, id) -> Option<String>`, on `ve_display`: `Master` off → `ve.gate.master`; `On(g)`: `g`'s own reason first (transitive, ruling 2), else `ve.gate.menu` / `ve.gate.tooltip` / `ve.gate.smoothing` when `g` is off; `ClearType`: `ve.gate.smoothing` when smoothing is off, else `ve.gate.cleartype` when the type is not 2.
  - `set_ve_wanted(&mut self, id, v: SettingValue)`: removes the entry when `v` equals live, else inserts; then removes the pending value of every id whose `ve_gate_reason` is now `Some`.
  - `pub(super) enum VePreset { Appearance, Performance }` and `apply_ve_preset(&mut self, p)`, through `set_ve_wanted`, `UiEffects` first: Appearance → `Flag(true)` for every id of sections 1-4 (fades: Fade); Performance → `UiEffects` `Flag(true)`, then `Flag(false)` for every toggle of sections 1-4 (fade rows untouched). Text and Caret and focus never.
  - `ve_changes() -> Vec<VeChange>` (empty before the first scan or when `!account_ok()`; pending, readable, differing, id order), `ve_pending_count()`, `discard_ve_changes()`, `ve_slot_name(key) -> String` (`ve_title`, else the raw key).
- **`jobs.rs`:** `VeOutcome { report: VeReport, store: VeBackupStore, restored: bool }`; `RunningJob::VisualEffects { job, restore }` in every arm (`job.apply_visual_effects` / `job.restore_visual_effects`, `job.name.*`, history `("Visual effects", "Apply" | "Restore")`; the keys test lists the four keys); `ve_blocked_reason()`; `start_ve_apply(ctx)` (busy → blocked → no changes → spawn; **no confirmation**); `start_ve_restore(ctx, ids: Vec<String>)`; `spawn_ve_job`; `finish_ve_job` (a clean apply clears `ve_pending`; a restore drops the pending value of each succeeded key; toast `ReportKind::AppliedVisualEffects` / `RestoredVisualEffects` with `ve_issue` translations; History gets the English report); `poll_job` ends with `request_ve_scan()`, its panic branch calls `reload_ve_store()`.
- **`confirm.rs`:** `ConfirmAction::RestoreVisualEffects(Vec<String>)`; `request_restore_visual_effects(ids)`: empty → nothing; title `ve.restore_title`; message `ve.restore_message` with `named_list(.., ve_slot_name, "ve.restore_many")`, plus `"\n\n"` and `ve.also_restores` when `ui_effects` is among the ids and has companions; `resolve_confirm` → `start_ve_restore`.
- **`messages.rs`:** `ve_title(tr, EffectId)`, `ve_description(tr, EffectId) -> Option<String>`, `ve_section(tr, VeSection)`, `ve_value(tr, id: Option<EffectId>, v: &SettingValue)` (fades `Flag` → Fade / Slide; other flags On / Off; type 1 / 2 → Standard / ClearType, other → `ve.value.unknown`; contrast → digits grouped by `ve.value.thousands`; caret and borders → `ve.value.px`; else the number), `ve_problem`, `ve_failure`, `ve_issue`, `ve_companions(tr, n: usize, titles: &[String]) -> String` (`ve.also_restores`), and `history_module` maps `"Visual effects"`.

**Keys added in this task (EN / FR):**

| Key | English | French |
|---|---|---|
| `job.apply_visual_effects` | Applying visual effects… | Application des effets visuels… |
| `job.restore_visual_effects` | Restoring visual effects… | Restauration des effets visuels… |
| `job.name.apply_visual_effects` | Applying visual effects | L'application des effets visuels |
| `job.name.restore_visual_effects` | Restoring visual effects | La restauration des effets visuels |
| `report.applied_visual_effects` | {0} setting(s) applied | {0} réglage(s) appliqué(s) |
| `report.restored_visual_effects` | {0} setting(s) restored | {0} réglage(s) restauré(s) |
| `history.module.visual_effects` | Visual effects | Effets visuels |
| `ve.scan_failed` | Could not read the visual effects: {0} | Impossible de lire les effets visuels : {0} |
| `ve.restore_title` | Restore original settings? | Restaurer les réglages d'origine ? |
| `ve.restore_message` | {0} will get back the value they had before WinTune changed them. | {0} retrouveront la valeur qu'ils avaient avant que WinTune les modifie. |
| `ve.restore_many` | {0} settings | {0} réglages |
| `ve.also_restores` | Also restores {0} effects: {1} | Restaure aussi {0} effets : {1} |
| `ve.section.ui_effects` / `.animations` / `.menus` / `.shadows` / `.text` / `.caret` | UI effects / Animations / Menus and tooltips / Shadows / Text / Caret and focus | Effets de l'interface / Animations / Menus et info-bulles / Ombres / Texte / Curseur et focus |
| `ve.ui_effects.title` | UI effects | Effets de l'interface |
| `ve.ui_effects.description` | Turns the menu, list, tooltip and pointer effects below on or off together. | Active ou désactive ensemble les effets de menus, de listes, d'info-bulles et du pointeur ci-dessous. |
| `ve.window_animation.title` | Animate windows when minimizing and maximizing | Animer les fenêtres lors de la réduction et de l'agrandissement |
| `ve.client_area_animation.title` | Animate controls and elements inside windows | Animer les contrôles et les éléments à l'intérieur des fenêtres |
| `ve.combobox_animation.title` | Slide open combo boxes | Ouvrir les zones de liste déroulante en glissant |
| `ve.listbox_smooth_scroll.title` | Smooth-scroll list boxes | Défilement fluide des zones de liste |
| `ve.menu_animation.title` | Menu animation | Animation des menus |
| `ve.menu_fade.title` | Menu effect | Effet des menus |
| `ve.selection_fade.title` | Fade out menu items after clicking | Estomper les éléments de menu après un clic |
| `ve.tooltip_animation.title` | Tooltip animation | Animation des info-bulles |
| `ve.tooltip_fade.title` | Tooltip effect | Effet des info-bulles |
| `ve.cursor_shadow.title` | Show shadows under mouse pointer | Afficher une ombre sous le pointeur de la souris |
| `ve.drop_shadow.title` | Show shadows under windows | Afficher des ombres sous les fenêtres |
| `ve.font_smoothing.title` | Smooth edges of screen fonts | Lisser les polices de l'écran |
| `ve.font_smoothing_type.title` | Smoothing type | Type de lissage |
| `ve.cleartype_contrast.title` | ClearType contrast | Contraste ClearType |
| `ve.caret_width.title` | Text cursor width | Épaisseur du curseur de texte |
| `ve.focus_border_width.title` | Focus rectangle width | Largeur du rectangle de focus |
| `ve.focus_border_height.title` | Focus rectangle height | Hauteur du rectangle de focus |
| `ve.value.on` / `.off` | On / Off | Activé / Désactivé |
| `ve.value.fade` / `.slide` | Fade / Slide | Fondu / Glissement |
| `ve.value.standard` / `.cleartype` | Standard / ClearType | Standard / ClearType |
| `ve.value.unknown` | Unknown ({0}) | Inconnu ({0}) |
| `ve.value.px` | {0} px | {0} px |
| `ve.value.thousands` | , | (U+202F narrow no-break space) |
| `ve.problem.out_of_range` | {0}: this value is outside what WinTune allows. | {0} : cette valeur est hors des limites permises par WinTune. |
| `ve.problem.unknown` | {0} is a setting this version of WinTune does not know. | {0} est un réglage que cette version de WinTune ne connaît pas. |
| `ve.problem.unreadable` | {0}: Windows could not read this setting (error {1}). | {0} : Windows n'a pas pu lire ce réglage (erreur {1}). |
| `ve.problem.wrong_account` | WinTune runs as {0}, but {1} is signed in. Visual effects would be saved for the wrong account. | WinTune s'exécute en tant que {0}, mais c'est {1} qui a ouvert la session. Les effets visuels seraient enregistrés pour le mauvais compte. |
| `ve.problem.account_unknown` | WinTune cannot tell which account is signed in ({0}), so it does not change visual effects. | WinTune ne peut pas savoir quel compte a ouvert la session ({0}) : il ne modifie donc pas les effets visuels. |
| `ve.problem.not_saved` | Nothing applied: the originals or the .reg backup could not be saved ({0}). | Rien n'a été appliqué : les valeurs d'origine ou la sauvegarde .reg n'ont pas pu être enregistrées ({0}). |
| `ve.problem.no_original` | {0}: no original value is saved. | {0} : aucune valeur d'origine n'est enregistrée. |
| `ve.failure.set` | {0}: Windows refused the change (error {1}). | {0} : Windows a refusé le changement (erreur {1}). |
| `ve.failure.mismatch` | {0}: Windows kept {1} instead of {2}. | {0} : Windows a gardé {1} au lieu de {2}. |
| `ve.failure.unverified` | {0}: WinTune could not read the setting back (error {1}). | {0} : WinTune n'a pas pu relire le réglage (erreur {1}). |
| `ve.failure.held` | {0}: Windows keeps this effect off while UI effects is off. | {0} : Windows garde cet effet désactivé tant que les effets de l'interface sont désactivés. |
| `ve.failure.store` | The change was made, but the saved originals could not be updated: {0} | Le changement est fait, mais les valeurs d'origine n'ont pas pu être mises à jour : {0} |
| `ve.gate.master` | Has no effect while UI effects is off | Sans effet tant que les effets de l'interface sont désactivés |
| `ve.gate.menu` | Has no effect while Menu animation is off | Sans effet tant que l'animation des menus est désactivée |
| `ve.gate.tooltip` | Has no effect while Tooltip animation is off | Sans effet tant que l'animation des info-bulles est désactivée |
| `ve.gate.smoothing` | Has no effect while font smoothing is off | Sans effet tant que le lissage des polices est désactivé |
| `ve.gate.cleartype` | Has no effect unless the smoothing type is ClearType | Sans effet si le type de lissage n'est pas ClearType |

The slash rows are one key per name, EN and FR in the same order.

- [ ] **Step 1: Write the failing tests:**
  - `visual_effects_state.rs`:
    - `the_visual_effects_scan_runs_in_background_and_waits_for_jobs`: 18 values, `account_ok()`.
    - `a_scan_drops_choices_the_live_value_matches_or_cannot_read`: pending caret 2, contrast 1600, drop shadow off; `put(CaretWidth, Number(2))`, `fail_get(CleartypeContrast)`; scan → `ve_pending == {DropShadow: Flag(false)}`.
    - `turning_a_gate_off_drops_the_choices_it_disables`: pending `MenuFade` `Flag(false)`, `CursorShadow` `Flag(true)`, `FontSmoothingType` `Number(1)`. `set_ve_wanted(MenuAnimation, Flag(false))` → menu fade dropped, cursor shadow kept. Re-add menu fade Slide with menu animation back on, then `set_ve_wanted(UiEffects, Flag(false))` → both cursor shadow and menu fade dropped, and `ve_gate_reason(MenuFade) == Some(tr.t("ve.gate.master"))`. `FontSmoothing` off → the type dropped, `ve_gate_reason(CleartypeContrast) == Some(tr.t("ve.gate.smoothing"))`.
    - `turning_the_master_on_in_the_batch_enables_its_rows_at_once`: `put(UiEffects, Flag(false))`, rescan → `ve_gate_reason(CursorShadow) == Some(master)`; `set_ve_wanted(UiEffects, Flag(true))` → `None`; `set_ve_wanted(CursorShadow, Flag(true))` is kept.
    - `contrast_needs_smoothing_on_and_type_cleartype`: live → `None`; pending contrast 1600 then type `Number(1)` → contrast dropped and `Some(tr.t("ve.gate.cleartype"))`.
    - `for_performance_turns_every_effect_off_and_the_master_on`: seed → `ve_pending` is `Flag(false)` for exactly `WindowAnimation, ClientAreaAnimation, ComboboxAnimation, ListboxSmoothScroll, MenuAnimation, SelectionFade, TooltipAnimation, DropShadow` (8; cursor shadow is already off, fades untouched or dropped). With `put(UiEffects, Flag(false))` and a rescan → the same 8 plus `UiEffects: Flag(true)`. No Text or Caret and focus id in either.
    - `for_appearance_turns_everything_on_with_fades`: seed → `{CursorShadow: Flag(true)}`; after `put(MenuFade, Flag(false))` and a rescan → also `MenuFade: Flag(true)`.
    - `changes_are_empty_before_the_first_scan_and_for_the_wrong_account`; `a_slot_is_named_by_its_title_or_its_raw_key` (`"caret_width"` → "Text cursor width"; `"hot_tracking"` → `"hot_tracking"`).
  - `jobs.rs`:
    - `a_visual_effects_apply_sets_logs_and_clears_pending`: pending cursor shadow on → `start_ve_apply`; `pending_confirm` stays `None`; `run_until_idle` → fake on; History `("Visual effects", "Apply")` without errors; `ve_pending` empty; one file ending `-visual-effects.reg`.
    - `a_held_effect_keeps_the_choice_and_the_toast_is_translated`: `set_language("fr")`; `master_mode(ReadsOff)`, master live off, pending cursor shadow on; after `finish_job_without_scanning`, still pending; `last_notice()` contains `messages::ve_failure(tr, &HeldByMaster { .. })` in French; the History error contains "keeps this effect off".
    - `a_restore_drops_the_pending_choice_of_what_it_restored`, `an_unreadable_visual_effects_store_blocks_apply_and_restore`, `a_restore_from_history_as_the_wrong_account_is_refused` (fake unchanged, the History error contains `"runs as PC\\Admin"`), `a_panicking_visual_effects_job_is_logged_and_reloads_the_store`, each shaped like its Mouse & keyboard twin.
  - `confirm.rs`: `restoring_the_master_names_its_companions`: apply master off; `request_restore_visual_effects(["ui_effects"])` → the message contains "UI effects" and `ve_companions(tr, 7, ..)` with "Menu animation"; `["caret_width"]` → no `ve.also_restores` text.
  - `src/i18n/tests.rs`: `every_visual_effects_message_translates_in_every_shipped_language`, like the Mouse & keyboard one: 18 unique `ve_title`, 6 `ve_section`, every `VeProblem` / `VeFailure` variant contains its argument; English `ve_value`: `(MenuFade, Flag(false))` → "Slide", `(FontSmoothingType, Number(2))` → "ClearType", `(FontSmoothingType, Number(3))` → "Unknown (3)", `(CleartypeContrast, Number(1200))` → "1,200", `(CaretWidth, Number(2))` → "2 px"; French contrast → "1\u{202F}200"; `history_module(tr, "Visual effects") == tr.t("history.module.visual_effects")`.
- [ ] **Step 2: Run** `cargo test --locked ui::` and `cargo test --locked i18n` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.** Add every key above to both `.resx` files.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(visual-effects): scan job, pending values, gates, presets, apply and restore jobs`.

### Task 5: The Visual effects page

**Files:**
- Create: `src/ui/visual_effects_tab.rs`
- Modify: `src/ui/kit/icons.rs` (`Icon::Effects` = `'\u{E771}'` Personalize; `'\u{E790}'` Color if the font test says the glyph is missing; `ALL: [Icon; 24]`), `src/ui/kit/tests.rs` (`Effects => 23`, `len() == 24`), `src/ui/mod.rs` (`ActiveTab::VisualEffects` after `MouseKeyboard`, `ALL: [ActiveTab; 12]`, `history_module` → `Some("Visual effects")`, `owns_action_bar`, dispatch), `src/ui/shell.rs` (nav item after Mouse & keyboard; `open_page` reloads the ve store when not busy and requests a scan on a page change; the French test), `src/ui/polish.rs` (`search_box_id` → `"search_visual_effects"`), `src/ui/home.rs` (`tile_title` arm only), both `.resx`.

**Interfaces — Consumes:** Task 4 (all of `visual_effects_state.rs`, `start_ve_apply`, `ve_blocked_reason`, `request_restore_visual_effects`, `messages::ve_*`); Task 2 (`VeControl`, `windows_default`, `FE_*`); kit (`page_header`, `section`, `SettingRow`, `toggle_enabled`, `segmented`, `slider`, `pill`, `overflow_menu`, `banner`, `secondary_button`, `primary_button`), `page_action_bar`; `widget_right_of` from `test_support`.

**Interfaces — Produces:**
- `pub(super) fn show_visual_effects_tab(&mut self, ctx)`.
- `pub(super) fn ve_control_disabled_reason(&self, id: EffectId) -> Option<String>`: first of the scan's read error (`ve_problem(Unreadable)`), `busy_or_blocked(&self.ve_store_error)`, `ve_gate_reason(id)`.

Content (spec 5.7):
- **Header:** `page_header` with `ve.subtitle`; its right side holds `ve.preset.appearance` and `ve.preset.performance` (secondary buttons → `apply_ve_preset`, disabled with the busy or account reason). Then `ve.note`, the search box (Ctrl+F, title filter, empty sections hidden, `ve.no_match`).
- **States:** `ve.scanning` until the first scan; when `!account_ok()`, an error `banner` with the `ve_problem` text of `account_problem(&scan.account)` replaces the sections and the preset buttons are disabled.
- **Rows** (`SettingRow`, title `ve_title`, description `ve_description`; trailing: control, `…` menu, `ve.pill.not_applied` when pending), every control disabled with `ve_control_disabled_reason`:
  - `Toggle` → `toggle_enabled` → `set_ve_wanted(id, Flag(on))`;
  - `FadeSlide` → `segmented` `[(true, ve.value.fade), (false, ve.value.slide)]`;
  - `SmoothingType` → `segmented` `[(1, Standard), (2, ClearType)]`; a live value other than 1 or 2 selects neither and shows `ve.value.unknown` beside it;
  - `Slider` → `kit::slider` with the label `ve_value`.
  - **`…` menu:** `ve.windows_default` (disabled with `ve.default_shown` when shown, and with the gate reason when gated off) and `tweaks.restore_original` when a snapshot exists (never gated: the engine writes what it is given). Both disabled while busy.
- **Animations** ends with `ui.link(ve.drag_link)` → `open_page(ActiveTab::MouseKeyboard)`. **Text** starts with the note `ve.text_note`.
- **Action bar:** `ve.pending` / `ve.up_to_date`; Discard; Apply → `start_ve_apply`, disabled with the first of `ve_blocked_reason`, the account text, busy, `ve.nothing_to_apply`.

| Key | English | French |
|---|---|---|
| `nav.visual_effects` | Visual effects | Effets visuels |
| `ve.subtitle` | Animations, shadows, font smoothing and the text cursor, each with an undo. | Animations, ombres, lissage des polices et curseur de texte, chaque réglage avec une annulation. |
| `ve.note` | Changes apply when you press Apply and are kept when you sign out. Some programs show them only after a restart. | Les changements s'appliquent quand vous cliquez sur Appliquer et sont conservés quand vous fermez la session. Certains programmes ne les affichent qu'après un redémarrage. |
| `ve.preset.appearance` | For appearance | Pour l'apparence |
| `ve.preset.performance` | For performance | Pour les performances |
| `ve.search` / `ve.scanning` / `ve.no_match` | Search settings / Reading your settings… / No setting matches your search. | Rechercher un réglage / Lecture de vos réglages… / Aucun réglage ne correspond à votre recherche. |
| `ve.pill.not_applied` | Not applied | Non appliqué |
| `ve.windows_default` / `ve.default_shown` | Windows default / This is already the Windows default. | Valeur par défaut de Windows / C'est déjà la valeur par défaut de Windows. |
| `ve.drag_link` | Show window contents while dragging is on the Mouse & keyboard page | « Afficher le contenu des fenêtres pendant le déplacement » se trouve sur la page Souris et clavier |
| `ve.text_note` | WinTune draws its own text, so look at another window, such as Notepad, to see the change. | WinTune dessine son propre texte : regardez une autre fenêtre, comme le Bloc-notes, pour voir le changement. |
| `ve.pending` / `ve.up_to_date` / `ve.nothing_to_apply` | {0} changes not applied / All changes applied / Nothing to apply | {0} changement(s) non appliqué(s) / Tous les changements sont appliqués / Rien à appliquer |

- [ ] **Step 1: Write the failing tests** (`visual_effects_tab.rs`, seeded through `TempIcons`):
  - `the_page_draws_six_sections_in_table_order_with_live_values`: six titles with increasing `y`; "1,200", "ClearType" and "1 px" drawn.
  - `a_toggle_marks_not_applied_and_apply_sets_it`: click the toggle right of "Show shadows under mouse pointer" → `{CursorShadow: Flag(true)}`, "Not applied", "1 changes not applied"; Apply, `run_until_idle` → fake on, and no confirmation was shown.
  - `an_out_of_range_live_value_is_shown_and_never_written_unless_moved`: `put(FontSmoothingType, Number(3))`, `put(CaretWidth, Number(25))`, rescan → "Unknown (3)" and "25 px" drawn, `ve_pending_count() == 0`; the type's `…` → "Windows default" → `ve_pending[FontSmoothingType] == Number(2)`.
  - `a_gated_row_is_disabled_with_the_reason`: `put(UiEffects, Flag(false))`, rescan → `ve_control_disabled_reason(MenuAnimation) == Some(tr.t("ve.gate.master"))`; its "Windows default" item is disabled, while "Restore original" stays enabled once it has a snapshot; `ve_control_disabled_reason(DropShadow) == None`.
  - `the_preset_buttons_only_fill_pending_values`: click "For performance" → `ve_pending_count() == 8`, `fake.sets()` empty; Discard → 0.
  - `the_drag_link_opens_mouse_and_keyboard`: click the link → `active_tab == ActiveTab::MouseKeyboard`.
  - `a_setting_windows_cannot_read_is_disabled_with_the_error`, `the_wrong_account_shows_a_banner_instead_of_the_sections` (both names in the banner, no section title, Apply's disabled hover is that text), `ctrl_f_focuses_the_visual_effects_search`, `search_filters_rows_by_title` ("shadow" → the two Shadows rows, no "Menu animation"), `the_page_shows_the_loading_line_until_the_first_scan`, `opening_the_page_requests_a_scan`.
  - `shell.rs`: extend `shell_pages_draw_no_english_only_text_in_french` with the page: Performance preset pending, type 3 live, `fail_get(CaretWidth)`, master live off (gate hovers), a `…` menu open; then no match, `ve_scan = None`, account `Different` and `Unknown`. Add the new keys to its `positive` list.
- [ ] **Step 2: Run** `cargo test --locked visual_effects_tab` — Expected: FAIL.
- [ ] **Step 3: Implement.** Fix every non-exhaustive `match` on `ActiveTab` or `Icon`.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS, including `every_tab_renders_without_panicking` over 12 tabs and the icon tests.
- [ ] **Step 5: Commit** — `feat(visual-effects): Visual effects page with presets, gates and the account banner`.

### Task 6: History and Home

**Files:**
- Modify: `src/ui/originals.rs`, `src/ui/history_panel.rs`, `src/ui/home.rs`, `src/ui/shell.rs` (`open_page(History)` also reloads the ve store; the French test), both `.resx`.

**Interfaces — Consumes:** Task 4 (`ve_store`, `ve_store_error`, `ve_slot_name`, `request_restore_visual_effects`, `messages::{ve_value, ve_companions}`); Task 5 (`ActiveTab::VisualEffects`, `Icon::Effects`).

**Interfaces — Produces:**
- **Saved originals:** `OriginalsModule::VisualEffects` after `MouseKeyboard` (title `history.module.visual_effects`); `originals_of` gives one row per `ve_store.own_backups(user)`: title `ve_slot_name(key)`, description `ve.original_desc` with `ve_value(tr, EffectId::from_key(key), &value)` and the timestamp; the `ui_effects` row adds `"\n"` + `ve_companions(tr, n, titles)` when `companions_of(user, "ui_effects")` is not empty. Companions have no row. `RestoreRequest::VisualEffects(Vec<String>)` → `request_restore_visual_effects`; "Restore all" passes every listed key (the engine adds the companions). Unknown keys listed raw with Restore enabled; an unreadable store shows `originals.unreadable`.
- **History filter:** `HistoryFilter::VisualEffects` matches `"Visual effects"`; chip `history.module.visual_effects` after Mouse & keyboard (`labels[8]`).
- **Home tile:** `ModuleSummary { page: ActiveTab::VisualEffects, icon: Icon::Effects, status: home.visual_effects_status(_one) with own_backups(user).len(), drift: 0 }` after the Mouse & keyboard tile, from the store (right before the first scan).

| Key | English | French |
|---|---|---|
| `ve.original_desc` | Original: {0}, saved on {1} | Valeur d'origine : {0}, enregistrée le {1} |
| `home.visual_effects_status` | {0} effects changed by WinTune | {0} effets modifiés par WinTune |
| `home.visual_effects_status_one` | {0} effect changed by WinTune | {0} effet modifié par WinTune |

- [ ] **Step 1: Write the failing tests:**
  - `originals.rs`: `the_master_row_lists_its_companions_and_they_have_no_row`: apply master off → one row "UI effects" whose description contains "Original: On" and "Also restores 7 effects" and "Menu animation"; no row "Show shadows under mouse pointer"; Restore, confirm, `run_until_idle` → fake master on, the group is gone.
  - `originals.rs`: `a_companion_changed_directly_gets_its_own_row`: then apply `MenuAnimation` off → rows "UI effects" ("Also restores 6 effects") and "Menu animation" (description contains "Original: On").
  - `originals.rs`: `an_unknown_effect_is_listed_by_its_raw_key` (`hot_tracking`), `an_unreadable_visual_effects_store_shows_a_banner`.
  - `history_panel.rs`: `the_visual_effects_filter_shows_only_its_operations`.
  - `home.rs`: `the_visual_effects_tile_counts_snapshots_without_companions`: a store with `ui_effects`, its 7 companions and `caret_width` → "2 effects changed by WinTune"; forget `caret_width` → the `_one` text with 1; clicking the tile opens `ActiveTab::VisualEffects`.
  - `shell.rs`: the French test shows a Visual effects original with companions, its operation and the Home tile.
- [ ] **Step 2: Run** `cargo test --locked ui::` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(visual-effects): Saved originals with companions, History filter and Home tile`.

### Task 7: Docs, drift test and the owner's checklist

**Files:**
- Modify: `docs/registry-reference.md`, `README.md`, `ARCHITECTURE.md`, `src/visual_effects/mod.rs` (the test).

- [ ] **Step 1: Write the failing drift test** `the_registry_reference_names_every_visual_effect` in `src/visual_effects/mod.rs`, shaped like `the_registry_reference_names_every_mouse_keyboard_setting`. The doc contains: `` `{key}` `` for every `EffectId::ALL`; `0x{:04X}` of every descriptor's `get_action` and `set_action`; `` `HKCU\{key}` `` for both `DESKTOP_KEYS`; `` `visual-effects-backups.json` ``, `` `ANIMATIONINFO` ``, `` `SPIF_UPDATEINIFILE` ``, `` `UserPreferencesMask` `` and `Pre-merge checklist (Visual effects)`. Run `cargo test --locked the_registry_reference_names_every_visual_effect` — Expected: FAIL.
- [ ] **Step 2: `docs/registry-reference.md`:** a `## Visual effects` section after `## Mouse & keyboard`, in its format:
  - **What WinTune reads:** the GET actions (`ANIMATIONINFO` with `uiParam` 8 for window animation), the account check, and `HKCU\Control Panel\Desktop` and `HKCU\Control Panel\Desktop\WindowMetrics`, read only, for the `.reg`.
  - **What WinTune writes:** nothing directly; the SETs with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`, each read back; the master's write order; never `UserPreferencesMask`, `Explorer\Advanced` or `DWM`.
  - **The settings:** spec 5.1's table (id, section, GET, SET action and family in uppercase `0x` hex, control, gate, default with its tag), then the gates, ranges, window animation and defaults calls.
  - **The master's companions:** spec 5.3 (snapshots, History, restore order, forget, `HeldByMaster`).
  - **The `.reg` net:** spec 5.6, including that importing it also reverts `UserPreferencesMask` (Mouse & keyboard's ClickLock, Sonar and Vanish too), the wallpaper and the window metrics, and takes effect only at sign-in.
  - **Evidence:** a table (claim, DOCUMENTED / OBSERVED / FOLKLORE, Learn page) from the recon and spec 3, OBSERVED values included (contrast 1,200 live while `FontSmoothingGamma` reads 0).
  - **Not handled here:** spec 2's non-goals with their keys and tags.
  - **`#### Pre-merge checklist (Visual effects)`:** spec 7 items 1-8 copied as checkboxes, each with "Result: ______".
- [ ] **Step 3: README and ARCHITECTURE.**
  - **README:** "It has eight modules"; the feature bullet; a `## Visual effects` section (six sections, presets, gates, Windows default and Restore original, the master's companions in plain words, the account banner, why the `.reg` is not for importing); Saved originals "Visual effects" in Safety and undo; `visual-effects-backups.json` in Data files; `sysparams` and `visual_effects` in Code layout.
  - **ARCHITECTURE:** the store table row (`visual-effects-backups.json`, per user then id, `{ timestamp, value }` plus optional `companion_of`, through `sysparams::SpiBackupStore`); `sysparams` in Layers (what it shares and that each module keeps its ids, descriptors, trait, fake and engine); a `## Visual effects engine` section (catalog and gates, `EffectsApi` and the fake's master modes, the apply order with companions and the master's write order, `HeldByMaster`, restore and forget); the Mouse & keyboard section names the moved items; the UI paragraph.
- [ ] **Step 4: Run** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `rtk bypass cargo test --locked`, `cargo build --release --locked` — Expected: all clean, both drift tests passing.
- [ ] **Step 5: Commit** — `docs(visual-effects): README, ARCHITECTURE, registry reference and pre-merge checklist`.

## Rulings the spec left open

Each ruling gives the decision, then what it costs if it is wrong.

1. **Companions are seven:** the six `master` rows of 5.1 plus `tooltip_fade`, the kept members of Learn's UI effects list (spec 3 and 5.1 "Gates"). The page gate of `tooltip_fade` stays `tooltip_animation`. If wrong: one extra snapshot and SET that change nothing.
2. **Gating is transitive:** a row whose gate row is itself disabled is disabled with the outer reason (Menu effect while UI effects is off), and its pending value is dropped. If wrong: a row looks enabled that does nothing.
3. **`HeldByMaster` is decided by one GET of `ui_effects`** right after a differing readback of a companion, so "live off" is the value at that moment. If wrong: one GET more per failure.
4. **A `HeldByMaster` row keeps its snapshot** even when the read value equals the original (spec 5.3 says kept). If wrong: one History row to clear by hand.
5. **`AccountCheck::problem()` stays in `mouse_keyboard`** as an inherent impl, so its callers do not change; Visual effects uses `account_problem()`. If wrong: a rename later.
6. **`AppEnv.input_account` and `mk_reg_source` serve both modules** (same session check, same HKCU source); renaming them would touch Mouse & keyboard code. If wrong: two field names that read as Mouse & keyboard's.
7. **The shared executor builds `ANIMATIONINFO`** (`SetCall::Animation`, `spi_get_animation`). If wrong: none.
8. **`VeProblem` / `VeFailure` stay separate types** (spec 9 allows it). If wrong: some duplicated translations.
9. **The contrast is grouped by a translated separator** (`ve.value.thousands`) to read "1,200" / "1 200". If wrong: cosmetic.
10. **"A companion is forgotten on its own once back at its original" is read as: when it is restored, or when the master is back at its original.** A check right after recording would forget every companion in the "master only gates" case and lose the "also restores" row. If wrong: a companion already back at its original stays listed under the master until the master returns; restoring it writes the same value.
11. **The step is not validated**, only the range (contrast 1,250 can be kept or restored). If wrong: a hand-made value off the step is accepted.
12. **Performance leaves the Fade / Slide rows alone**; the gate rule then drops any of their pending values. If wrong: cosmetic.
13. **The drift test is a new one per module** (`the_registry_reference_names_every_visual_effect`), like each module's today. If wrong: none.
14. **Nav icon `E771` (Personalize).** If wrong: cosmetic.

## Not in this PR

- Spec 2 non-goals: thumbnails, translucent selection, icon label shadows, taskbar animations, Peek and thumbnail previews, `VisualFXSetting`, Transparency, gradient captions, hot tracking, `SPI_SETCLEARTYPE`, smoothing orientation, menu underlines, overlapped content, drag full windows (Mouse & keyboard).
- A `WM_SETTINGCHANGE` listener, a Home drift count, an in-page text sample (spec calls).
