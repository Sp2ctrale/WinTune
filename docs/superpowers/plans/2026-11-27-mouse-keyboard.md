# Mouse & keyboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Mouse & keyboard" page (French "Souris et clavier") that shows and changes the 18 settings of spec 5.1 in five sections, applies each one live through `SystemParametersInfoW`, checks each write by reading it back, and undoes it from a row or from History.

**Architecture:** A new `src/mouse_keyboard/` engine:
- `catalog.rs`: pure. The 18 ids, their sections, shapes, page ranges, Windows defaults, the pending `Wanted` choice and how it resolves to a value, and the problem / failure enums.
- `backend.rs`: the injectable `InputApi` trait, the real `SpiInput` (one static descriptor per setting), the test `FakeInput`, and the account check.
- `store.rs`: `mouse-keyboard-backups.json`, a parallel type with the shape of `backups::UserBackups` holding a typed value per setting, plus the read-only `.reg` export of the three `Control Panel` keys.
- `engine.rs`: the scan, one Apply pass (account → validate → read → snapshot → save store + `.reg` → write + read back → forget) and a Restore with `restore_ids`' rules.

WinTune writes no registry value for this module: Windows does, through the SET calls. The UI copies the Startup apps pattern (background scan, pending choices until Apply, one `RunningJob` variant, a Saved originals group, a Home tile). The setting id string (`SettingId::key`) is the join key between row, pending choice, store and History.

**Tech Stack:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52 (read-only `.reg`), windows-sys 0.59, serde. No new crate.

**Spec:** `docs/superpowers/specs/2026-11-27-mouse-keyboard-design.md` (binding, including its "Settled after the builder check" calls). Windows evidence: its section 3, from `recon-mouse-keyboard.md`. The owner's real-machine checklist: its section 7.

## Global Constraints

- **Versions:** Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59. **No new crate.** Task 2 adds the windows-sys features `Win32_UI_Input_KeyboardAndMouse` (`GetDoubleClickTime`), `Win32_System_RemoteDesktop` (WTS) and `Win32_Security_Authentication_Identity` (`GetUserNameExW`); `Win32_UI_WindowsAndMessaging` is already on.
- **Writes:** every change goes through a `SystemParametersInfoW` SET with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`, on a job worker thread, never the UI thread. **Nothing is ever written under `HKCU\Control Panel`** (above all `UserPreferencesMask`). `SPI_SETTHREADLOCALINPUTSETTINGS` (0x104F) is never called. A setting is never restored by replaying registry bytes.
- **Reads:** one GET per setting (an SPI GET, `GetSystemMetrics(SM_SWAPBUTTON)` or `GetDoubleClickTime()`), plus `SPI_GETFILTERKEYS`. The values directly in `HKCU\Control Panel\Mouse`, `\Desktop` and `\Keyboard` are read only, for the `.reg` net.
- **Safety order (spec 5.4):** account check → validate (any problem refuses the whole Apply, nothing read further or written) → GET every touched setting (a failed GET refuses the whole Apply) → `record_first` → save `mouse-keyboard-backups.json` **then** the `.reg` (either failure: nothing applied) → SET + GET each, in table order with `swap_buttons` last → forget each snapshot back at its original (exact equality; precision: the whole array). A SET whose readback differs is a failure, never a success. An unreadable store blocks Apply and Restore.
- **Tests:** they use `FakeInput` and **never call a real SET**. The SPI backend, the account check and the `.reg` source keys are injected through `AppEnv` (and `MkOptions`). Registry access only under `HKCU\Software\WinTuneTest` (`regsnap::test_support::Sandbox`), files only in temp folders. Never launch `WinTune.exe` (it needs UAC). UI tests live in `src/ui/*.rs` and use `TempIcons`.
- **Texts:** engine reasons are enums (`MkProblem`, `MkFailure`, `MkIssue`) translated in `src/i18n/messages.rs`. Every UI text is a literal key (`tr.t("mk.x")`) in both `i18n/strings.resx` and `i18n/strings.fr.resx`, French following that file's punctuation. `OperationReport` lines written to History stay English; the toast shows the translated issues.
- **History:** module label `Mouse & keyboard`; actions `Apply` and `Restore`; store `mouse-keyboard-backups.json` in `models::app_data_dir()`; the `.reg` is `reg-backups\<stamp>-mouse-keyboard.reg`.
- **Page:** kit only, no colors, sizes or fonts set on the page. Nav entry "Mouse & keyboard" in the Customize group, after Environment variables.
- **Commands:** run cargo from the PowerShell tool, not the Bash tool (its sandbox breaks the registry case tests). Write and edit files with the Write and Edit tools (Bash heredocs mangle backslashes here).
- **CI after every task:** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`. `rtk bypass cargo test --locked` shows the full result lines.
- **Commits:** on `feat/mouse-keyboard-1`; messages carry no attribution lines.

## Review Focus

1. **A live value outside the page span** (a 1,200 ms double-click set in the old dialog, wheel lines 0, trails 1). It is shown as it is, never written unless the user moves that control, and Restore puts it back exactly. (Task 5: `an_out_of_range_live_value_is_shown_and_never_written_unless_moved`; Task 3: `restore_sets_the_stored_value_exactly_even_outside_the_page_range`.)
2. **Settings changes a value while WinTune is open.** The GET at Apply time is the snapshot; precision off keeps the thresholds read then; a pending choice the new live value already matches is dropped by the next scan. (Task 3: `precision_off_uses_the_thresholds_read_at_apply_time`; Task 4: `a_scan_drops_choices_the_live_value_matches_or_cannot_read`.)
3. **The buttons swap in the middle of a batch.** Swap is written last; Apply asks first in both directions; a restore that swaps back warns too. (Task 3: `swap_buttons_is_written_last`; Task 4: `applying_a_swap_asks_first_either_way`, `restoring_the_swap_warns`.)
4. **WinTune elevated as another administrator** (over-the-shoulder UAC). The page shows the banner; Apply, and a Restore started from History, are refused with no GET or SET. (Task 3: `the_wrong_account_refuses_apply_and_restore_before_any_call`; Task 4: `a_restore_from_history_as_the_wrong_account_is_refused`; Task 5: `the_wrong_account_shows_a_banner_instead_of_the_sections`.)
5. **A control that depends on another pending choice:** the ClickLock delay while ClickLock is pending on or off, the trails length while trails are pending off. (Task 5: `clicklock_delay_and_trails_length_follow_the_shown_toggle`.)

---

## File Structure

- **Create `src/mouse_keyboard/`:**
  - `mod.rs`: module docs, re-exports, the drift test (Task 7);
  - `catalog.rs`: `SettingId`, `Section`, `Shape`, `SettingValue`, `SliderSpec`, `Control`, `Wanted`, `MkChange`, `resolve`, `wanted_matches`, `is_on`, `trails_start_length`, `check_change`, `MkProblem`, `MkFailure`, `MkIssue`;
  - `backend.rs`: `ApiError`, `InputApi`, `SpiInput`, the descriptors, `FakeInput` (`#[cfg(test)]`), `AccountCheck`, `compare_accounts`, `session_account_check`;
  - `store.rs`: `MkBackup`, `MkBackupStore`, load / save, `CONTROL_PANEL_KEYS`, `export_control_panel`;
  - `engine.rs`: `MkScan`, `scan_mouse_keyboard`, `MkOptions`, `MkReport`, `english_issue`, `apply_mouse_keyboard`, `restore_mouse_keyboard`.
- **Create in `src/ui/`:** `mouse_keyboard_state.rs` (scan job, pending choices, display values) and `mouse_keyboard_tab.rs` (the page, the double-click test).
- **Modify:** `Cargo.toml`, `src/lib.rs`; in `src/ui/`: `mod.rs`, `jobs.rs`, `confirm.rs`, `originals.rs`, `history_panel.rs`, `home.rs`, `shell.rs`, `polish.rs`, `test_support.rs`, `kit/controls.rs`, `kit/icons.rs`, `kit/tests.rs`; `src/i18n/messages.rs`, `src/i18n/tests.rs`; both `.resx`; `README.md`, `ARCHITECTURE.md`, `docs/registry-reference.md`.

---

### Task 1: The catalog (pure)

**Files:**
- Create: `src/mouse_keyboard/mod.rs`, `src/mouse_keyboard/catalog.rs`
- Modify: `src/lib.rs` (`pub mod mouse_keyboard;`)

**Interfaces — Produces (`catalog.rs`, re-exported by `mod.rs`):**
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub enum SettingId { PointerSpeed, PointerPrecision, PointerTrails, PointerSonar, PointerVanish, SnapToDefault, SwapButtons, DoubleClickTime, ClickLock, ClickLockTime, WheelLines, WheelChars, ScrollInactive, KeyRepeatDelay, KeyRepeatRate, HoverTime, MenuShowDelay, DragFullWindows }`, declared in spec 5.1 table order (so `Ord` is table order), with:
  - `pub const ALL: [SettingId; 18]`, table order;
  - `pub fn key(self) -> &'static str`: the spec's id (`"pointer_speed"` … `"drag_full_windows"`); `pub fn from_key(key: &str) -> Option<SettingId>` (exact match);
  - `pub fn section(self) -> Section`, `pub fn shape(self) -> Shape`, `pub fn control(self) -> Control`;
  - `pub fn windows_default(self) -> SettingValue`: spec 5.1 "Windows default" column (`PointerPrecision` → `Mouse(PRECISION_DEFAULT)`, `ScrollInactive` → `Number(2)`, `PointerVanish` and `DragFullWindows` → `Flag(true)`, trails → `Number(0)`);
  - `pub fn in_page_range(self, v: &SettingValue) -> bool`: false on a shape mismatch. Number ids use their `SliderSpec` span, except `PointerTrails` (`0` or `2..=7`), `WheelLines` (`1..=100` or `WHEEL_PAGESCROLL`) and `ScrollInactive` (`0..=2`). Flags: always true. Precision `Mouse([t1, t2, a])`: `t1 >= 0 && t2 >= 0 && a <= 2`. The step is not checked.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)] pub enum Section { Pointer, Buttons, Wheel, Keyboard, WindowsMenus }` with `pub const ALL: [Section; 5]`.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Shape { Number, Flag, Mouse }`. Flags: sonar, vanish, snap, swap, ClickLock, drag full windows. Mouse: precision. Every other id: Number.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(tag = "kind", content = "data")] pub enum SettingValue { Number(u32), Flag(bool), Mouse([i32; 3]) }` with `pub fn shape(&self) -> Shape`.
- `pub const WHEEL_PAGESCROLL: u32 = u32::MAX;` and `pub const PRECISION_DEFAULT: [i32; 3] = [6, 10, 1];`
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct SliderSpec { pub min: u32, pub max: u32, pub step: u32 }`.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Control { Toggle, Slider(SliderSpec), TrailsLength(SliderSpec), LinesOrScreen(SliderSpec) }`, from spec 5.1 "Page control": speed `Slider(1,20,1)`; trails `TrailsLength(2,7,1)`; double-click `Slider(200,900,10)`; ClickLock delay `Slider(200,2200,100)`; wheel lines `LinesOrScreen(1,100,1)`; wheel chars `Slider(1,100,1)`; delay `Slider(0,3,1)`; rate `Slider(0,31,1)`; hover `Slider(100,3000,50)`; menu delay `Slider(0,2000,50)`; every Flag id, precision and scroll inactive: `Toggle`.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Wanted { Value(SettingValue), Toggle(bool) }`. `Toggle` is only for `PointerPrecision` and `ScrollInactive`; the engine resolves it from the GET it makes at Apply time.
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct MkChange { pub id: SettingId, pub wanted: Wanted }`.
- `pub fn is_on(id: SettingId, v: &SettingValue) -> Option<bool>`: `Flag(b)` → `b`; precision → acceleration ≠ 0; scroll inactive → ≠ 0 (a live 1 is on); trails → `>= 2` (0 and 1 are off); other ids → `None`.
- `pub fn resolve(id: SettingId, wanted: &Wanted, live: &SettingValue, original: &SettingValue) -> Option<SettingValue>` (spec 5.1 calls):
  - `Value(v)` → `v`;
  - precision `Toggle(false)` → `Mouse([t1, t2, 0])` from `live`;
  - precision `Toggle(true)` → `original` when its acceleration ≠ 0; else `Mouse([t1, t2, 1])` when both live thresholds ≠ 0; else `Mouse(PRECISION_DEFAULT)`;
  - scroll inactive `Toggle(false)` → `Number(0)`; `Toggle(true)` → `original` when ≠ 0, else `Number(2)`;
  - any other `Toggle`, or a shape mismatch → `None`.
- `pub fn wanted_matches(id: SettingId, wanted: &Wanted, live: &SettingValue) -> bool`: `Value(v)` → `*live == v`; `Toggle(b)` → `is_on(id, live) == Some(b)`.
- `pub fn trails_start_length(original: &SettingValue) -> u32`: `Number(n)` with `n >= 2` → `n.min(7)`, else 7.
- `pub fn check_change(c: &MkChange) -> Option<MkProblem>`: `OutOfRange(id)` for a `Value` not `in_page_range`, or a `Toggle` on an id other than precision and scroll inactive.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum MkProblem { OutOfRange(SettingId), UnknownSetting(String), Unreadable { id: SettingId, code: u32 }, WrongAccount { process: String, session: String }, AccountUnknown(String), NotSaved(String), NoOriginal(String) }`. `NotSaved` = the store or `.reg` could not be written before any SET; `NoOriginal` = a restore id with no snapshot.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum MkFailure { SetFailed { id: SettingId, code: u32 }, Mismatch { id: SettingId, wanted: SettingValue, read: SettingValue }, Unverified { id: SettingId, code: u32 }, StoreNotSaved(String) }`. `Unverified` = the readback GET failed; `StoreNotSaved` = the store save after the writes failed.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum MkIssue { Problem(MkProblem), Failure(MkFailure) }`.

- [ ] **Step 1: Write the failing tests** in `catalog.rs`:
  - `ids_are_the_spec_table_in_order`: `ALL.map(key)` equals the 18 spec ids in table order; `from_key(k)` round-trips each; `from_key("caret_blink") == None`; sections of `ALL` are `[Pointer ×6, Buttons ×4, Wheel ×3, Keyboard ×2, WindowsMenus ×3]`.
  - `defaults_follow_spec_5_1`: speed `Number(10)`, precision `Mouse([6,10,1])`, vanish `Flag(true)`, double-click `Number(500)`, ClickLock delay `Number(1200)`, scroll inactive `Number(2)`, delay `Number(1)`, rate `Number(31)`, hover and menu `Number(400)`, drag `Flag(true)`, trails `Number(0)`; every default is `in_page_range` of its id.
  - `page_ranges_follow_spec_5_1`: accepted: speed 1 and 20, trails 0, 2 and 7, double-click 200 and 900, wheel lines 1, 100 and `WHEEL_PAGESCROLL`, scroll inactive 0, 1, 2, delay 3, rate 31, hover 100 and 3000, menu 0 and 2000, ClickLock delay 2200, precision `[4,12,2]`. Refused: speed 0 and 21, trails 1 and 8, double-click 199 and 901, wheel lines 0 and 101, wheel chars 0, scroll inactive 3, delay 4, rate 32, precision `[6,10,3]` and `[-1,10,1]`, `Flag(true)` for speed, `Number(1)` for swap.
  - `toggles_show_on_as_the_spec_says`: `is_on(PointerTrails, 1) == Some(false)`, `(PointerTrails, 2) == Some(true)`, `(ScrollInactive, 1) == Some(true)`, `(PointerPrecision, [6,10,0]) == Some(false)`, `(PointerSpeed, 10) == None`.
  - `precision_toggles_resolve_from_live_and_original`:
    - `Toggle(false)`, live `[4,12,2]` → `[4,12,0]`;
    - `Toggle(true)`, live `[4,12,0]`, original `[4,12,2]` → `[4,12,2]`;
    - `Toggle(true)`, live `[5,9,0]`, original `[5,9,0]` → `[5,9,1]`;
    - `Toggle(true)`, live `[0,0,0]`, original `[0,0,0]` → `[6,10,1]`;
    - `Value(Mouse([6,10,1]))` with original `[4,12,2]` → `[6,10,1]` (the "Windows default" call).
  - `scroll_inactive_toggles_resolve_from_the_original`: off → 0; on with original 1 → 1; on with original 0 → 2.
  - `wanted_matches_and_trails_length`: `wanted_matches(PointerPrecision, Toggle(true), [6,10,1])`; `!wanted_matches(PointerSpeed, Value(Number(12)), Number(10))`; `trails_start_length` of 0, 1, 5, 9 → 7, 7, 5, 7.
  - `check_change_refuses_out_of_range_and_stray_toggles`: speed 21 → `Some(OutOfRange(PointerSpeed))`; `Toggle(true)` on `SwapButtons` → `Some(OutOfRange(SwapButtons))`; speed 12 → `None`.
  - `values_serialize_tagged`: `serde_json::to_string(&Number(10)) == r#"{"kind":"Number","data":10}"#`, `Flag(false)` → `{"kind":"Flag","data":false}`, `Mouse([6,10,1])` → `{"kind":"Mouse","data":[6,10,1]}`, and each parses back.
- [ ] **Step 2: Run** `cargo test --locked mouse_keyboard::` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `catalog.rs`, `mod.rs` (`//!` docs naming the "never write under `HKCU\Control Panel`" rule; `mod catalog; pub use catalog::*;`) and the `lib.rs` line.
- [ ] **Step 4: Run** `cargo test --locked mouse_keyboard::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(mouse-keyboard): settings catalog, page ranges, defaults and toggle rules`.

### Task 2: The backend: `InputApi`, `SpiInput`, `FakeInput` and the account check

**Files:**
- Create: `src/mouse_keyboard/backend.rs`
- Modify: `src/mouse_keyboard/mod.rs` (`mod backend; pub use backend::*;`), `Cargo.toml` (the three windows-sys features of Global Constraints)

**Interfaces — Consumes:** Task 1.

**Interfaces — Produces:**
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct ApiError { pub code: u32 }` (the `GetLastError` code; 87 for a value of the wrong shape).
- `pub trait InputApi: Send + Sync { fn get(&self, id: SettingId) -> Result<SettingValue, ApiError>; fn set(&self, id: SettingId, value: &SettingValue) -> Result<(), ApiError>; fn filter_keys_on(&self) -> Result<bool, ApiError>; }`
- The descriptors (`pub(crate)`):
  - `enum GetCall { Spi(u32), SwapMetric, DoubleClickTime }`, `enum SetFamily { Ui, PvValue, PvArray }`;
  - `struct Descriptor { id: SettingId, get: GetCall, set: SetFamily, set_action: u32 }`, `const DESCRIPTORS: [Descriptor; 18]` in table order, and `fn descriptor(id) -> &'static Descriptor`;
  - `enum SetCall { Ui { action: u32, ui: u32 }, PvValue { action: u32, pv: usize }, PvArray { action: u32, array: [i32; 3] } }` and `fn set_call(id: SettingId, value: &SettingValue) -> Result<SetCall, ApiError>`: `Ui` puts the number (or the flag as 0/1) in `uiParam`; `PvValue` puts it in `pvParam` as a value (`uiParam` 0); `PvArray` passes the array. A shape mismatch gives `ApiError { code: 87 }`.
- `pub struct SpiInput;` implementing `InputApi`:
  - GET: `SystemParametersInfoW(action, 0, &mut out, 0)` into a `u32` (Number), an `i32` BOOL (Flag, ≠ 0) or `[i32; 3]` (Mouse); `GetSystemMetrics(SM_SWAPBUTTON) != 0`; `GetDoubleClickTime()`.
  - SET: the `set_call` parameters with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`; a `FALSE` return gives `ApiError { code: GetLastError() }`.
  - `filter_keys_on`: `SPI_GETFILTERKEYS` (0x0032), `uiParam` = `size_of::<FilterKeys>()`, into a local `#[repr(C)] struct FilterKeys { cb_size: u32, dw_flags: u32, i_wait_msec: u32, i_delay_msec: u32, i_repeat_msec: u32, i_bounce_msec: u32 }` with `cb_size` set; on when `dw_flags & FKF_FILTERKEYSON != 0`.
  - Every `unsafe` block has a `// SAFETY:` comment, like `tweaks::notify_explorer_settings`.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum AccountCheck { Same, Different { process: String, session: String }, Unknown(String) }` with `pub fn problem(&self) -> Option<MkProblem>` (`Different` → `WrongAccount`, `Unknown` → `AccountUnknown`).
- `pub fn compare_accounts(process: &str, session: &str) -> AccountCheck`: `Same` when equal after `to_lowercase()`.
- `pub fn session_account_check() -> AccountCheck`: the process account from `GetUserNameExW(NameSamCompatible)` (`DOMAIN\user`, from the token); the session's from `WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTSDomainName | WTSUserName)` joined as `domain\user`, each buffer freed with `WTSFreeMemory`. A failed call gives `Unknown("<api> failed: error <code>")`; an empty session user gives `Unknown("no user is signed in to this session")`. No test calls it.
- `#[cfg(test)] pub(crate) struct FakeInput` (a map behind a `Mutex`, `Send + Sync`) implementing `InputApi`:
  - `pub(crate) fn new() -> Self`, seeded with spec 3 "Seen here": speed 10, precision `[6,10,1]`, swap off, double-click 500, wheel 3 / 3, routing 2, delay 1, rate 31, snap off, trails 0, Sonar off, Vanish **on**, ClickLock off, ClickLock delay 1200, hover 400, menu 400, drag on; Filter Keys off;
  - `value(&self, id) -> SettingValue`; `put(&self, id, v)` (a change made elsewhere);
  - `fail_get(&self, id)`, `fail_set(&self, id)`: those calls return `ApiError { code: 5 }`;
  - `keep(&self, id, v)`: a later SET of `id` returns `Ok` but the value becomes `v`;
  - `set_filter_keys(&self, on: bool)`;
  - `calls(&self) -> Vec<FakeCall>` with `#[derive(Debug, Clone, PartialEq)] pub(crate) enum FakeCall { Get(SettingId), Set(SettingId, SettingValue), FilterKeys }`, and `sets(&self) -> Vec<(SettingId, SettingValue)>`.

- [ ] **Step 1: Write the failing tests** in `backend.rs`:
  - `descriptors_follow_the_spec_table`: `DESCRIPTORS.map(|d| d.id) == SettingId::ALL`, and each `(get, set, set_action)` equals:
    ```text
    speed       Spi(0x0070)  PvValue 0x0071   swap        SwapMetric       Ui 0x0021
    precision   Spi(0x0003)  PvArray 0x0004   double      DoubleClickTime  Ui 0x0020
    trails      Spi(0x005E)  Ui      0x005D   clicklock   Spi(0x101E) PvValue 0x101F
    sonar       Spi(0x101C)  PvValue 0x101D   cl. delay   Spi(0x2008) PvValue 0x2009
    vanish      Spi(0x1020)  PvValue 0x1021   wheel lines Spi(0x0068) Ui 0x0069
    snap        Spi(0x005F)  Ui      0x0060   wheel chars Spi(0x006C) Ui 0x006D
    scroll inactive Spi(0x201C) PvValue 0x201D   delay Spi(0x0016) Ui 0x0017
    rate  Spi(0x000A) Ui 0x000B   hover Spi(0x0066) Ui 0x0067
    menu  Spi(0x006A) Ui 0x006B   drag  Spi(0x0026) Ui 0x0025
    ```
    No descriptor's `set_action` is `0x104F`.
  - `set_calls_put_the_value_where_each_family_takes_it`: `set_call(PointerSpeed, Number(14)) == PvValue { action: 0x71, pv: 14 }`; `(ClickLockTime, Number(900)) == PvValue { 0x2009, 900 }`; `(ScrollInactive, Number(2)) == PvValue { 0x201D, 2 }`; `(SwapButtons, Flag(true)) == Ui { 0x21, 1 }`; `(WheelLines, Number(WHEEL_PAGESCROLL)) == Ui { 0x69, u32::MAX }`; `(PointerPrecision, Mouse([6,10,1])) == PvArray { 0x4, [6,10,1] }`; `(PointerSonar, Flag(false)) == PvValue { 0x101D, 0 }`; `(PointerSpeed, Flag(true)) == Err(ApiError { code: 87 })`.
  - `every_real_get_reads_its_shape_in_the_documented_range`: for each id, `SpiInput.get(id)` is `Ok` with `shape() == id.shape()`; speed in `1..=20`, delay `<= 3`, rate `<= 31`, routing `<= 2`, double-click `<= 5000`, precision acceleration `<= 2`; `SpiInput.filter_keys_on()` is `Ok`. **It never calls `set`.** If CI's session makes it flaky, mark it `#[ignore]` with a comment, as spec 5.2 allows.
  - `accounts_compare_ignoring_case`: `("PC\\Ana", "pc\\ana")` → `Same`; `("PC\\Admin", "PC\\Ana")` → `Different { process: "PC\\Admin", session: "PC\\Ana" }`; `Same.problem() == None`; `Unknown("x").problem() == Some(AccountUnknown("x"))`.
  - `the_fake_records_calls_fails_and_keeps_other_values`: `new().value(PointerVanish) == Flag(true)`; `set(PointerSpeed, 12)` then `get` → 12; after `keep(DoubleClickTime, Number(500))`, `set(DoubleClickTime, 700)` is `Ok` and `get` → 500; `fail_get(HoverTime)` → `get` is `Err(ApiError { code: 5 })`; `calls()` lists every call in order.
- [ ] **Step 2: Run** `cargo test --locked mouse_keyboard::backend` — Expected: FAIL.
- [ ] **Step 3: Implement** `backend.rs` and the `Cargo.toml` features.
- [ ] **Step 4: Run** `cargo test --locked mouse_keyboard::` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(mouse-keyboard): SystemParametersInfo backend, fake backend and account check`.

### Task 3: The store, the `.reg` net, and scan / apply / restore

**Files:**
- Create: `src/mouse_keyboard/store.rs`, `src/mouse_keyboard/engine.rs`
- Modify: `src/mouse_keyboard/mod.rs` (`mod store; pub use store::*; mod engine; pub use engine::*;`)

**Interfaces — Consumes:** Tasks 1-2; `backups::RegTarget`; `regsnap::{ValueSnapshot, RegValueExport, export_reg_full, unique_reg_file, write_reg_file}`; `clock::{now_local, file_stamp}`; `persist::{load_json, save_json, LoadError}`; `associations::OperationReport`.

**Interfaces — Produces (`store.rs`):**
- `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct MkBackup { pub timestamp: String, pub value: SettingValue }`.
- `#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] pub struct MkBackupStore { pub users: BTreeMap<String, BTreeMap<String, MkBackup>> }` (keys: user, then `SettingId::key`; an unknown key loads), with `backups_of(&self, user) -> Option<&BTreeMap<String, MkBackup>>`, `has_backup(&self, user, id) -> bool`, `original(&self, user, id) -> Option<&SettingValue>`, `record_first(&mut self, user, id, timestamp, value: SettingValue)` (keeps the first) and `forget(&mut self, user, id)` (drops an emptied user), like `backups::UserBackups`.
- `pub fn default_mk_store_path() -> PathBuf` (`app_data_dir().join("mouse-keyboard-backups.json")`), `pub fn load_mk_store(path: &Path) -> Result<MkBackupStore, LoadError>`, `pub fn save_mk_store(store: &MkBackupStore, path: &Path) -> Result<(), String>`.
- `pub const CONTROL_PANEL_KEYS: [&str; 3] = [r"Control Panel\Mouse", r"Control Panel\Desktop", r"Control Panel\Keyboard"];`
- `pub fn export_control_panel(source: &RegTarget, reg_dir: &Path) -> Result<PathBuf, String>`: for each key under `source.path(key)`, every value directly in it (never a subkey) as a `RegValueExport` with `source.root_name`; a missing key is skipped; `export_reg_full(&[], &values)` (no `[-key]` line) to `unique_reg_file(reg_dir, "{file_stamp}-mouse-keyboard")`. A read or write error gives `Err("Nothing applied: …")`. Read only.

**Interfaces — Produces (`engine.rs`):**
- `#[derive(Debug, Clone, PartialEq)] pub struct MkScan { pub values: BTreeMap<SettingId, Result<SettingValue, ApiError>>, pub filter_keys_on: bool, pub account: AccountCheck }` with `live(&self, id) -> Option<SettingValue>`, `read_error(&self, id) -> Option<ApiError>`, `account_ok(&self) -> bool`.
- `pub fn scan_mouse_keyboard(api: &dyn InputApi, account: &dyn Fn() -> AccountCheck) -> MkScan`: one GET per id in table order, then `filter_keys_on` (an error counts as off), then `account()`. No SET.
- `pub struct MkOptions<'a> { pub user: String, pub store_path: PathBuf, pub reg_backup_dir: PathBuf, pub reg_source: &'a RegTarget, pub account: &'a dyn Fn() -> AccountCheck, pub progress: &'a dyn Fn(usize, usize, &str) }`.
- `#[derive(Debug, Clone, Default)] pub struct MkReport { pub report: OperationReport, pub issues: Vec<MkIssue> }`. Invariant: `report.errors[i] == english_issue(&issues[i])`, one line per issue, nothing else in `errors`. `report.succeeded` holds `SettingId::key` strings.
- `pub fn english_issue(issue: &MkIssue) -> String`: English lines naming the setting by `key()`, values by a private `english_value` (`"500"`, `"one screen at a time"` for `WHEEL_PAGESCROLL`, `"on"` / `"off"`, `"{6, 10, 1}"`). `Mismatch` reads `"{key}: Windows kept {read} instead of {wanted}"`; `WrongAccount` reads `"WinTune runs as {process}, but {session} is signed in"`.
- `pub fn apply_mouse_keyboard(api: &dyn InputApi, store: &mut MkBackupStore, changes: &[MkChange], opts: &MkOptions) -> MkReport`, in the Global Constraints order:
  1. `(opts.account)()`: not `Same` → its problem, return (no API call).
  2. `check_change` on each; any problem → return.
  3. GET each touched id; any error → one `Unreadable` per failure, return.
  4. `record_first(user, id.key(), now_local(), live)`; `save_mk_store`, then `export_control_panel(opts.reg_source, &opts.reg_backup_dir)`; either error → `NotSaved(e)`, return. Set `report.reg_backup_file`.
  5. Ids sorted by `(id == SwapButtons, id)`. For each: `target = resolve(id, &wanted, &live, store.original(..))`; SET (always, even when equal to live); `Err` → `SetFailed`, next id. Then GET: `Err` → `Unverified`; a different value → `Mismatch { wanted: target, read }`; equal → `succeeded.push(id.key())`.
  6. "After" value per id: the readback (`Mismatch`: the read value; `SetFailed`: the live value of step 3; `Unverified`: none, keep). Forget each id whose after value equals its original; if anything was forgotten, `save_mk_store`; an error → `StoreNotSaved(e)`.
- `pub fn restore_mouse_keyboard(api: &dyn InputApi, store: &mut MkBackupStore, ids: &[String], opts: &MkOptions) -> MkReport`: account check first, as in Apply. Then, `swap_buttons` last: an unknown key → `UnknownSetting(key)`; no snapshot → `NoOriginal(key)`; else SET the stored value exactly (no range check), GET it back, forget only on equality (`SetFailed` / `Unverified` / `Mismatch` keep it). Save the store when anything was forgotten (`StoreNotSaved` on error). No `.reg`.

Test setup: `FakeInput::new()`, a temp folder for the store and `reg`, a sandbox `RegTarget` with base `{sandbox}\HKCU` whose `Control Panel\Mouse` holds `MouseSensitivity` = `"10"` and `Control Panel\Desktop` holds `UserPreferencesMask` (`REG_BINARY` `9E 1E 07 80 12 00 00 00`), `account = || AccountCheck::Same`, `USER = "TEST\\user"`.

- [ ] **Step 1: Write the failing tests.** In `store.rs`:
  - `the_store_json_has_the_spec_shape`: a store with `pointer_precision` `Mouse([6,10,1])`, `pointer_speed` `Number(10)`, `swap_buttons` `Flag(false)`, all timestamp `"x"`, under `"U"`, serializes to exactly `{"users":{"U":{"pointer_precision":{"timestamp":"x","value":{"kind":"Mouse","data":[6,10,1]}},"pointer_speed":{"timestamp":"x","value":{"kind":"Number","data":10}},"swap_buttons":{"timestamp":"x","value":{"kind":"Flag","data":false}}}}}` and parses back equal; a missing file loads the default; `{"users":{"U":{"caret_blink":{"timestamp":"x","value":{"kind":"Number","data":530}}}}}` loads.
  - `record_first_keeps_the_oldest_and_forget_drops_an_empty_user`.
  - `the_reg_net_exports_top_level_values_only`: add a subkey `Control Panel\Desktop\WindowMetrics` with a value `BorderWidth`; export → the file (UTF-16LE with BOM) contains `"MouseSensitivity"="10"` and `"UserPreferencesMask"=hex:9e,1e,07,80,12,00,00,00`, contains neither `WindowMetrics` nor `BorderWidth` nor `[-`, and has no `Control Panel\Keyboard]` header (that key is missing). The sandbox values are unchanged afterwards.
  
  In `engine.rs`:
  - `the_scan_reads_every_setting_once_and_records_failed_reads`: `fail_get(ClickLockTime)` → `read_error(ClickLockTime) == Some(ApiError { code: 5 })`, 17 `live` values, `filter_keys_on == false`, `account == Same`; `calls()` is 18 `Get`s in table order then `FilterKeys`, no `Set`.
  - `apply_sets_reads_back_and_keeps_the_original`: `Value(Number(14))` for speed → `value(PointerSpeed) == Number(14)`; the store holds `pointer_speed` → `Number(10)` under `USER`; one `.reg` file in the reg folder; `report.succeeded == ["pointer_speed"]`; `calls()` == `[Get(PointerSpeed), Set(PointerSpeed, Number(14)), Get(PointerSpeed)]`.
  - `the_whole_apply_is_refused_on_a_bad_value`: speed 21 plus a valid wheel chars 5 → `issues == [Problem(OutOfRange(PointerSpeed))]`, no `Get` or `Set` call, no store file, no `.reg`.
  - `a_failed_get_refuses_the_whole_apply`: `fail_get(HoverTime)`, change hover and speed → `issues == [Problem(Unreadable { id: HoverTime, code: 5 })]`, `sets()` empty, no store file.
  - `windows_keeping_another_value_fails_that_setting_only`: `keep(DoubleClickTime, Number(500))`; double-click 700 and speed 14 → `issues == [Failure(Mismatch { id: DoubleClickTime, wanted: Number(700), read: Number(500) })]`, `report.errors[0]` contains `"Windows kept 500 instead of 700"`, `succeeded == ["pointer_speed"]`; `double_click_time` is forgotten (back at its original), `pointer_speed` is kept; `report.errors.len() == issues.len()`.
  - `a_failed_set_is_reported_and_the_next_is_still_tried`: `fail_set(PointerSpeed)` with speed and rate → `SetFailed { id: PointerSpeed, code: 5 }`, rate set; `pointer_speed` forgotten (unchanged).
  - `swap_buttons_is_written_last`: swap `Flag(true)`, speed 12, rate 20 → `sets()` ids are `[PointerSpeed, KeyRepeatRate, SwapButtons]`.
  - `precision_off_uses_the_thresholds_read_at_apply_time`: the fake holds `[4,12,2]` (changed after any scan) → `Toggle(false)` sets `[4,12,0]`, the snapshot is `[4,12,2]`; then `Toggle(true)` sets `[4,12,2]` and the snapshot is forgotten.
  - `precision_on_without_an_accelerated_original_and_the_windows_default`: fake `[6,10,0]` → `Toggle(true)` sets `[6,10,1]`; fake `[0,0,0]` → sets `[6,10,1]`; fake `[4,12,2]` with `Value(Mouse([6,10,1]))` → sets exactly `[6,10,1]`.
  - `scroll_inactive_on_brings_back_an_original_one`: fake routing 1; `Toggle(false)` → 0; `Toggle(true)` → 1 and forgotten. Fresh fake routing 0: `Toggle(true)` → 2.
  - `editing_back_to_the_original_forgets_the_snapshot`: speed 14, then 10 → `store.users` is empty and the store file holds `{"users":{}}`.
  - `restore_sets_the_stored_value_exactly_even_outside_the_page_range`: fake double-click 1200 and precision `[4,12,2]`; apply double-click 500 and precision `Toggle(false)`; `restore_mouse_keyboard(["double_click_time", "pointer_precision"])` → the fake holds 1200 and `[4,12,2]`, `store.users` is empty, no new `.reg` file.
  - `a_restore_windows_does_not_keep_keeps_the_snapshot`: after an apply of speed 14, `keep(PointerSpeed, Number(14))`, restore → `Mismatch`, the snapshot is still there.
  - `restore_of_an_unknown_or_unrecorded_id_reports_it`: a hand-made `caret_blink` snapshot and an unrecorded `pointer_speed` → issues `UnknownSetting("caret_blink")` and `NoOriginal("pointer_speed")`; no `Set` call; `caret_blink` stays in the store.
  - `the_wrong_account_refuses_apply_and_restore_before_any_call`: `account = || Different { process: "PC\\Admin", session: "PC\\Ana" }` → apply and restore each give exactly `[Problem(WrongAccount { .. })]`, `calls()` is empty, no store file; `Unknown("x")` → `AccountUnknown("x")`.
  - `an_unsavable_store_or_reg_refuses_before_any_write`: `store_path` under a path whose parent is a file → `NotSaved`, `sets()` empty; then a valid store path with `reg_backup_dir` = an existing file → `NotSaved`, `sets()` empty.
  - `nothing_under_control_panel_is_written`: snapshot every value of the three sandbox keys (and the `WindowMetrics` subkey) before, run an apply of 6 settings and a restore of all → identical after.
- [ ] **Step 2: Run** `cargo test --locked mouse_keyboard::store` and `cargo test --locked mouse_keyboard::engine` — Expected: FAIL.
- [ ] **Step 3: Implement** `store.rs` and `engine.rs`.
- [ ] **Step 4: Run** `cargo test --locked mouse_keyboard` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(mouse-keyboard): snapshot store, .reg net, scan, apply with readback and restore`.

### Task 4: App wiring: scan job, pending choices, jobs, confirmations and messages

**Files:**
- Create: `src/ui/mouse_keyboard_state.rs`
- Modify: `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/confirm.rs`, `src/ui/test_support.rs`, `src/i18n/messages.rs`, `src/i18n/tests.rs`, both `.resx`.

**Interfaces — Consumes:** Tasks 1-3; `tasks::BackgroundJob`; `refuse_if_busy`; `busy_or_blocked`; `record_history`; `show_report`; `confirm::named_list`.

**Interfaces — Produces:**
- **`AppEnv` gains** `input: Arc<dyn InputApi>`, `input_account: Arc<dyn Fn() -> AccountCheck + Send + Sync>`, `mk_store_path: PathBuf`, `mk_reg_source: Arc<dyn Fn() -> RegTarget + Send + Sync>`. Production in `system()`: `Arc::new(SpiInput)`, `Arc::new(session_account_check)`, `default_mk_store_path()`, `Arc::new(RegTarget::current_user)`.
- **`TempIcons`** gains `input: Arc<FakeInput>` and `account: Arc<Mutex<AccountCheck>>` (`Same`); `env()` wires them, `dir/mouse-keyboard-backups.json`, and `mk_reg_source` = sandbox `{sb}\HKCU`. Helpers: `fake_input(&self) -> &FakeInput`, `set_account(&self, a: AccountCheck)`, and `run_until_mk_scanned(app, ctx)` like `run_until_env_scanned`.
- **`WinTuneApp` fields:** `mk_store: MkBackupStore`, `mk_store_error: Option<LoadError>`, `mk_scan: Option<MkScan>`, `mk_scan_job: Option<BackgroundJob<MkScan>>`, `mk_scan_needed: bool`, `mk_pending: BTreeMap<SettingId, Wanted>`, `mk_search: String`. `new()` calls `reload_mk_store()` and `request_mk_scan()`; `ui()` calls `pump_mk_scan(ctx)` after `pump_env_scan`.
- **`mouse_keyboard_state.rs`:**
  - `reload_mk_store`, `request_mk_scan`, `pump_mk_scan(ctx)`, shaped like the Startup ones (scan never while `is_busy()`; panic → toast `mk.scan_failed`). The scan job runs `scan_mouse_keyboard(&*env.input, &*env.input_account)`. When it finishes, pending entries are dropped when the scan could not read that id or `wanted_matches(id, w, live)`.
  - `mk_original(&self, id) -> Option<SettingValue>`: this user's snapshot.
  - `mk_display(&self, id) -> Option<SettingValue>`: the pending `Value`, or a pending `Toggle` previewed with `resolve(id, w, live, original or live)`, else the live value; `None` when unread.
  - `set_mk_wanted(&mut self, id, w: Wanted)`: removes the entry when `wanted_matches` the live value, else inserts.
  - `mk_changes(&self) -> Vec<MkChange>`: empty before the first scan or when `!account_ok()`; else one per pending id that is readable and not matched, in id order. `mk_pending_count()` is its length; `discard_mk_changes()` clears `mk_pending`.
  - `mk_slot_name(&self, key: &str) -> String`: `messages::mk_title` of a known key, else the raw key.
- **`jobs.rs`:**
  - `MkOutcome { report: MkReport, store: MkBackupStore, restored: bool }`; `RunningJob::MouseKeyboard { job, restore }` in every arm: `title_key` `job.apply_mouse_keyboard` / `job.restore_mouse_keyboard`, `name_key` `job.name.apply_mouse_keyboard` / `job.name.restore_mouse_keyboard`, `history_label` `("Mouse & keyboard", "Apply" | "Restore")`. The keys test lists the four keys.
  - `mk_blocked_reason(&self) -> Option<String>`: `tweaks.blocked` of an unreadable store.
  - `start_mk_apply(ctx)`: `refuse_if_busy` → blocked (error toast) → no changes, nothing → a change of `SwapButtons` → `request_apply_mouse_keyboard()`; else `run_mk_apply(ctx)`.
  - `run_mk_apply(ctx)` (busy and blocked re-checked, changes recomputed) and `start_mk_restore(ctx, ids: Vec<String>)` (blocked only by the store) both go through `spawn_mk_job`, which builds `MkOptions` inside the job from `env` (`reg_source` = `(env.mk_reg_source)()`, `account` = `&*env.input_account`).
  - `finish_mk_job`: replaces `mk_store`; a clean apply clears `mk_pending`; a restore removes the pending entry of every `SettingId::from_key` in `succeeded`. `show_report` (`ReportKind::AppliedMouseKeyboard` / `RestoredMouseKeyboard`) gets a copy of the report whose `errors` are `issues` translated by `messages::mk_issue`; `record_history("Mouse & keyboard", …)` gets the English report.
  - `poll_job` ends with `request_mk_scan()`; its panic branch also calls `reload_mk_store()`.
- **`confirm.rs`:** `ConfirmAction::ApplyMouseKeyboard` and `ConfirmAction::RestoreMouseKeyboard(Vec<String>)`.
  - `request_apply_mouse_keyboard()`: title `mk.apply_title`, message `mk.swap_warning`, confirm label `tweaks.apply`.
  - `request_restore_mouse_keyboard(ids: Vec<String>)`: empty ids do nothing; title `mk.restore_title`; message `mk.restore_message` with `named_list(…, mk_slot_name, "mk.restore_many")`, plus `"\n\n"` + `mk.swap_warning` when `"swap_buttons"` is among the ids; label `confirm.restore`.
  - `resolve_confirm`: `ApplyMouseKeyboard` → `run_mk_apply`; `RestoreMouseKeyboard(ids)` → `start_mk_restore`.
- **`messages.rs`** (each key a literal match arm):
  - `mk_title(tr, SettingId)`, `mk_description(tr, SettingId) -> Option<String>`, `mk_section(tr, Section)`;
  - `mk_value(tr, id: Option<SettingId>, v: &SettingValue) -> String` (spec 5.7 "as on the page"): `Flag` → `mk.value.on` / `mk.value.off`; `Mouse([a,b,c])` → `"{a, b, c}"`; `Number`: `WHEEL_PAGESCROLL` for wheel lines → `mk.value.one_screen`, wheel lines 0 → `mk.value.no_scroll`; trails 0 or 1 and scroll inactive 0 → off; scroll inactive 1 or 2 → on; double-click, ClickLock delay, hover and menu delay → `mk.value.ms`; anything else (and `None` ids) → the number;
  - `mk_problem(tr, &MkProblem)`, `mk_failure(tr, &MkFailure)`, `mk_issue(tr, &MkIssue)`; settings named by `mk_title`, unknown ones by their raw key;
  - `history_module` maps `"Mouse & keyboard"` to `history.module.mouse_keyboard`.

**Keys added in this task (EN / FR):**

| Key | English | French |
|---|---|---|
| `job.apply_mouse_keyboard` | Applying mouse and keyboard changes… | Application des changements de souris et clavier… |
| `job.restore_mouse_keyboard` | Restoring mouse and keyboard settings… | Restauration des réglages de souris et clavier… |
| `job.name.apply_mouse_keyboard` | Applying mouse and keyboard changes | L'application des changements de souris et clavier |
| `job.name.restore_mouse_keyboard` | Restoring mouse and keyboard settings | La restauration des réglages de souris et clavier |
| `report.applied_mouse_keyboard` | {0} setting(s) applied | {0} réglage(s) appliqué(s) |
| `report.restored_mouse_keyboard` | {0} setting(s) restored | {0} réglage(s) restauré(s) |
| `history.module.mouse_keyboard` | Mouse & keyboard | Souris et clavier |
| `mk.scan_failed` | Could not read the mouse and keyboard settings: {0} | Impossible de lire les réglages de la souris et du clavier : {0} |
| `mk.apply_title` | Swap the mouse buttons? | Inverser les boutons de la souris ? |
| `mk.swap_warning` | Your primary and secondary mouse buttons swap as soon as this is applied. To undo it, click with the other button. | Vos boutons principal et secondaire s'inversent dès que ce changement est appliqué. Pour l'annuler, cliquez avec l'autre bouton. |
| `mk.restore_title` | Restore original settings? | Restaurer les réglages d'origine ? |
| `mk.restore_message` | {0} will get back the value they had before WinTune changed them. | {0} retrouveront la valeur qu'ils avaient avant que WinTune les modifie. |
| `mk.restore_many` | {0} settings | {0} réglages |
| `mk.section.pointer` | Pointer | Pointeur |
| `mk.section.buttons` | Buttons and clicks | Boutons et clics |
| `mk.section.wheel` | Wheel | Molette |
| `mk.section.keyboard` | Keyboard | Clavier |
| `mk.section.windows` | Windows and menus | Fenêtres et menus |
| `mk.pointer_speed.title` | Pointer speed | Vitesse du pointeur |
| `mk.pointer_precision.title` | Enhance pointer precision | Améliorer la précision du pointeur |
| `mk.pointer_precision.description` | The pointer goes farther when you move the mouse faster. | Le pointeur va plus loin quand vous déplacez la souris plus vite. |
| `mk.pointer_trails.title` | Pointer trails | Traînées du pointeur |
| `mk.pointer_sonar.title` | Show the pointer's location when I press Ctrl | Afficher l'emplacement du pointeur quand j'appuie sur Ctrl |
| `mk.pointer_vanish.title` | Hide the pointer while typing | Masquer le pointeur pendant la saisie |
| `mk.snap_to_default.title` | Move the pointer to the default button in dialogs | Placer le pointeur sur le bouton par défaut des boîtes de dialogue |
| `mk.swap_buttons.title` | Swap the primary and secondary buttons | Inverser les boutons principal et secondaire |
| `mk.double_click_time.title` | Double-click speed | Vitesse du double-clic |
| `mk.click_lock.title` | ClickLock | Verrouillage du clic |
| `mk.click_lock.description` | Hold the button briefly to lock it; click again to release it. | Maintenez le bouton un instant pour le verrouiller ; cliquez à nouveau pour le libérer. |
| `mk.click_lock_time.title` | ClickLock delay | Délai du verrouillage du clic |
| `mk.click_lock_time.description` | How long to hold the button before it locks. | Durée d'appui avant le verrouillage. |
| `mk.wheel_lines.title` | Lines to scroll at a time | Lignes à faire défiler à la fois |
| `mk.wheel_chars.title` | Characters to scroll sideways at a time | Caractères à faire défiler horizontalement à la fois |
| `mk.scroll_inactive.title` | Scroll inactive windows when hovering over them | Faire défiler les fenêtres inactives au survol |
| `mk.key_repeat_delay.title` | Repeat delay | Délai avant répétition |
| `mk.key_repeat_delay.description` | From short (0, about 250 ms) to long (3, about 1 second). | De court (0, environ 250 ms) à long (3, environ 1 seconde). |
| `mk.key_repeat_rate.title` | Repeat rate | Vitesse de répétition |
| `mk.key_repeat_rate.description` | From slow (0) to fast (31). | De lent (0) à rapide (31). |
| `mk.hover_time.title` | Hover time | Délai de survol |
| `mk.hover_time.description` | How long the pointer rests before a tooltip or hover effect shows. | Temps d'arrêt du pointeur avant qu'une info-bulle ou un effet de survol apparaisse. |
| `mk.menu_show_delay.title` | Menu delay | Délai d'ouverture des menus |
| `mk.menu_show_delay.description` | How long before a submenu opens. | Temps avant l'ouverture d'un sous-menu. |
| `mk.drag_full_windows.title` | Show window contents while dragging | Afficher le contenu des fenêtres pendant le déplacement |
| `mk.value.on` | On | Activé |
| `mk.value.off` | Off | Désactivé |
| `mk.value.ms` | {0} ms | {0} ms |
| `mk.value.one_screen` | One screen at a time | Un écran à la fois |
| `mk.value.no_scroll` | 0 (no scrolling) | 0 (pas de défilement) |
| `mk.problem.out_of_range` | {0}: this value is outside what WinTune allows. | {0} : cette valeur est hors des limites permises par WinTune. |
| `mk.problem.unknown` | {0} is a setting this version of WinTune does not know. | {0} est un réglage que cette version de WinTune ne connaît pas. |
| `mk.problem.unreadable` | {0}: Windows could not read this setting (error {1}). | {0} : Windows n'a pas pu lire ce réglage (erreur {1}). |
| `mk.problem.wrong_account` | WinTune runs as {0}, but {1} is signed in. Mouse and keyboard settings would be saved for the wrong account. | WinTune s'exécute en tant que {0}, mais c'est {1} qui a ouvert la session. Les réglages de la souris et du clavier seraient enregistrés pour le mauvais compte. |
| `mk.problem.account_unknown` | WinTune cannot tell which account is signed in ({0}), so it does not change mouse and keyboard settings. | WinTune ne peut pas savoir quel compte a ouvert la session ({0}) : il ne modifie donc pas les réglages de la souris et du clavier. |
| `mk.problem.not_saved` | Nothing applied: the originals or the .reg backup could not be saved ({0}). | Rien n'a été appliqué : les valeurs d'origine ou la sauvegarde .reg n'ont pas pu être enregistrées ({0}). |
| `mk.problem.no_original` | {0}: no original value is saved. | {0} : aucune valeur d'origine n'est enregistrée. |
| `mk.failure.set` | {0}: Windows refused the change (error {1}). | {0} : Windows a refusé le changement (erreur {1}). |
| `mk.failure.mismatch` | {0}: Windows kept {1} instead of {2}. | {0} : Windows a gardé {1} au lieu de {2}. |
| `mk.failure.unverified` | {0}: WinTune could not read the setting back (error {1}). | {0} : WinTune n'a pas pu relire le réglage (erreur {1}). |
| `mk.failure.store` | The change was made, but the saved originals could not be updated: {0} | Le changement est fait, mais les valeurs d'origine n'ont pas pu être mises à jour : {0} |

- [ ] **Step 1: Write the failing tests:**
  - `mouse_keyboard_state.rs`:
    - `the_mouse_keyboard_scan_runs_in_background_and_waits_for_jobs`: like the Startup one; after it, `mk_scan` has 18 values and `account_ok()`.
    - `a_scan_drops_choices_the_live_value_matches_or_cannot_read`: pending speed `Value(14)`, precision `Toggle(false)`, wheel chars `Value(5)`; then `fake.put(PointerSpeed, Number(14))` and `fake.fail_get(WheelChars)`; scan → `mk_pending == {PointerPrecision: Toggle(false)}`.
    - `the_display_previews_a_pending_toggle`: fake precision `[4,12,2]`, pending `Toggle(false)` → `mk_display(PointerPrecision) == Some(Mouse([4,12,0]))`; `set_mk_wanted(PointerSpeed, Value(Number(10)))` with live 10 leaves no entry.
    - `changes_are_empty_before_the_first_scan_and_for_the_wrong_account`: pending speed 14 before a scan → `mk_changes()` empty; scanned → one; `set_account(Different{..})` and rescan → empty.
    - `a_slot_is_named_by_its_title_or_its_raw_key`: `"pointer_speed"` → "Pointer speed"; `"caret_blink"` → `"caret_blink"`.
  - `jobs.rs`:
    - `a_mouse_keyboard_apply_sets_logs_and_clears_pending`: pending speed 14 → `start_mk_apply`, `run_until_idle` → fake speed 14; history `("Mouse & keyboard", "Apply")` without errors; `mk_pending` empty; `mk_store` holds `pointer_speed`; one `.reg` file in `reg_backup_dir`; the toast starts with "1 setting(s) applied".
    - `a_partial_apply_keeps_the_choices_and_the_toast_is_translated`: `set_language("fr")`; `keep(DoubleClickTime, Number(500))`; pending double-click 700 and speed 14 → after `finish_job_without_scanning` (a frame would run the scan that prunes), both entries are still pending; `last_notice()` contains `messages::mk_failure(tr, &Mismatch { .. })` in French; the history entry's error contains "Windows kept 500 instead of 700".
    - `a_restore_drops_the_pending_choice_of_what_it_restored`: apply speed 14 and rate 20; pending speed 12 and delay 3; restore `["pointer_speed"]` → fake speed 10; `mk_pending` holds delay only.
    - `an_unreadable_mouse_keyboard_store_blocks_apply_and_restore`: first assert `mk_pending_count() == 1`; then `mk_store_error = Some(corrupt(..))` → `start_mk_apply` and `start_mk_restore` start no job, both toasts start with "Changes are blocked".
    - `a_restore_from_history_as_the_wrong_account_is_refused`: apply speed 14; `set_account(Different { process: "PC\\Admin", session: "PC\\Ana" })`; `start_mk_restore(["pointer_speed"])`, `run_until_idle` → fake speed still 14, the snapshot kept, the history error contains `"runs as PC\\Admin"`.
    - `a_panicking_mouse_keyboard_job_is_logged_and_reloads_the_store`, like the Environment one.
  - `confirm.rs`:
    - `applying_a_swap_asks_first_either_way`: pending swap `Flag(true)` → `pending_confirm.action == ApplyMouseKeyboard`, message == `mk.swap_warning`; accept, `run_until_idle` → fake swap on. Rescan, pending `Flag(false)` → asks again. A batch of speed only → no confirm and a job runs.
    - `restoring_the_swap_warns`: after the swap above, `request_restore_mouse_keyboard(["swap_buttons"])` → message contains the title "Swap the primary and secondary buttons" and `mk.swap_warning`; `request_restore_mouse_keyboard(["pointer_speed"])` → no swap warning.
  - `src/i18n/tests.rs`: `every_mouse_keyboard_message_translates_in_every_shipped_language`, shaped like the Environment one: all 18 `mk_title` (unique), 5 `mk_section`, every `MkProblem` and `MkFailure` variant (with `PointerSpeed`, code 5, strings `"X"`; each text contains its argument), `mk_value(tr, Some(WheelLines), &Number(WHEEL_PAGESCROLL)) == tr.t("mk.value.one_screen")`, `mk_value(tr, Some(DoubleClickTime), &Number(500))` contains `500`, `mk_value(tr, Some(PointerPrecision), &Mouse([6,10,1])) == "{6, 10, 1}"` (left out of the no-`{` check), and `history_module(tr, "Mouse & keyboard") == tr.t("history.module.mouse_keyboard")` and is not the key. Every other text is non-empty, has no `{` and does not start with `mk.` or `history.`.
- [ ] **Step 2: Run** `cargo test --locked ui::` and `cargo test --locked i18n` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.** Add every key above to both `.resx` files.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(mouse-keyboard): scan job, pending choices, apply and restore jobs and confirmations`.

### Task 5: The Mouse & keyboard page

**Files:**
- Create: `src/ui/mouse_keyboard_tab.rs`
- Modify:
  - `src/ui/kit/controls.rs` (`slider`) and `src/ui/kit/tests.rs`;
  - `src/ui/kit/icons.rs`: `Icon::Mouse` = `'\u{E962}'` (Mouse), `ALL: [Icon; 23]`; if the font test says the glyph is missing, `'\u{E765}'` (KeyboardClassic). `kit/tests.rs`: `icon_ordinal` gets `Mouse => 22`, `Icon::ALL.len() == 23`;
  - `src/ui/mod.rs`: `ActiveTab::MouseKeyboard` after `Environment` (`ALL: [ActiveTab; 11]`), `history_module` → `Some("Mouse & keyboard")`, `owns_action_bar`, the dispatch; fields `mk_click_test: ClickTest` and `mk_key_test: String`;
  - `src/ui/shell.rs`: the nav item after Environment variables; `open_page`: when not busy reload the mk store, on a page change request an mk scan; the French test;
  - `src/ui/polish.rs`: `search_box_id(MouseKeyboard)` → `Some(egui::Id::new("search_mouse_keyboard"))`;
  - `src/ui/home.rs`: the `tile_title` arm `ActiveTab::MouseKeyboard => tr.t("nav.mouse_keyboard")` (the tile comes in Task 6);
  - `src/ui/test_support.rs`: `widget_right_of(app, ctx, text: &str, size: egui::Vec2) -> Option<egui::Pos2>`: `row_widget`'s search, keyed on any drawn row text (a title) instead of a tweak description;
  - both `.resx`.

**Interfaces — Consumes:** Task 4 (fields, `mk_display`, `mk_original`, `set_mk_wanted`, `mk_changes`, `mk_pending_count`, `discard_mk_changes`, `start_mk_apply`, `mk_blocked_reason`, `request_restore_mouse_keyboard`, `messages::mk_*`); Task 1 (`Control`, `SliderSpec`, `is_on`, `trails_start_length`, `windows_default`); kit (`page_header`, `section`, `SettingRow`, `toggle_enabled`, `segmented`, `pill`, `overflow_menu`, `banner`, `secondary_button`, `primary_button`), `page_action_bar`.

**Interfaces — Produces:**
- `kit::slider(ui: &mut Ui, value: &mut u32, spec: (u32, u32, u32), label: &str, disabled_reason: Option<&str>) -> Response` (`(min, max, step)`): `egui::Slider` on a local copy clamped into the span, `show_value(false)`, `step_by(step)`, width `kit::SLIDER_WIDTH` (160); `label` drawn right of the track; `*value` changes only when the response `changed()`, so an out-of-span value stays until the user moves the knob; disabled with the reason on hover.
- `pub(super) fn show_mouse_keyboard_tab(&mut self, ctx)`.
- `pub(super) fn mk_control_disabled_reason(&self, id: SettingId) -> Option<String>`: first match of: the scan's read error (`mk_problem(Unreadable { id, code })`); `busy_or_blocked(&self.mk_store_error)`; for `ClickLockTime`, `mk.click_lock_off` while `mk_display(ClickLock) == Some(Flag(false))`.
- `pub(super) fn mk_trails_length_reason(&self) -> Option<String>`: `mk_control_disabled_reason(PointerTrails)`, else `mk.trails_off` while trails show off. It disables only the length slider, never the trails toggle.
- `#[derive(Debug, Clone, Copy, Default, PartialEq)] pub(super) struct ClickTest { last_click: Option<f64>, lit: bool }` and `fn click_test_step(t: &mut ClickTest, now: f64, limit_ms: u32)`: a click within `limit_ms` of the previous one lights it and clears `last_click`; any other click sets `lit = false` and `last_click = Some(now)`.

Content (spec 5.6):
- **Header:** `page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |_| {})`, then the secondary line `mk.note`, then the search box `search_mouse_keyboard` (Ctrl+F) filtering rows by translated title, case-insensitively; a section with no visible row is not drawn; `mk.no_match` when none is.
- **States:** `mk.scanning` while `mk_scan` is `None`. When `!account_ok()`, an error `banner` with `mk_problem(account.problem())` replaces the sections.
- **Sections:** `kit::section(mk_section(..))` for each `Section::ALL`, rows in `SettingId::ALL` order. Each row is a `SettingRow` (title `mk_title`, description `mk_description`; trailing right to left: the control, the `…` menu, then the `mk.pill.not_applied` pill (Accent) when the id is in `mk_pending`):
  - `Toggle`: `toggle_enabled` on `is_on(mk_display)`. A click calls `set_mk_wanted` with `Toggle(on)` for precision and scroll inactive, else `Value(Flag(on))`.
  - `Slider`: `kit::slider` on the displayed number, label `mk_value(Some(id), ..)`; a move → `set_mk_wanted(id, Value(Number(n)))`.
  - `TrailsLength`: a toggle (disabled with `mk_control_disabled_reason`), then a length slider (disabled with `mk_trails_length_reason`) showing `mk_display` when on, else `trails_start_length(original or live)`. Toggle off → `Value(Number(0))`; on → `Value(Number(the slider's length))`; slider move → `Value(Number(n))`.
  - `LinesOrScreen`: `kit::segmented` of `mk.wheel.lines` / `mk.value.one_screen`, and the slider only while Lines is shown. One screen → `Value(Number(WHEEL_PAGESCROLL))`; Lines → `Value(Number(n))` with `n` = the live value when in `1..=100`, else 3.
  - Every control is disabled with `mk_control_disabled_reason(id)`.
  - **`…` menu** (`overflow_menu`, salt `mk_{key}`): `mk.windows_default` (disabled with `mk.default_shown` when `mk_display == Some(windows_default)`) → `set_mk_wanted(id, Value(windows_default))`; `tweaks.restore_original` only when `mk_original` exists → `request_restore_mouse_keyboard(vec![key])`. Both disabled with the busy reason while a job runs. Actions are applied after drawing, as on the Startup rows.
  - **Buttons and clicks**, after `double_click_time`, when the search is empty: row `mk.click_test.title` / `mk.click_test.desc` whose control is a `secondary_button` labelled `mk.click_test.ok` when lit, else `mk.click_test.target`; each click runs `click_test_step(.., ctx time, displayed double-click ms)` (not egui's `double_clicked`).
  - **Keyboard**, first, when `filter_keys_on`: an Info `banner` `mk.filter_keys`. Last, when the search is empty: row `mk.key_test.title` / `mk.key_test.note` with a `TextEdit` on `mk_key_test`, hint `mk.key_test.hint`.
- **Action bar:** status `mk.pending` (count) or `mk.up_to_date`; Discard (`tweaks.discard`) when `mk_pending` is non-empty; Apply (`tweaks.apply`) → `start_mk_apply`, disabled with the first of `mk_blocked_reason`, the account problem text, the busy reason, `mk.nothing_to_apply`.

**Keys added in this task (EN / FR):**

| Key | English | French |
|---|---|---|
| `nav.mouse_keyboard` | Mouse & keyboard | Souris et clavier |
| `mk.subtitle` | Pointer, buttons, wheel and keyboard settings, each with an undo. | Pointeur, boutons, molette et clavier, chaque réglage avec une annulation. |
| `mk.note` | Changes apply to every program when you press Apply, and are kept when you sign out. Some programs read the wheel setting only when they start. | Les changements s'appliquent à tous les programmes quand vous cliquez sur Appliquer, et sont conservés quand vous fermez la session. Certains programmes ne lisent le réglage de la molette qu'à leur démarrage. |
| `mk.search` | Search settings | Rechercher un réglage |
| `mk.scanning` | Reading your settings… | Lecture de vos réglages… |
| `mk.no_match` | No setting matches your search. | Aucun réglage ne correspond à votre recherche. |
| `mk.pill.not_applied` | Not applied | Non appliqué |
| `mk.windows_default` | Windows default | Valeur par défaut de Windows |
| `mk.default_shown` | This is already the Windows default. | C'est déjà la valeur par défaut de Windows. |
| `mk.click_lock_off` | Turn on ClickLock first. | Activez d'abord le verrouillage du clic. |
| `mk.trails_off` | Turn on pointer trails first. | Activez d'abord les traînées du pointeur. |
| `mk.wheel.lines` | Lines | Lignes |
| `mk.click_test.title` | Double-click test | Test du double-clic |
| `mk.click_test.desc` | Uses the speed on the slider, applied or not. | Utilise la vitesse du curseur, appliquée ou non. |
| `mk.click_test.target` | Double-click here to test | Double-cliquez ici pour tester |
| `mk.click_test.ok` | Double-click detected | Double-clic détecté |
| `mk.key_test.title` | Key repeat test | Test de la répétition |
| `mk.key_test.note` | Uses the applied settings | Utilise les réglages appliqués |
| `mk.key_test.hint` | Type here to test key repeat | Tapez ici pour tester la répétition |
| `mk.filter_keys` | Filter Keys is on, so Windows may use its own repeat settings. Turn it off in Settings > Accessibility > Keyboard. | Les touches filtres sont activées : Windows peut utiliser ses propres réglages de répétition. Désactivez-les dans Paramètres > Accessibilité > Clavier. |
| `mk.pending` | {0} changes not applied | {0} changement(s) non appliqué(s) |
| `mk.up_to_date` | All changes applied | Tous les changements sont appliqués |
| `mk.nothing_to_apply` | Nothing to apply | Rien à appliquer |

- [ ] **Step 1: Write the failing tests.**
  - `kit/tests.rs`: `slider_keeps_an_out_of_span_value_until_moved`: value 1200 on `(200, 900, 10)`; a frame with no input leaves 1200; a click at the track's right end sets 900; a disabled slider ignores the click.
  - `mouse_keyboard_tab.rs` (seed through `TempIcons`, finish the scan, set `active_tab`):
    - `click_test_lights_within_the_slider_time`: limit 500: clicks at 0.0 and 0.3 → lit; at 0.0 and 0.6 → not lit; a third click after a lit pair → not lit.
    - `the_page_draws_five_sections_in_table_order_with_live_values`: the five section titles are drawn with increasing `y`; `"10"`, `"500 ms"` and `"1200 ms"` are drawn.
    - `a_toggle_marks_not_applied_and_apply_sets_it`: click the toggle right of "Hide the pointer while typing" (`widget_right_of(.., vec2(40.0, 20.0))`) → `mk_pending == {PointerVanish: Value(Flag(false))}`, "Not applied" and "1 changes not applied" drawn; click Apply, `run_until_idle` → fake vanish `Flag(false)`.
    - `an_out_of_range_live_value_is_shown_and_never_written_unless_moved`: `fake.put(DoubleClickTime, Number(1200))`, rescan → "1200 ms" drawn and `mk_pending_count() == 0`; open the row's `…`, click "Windows default" → `mk_pending[DoubleClickTime] == Value(Number(500))`; reopen it → the item is disabled with `mk.default_shown`.
    - `one_screen_at_a_time_sets_the_page_scroll_value`: click `mk.value.one_screen` → `Value(Number(WHEEL_PAGESCROLL))`; click `mk.wheel.lines` → no entry (live 3).
    - `clicklock_delay_and_trails_length_follow_the_shown_toggle`: `mk_control_disabled_reason(ClickLockTime) == Some(tr.t("mk.click_lock_off"))`; pending ClickLock `Flag(true)` → `None`; trails off → `mk_trails_length_reason() == Some(tr.t("mk.trails_off"))` and `mk_control_disabled_reason(PointerTrails) == None`; click the trails toggle → `mk_pending[PointerTrails] == Value(Number(7))` and `mk_trails_length_reason() == None`.
    - `a_setting_windows_cannot_read_is_disabled_with_the_error`: `fail_get(HoverTime)`, rescan → `mk_control_disabled_reason(HoverTime) == Some(mk_problem(Unreadable { id: HoverTime, code: 5 }))`.
    - `the_double_click_target_uses_the_slider_speed`: two clicks on the target with `RawInput.time` 0.0 and 0.3 → "Double-click detected" drawn; pending double-click 200, clicks 0.3 apart → not drawn.
    - `the_filter_keys_note_shows_only_when_on`.
    - `the_wrong_account_shows_a_banner_instead_of_the_sections`: `set_account(Different { process: "PC\\Admin", session: "PC\\Ana" })`, rescan → the banner text (with both names) drawn, no section title drawn, Apply's disabled hover is that text.
    - `ctrl_f_focuses_the_mouse_keyboard_search` and `search_filters_rows_by_title`: "scroll" shows the three Wheel rows and no "Pointer speed".
    - `the_page_shows_the_loading_line_until_the_first_scan`.
    - `opening_the_page_requests_a_scan`: `open_page(ActiveTab::MouseKeyboard)` from Home → `mk_scan_needed`.
  - `shell.rs`: extend `shell_pages_draw_no_english_only_text_in_french` with the page: pending speed, precision toggle, trails on, wheel one screen and swap (so Discard and every pill show); Filter Keys on; `fail_get(HoverTime)`; the Pointer speed `…` menu open; the double-click target lit; the swap confirmation drawn (`start_mk_apply`); then a search with no match, `mk_scan = None`, and an `MkScan` whose account is `Different` and then `Unknown`. Add the new keys to its `positive` list.
- [ ] **Step 2: Run** `cargo test --locked mouse_keyboard_tab` and `cargo test --locked kit` — Expected: FAIL.
- [ ] **Step 3: Implement.** Fix every non-exhaustive `match` on `ActiveTab` or `Icon`.
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS, including `every_tab_renders_without_panicking` over the 11 tabs and the kit icon tests.
- [ ] **Step 5: Commit** — `feat(mouse-keyboard): Mouse & keyboard page with sliders, tests and the account banner`.

### Task 6: History and Home

**Files:**
- Modify: `src/ui/originals.rs`, `src/ui/history_panel.rs`, `src/ui/home.rs`, `src/ui/shell.rs` (`open_page(History)` also reloads the mk store; the French test), both `.resx`.

**Interfaces — Consumes:** Task 4 (`mk_store`, `mk_store_error`, `mk_slot_name`, `request_restore_mouse_keyboard`, `messages::mk_value`); Task 5 (`ActiveTab::MouseKeyboard`, `Icon::Mouse`).

**Interfaces — Produces:**
- **Saved originals:** `OriginalsModule::MouseKeyboard` after `Environment`, titled `history.module.mouse_keyboard`. `originals_of` gives one row per `mk_store` backup of `env.user_label`: `key` = setting key, `title` = `mk_slot_name(key)`, `description` = `mk.original_desc` with `mk_value(tr, SettingId::from_key(key), &backup.value)` and the timestamp. `RestoreRequest::MouseKeyboard(Vec<String>)` → `request_restore_mouse_keyboard`; "Restore all" passes every key. An unknown key is listed by its raw key and its Restore is enabled (the engine reports `UnknownSetting`). An unreadable store shows `originals.unreadable` in place of the group.
- **History filter:** `HistoryFilter::MouseKeyboard` matches `"Mouse & keyboard"`; chip `history.module.mouse_keyboard` after Environment variables.
- **Home tile:** `ModuleSummary { page: ActiveTab::MouseKeyboard, icon: Icon::Mouse, status: home.mouse_keyboard_status with the count of this user's snapshots, drift: 0 }`, after the Environment tile. The count comes from the store, so it is right before the first scan.

| Key | English | French |
|---|---|---|
| `mk.original_desc` | Original: {0}, saved on {1} | Valeur d'origine : {0}, enregistrée le {1} |
| `home.mouse_keyboard_status` | {0} settings changed by WinTune | {0} réglage(s) modifié(s) par WinTune |

- [ ] **Step 1: Write the failing tests:**
  - `originals.rs`: `a_changed_setting_is_listed_with_its_original_and_restores`: apply speed 14 → the "Mouse & keyboard" group shows "Pointer speed" with `mk.original_desc` containing `10`; click its Restore, confirm, `run_until_idle` → fake speed 10, the group is gone.
  - `originals.rs`: `an_unknown_setting_is_listed_by_its_raw_key`: a store file with `caret_blink` → a row titled `caret_blink`.
  - `originals.rs`: `an_unreadable_mouse_keyboard_store_shows_a_banner`.
  - `history_panel.rs`: `the_mouse_keyboard_filter_shows_only_its_operations`.
  - `home.rs`: `the_mouse_keyboard_tile_counts_saved_originals`: a store with two snapshots and `mk_scan = None` → "2 settings changed by WinTune"; clicking the tile opens `ActiveTab::MouseKeyboard`.
  - `shell.rs`: in the French test, History shows a Mouse & keyboard original and operation, and Home shows the tile.
- [ ] **Step 2: Run** `cargo test --locked ui::` — Expected: the new tests FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(mouse-keyboard): Saved originals, History filter and Home tile`.

### Task 7: Docs, drift test and the owner's checklist

**Files:**
- Modify: `docs/registry-reference.md`, `README.md`, `ARCHITECTURE.md`, `src/mouse_keyboard/mod.rs` (the test).

- [ ] **Step 1: Write the failing drift test** `the_registry_reference_names_every_mouse_keyboard_setting` in `src/mouse_keyboard/mod.rs`. With `include_str!("../../docs/registry-reference.md")`, assert the doc contains:
  - `` format!("`{}`", id.key()) `` for every `SettingId::ALL`;
  - `` format!("`HKCU\\{key}`") `` for every `CONTROL_PANEL_KEYS` entry;
  - `format!("0x{:04X}", d.set_action)` for every descriptor (the table and the code cannot drift apart);
  - `` `mouse-keyboard-backups.json` ``, `` `SystemParametersInfoW` ``, `` `SPIF_UPDATEINIFILE` `` and the heading text `Pre-merge checklist (Mouse & keyboard)`.

  Run `cargo test --locked the_registry_reference_names_every_mouse_keyboard_setting` — Expected: FAIL.
- [ ] **Step 2: `docs/registry-reference.md`.** A `## Mouse & keyboard` section before `## Sources`:
  - **What WinTune reads:** the GET calls (SPI GET actions, `GetSystemMetrics(SM_SWAPBUTTON)`, `GetDoubleClickTime`, `SPI_GETFILTERKEYS`), the account check (`GetUserNameExW`, `WTSQuerySessionInformationW`), and `HKCU\Control Panel\Mouse`, `HKCU\Control Panel\Desktop`, `HKCU\Control Panel\Keyboard`, read only, for the `.reg`.
  - **What WinTune writes:** nothing directly. The SET calls of the table, with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`, each followed by its GET; never `UserPreferencesMask`, never `SPI_SETTHREADLOCALINPUTSETTINGS`.
  - **The settings:** spec 5.1's table (id, section, GET, SET action with uppercase `0x` hex and family, page control, Windows default with its tag).
  - **The `.reg` net:** what it holds, that WinTune never imports it, and that importing it by hand also puts back `UserPreferencesMask` (reverting visual effects changed since) and takes effect only at sign-in.
  - **Evidence:** a table (claim, DOCUMENTED / OBSERVED / FOLKLORE, Learn page) from `recon-mouse-keyboard.md` and spec section 3.
  - **`#### Pre-merge checklist (Mouse & keyboard)`:** spec section 7 items 1-9 as checkboxes with a "Result:" blank each. The owner records each result there before merging.
- [ ] **Step 3: README and ARCHITECTURE.**
  - **README:** the feature bullet; a `## Mouse & keyboard` section (the five sections, live apply, readback, Windows default and Restore original, the double-click and key repeat tests, the account banner, why the `.reg` is not for importing); Saved originals "Mouse & keyboard" in Safety and undo; `mouse-keyboard-backups.json` in Data files; `src/mouse_keyboard/` in Code layout; the Roadmap, if it lists this module.
  - **ARCHITECTURE:** the store table row (`mouse-keyboard-backups.json`, per user then setting id, `{ timestamp, value }` read through the GET; no registry bytes); a `## Mouse & keyboard engine` section (catalog, the injected `InputApi` and its fake, the descriptors, the apply order with readback, the account check in the engine, the `.reg` net); the UI paragraph (modules, Home tiles).
- [ ] **Step 4: Run** `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `rtk bypass cargo test --locked` and `cargo build --release --locked` — Expected: all clean, the drift test passing.
- [ ] **Step 5: Commit** — `docs(mouse-keyboard): README, ARCHITECTURE, registry reference and pre-merge checklist`.

## Rulings the spec left open

Each ruling gives the decision, then what it costs if it is wrong.

1. **Two more windows-sys features than spec 6 lists.** `GetDoubleClickTime` lives in `Win32_UI_Input_KeyboardAndMouse`, and the process account comes from `GetUserNameExW` (`Win32_Security_Authentication_Identity`), read from the token, not from `%USERNAME%`. If wrong: two feature lines; no crate.
2. **The account check that fails** gets its own banner text (`mk.problem.account_unknown`); spec 5.6 words only the "differ" case. If wrong: wording.
3. **Precision and scroll-inactive toggles stay `Wanted::Toggle` until Apply**, resolved from the GET made then, so precision off really is a read-modify-write of the thresholds of that moment. Every other choice is an exact value. If wrong: none in safety.
4. **The trails "stepper" is a kit slider 2..7**, and wheel lines is a segmented Lines / One screen choice plus a slider. One new kit control (`slider`), no stepper. If wrong: cosmetic.
5. **Issues are enums end to end:** `MkReport.issues` matches `report.errors` one to one. The toast shows them translated; History keeps the English lines. `MkProblem` gains `NotSaved` and `NoOriginal`, and `MkFailure` gains `Unverified` and `StoreNotSaved`, so every error line has an enum. If wrong: a few unused translations.
6. **`report.succeeded` holds setting keys** (`pointer_speed`), which History shows and the restore uses to drop pending choices. If wrong: History rows read as ids.
7. **Every SET is sent, even one equal to the live value.** If wrong: one needless `WM_SETTINGCHANGE`.
8. **The step of a slider is not validated**, only the span. If wrong: a hand-made value off the step is accepted.
9. **The trails length starts at the original when 2..7, at 7 when it is 8 or more**, so the start never fails validation. If wrong: cosmetic.
10. **A failed Filter Keys read counts as off** (the note is informational). If wrong: a missing note.
11. **A scan drops the pending choice of a setting it could not read**, as Startup drops locked items. If wrong: the user picks again.
12. **A restore warns about the swap whenever `swap_buttons` is among its ids**, whatever the current value. If wrong: one needless sentence.
13. **An unknown key in History has an enabled Restore** that fails with `UnknownSetting`, keeping the row. If wrong: same outcome as a disabled button, one click later.
14. **The real-GET test is not ignored by default.** It reads only. If CI's session makes it flaky, `#[ignore]` with a comment (spec 5.2).
15. **The double-click and key repeat test rows hide while the search is not empty.** If wrong: cosmetic.

## Not in this PR

- Caret blink rate, focus follows mouse, `SmoothMouse*Curve`, cursor schemes, FilterKeys and MouseKeys writes, the double-click and hover rectangles, touchpad settings (spec 2, non-goals).
- A `WM_SETTINGCHANGE` listener (spec 5.6 call), a Home drift count (spec 5.7 call), a timed swap auto-revert (spec 5.4 call).
