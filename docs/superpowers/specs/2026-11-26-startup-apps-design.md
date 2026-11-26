# Startup apps module: design

Date: 2026-11-26. Status: design approved by delegation. The owner asked the
builder to choose the module and make the calls, and to ask only when the
owner is really needed. Every call made that way is marked **(call)**, with
what it costs if it is wrong.

Evidence: `recon-startup-windows.md` (Windows research, each claim tagged
DOCUMENTED / OBSERVED / FOLKLORE) was summarised into section 3. The real
values seen on the development PC are in section 3.4.

## 1. Intent

WinTune is growing into a toolbox of Windows customisations, each one per
user, snapshotted and undoable from History. The next module lists the
programs that start when the user signs in, and lets them be turned off and
back on. That is what Task Manager's "Startup apps" page and Settings > Apps
> Startup do, but done the WinTune way: changes stay pending until Apply, a
snapshot is taken before any write, a `.reg` backup is exported, and History
can put back the original. **(call)** This module was chosen over an
appearance page or an "ads and suggestions" page because it affects sign-in
time, a common complaint, and because it reuses the context menu's
"existing entries" pattern (scan, toggle, snapshot, restore) almost whole.

## 2. Goals, non-goals, anti-goals

Goals:
- List the classic startup programs from every source Task Manager reads:
  the `Run` keys (per user, all users, 32-bit all users) and the Startup
  folders (per user, all users).
- Turn each one off or on with a toggle, pending until Apply, in one job.
- Show, for each row: its name, the program's product name (else the file
  name), the command or file, whether it applies to all users, and whether
  it is off, and who turned it off (WinTune, or something else such as Task
  Manager).
- Undo: "Restore original" on a row, and the History "Saved originals"
  group, as for the other modules.
- A Home tile: "{0} apps, {1} turned off".

Non-goals (for this module, maybe later):
- Packaged (Store / MSIX) apps' startup tasks. Their state is a DWORD
  `State` under `HKCU\Software\Classes\Local Settings\...\AppModel\
  SystemAppData\<package>\<task>` (OBSERVED). Microsoft says a task the user
  turned off cannot be turned back on by code (DOCUMENTED, for the
  StartupTask API). **(call)** Left out, because writing that state is not
  documented at all and the API only works for an app's own package. Cost if
  wrong: Store apps such as Teams (new) or Spotify are missing from the page.
- `RunOnce` keys (run once, then removed; the HKLM one runs only for
  administrators), scheduled tasks, services, and the "Startup impact"
  figure (where Task Manager stores it is unknown).
- Adding new startup programs.

Anti-goals (a failure even if it works):
- Deleting or editing a `Run` value, or deleting, moving or renaming a file
  in a Startup folder. WinTune only ever writes the "approved" switch.
- Writing under HKLM. **(call)** Same rule as every other module.
- A state that disagrees with Task Manager: an app WinTune turned off must
  show as Disabled in Task Manager, and one turned off in Task Manager must
  show as off in WinTune.

## 3. What Windows does

### 3.1 Sources (DOCUMENTED)

The Learn page on startup apps lists: `HKCU\...\CurrentVersion\Run`,
`HKLM\...\CurrentVersion\Run`, `HKLM\SOFTWARE\WOW6432Node\...\Run`, the
`RunOnce` keys, the per-user Startup folder
(`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`) and the
all-users one (`%ProgramData%\Microsoft\Windows\Start Menu\Programs\
StartUp`), plus packaged `windows.startupTask`. A command line is limited
to 260 characters. Run values are `REG_SZ` or `REG_EXPAND_SZ`.

### 3.2 The approved switch (OBSERVED, not documented by Microsoft)

Task Manager and Settings turn an item off without deleting it by writing a
value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\
StartupApproved\<sub>`, named exactly like the item:
- `Run`: for a value of either `Run` key (per user or all users, 64-bit).
- `Run32`: for a value of the 32-bit all-users `Run` key.
- `StartupFolder`: for a file in either Startup folder, named by its file
  name (`Discord.lnk`).

The value is `REG_BINARY`, 12 bytes: a little-endian DWORD flag, then a
FILETIME. The flag is `2` or `6` when on, and `3` (or `7`) when off. Task
Manager writes `3` and the time it was turned off. No value means on. The
same keys exist under HKLM (written by installers or administrators).

### 3.3 Inferred, to be checked on the real machine

A per-user `StartupApproved` value turns off an all-users item for this
user only. This is an inference, and a weak one: on an administrator
account Task Manager usually runs elevated and could write HKLM instead. So
the fallback is built from the start: each all-users source has its own
"per-user switch works" flag in the code (all-users `Run`, 32-bit `Run32`,
all-users Startup folder). A source whose flag is off shows its rows locked
("Turned on for every account"). **(call)** Initial flags: all-users `Run`
and all-users Startup folder **on**, checked by checklist items 2 and 3
before merge (turned off if a check fails); `Run32` **off**, because the
development PC has no 32-bit item to check it with. Cost if wrong: 32-bit
all-users items (rare) are read-only until someone checks them.

### 3.4 Real shapes seen (development PC)

14 per-user Run values, 3 all-users Run values, an empty 32-bit Run key, 4
values turned off in HKCU StartupApproved (one of them `03` with a zero
time, written by another tool), one approved value without a Run value, and
several Run values without an approved value. Commands with unquoted paths
containing spaces, nested quotes and `REG_EXPAND_SZ`. Both Startup folders
hold a `desktop.ini`.

## 4. Approaches considered

- **A. The approved switch, like Task Manager (chosen).** Write only
  `StartupApproved` values under HKCU. Pros: the item and its command are
  never touched; Task Manager and Settings show the same state; works for
  all-users items without HKLM writes (if 3.3 holds). Cons: undocumented
  format, so the page says so (Unofficial pill on the page header, not on
  every row).
- **B. Remove the Run value / move the shortcut, keep it in WinTune's
  store.** Documented locations only. Cons: Task Manager no longer lists
  the item at all; apps that rewrite their Run value on launch undo it;
  all-users items need HKLM writes and file moves; restore must recreate
  things. Rejected.
- **C. A for per-user items, B for all-users items.** Gets the worst of B
  for half the rows. Rejected.

## 5. Design

### 5.1 Rows

Each row is one approved value, because that is what Windows turns on and
off:
- **Id**: `run:<value name>`, `run32:<value name>`, `folder:<file name>`,
  matching `StartupApproved\Run`, `Run32` and `StartupFolder`. Lowercase
  for matching, original spelling for display. The row's one `Slot` is that
  approved value, the join key between row, pending choice, store and
  History.
- **Shared value (call):** the per-user and all-users `Run` keys can both
  hold a value named X, and both Startup folders can hold `F.lnk`; each pair
  shares one approved value. Such a pair is **one row** listing both
  sources ("For you and for every account"); its toggle turns both on or
  off, which is what Windows does anyway. Its detail shows both commands.
  If either source's flag (3.3) is off, the row is locked. Cost if wrong:
  none in behaviour, only how the pair is drawn.
- **Name**: the value name, or the file name without its extension.
- **Program**: the product name from the file's version info, else the
  file name (reuse `context_menu::fileinfo::product_name` and the command
  parser `origin::command_program`, made `pub(crate)` or moved to a shared
  module). For a `.lnk`, the shortcut's target is **(call)** not resolved in
  this module: the row shows the file name as the program. Cost if wrong:
  shortcuts show "Discord" from the file name rather than "Discord Inc."
  product name.
- **Detail**: the command line (or the file path), shown on hover in full.
- **All users** pill for HKLM and all-users folder items.
- **Program not found** pill when the program file does not exist (the
  item still toggles; this helps clean up leftovers). **(call)**
- **State**: On, Off (turned off by WinTune: a snapshot exists), Off
  elsewhere (no snapshot: Task Manager, another tool or an installer).
- **Locked** (toggle disabled, with the reason on hover):
  - the approved value is unreadable or has an unknown shape (not
    `REG_BINARY`, shorter than 4 bytes, or a flag other than 2, 3, 6, 7);
  - the item is turned off by an HKLM `StartupApproved` value (turning it
    on for one user may not override it): "Turned off for every account";
  - an all-users source whose per-user flag (3.3) is off.
- Orphan approved values (no matching item) are ignored. Microsoft's own
  items (Windows Security's tray icon) are listed, as in Task Manager.
  Hidden and system files and `desktop.ini` in the folders are skipped.

Sorted by name. A search box (Ctrl+F) filters by name or program, as on the
context menu page. One list, no sections. **(call)** Cost if wrong: a long
list on PCs with many items; sections can be added later.

### 5.2 Writes

- **Turn off**: write `03 00 00 00` + the current FILETIME (12 bytes,
  `REG_BINARY`) to the HKCU approved value.
- **Turn on**: if the snapshot's original was on (absent, or flag 2/6),
  put the original back exactly (delete the value if it was absent);
  otherwise write `02 00 00 00` + 8 zero bytes.
- Each approved value is one `backups::Slot::Value`. Snapshot on first
  touch, store saved and `.reg` exported before any write, every failure
  reported; any validation problem refuses the whole Apply. After Apply, a
  slot back at its original (same presence and same bytes) is forgotten,
  as in the context menu.
- No refresh is needed. The page says "Takes effect at next sign-in".
- Restore: `backups::restore_ids` + `restore_snapshot` (exact bytes or
  deletion). Nothing else to restore.
- Its own store, `startup-backups.json`, loaded like the others; an
  unreadable store blocks Apply and Restore.

### 5.3 Page

Nav: "Startup apps" in the Customize group, after Context menu. Page header
with the Unofficial pill and its hint ("Uses the same switch as Task
Manager, which Microsoft does not document"), a one-line note "Changes take
effect the next time you sign in". Rows built on the kit (`SettingRow`,
`toggle_enabled`, `overflow_menu` with Restore original when a snapshot
exists, pills). Action bar: "N not applied", Discard, Apply. Pending
choices behave as on the context menu page since PR #27: cleared after a
clean Apply, kept after a refused one, pruned by each scan. The scan is a
background job at startup, on page open and after every job, never while a
job runs. History: module "Startup apps", filter chip, Saved originals group
naming rows by their scanned name, else the value or file name. Home tile.
English and French texts.

## 6. Constraints

- Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59. No new
  crates unless needed for the Known Folder path (prefer
  `SHGetKnownFolderPath` through windows-sys, or the `APPDATA` /
  `ProgramData` environment variables).
- 32-bit view: read `SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\
  Run` by path (WinTune is 64-bit), not through `KEY_WOW64_32KEY` flags.
- Read Run values unexpanded; expand only to find the program file.
- Tests touch only `HKCU\Software\WinTuneTest` and temp folders (the
  engine takes injectable registry targets and folder paths).
- Engine messages are enums translated in `i18n/messages.rs`; every text is
  a key in both `.resx` files.

## 7. Delivery

One PR (`feat/startup-1`). The owner runs a real-machine checklist before
merge, written into `docs/registry-reference.md`:
1. Turn off a per-user Run item in WinTune: Task Manager shows Disabled;
   after sign-out/sign-in it does not start. Turn it back on.
2. Same for an all-users Run item (checks 3.3). If it fails, its flag is
   turned off before merge.
3. Same for a shortcut in the per-user Startup folder, then one in the
   all-users folder (if none exists, copy any shortcut there; remove it
   after). If the all-users one fails, its flag is turned off.
4. Turn an item off in Task Manager: WinTune shows "Off elsewhere".
5. Restore an item from History.

## 8. Open questions for the owner

None now. Section 3.3 is settled by checklist items 2 and 3; `Run32` stays
locked until someone has a 32-bit all-users item to check it with.

## 9. Left to the builder

- Module layout (`src/startup/` with scan and engine files; copy the
  per-module UI pattern rather than extracting a generic layer, keeping
  `Slot` ids as the join key between rows, store and History).
- Exact enum and key names, wording in English and French, the nav icon.
- Whether to add a drift test between the reference doc and the startup
  locations.
