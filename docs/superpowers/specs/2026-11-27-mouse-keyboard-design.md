# Mouse & keyboard module: design

Date: 2026-11-27. Status: design approved by delegation. The owner asked the
builder to choose the module and make the calls, and to ask only when the
owner is really needed. Every call made that way is marked **(call)**, with
what it costs if it is wrong. Evidence: `recon-mouse-keyboard.md`, with each
claim tagged DOCUMENTED / OBSERVED / FOLKLORE, summarised in section 3.
French name: "Souris et clavier".

## 1. Intent

WinTune is a toolbox of per-user Windows customisations, each one
snapshotted and undoable from History. The next module gathers the mouse and
keyboard settings Windows spreads over Settings > Bluetooth & devices > Mouse,
the old Mouse Properties dialog and the Keyboard control panel. None of them
has an undo, and some values (hover time, menu delay) have no UI at all in
Windows 11. **(call)** Chosen next because every setting in it has a
documented API to read it and to set it, so WinTune never guesses a registry
format.

It is the first module that does not write the registry itself: Windows
does, through `SystemParametersInfoW`. The safety model stays (pending until
Apply, snapshot before the first write, a `.reg` backup, undo from a row and
from History); only what a snapshot holds changes.

## 2. Goals, non-goals, anti-goals

Goals:
- Show and change the 18 settings of 5.1, in sections; apply each one live
  through its documented API, so running programs see it at once and it is
  still there after the next sign-in; check each write by reading it back.
- "Windows default" and "Restore original" on each row; restore from
  History; a Home tile; a double-click test target and a key-repeat field.

Non-goals (maybe later):
- Caret blink rate: `SetCaretBlinkTime` does not persist and is meant for
  Control Panel-type apps (DOCUMENTED); persisting means writing
  `CursorBlinkRate` by hand (FOLKLORE). **(call)** Cost if wrong: a wanted
  setting is missing.
- Focus follows mouse (three coupled values, can strand a user),
  `SmoothMouse*Curve`, cursor schemes, FilterKeys and MouseKeys writes,
  double-click and hover rectangles, touchpad settings.

Anti-goals:
- Writing any value under `HKCU\Control Panel` directly, above all
  `UserPreferencesMask` (a bitfield shared with unrelated visual effects).
- Restoring a setting by replaying raw registry bytes.
- Reporting a write as done when Windows kept another value.
- A test that calls a real SET.

## 3. What Windows does

- **One API.** `SystemParametersInfoW` GET and SET actions read and set each
  setting (DOCUMENTED). The swapped buttons are read with
  `GetSystemMetrics(SM_SWAPBUTTON)` and the double-click time with
  `GetDoubleClickTime()`, as there is no SPI GET for them (DOCUMENTED).
- **Persistence.** `SPIF_UPDATEINIFILE` writes the setting "to the user
  profile", `SPIF_SENDCHANGE` then broadcasts `WM_SETTINGCHANGE` (DOCUMENTED).
  Learn never says the profile is HKCU nor names values. The
  `HKCU\Control Panel\Mouse`, `\Desktop` and `\Keyboard` values in the recon
  are OBSERVED, and agree with the live GETs wherever a named value exists.
- **Two parameter families.** Some SETs take the value in `uiParam`, others
  in `pvParam` cast to a pointer with `uiParam` 0 (5.1 says which). The wrong
  one silently sets 0 or garbage; Learn's wording is vague for several
  (DOCUMENTED shape, FOLKLORE for "cast to a pointer").
- **Precision** is one array `{threshold1, threshold2, acceleration}`;
  acceleration 0 is off and the thresholds are then ignored, 1 doubles the
  speed above threshold 1, 2 doubles it again above threshold 2 (DOCUMENTED,
  `mouse_event` remarks). Windows' toggle writes `{6,10,1}` / `{0,0,0}`
  (FOLKLORE).
- **Shared bitfield.** ClickLock, Sonar and Vanish have no value of their
  own; they live in `Desktop\UserPreferencesMask` (FOLKLORE for the bits).
- **Scroll inactive windows** (`SPI_[GS]ETMOUSEWHEELROUTING`): 0 focus, 1
  hybrid, 2 mouse position, the default "starting with Windows 10"
  (DOCUMENTED on the `SystemParametersInfoA` page; the W page lists only 0
  and 1). Settings' toggle is 2 / 0 (FOLKLORE).
- **Clamping.** Double-click above 5,000 ms, keyboard speed above 31 and
  hover time outside the timer limits are clamped; keyboard delay and speed
  "may vary by hardware" (DOCUMENTED).
- **Side notes.** The SET is synchronous and its broadcast can wait on slow
  windows (FOLKLORE for how long). Programs that cache the wheel setting see
  it after a restart (DOCUMENTED that they must re-query). FilterKeys, when
  on, overrides key repeat in practice (FOLKLORE); it is off here. A policy
  could reset a value at sign-in (FOLKLORE).
- **Seen here (OBSERVED, live = registry):** speed 10, precision
  `{6,10,1}`, swap off, double-click 500, wheel 3 lines / 3 chars, routing 2,
  delay 1, rate 31, snap off, trails 0, Sonar off, Vanish **on**, ClickLock
  off with time 1200, hover 400, menu delay 400, drag full windows on.

## 4. Approaches considered

- **A. The documented API, live (chosen).** SET each setting with
  `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE` on the job worker thread. Snapshot =
  the value read through its GET before the first change, in its own store;
  a read-only `.reg` of the three keys as an extra net. Restore = SET that
  value through the same API. Each SET is followed by a GET; a difference is
  a failure. Pros: documented end to end, takes effect at once, persists the
  way Windows' dialogs do, never touches other bits of the shared bitfield.
  Cons: no bytes to replay, so History needs a store of a new shape (5.3).
- **B. Write the registry values.** Takes effect only at the next sign-in;
  names and formats undocumented; three settings would mean editing
  `UserPreferencesMask` bits. Rejected.
- **C. Registry snapshot, SPI apply.** Restoring raw `UserPreferencesMask`
  bytes clobbers every unrelated bit changed since, and a restored registry
  value does not reach the running session. Rejected.

## 5. Design

### 5.1 Settings

One row per setting; the id is the join key between row, pending choice,
store and History. "ui" / "pv" is where the SET takes the value.

| Id | Section | Read | Set | Page control | Windows default |
| --- | --- | --- | --- | --- | --- |
| `pointer_speed` | Pointer | SPI 0x0070 | 0x0071 pv | slider 1..20 | 10 (DOC) |
| `pointer_precision` | Pointer | SPI 0x0003 array | 0x0004 array | toggle | on (FOLK) |
| `pointer_trails` | Pointer | SPI 0x005E | 0x005D ui | toggle + length 2..7 | off (FOLK) |
| `pointer_sonar` | Pointer | SPI 0x101C | 0x101D pv | toggle (show location with Ctrl) | off (FOLK) |
| `pointer_vanish` | Pointer | SPI 0x1020 | 0x1021 pv | toggle (hide while typing) | on (FOLK) |
| `snap_to_default` | Pointer | SPI 0x005F | 0x0060 ui | toggle | off (FOLK) |
| `swap_buttons` | Buttons and clicks | `SM_SWAPBUTTON` | 0x0021 ui | toggle | off (FOLK) |
| `double_click_time` | Buttons and clicks | `GetDoubleClickTime` | 0x0020 ui | slider 200..900 ms, step 10 | 500 (DOC) |
| `click_lock` | Buttons and clicks | SPI 0x101E | 0x101F pv | toggle | off (FOLK) |
| `click_lock_time` | Buttons and clicks | SPI 0x2008 | 0x2009 pv | slider 200..2,200 ms, step 100 | 1,200 (DOC) |
| `wheel_lines` | Wheel | SPI 0x0068 | 0x0069 ui | 1..100 lines, or one screen | 3 (DOC) |
| `wheel_chars` | Wheel | SPI 0x006C | 0x006D ui | slider 1..100 | 3 (DOC) |
| `scroll_inactive` | Wheel | SPI 0x201C | 0x201D pv | toggle | on = 2 (DOC, A page) |
| `key_repeat_delay` | Keyboard | SPI 0x0016 | 0x0017 ui | slider 0..3, Short..Long | 1 (FOLK) |
| `key_repeat_rate` | Keyboard | SPI 0x000A | 0x000B ui | slider 0..31, Slow..Fast | 31 (FOLK) |
| `hover_time` | Windows and menus | SPI 0x0066 | 0x0067 ui | slider 100..3,000 ms, step 50 | 400 (FOLK) |
| `menu_show_delay` | Windows and menus | SPI 0x006A | 0x006B ui | slider 0..2,000 ms, step 50 | 400 (FOLK) |
| `drag_full_windows` | Windows and menus | SPI 0x0026 | 0x0025 ui | toggle | on (FOLK) |

- **Ranges (call).** Apply refuses anything outside the page ranges above.
  Speed, delay and rate are DOCUMENTED; the rest are chosen: double-click
  200..900 is the old dialog's span (FOLKLORE; the API takes up to 5,000),
  wheel 1..100 leaves out 0 ("no scrolling", a lock-out risk), trails 2..7 is
  the old dialog's span, the time spans are round spans around the defaults.
  A live value outside a span is shown as it is until the user moves the
  control. Restore original and Windows default are not checked against the
  page range. Cost if wrong: such a value (a 1,200 ms double-click) can be
  kept or restored but not chosen.
- **Defaults (call).** "Windows default" is on every row. A FOLKLORE default
  is the value commonly reported for a new Windows 11 profile (Vanish on: the
  recon's table says off without a source, and this machine has it on). Cost
  if wrong: harmless and undoable; optional checklist item 9 checks them.
- **Precision (call).** A toggle, on when acceleration is not 0. Off writes
  `{T1, T2, 0}` with the current thresholds (read-modify-write of the whole
  array). On writes the snapshot's original array when its acceleration is
  not 0 (a custom `{T1, T2, 2}` comes back), else `{T1, T2, 1}` when both
  current thresholds are not 0, else `{6, 10, 1}`. Cost if wrong: if Settings
  derives its toggle from the thresholds, it would still show "on"; checklist
  item 1 catches it and the fallback is `{0, 0, 0}` like Windows.
- **Scroll inactive windows.** Off writes 0. On writes the snapshot's
  original when not 0, else 2. A live 1 shows as on; WinTune writes 1 only to
  restore an original 1.
- **ClickLock time** is disabled while ClickLock is off (pending or live): it
  has no effect then (DOCUMENTED).
- **Trails.** Off writes 0; a live 1 is off too (DOCUMENTED). On writes the
  stepper's length, which starts at the original length when it is 2 or
  more, else 7. **(call)** Cost if wrong: cosmetic.
- **Wheel lines.** "One screen at a time" is `WHEEL_PAGESCROLL` (`u32::MAX`,
  DOCUMENTED). A live 0 shows as "0 (no scrolling)".

### 5.2 Backend: an injectable trait

```text
trait InputApi: Send + Sync {
    fn get(&self, id: SettingId) -> Result<SettingValue, ApiError>;
    fn set(&self, id: SettingId, value: &SettingValue) -> Result<(), ApiError>;
    fn filter_keys_on(&self) -> Result<bool, ApiError>;
}
```

`SettingValue` is `Number(u32)`, `Flag(bool)` or `Mouse([i32; 3])`; each id
has one shape. The real backend (`SpiInput`, windows-sys) holds one static
descriptor per setting (GET call and output type; SET action and family: ui,
pv value or pv array) and always passes `SPIF_UPDATEINIFILE |
SPIF_SENDCHANGE`; a `FALSE` return is an error with `GetLastError`. The fake
(`FakeInput`) is a map behind a mutex with a call log, and switches to fail a
GET or a SET or to keep another value than the one set. `AppEnv` carries an
`Arc<dyn InputApi>`: `SpiInput` only in `AppEnv::system`, the fake in the
test fixture. **(call)** One read-only test runs every real GET once and
checks shape and range, to catch a wrong descriptor; it never calls `set`,
and is marked `#[ignore]` if CI's session makes it flaky. Cost if wrong: a
bad GET descriptor reaches the owner's checklist.

### 5.3 Store, and how it fits History **(call)**

`mouse-keyboard-backups.json`, loaded like the others; an unreadable store
blocks Apply and Restore. It is a **parallel type with the same shape** as
`backups::UserBackups`: `users` → user → setting id → `{ timestamp, value }`
(`value` tagged `kind` / `data` like `Snapshot`), with the same
`backups_of`, `has_backup`, `record_first` and `forget`.

Why not `backups.rs` itself: its `Snapshot` is registry-shaped and feeds
`save_then_export` (the `.reg`) and `restore_snapshot` (regsnap). A
"semantic value" variant would give every registry engine an arm to reject;
making `UserBackups` generic would change four modules' types and put the
byte-identical store test at risk, for about 40 lines saved. So the module
has its own small restore loop with `restore_ids`'s rules. History and Home
need only what both shapes give: per user, ids with a timestamp, a name per
id, and a restore job taking ids. Cost if wrong: about 60 lines that look
like `backups.rs`; folding them later is mechanical. An id a later version
no longer knows is listed with its raw id and its restore fails, like an
unknown tweak.

### 5.4 Apply

On the job worker thread:
1. **Validate** every pending value (known id, right shape, in range). Any
   problem refuses the whole Apply with nothing written.
2. **Read** each touched setting through its GET, now. A failed GET refuses
   the whole Apply (it cannot be snapshotted).
3. **Snapshot on first touch** (`record_first`), save the store, then write
   the `.reg`. If either fails, nothing is applied.
4. **Write** in table order, `swap_buttons` last **(call: a failure report
   is not read with buttons swapped halfway; no cost)**. Each SET is
   followed by its GET; a different value fails that setting ("Windows kept
   5,000 ms") and the next is still tried.
5. **Forget** each snapshot whose live value is back at its original (exact
   equality; for precision the whole array), and save the store again.

The report lists every success and failure. Pending choices: cleared after a
clean Apply, kept after a refused or partial one, and each scan drops those
the live value already matches.

**The `.reg` (call).** Each Apply first exports the values directly in
`HKCU\Control Panel\Mouse`, `\Desktop` and `\Keyboard` (not subkeys) as
single values (`RegValueExport`, `export_reg_full` with no keys) to
`reg-backups\<stamp>-mouse-keyboard.reg`. Never a key delete-and-recreate:
`Desktop` also holds wallpaper and `WindowMetrics`. It is a last-resort net:
WinTune never imports it; importing it by hand also puts back
`UserPreferencesMask` (reverting visual effects changed since) and takes
effect only at sign-in. The reference doc says so. Cost if wrong: a user who
imports it loses unrelated changes, having been warned.

**Confirmation (call).** Apply asks only when the batch swaps the buttons
(either way): "Your primary and secondary mouse buttons swap as soon as this
is applied. To undo it, click with the other button." No timed auto-revert.
Cost if wrong: a user who swaps by mistake undoes it with the other button
or the keyboard, as warned.

### 5.5 Restore

From a row or History, always after a confirmation (with the swap warning
when `swap_buttons` is among the ids). Each id: SET the stored value exactly
(precision the whole array, routing the stored number), GET it back, forget
the snapshot only if they match; a failure keeps it. The pending choices of
restored settings are then dropped, so Apply cannot undo the restore. No
`.reg` for a restore, as in the other modules.

### 5.6 Scan and page

**Scan.** A background job like the other scans: at startup, on page open
and after each job, never while a job runs. One GET per setting plus
`filter_keys_on`. A setting whose GET fails is **locked** (control disabled,
error on hover). No `WM_SETTINGCHANGE` listener **(call)**: a change made in
Settings while WinTune is open shows at the next scan. Cost: a stale value
until the page reopens.

**Account check (call).** WinTune runs elevated (manifest). With
over-the-shoulder elevation it runs as another account: a SET would change
this session live but persist in the other account's profile, and the
readback would agree. The scan compares WinTune's account with the session's
user (`WTSQuerySessionInformationW`, `WTSUserName` + `WTSDomainName`, current
session); if they differ or the check fails, a banner replaces the sections:
"WinTune runs as {0}, but {1} is signed in. Mouse and keyboard settings would
be saved for the wrong account." Cost if wrong: a false lock makes the page
read-only for that run.

**Page.** Nav: "Mouse & keyboard" in the Customize group, after Environment
variables. Header note: "Changes apply to every program when you press
Apply, and are kept when you sign out. Some programs read the wheel setting
only when they start." Five kit sections in table order **(call: a fifth,
"Windows and menus", for hover time, menu delay and dragging; no cost)**.
Each row: a `SettingRow` with a toggle, slider plus value label, or stepper;
a "Not applied" pill when pending; the `…` menu with **Windows default**
(sets the pending value; disabled when already shown) and **Restore
original** (when a snapshot exists). Action bar (`owns_action_bar`): "N
changes not applied", Discard, Apply. Search (Ctrl+F) filters by title.

- **Double-click test:** a target "Double-click here to test" that lights
  when two clicks land within the time **on the slider** (pending or live).
  It measures the gap itself; egui's `double_clicked()` uses egui's own
  delay. **(call)** The user feels a speed before applying it. Cost: it does
  not prove Windows took it (the readback and checklist item 3 do).
- **Key repeat test:** a text field "Type here to test key repeat", hint
  "Uses the applied settings" (Windows repeats the keys).
- **FilterKeys note (call):** when on, the Keyboard section says "Filter Keys
  is on, so Windows may use its own repeat settings" with where to turn it
  off. Read only. Cost if wrong: a needless note.

### 5.7 History and Home

- History: module "Mouse & keyboard" (`record_history`, translated by
  `history_module`), a filter chip, a Saved originals group
  (`OriginalsModule::MouseKeyboard`) with one row per snapshot, named by the
  setting's title and described "Original: {0}, saved on {1}" with the value
  formatted as on the page ("10", "On", "One screen at a time", "{6, 10, 1}").
  Item and group restore run the restore job. A slot back at its original
  is forgotten (5.4), so it leaves the group.
- Home tile: "{0} settings changed by WinTune", this user's snapshot count
  (known before the first scan). No drift count **(call; cost: no Home
  warning when a setting is changed elsewhere after WinTune)**.
- English and French for every text. Engine reasons are enums (`MkProblem`:
  out of range, unreadable, unknown setting, wrong account; `MkFailure`: SET
  failed with its code, readback mismatch with wanted and read values)
  translated in `i18n/messages.rs`; report detail lines stay English.

## 6. Constraints

- Rust 1.88, egui/eframe 0.29.1, windows-sys 0.59 (add the
  `Win32_UI_WindowsAndMessaging` and `Win32_System_RemoteDesktop` features if
  missing), winreg 0.52 for the read-only `.reg`. No new crate.
- Never write under `HKCU\Control Panel`; never call
  `SPI_SETTHREADLOCALINPUTSETTINGS`; every SET on the job worker thread.
- Tests use `FakeInput`; the `.reg` export is pointed at
  `HKCU\Software\WinTuneTest\<unique>` and a temp folder.
- Follow CONTRIBUTING.md "For a new module": `src/mouse_keyboard/`
  (catalog, store, engine, backend), `ui/mouse_keyboard_state.rs`,
  `ui/mouse_keyboard_tab.rs`, jobs in `jobs.rs`, `history_panel.rs`,
  `originals.rs`, `home.rs`, the page in
  `shell_pages_draw_no_english_only_text_in_french`. ARCHITECTURE.md gets an
  engine section and a store table row.

## 7. Delivery

One PR (`feat/mouse-keyboard-1`). The plan adds a "Mouse & keyboard" section
to `docs/registry-reference.md`: what WinTune reads (the GET calls; the three
keys, read only, for the `.reg`), what it writes (nothing directly: the SET
calls and their flags), the 5.1 table, an Evidence table (claim, status,
Learn page) from the recon, and this checklist. No test calls a real SET, so
the owner runs it before merge and records each result:

1. Change pointer speed and turn Enhance pointer precision off, Apply:
   Settings > Bluetooth & devices > Mouse shows the same. Turn precision back
   on: Settings shows it on. Result: ______
2. Swap the primary button, Apply (the confirmation warns); the right button
   is primary. Restore it with the other button. Result: ______
3. Change double-click speed, Apply: the test target agrees, and a folder in
   Explorer opens only at that speed. Result: ______
4. Set repeat delay Short and rate Slow, Apply: holding a key repeats the
   same way in the test field and in Notepad. Result: ______
5. Turn "Scroll inactive windows" off, Apply: the wheel no longer scrolls a
   window behind Notepad. Turn it on: it does. Result: ______
6. Turn "Hide pointer while typing" off and Sonar on, Apply: in Notepad the
   pointer stays while typing, and Ctrl shows the circles. Result: ______
7. History, Saved originals, Mouse & keyboard, Restore all: Settings and the
   page show the original values; the group is empty. Result: ______
8. Change two settings, Apply, sign out and in: WinTune and Settings still
   show them. Then restore them. Result: ______
9. (Optional) On a fresh local account, the page's values match the
   "Windows default" column; note any that differ. Result: ______

## 8. Open questions for the owner

None. The FOLKLORE defaults are checked by optional item 9 when a fresh
account is at hand; until then the reference doc labels them FOLKLORE.

## 9. Left to the builder

- Module layout within section 6, enum, key and id names, wording in English
  and French, the nav icon.
- How sliders, steppers and the lines / one-screen choice are drawn (a kit
  slider if none exists, tokens only).
- The exact descriptor table, each SET's family checked against Learn; where
  Learn is vague, the "cast to a pointer" form, which the readback checks.
- Whether a drift test checks that every setting id is in the reference
  doc's table; the order of rows within a section.

Settled after the builder check, as the builder's calls:

- **(call)** The account check (5.6) runs again inside the Apply and Restore
  engines, not only on the page, so a restore started from History or Home
  as the wrong account is refused with the wrong-account problem. It is
  injected through the app environment (like the user label), so tests
  control it. Cost if wrong: none in safety; one more check per job.
- **(call)** "Windows default" for enhance pointer precision writes exactly
  `{6,10,1}`, never a restored custom array. Cost if wrong: a user with a
  custom array loses it on "Windows default", which is what the label says.
- **(call)** The `.reg` backup enumerates the top-level values of the three
  `Control Panel` keys itself (the existing export takes values already
  snapshotted), and its source keys are injected so tests read a sandbox
  under `HKCU\Software\WinTuneTest`.
- **(call)** Filter Keys is read through a local `#[repr(C)]` `FILTERKEYS`
  struct (or the windows-sys `Win32_UI_Accessibility` feature, if already
  enabled); no new crate.
