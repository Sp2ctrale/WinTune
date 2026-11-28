# Visual effects module: design

Date: 2026-11-28. Status: design approved by delegation. The owner asked the
builder to make the calls and to ask only when the owner is really needed.
Every call made that way is marked **(call)**, with what it costs if it is
wrong. Evidence: `recon-visual-effects.md`, each claim tagged DOCUMENTED /
OBSERVED / FOLKLORE, summarised in section 3. French name: "Effets visuels".

## 1. Intent

Windows keeps its visual effects in the old Performance Options dialog
(`SystemPropertiesPerformance.exe`): a list of checkboxes and two presets,
"Adjust for best appearance" and "Adjust for best performance", with no undo
and no record of what was on before. A few more (ClearType contrast, caret
width, focus rectangle) are scattered over the ClearType Text Tuner and
Accessibility settings. The module shows the ones Windows exposes through
`SystemParametersInfoW`, one row per setting, applied live and undoable from
History.

It is built like Mouse & keyboard: Windows writes the values, through
documented SET actions with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`; each SET
is read back; snapshots are semantic values in the module's own store; a
read-only `.reg` is the last-resort net; a wrong-account process writes
nothing.

## 2. Goals, non-goals, anti-goals

Goals:
- Show and change the 18 settings of 5.1 in six sections, live, persisted
  for the next sign-in, each write checked by a readback.
- Two presets that only fill pending choices ("For appearance", "For
  performance"); "Windows default" and "Restore original" on each row;
  restore from History; a Home tile.

Non-goals (not `SystemParametersInfo` settings; Explorer or DWM read them,
often only after a restart, and each needs its own registry evidence):
- Show thumbnails instead of icons (`Explorer\Advanced\IconsOnly`, FOLKLORE).
- Show translucent selection rectangle (`ListviewAlphaSelect`, FOLKLORE).
- Drop shadows for icon labels on the desktop (`ListviewShadow`, FOLKLORE).
- Animations in the taskbar (`TaskbarAnimations`, FOLKLORE).
- Enable Peek and Save taskbar thumbnail previews (`DWM` key, FOLKLORE).
- The dialog's radio choice (`Explorer\VisualEffects\VisualFXSetting`,
  FOLKLORE) and Transparency effects (`Personalize\EnableTransparency`).

Non-goals with an SPI pair, dropped **(call: cost if wrong, a wanted
setting is missing; each can be added as a row later)**:
- Gradient captions (`SPI_[GS]ETGRADIENTCAPTIONS`): Windows 10 and later do
  not support the gradient caption colours (DOCUMENTED, GetSysColor); DWM
  draws title bars. Windows ignores it.
- Hot tracking (`SPI_[GS]ETHOTTRACKING`): not in the dialog, and its visible
  effect under visual styles is unclear (FOLKLORE).
- `SPI_[GS]ETCLEARTYPE`: a second ClearType switch; Learn documents
  smoothing plus smoothing type as the way to turn ClearType on.
- Font smoothing orientation (RGB/BGR): per-panel tuning, the ClearType Text
  Tuner's job. Menu underlines and "disable overlapped content": accessibility,
  not effects.
- "Show window contents while dragging" (`SPI_[GS]ETDRAGFULLWINDOWS`): Mouse &
  keyboard owns it (`drag_full_windows`); this page links there.

Anti-goals:
- Writing any value under `HKCU\Control Panel` directly, above all
  `UserPreferencesMask` (most of these effects are bits in it).
- Restoring a setting by replaying registry bytes.
- Reporting a write as done when Windows kept another value.
- A test that calls a real SET.

## 3. What Windows does

- **One API.** Every kept setting has a documented GET and SET action
  (DOCUMENTED, `SystemParametersInfoW`). Persistence and broadcast are the
  same as in Mouse & keyboard (`SPIF_UPDATEINIFILE`, `SPIF_SENDCHANGE`,
  DOCUMENTED); the values land in `HKCU\Control Panel\Desktop` and
  `Desktop\WindowMetrics\MinAnimate` (OBSERVED).
- **Three SET families.** Most effects take a BOOL in `pvParam` cast to a
  pointer. Font smoothing on/off takes it in `uiParam` (DOCUMENTED, and the
  Learn ClearType sample does so). Minimize/maximize animation takes a
  pointer to `ANIMATIONINFO { cbSize, iMinAnimate }` with `uiParam =
  sizeof` = 8, for the GET too; non-zero `iMinAnimate` means on
  (DOCUMENTED).
- **The master switch.** `SPI_SETUIEFFECTS` FALSE "disable[s] all UI
  effects", TRUE enables them (DOCUMENTED). Learn lists the UI effects:
  combo box, cursor shadow, gradient captions, hot tracking, list box smooth
  scrolling, menu animation, menu underlines, selection fade, tooltip
  animation and fade (DOCUMENTED). Menu fade, drop shadow, window and client
  area animation are in other tables. Whether the master overwrites each
  effect or only gates it, and whether an effect's GET still reports its own
  value while the master is off, is not documented: FOLKLORE says it is a
  separate bit that gates the others, which keep theirs. The design is safe
  either way (5.3); checklist item 4 records which case happens.
- **Pairs.** Menu fade is ignored unless menu animation is on; tooltip fade
  likewise (DOCUMENTED). Smoothing type needs smoothing on; contrast needs
  type ClearType, valid 1,000..2,200, default 1,400 (DOCUMENTED).
- **Defaults.** Caret width default and minimum 1, contrast 1,400
  (DOCUMENTED). Smoothing type: the API default is Standard (DOCUMENTED) but
  Windows ships ClearType (FOLKLORE). The rest: everything on, fades as fade,
  focus border 1 (FOLKLORE).
- **Labels.** Which dialog checkbox maps to which action is FOLKLORE (no Learn
  page maps them), notably "Animate controls and elements inside windows" =
  client area animation and "Show shadows under windows" = drop shadow (Learn
  ties drop shadow to `CS_DROPSHADOW` windows, menus and tooltips). Settings >
  Accessibility > Visual effects > "Animation effects" may flip several rows
  at once (FOLKLORE). The dialog may show "Custom" after a change (FOLKLORE).
- **Side notes.** Fades and shadows need more than 256 colours (DOCUMENTED,
  always true today). Some programs read these only at start (FOLKLORE).
  WinTune draws its own text and animations (egui), so these settings never
  change WinTune itself.
- **Seen here (OBSERVED):** UI effects on; window, client area, combo box,
  list box, menu, menu fade, selection fade, tooltip and tooltip fade on;
  cursor shadow **off**; drop shadow on; smoothing on, type 2 (ClearType),
  contrast **1,200** (the registry's `FontSmoothingGamma` reads 0, so the
  registry does not hold the live number); caret 1, focus border 1 x 1;
  `UserPreferencesMask` `9E1E078012000000`; no `VisualFXSetting`.

## 4. Approaches considered

- **A. Own module, small shared SPI layer (chosen) (call).** New
  `src/visual_effects/` (catalog, backend, engine), with what both modules
  need moved to a new `src/sysparams/`: `SettingValue` and `Shape`,
  `ApiError` and the raw `SystemParametersInfoW` GET/SET executor (families
  ui, pv value, pv array, plus the new `ANIMATIONINFO` one), the account
  check, the snapshot store type and the `.reg` value export (5.6). Each
  module keeps its own id enum, descriptor table, trait, fake, catalog and
  engine. Mouse & keyboard re-exports the moved names so its code and tests
  are unchanged, and its store file stays byte-identical. Cost if wrong: if
  the move spills into Mouse & keyboard's engine, the builder copies the
  ~150 lines instead; folding them later is mechanical.
- **B. One generic SPI engine for both modules.** One trait and engine over a
  descriptor table. Rejected for now: the two catalogs differ (precision
  array, Filter Keys, swap confirmation, gating), and it rewrites a module
  that just shipped.
- **C. Copy Mouse & keyboard whole.** No refactor, but a third copy of the
  account check and store code. Rejected.
- **D. Write `UserPreferencesMask` bits.** Undocumented layout, effect only at
  sign-in, clobbers unrelated bits. Rejected (anti-goal).

## 5. Design

### 5.1 Settings

"ui" / "pv" / "ai" is where the SET takes the value (ai = `ANIMATIONINFO`).
Gate: the row is enabled only while that value is on (5.3).

| Id | Section | Get | Set | Page control | Gate | Windows default |
| --- | --- | --- | --- | --- | --- | --- |
| `ui_effects` | UI effects | 0x103E | 0x103F pv | toggle | | on (FOLK) |
| `window_animation` | Animations | 0x0048 ai | 0x0049 ai | toggle | | on (FOLK) |
| `client_area_animation` | Animations | 0x1042 | 0x1043 pv | toggle | | on (FOLK) |
| `combobox_animation` | Animations | 0x1004 | 0x1005 pv | toggle | master | on (FOLK) |
| `listbox_smooth_scroll` | Animations | 0x1006 | 0x1007 pv | toggle | master | on (FOLK) |
| `menu_animation` | Menus and tooltips | 0x1002 | 0x1003 pv | toggle | master | on (FOLK) |
| `menu_fade` | Menus and tooltips | 0x1012 | 0x1013 pv | Fade / Slide | `menu_animation` | Fade (FOLK) |
| `selection_fade` | Menus and tooltips | 0x1014 | 0x1015 pv | toggle | master | on (FOLK) |
| `tooltip_animation` | Menus and tooltips | 0x1016 | 0x1017 pv | toggle | master | on (FOLK) |
| `tooltip_fade` | Menus and tooltips | 0x1018 | 0x1019 pv | Fade / Slide | `tooltip_animation` | Fade (FOLK) |
| `cursor_shadow` | Shadows | 0x101A | 0x101B pv | toggle | master | on (FOLK) |
| `drop_shadow` | Shadows | 0x1024 | 0x1025 pv | toggle | | on (FOLK) |
| `font_smoothing` | Text | 0x004A | 0x004B ui | toggle | | on (FOLK) |
| `font_smoothing_type` | Text | 0x200A | 0x200B pv | Standard / ClearType | `font_smoothing` | ClearType = 2 (FOLK) |
| `cleartype_contrast` | Text | 0x200C | 0x200D pv | slider 1,000..2,200, step 100 | type ClearType | 1,400 (DOC) |
| `caret_width` | Caret and focus | 0x2006 | 0x2007 pv | slider 1..20 px | | 1 (DOC) |
| `focus_border_width` | Caret and focus | 0x200E | 0x200F pv | slider 1..10 px | | 1 (FOLK) |
| `focus_border_height` | Caret and focus | 0x2010 | 0x2011 pv | slider 1..10 px | | 1 (FOLK) |

Every GET is `SystemParametersInfoW(action, 0, &mut u32, 0)` except
`window_animation` (`ANIMATIONINFO`, `uiParam` 8). The fade rows are flags
(TRUE = Fade). `font_smoothing_type` is a number.

- **Gates (call).** Master-gated = the kept members of Learn's UI effects
  list, nothing more. Drop shadow, window and client area animation are not
  gated, though FOLKLORE says some are bits under the master too. Cost if
  wrong: a row stays enabled while the master makes it do nothing;
  checklist item 4 shows it.
- **Ranges (call).** Apply refuses anything outside the page ranges (flags
  always pass; type is 1 or 2). Contrast is DOCUMENTED; caret 1..20 is the
  Settings text-cursor span (FOLKLORE); focus border 1..10 is chosen; contrast
  step 100 keeps the default and this machine's 1,200 on a step. A live value
  outside a range or off a step is shown as it is ("Unknown (3)" for a type)
  until the control is moved; Restore original and Windows default are not
  checked against the range. Cost if wrong: such a value can be kept or
  restored but not chosen.
- **Window animation.** Off writes `iMinAnimate` 0, on writes 1; any non-zero
  live value reads as on and is snapshotted as on **(call: Learn gives only
  zero / non-zero, no cost)**.
- **Defaults (call).** "Windows default" is on every row. Smoothing type uses
  ClearType, what Windows ships, not the API's Standard. Cost if wrong:
  harmless and undoable; the reference doc keeps the FOLKLORE tags.

### 5.2 Backend: an injectable trait

```text
trait EffectsApi: Send + Sync {
    fn get(&self, id: EffectId) -> Result<SettingValue, ApiError>;
    fn set(&self, id: EffectId, value: &SettingValue) -> Result<(), ApiError>;
}
```

`SettingValue` is the shared `Number(u32)` / `Flag(bool)` (no array here).
The real backend (`SpiEffects`) holds one static descriptor per id (GET call,
SET action and family) and calls the shared executor, always with
`SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`; a `FALSE` return is an error with
`GetLastError`. The fake (`FakeEffects`) is a map behind a mutex, seeded
with the OBSERVED values, with a call log and switches to fail a GET or a
SET or keep another value. `AppEnv` carries an `Arc<dyn EffectsApi>`:
`SpiEffects` only in `AppEnv::system`, the fake in the test fixture. One
read-only test runs every real GET once and checks shape and range (type 1
or 2, contrast 1,000..2,200, caret and borders at least 1); it never calls
`set`, and is `#[ignore]` if CI's session makes it flaky.

### 5.3 Gating, and the master's companions

- A gated row is **disabled** while its gate is off, where the gate's state
  is its pending value, else its live value (Mouse & keyboard's ClickLock
  rule). Turning the master on in the same batch enables its rows at once.
  Contrast is enabled only while smoothing is on and type is ClearType.
- Turning a gate off drops the pending choices of the rows it disables, so a
  disabled row never shows "Not applied" **(call; cost: a choice made
  before turning the gate off is lost, one click to redo)**.
- Gating is a page rule. The engine writes what it is given: a restore from
  History sets a gated id even when its gate is off.
- A disabled gated row says why on hover: "Has no effect while UI effects
  is off" / "while Menu animation is off" / "while font smoothing is off".

**The master's companions (call, builder check).** Turning `ui_effects` off
may clear the gated effects, or make their GETs read off (FOLKLORE, 3). The
design is safe in both cases, so the PR does not depend on checklist item 4.
Cost if wrong (the master only gates): a few extra snapshots and SETs that
change nothing.
- **Snapshots.** Any Apply or Restore that touches `ui_effects` first reads
  every master-gated row (5.1) through its GET and records it with
  `record_first`, before any write, as a **companion** of the master. A
  failed GET refuses the whole job, as for a touched row. A row that already
  has its own snapshot keeps it.
- **History and Home.** Companions are not History rows of their own and are
  not counted on Home; the master's row says "also restores {0} effects".
  A gated row the user changes directly keeps its first-touch original but
  becomes its own History row (its companion mark is cleared).
- **Restore.** Restoring the master restores its companions with it: when
  the restored master is on, the master first, then the gated rows; when it
  is off, the gated rows first, the master last.
- **Forget.** A companion is forgotten on its own once its value is back at
  its original. Companions are forgotten with the master when the master is
  back at its original, after a GET of each **(call: a companion whose live
  value still differs is kept, as its own History row, instead of being
  dropped; cost: one more History row in case 1, where a master turned back on
  in Apply did not bring the effects back)**.
- **Readback while the master is off.** When a gated row's SET is read back
  while `ui_effects` is live off and the value differs, the issue is
  `VeFailure::HeldByMaster` ("Windows keeps this effect off while UI effects
  is off"), not a plain mismatch, and the row's snapshot is kept. A gated row
  written in an Apply that turns the master on is written and read back after
  the master's SET (5.4 step 4), but its snapshot GET still comes before any
  write. When that differing readback's GET of the master fails, WinTune
  cannot tell the two apart: the issue is `VeFailure::Unverified` and the
  snapshot is kept.
- **Re-read after the master goes on (call, review T3-C1).** In case 2 a
  companion's GET reads off while the master is off, whatever its stored
  value, so a snapshot taken then can be wrong. When an Apply turns the
  master from live off to on, or a Restore sets the master on before its
  companions, every companion and gated row snapshotted in that job is read
  again once the master's SET and readback succeed; that value replaces the
  snapshot, and the store is saved again before any gated row is written.
  This is the one exception to "every snapshot GET before any write". If
  that save fails, no gated row is written and the report says so. If the
  master does not end on, or a re-read fails, the snapshots it concerns are
  dropped and their rows are not written (they were never changed). Cost if
  wrong (a master SET on that does change a companion): its snapshot is the
  value after that SET, not before.
- **A gated row touched while the master goes off (review T3-C2).** In an
  Apply that turns the master off, a touched master-gated row keeps its
  snapshot even when its readback matches its original: the master's SET,
  written last, may clear it (case 1).
- The store entry gets an optional `companion_of` field (the master's id),
  left out of the JSON when empty, so Mouse & keyboard's file stays
  byte-identical.

### 5.4 Apply

On the job worker thread, as Mouse & keyboard:
1. **Account check**, then **validate** every pending value (known id,
   right shape, in range). Any problem refuses the whole Apply with nothing
   written.
2. **Read** each touched setting through its GET, and every master-gated row
   when `ui_effects` is touched (5.3); a failure refuses all.
3. **Snapshot on first touch** (`record_first`, companions included), save
   the store, then write the `.reg` (5.6). If either fails, nothing is
   applied.
4. **Write** in table order, except `ui_effects`: first when it goes on,
   last when it goes off **(call: each effect is written while the master is
   on, in case a SET under a disabled master misbehaves; no cost)**. Table
   order already puts each gate before its rows. Each SET is followed by its
   GET; a different value fails that setting (`HeldByMaster` while the
   master is live off, 5.3) and the next is still tried. When the master
   goes from live off to on, the companions and gated rows snapshotted in
   this Apply are read again right after its SET, and the store saved,
   before any gated row is written (5.3, the one exception to step 2).
5. **Forget** each snapshot whose live value is back at its original, with
   the companion rules of 5.3 (an Unverified or held row, and a gated row
   touched while the master goes off, keep theirs), and save the store
   again.

The report lists every success and failure. Pending choices: cleared after a
clean Apply, kept after a refused or partial one, and each scan drops those
the live value already matches. No confirmation dialog **(call: nothing here
can lock the user out; the worst case is ugly text, undone from the row)**.

### 5.5 Restore

From a row or History, after a confirmation. Each id: SET the stored value
exactly, GET it back, forget the snapshot only if they match. Same order rule
as Apply, using the restored master value; restoring the master also takes
companion snapshots first and restores the companions in the order of 5.3.
The pending choices of restored settings are dropped. No `.reg` for a
restore.

### 5.6 Store and `.reg` net

- `visual-effects-backups.json`, the shared store type (today's
  `MkBackupStore`, moved): `users` → user → id → `{ timestamp, value }`
  plus the optional `companion_of` (5.3),
  `record_first`, `forget`, `has_backup`, `original`. An unreadable store
  blocks Apply and Restore. A key this version does not know is listed with
  its raw id and its restore fails.
- **The `.reg` (call).** Each Apply first exports the values directly in
  `HKCU\Control Panel\Desktop` and `HKCU\Control Panel\Desktop\WindowMetrics`
  (not other subkeys) as single values to
  `reg-backups\<stamp>-visual-effects.reg`, never a key delete. The shared
  export takes the key list and the file label; Mouse & keyboard passes its
  three keys. WinTune never imports it. Importing it by hand also reverts
  `UserPreferencesMask` (Mouse & keyboard's ClickLock, Sonar and Vanish too),
  the wallpaper and the window metrics (title and menu fonts), and takes
  effect only at sign-in; the reference doc says so. Cost if wrong: a user
  who imports it loses unrelated changes, having been warned.

### 5.7 Scan and page

**Scan.** A background job like the other scans: at startup, on page open
and after each job, never while a job runs. One GET per setting; a setting
whose GET fails is **locked** (control disabled, error on hover). No
`WM_SETTINGCHANGE` listener **(call; cost: a change made in Performance
Options while WinTune is open shows at the next scan)**.

**Account check.** The shared check (`GetUserNameExW` against
`WTSQuerySessionInformationW` for the current session), at each scan and
again inside the Apply and Restore engines. If the accounts differ or the
check fails, a banner replaces the sections: "WinTune runs as {0}, but {1} is
signed in. Visual effects would be saved for the wrong account." Cost if
wrong: a false lock makes the page read-only for that run.

**Page.** Nav: "Visual effects" in the Customize group, after Mouse &
keyboard. Header note: "Changes apply when you press Apply and are kept when
you sign out. Some programs show them only after a restart." Header buttons
**For appearance** and **For performance (call)**: they set pending values
only, for the toggles and fade choices of sections 1 to 4. Appearance turns
everything on, fades as Fade. Performance turns every effect off and sets the
master on (pending, when it is off), so the gated rows are enabled and get
their pending "off" and each row shows its real state **(call, builder
check: with the master left off, the gated rows would stay disabled and keep
their values; cost if wrong: a user who wanted the master itself off turns it
off with one more click)**. Neither touches Text or
Caret and focus: Windows' own "best performance" also turns font smoothing
off (FOLKLORE), which costs readability for no gain today. Cost if wrong:
one more click for a user who wants smoothing off too.

Six kit sections in table order. Each row: a `SettingRow` with a toggle, a
two-choice control or a slider plus value label; a "Not applied" pill when
pending; the `…` menu with **Windows default** (sets the pending value;
disabled when already shown) and **Restore original** (when a snapshot
exists). The Animations section ends with a link line: "Show window contents
while dragging is on the Mouse & keyboard page" (`open_page`). The Text
section says "WinTune draws its own text, so look at another window, such
as Notepad, to see the change" **(call: no in-page sample, since egui
ignores these settings and a GDI sample is extra code; cost: the user
checks in another window)**. Action bar (`owns_action_bar`):
"N changes not applied", Discard, Apply. Search (Ctrl+F) filters by title.

### 5.8 History and Home

- History: module "Visual effects" (`record_history`, translated by
  `history_module`), a filter chip, a Saved originals group
  (`OriginalsModule::VisualEffects`), one row per snapshot, named by the
  setting's title and described "Original: {0}, saved on {1}" with the value
  as on the page ("On", "Slide", "ClearType", "1,200", "2 px"). Item and group
  restore run the restore job. A slot back at its original is forgotten.
- Home tile: "{0} effects changed by WinTune", this user's snapshot count,
  companions left out (5.3).
  No drift count **(call; cost: no Home warning when Performance Options
  changes a setting after WinTune)**.
- English and French for every text. Engine reasons are enums (`VeProblem`:
  out of range, unreadable, unknown setting, wrong account, account unknown,
  not saved, no original; `VeFailure`: SET failed, readback mismatch,
  unverified, held by master, store not saved) translated in `i18n/messages.rs`; report
  detail lines stay English.

## 6. Constraints

- Rust 1.88, egui/eframe 0.29.1, windows-sys 0.59 (`ANIMATIONINFO` and every
  `SPI_*` and `FE_*` constant are in `Win32_UI_WindowsAndMessaging`, already
  enabled), winreg 0.52 for the read-only `.reg`. No new crate.
- Never write under `HKCU\Control Panel`, `Explorer\Advanced` or `DWM`; every
  SET on the job worker thread.
- Tests use `FakeEffects`; nothing calls a real SET. The `.reg` export reads a
  sandbox under `HKCU\Software\WinTuneTest\<unique>` and writes to a temp
  folder; tests touch no other registry key.
- Follow CONTRIBUTING.md "For a new module" (the "build on `regsnap`" step is
  replaced by the SPI layer, as in Mouse & keyboard): `src/visual_effects/`,
  `ui/visual_effects_state.rs`, `ui/visual_effects_tab.rs`, jobs in `jobs.rs`,
  `history_panel.rs`, `originals.rs`, `home.rs`, the page in
  `shell_pages_draw_no_english_only_text_in_french`. ARCHITECTURE.md gets an
  engine section, the `sysparams` layer and a store table row.

## 7. Delivery

One PR (`feat/visual-effects-1`). First commit: the `src/sysparams/`
extraction alone, every Mouse & keyboard test green and its store file
byte-identical. Then the module. The plan adds a "Visual effects" section to
`docs/registry-reference.md` in the Mouse & keyboard format (what WinTune
reads, what it writes: nothing directly, the 5.1 table, the `.reg` net, an
Evidence table from the recon, the non-goals) and a drift test that the doc
names every id and SET action. No test calls a real SET, so the owner runs
this before merge and records each result:

1. Turn "Animate windows when minimizing and maximizing" off, Apply:
   minimizing Notepad no longer animates, and Performance Options shows the
   box cleared. Turn it on again. Result: ______
2. In a classic menu bar (Registry Editor), menu animation on with Slide,
   Apply: menus slide. Fade: they fade. Menu animation off: they just appear.
   Result: ______
3. Turn "Show shadows under mouse pointer" on, Apply: the pointer has a
   shadow and Performance Options shows it checked. Result: ______
4. Note the gated rows, turn UI effects off, Apply: Registry Editor menus no
   longer animate. After the rescan, record which case happens: the gated
   rows keep their values (the master only gates), or they read off (case 1:
   cleared, or case 2: GETs report off). Then restore UI effects from
   History: menus animate again and the gated rows show their noted values.
   Both cases are safe by design (5.3): in case 1 the master's restore brings
   the effects back from its companions; in case 2 a write is reported as
   "Windows keeps this effect off while UI effects is off" and nothing is
   lost. Result: ______
5. Set smoothing type Standard, Apply: Magnifier shows grey text edges, no
   colour fringes. Back to ClearType, contrast 2,200: text is darker.
   Restore both from the rows. Result: ______
6. Caret width 5 and focus border 3 x 3, Apply: Notepad's caret is thick,
   Settings > Accessibility > Text cursor shows 5, and Tab in the Run dialog
   (Win+R) shows a thick focus rectangle. Result: ______
7. For performance, Apply; then History, Saved originals, Visual effects,
   Restore all: the page and Performance Options show the originals; the
   group is empty. Result: ______
8. Change two settings, Apply, sign out and in: WinTune and Performance
   Options still show them. Restore them. Result: ______

## 8. Open questions for the owner

None. The FOLKLORE defaults and checkbox labels stay tagged FOLKLORE in the
reference doc until a fresh account is at hand.

## 9. Left to the builder

- Layout within sections 4 and 6: the names in `src/sysparams/`, enum, key
  and id names, wording in English and French, the nav icon.
- The exact descriptor table, each family checked against Learn (the
  readback catches a wrong one); whether the shared executor or each
  module's backend builds the `ANIMATIONINFO`.
- Whether `VeProblem` / `VeFailure` stay separate types or join Mouse &
  keyboard's in `sysparams` as generic enums, if that stays small.
- How the two-choice controls and sliders are drawn (kit, tokens only); the
  order of rows within a section.
