# Registry reference

This page lists every registry key and value WinTune can write, and how it
undoes each change. A unit test (`tweaks::tests::the_registry_reference_lists_every_tweak`)
fails if a tweak is added to the catalog without being listed here.

All paths of the tweak modules are relative to `HKEY_CURRENT_USER` (HKCU),
the account WinTune runs as. "Off" is the value Windows uses by default; a
missing value is also read as "off". **Restore original** does not write
"off": it puts back exactly what was there before WinTune first changed the
setting, including "not set".

Abbreviations: `CV` = `Software\Microsoft\Windows\CurrentVersion`.

## File associations (HKLM and HKU)

Applying a rule (for example `.rs` → `CustomRustFile`) writes:

| Key | Change | Undone by |
| --- | --- | --- |
| `HKLM\SOFTWARE\Classes\<ProgID>` | Description, `DefaultIcon`, `shell\open\command` (REG_EXPAND_SZ), optional `shell\edit`, `shell\runas` and custom verbs | Rollback restores the key's original tree (or deletes it if WinTune created it) |
| `HKLM\SOFTWARE\Classes\<ext>` | Default value = ProgID, `PerceivedType`, `Content Type`; the previous ProgID is added to `OpenWithProgids` (REG_NONE) | Rollback restores the original tree |
| `HKU\<SID>\Software\Classes\<ext>` | Deleted when "Clean per-user overrides" is checked | Rollback restores it (for each signed-in user) |
| `HKU\<SID>\CV\Explorer\FileExts\<ext>` | Deleted when "Reset Explorer UserChoice override" is checked | Rollback restores it (see the UserChoice limit in the README) |

Refused names: structural `Classes` keys (`CLSID`, `Interface`,
`Wow6432Node`, `Directory`, `Folder`…), executable launch types (`exefile`,
`lnkfile`, `batfile`…) and the extensions `.com`, `.cpl`, `.dll`, `.exe`,
`.lnk`, `.msi`, `.msp`, `.pif`, `.scr`, `.sys`, `.url`.

## Explorer & Taskbar

| Id | Setting | Path (HKCU) | Value | On | Off | Visible | Windows |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `show_extensions` | Show file extensions | `CV\Explorer\Advanced` | `HideFileExt` | 0 | 1 | new windows | 10, 11 |
| `show_hidden` | Show hidden files | `CV\Explorer\Advanced` | `Hidden` | 1 | 2 | new windows | 10, 11 |
| `show_protected_os_files` | Show protected system files | `CV\Explorer\Advanced` | `ShowSuperHidden` | 1 | 0 | new windows | 10, 11 |
| `open_this_pc` | Open Explorer to This PC | `CV\Explorer\Advanced` | `LaunchTo` | 1 | 2 | new windows | 10, 11 |
| `full_path_title` | Full path in the title bar | `CV\Explorer\CabinetState` | `FullPath` | 1 | 0 | Explorer restart | 10, 11 |
| `compact_mode` | Compact view | `CV\Explorer\Advanced` | `UseCompactMode` | 1 | 0 | new windows | 11 |
| `item_checkboxes` | Item check boxes | `CV\Explorer\Advanced` | `AutoCheckSelect` | 1 | 0 | new windows | 10, 11 |
| `hide_recent_files` | Hide recent files in Quick access | `CV\Explorer` | `ShowRecent` | 0 | 1 | new windows | 10, 11 |
| `hide_frequent_folders` | Hide frequent folders in Quick access | `CV\Explorer` | `ShowFrequent` | 0 | 1 | new windows | 10, 11 |
| `classic_context_menu` | Classic right-click menu (**unofficial**) | `Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32` | key, empty default value | present | `{86ca1aa0-…}` tree deleted | Explorer restart | 11 |
| `taskbar_left` | Align taskbar to the left | `CV\Explorer\Advanced` | `TaskbarAl` | 0 | 1 | instantly | 11 |
| `hide_task_view` | Hide the Task View button | `CV\Explorer\Advanced` | `ShowTaskViewButton` | 0 | 1 | instantly | 10, 11 |
| `hide_search` | Hide the taskbar search | `CV\Search` | `SearchboxTaskbarMode` | 0 | 1 (search icon) | instantly | 10, 11 |

## Start & Privacy

| Id | Setting | Path (HKCU) | Value | On | Off | Windows |
| --- | --- | --- | --- | --- | --- | --- |
| `start_recommendations` | Hide recommendations in Start | `CV\Explorer\Advanced` | `Start_IrisRecommendations` | 0 | 1 | 11 |
| `start_more_pins` | More pins in Start | `CV\Explorer\Advanced` | `Start_Layout` | 1 | 0 | 11 |
| `start_hide_recent_apps` | Hide recently added apps | `CV\Start` | `ShowRecentList` | 0 | 1 | 11 |
| `start_show_all_pins` | Show all pins by default | `CV\Start` | `ShowAllPinsList` | 1 | 0 | 11 |
| `no_app_launch_tracking` | Don't track app launches | `CV\Explorer\Advanced` | `Start_TrackProgs` | 0 | 1 | 10, 11 |
| `no_recent_documents` | Don't show recent files | `CV\Explorer\Advanced` | `Start_TrackDocs` | 0 | 1 | 10, 11 |

All values are REG_DWORD, and every Start & Privacy change is picked up the
next time Start opens.

## Context menu

Keys the Context menu module works with (all under `HKEY_CURRENT_USER`, so no
elevation is needed; HKCU entries are merged over HKLM by `HKCR`):

| Key | Used for |
| --- | --- |
| `Software\Classes\*\shell\<verb>` | Entries for every file |
| `Software\Classes\Directory\shell\<verb>` | Entries for folders |
| `Software\Classes\Drive\shell\<verb>` | Entries for drives |
| `Software\Classes\Directory\Background\shell\<verb>` | Entries for the folder background (not probed) |
| `<verb>\command` (default value) | Command line. Placeholder for the clicked item: `%1` for Files (`*`), Folders (`Directory`) and Drives (`Drive`); `%V` for the folder background (`Directory\Background`, which Explorer also shows on a drive root's empty area). WinTune quotes it and adds a dot or `\.` where a drive root can arrive (see "What WinTune writes") |
| `Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked` | Value name = handler CLSID, data `""` (undocumented) |

Values on a verb key: `MUIVerb` (label), `Icon` (path, optionally `,index`),
`Extended` (only with Shift), `ProgrammaticAccessOnly` (hides the verb from
the menu). `MUIVerb`, `Icon` and `ProgrammaticAccessOnly` are described in
Microsoft Learn, *Creating Shortcut Menu Handlers* / *Customizing a Shortcut
Menu*. `Blocked` is undocumented.

### What WinTune writes

For each entry and each place it is ticked for, WinTune creates one verb key
`HKCU\Software\Classes\<class>\shell\WinTune.<id>` (`<id>` is `e1`, `e2`, ...)
with these values. Nothing is written under `HKLM`, and only keys named
`WinTune.*` are ever created or deleted.

| Place | `<class>` | Placeholder written for a program that takes the path as an argument (VS Code, any `.exe`) |
| --- | --- | --- |
| Folders | `Directory` | `%1` |
| Folder background | `Directory\Background` | `%V\.` |
| Drives | `Drive` | `%1.` |
| Files | `*` | `%1` |

| Value | Type | Content |
| --- | --- | --- |
| `MUIVerb` (on the verb key) | REG_SZ | The entry's label, trimmed, with each `&` written as `&&` (a single `&` would turn the next letter into an accelerator). A label starting with `@` is refused: `MUIVerb` reads it as an indirect string (`@file,-id`) |
| `Icon` (on the verb key) | REG_SZ | The icon file chosen for the entry, else the program's `.exe`, in quotes |
| `command\(Default)` | REG_SZ | See below |

Command line by program (`<exe>` is the program's full path). A drive root
ends in a backslash (`C:\`), and `\"` would be read as an escaped quote and
leave `C:"`. So Drives passes `%1.` (`C:\.`), and the folder background, which
Explorer also shows on a drive root's empty area, passes `%V\.` (`C:\\.` there,
`C:\folder\.` in a folder; all name the same folder). Folders and Files pass
the plain placeholder.

| Program | Folders | Folder background | Drives | Files |
| --- | --- | --- | --- | --- |
| VS Code, any `.exe` | `"<exe>" "%1"` | `"<exe>" "%V\."` | `"<exe>" "%1."` | `"<exe>" "%1"` |
| Windows Terminal | `"<exe>" -d "%V"` | `"<exe>" -d "%V\."` | `"<exe>" -d "%V."` | refused |
| PowerShell | `"<cmd.exe>" /s /c "pushd "%V" && "<exe>" -NoExit"` | same | same | refused |
| Command Prompt | `"<exe>" /s /k pushd "%V"` | same | same | refused |

Terminal, PowerShell and Command Prompt start in a folder, so they are refused
on Files. The PowerShell and Command Prompt presets go through `cmd.exe`
(PowerShell with `pushd`, so a network folder gets a temporary drive letter)
because cmd's quoting copes with apostrophes in folder names. Known limits:
`cmd` expands `%VAR%` text inside a folder name, in both presets; and a
semicolon in a folder name can break Windows Terminal's `-d`, which splits its
command line at semicolons. Apply makes the keys match the list: the verb key of a removed
entry, or of a place that was unticked, is deleted. Before the first change to
a key, its original state is saved in
`%ProgramData%\Wintune\context-menu-backups.json` (per user, by key path) and
a `.reg` export is written; History, Saved originals, Context menu puts it
back. On Windows 11 these entries appear under "Show more options".

### Hiding and renaming existing entries

Written by the Context menu page (Windows and Other programs). Nothing is
written under `HKLM`. Before each first change, the slot is snapshotted in
`context-menu-backups.json` (per user, by slot) and a `.reg` is exported.

| Action | Key | Value | Content |
| --- | --- | --- | --- |
| Hide a verb | `HKCU\Software\Classes\<class>\shell\<verb>` (a copy of the HKLM verb key, with no `command` of its own) | `ProgrammaticAccessOnly` | REG_SZ `""` |
| Show a verb | same | `ProgrammaticAccessOnly` removed | The HKCU copy is deleted if WinTune created it and it is now empty |
| Rename a verb | same | `MUIVerb` | REG_SZ, 1 to 80 characters, each `&` written as `&&`, no leading `@` |
| Reset a name | same | `MUIVerb` | The value of the first snapshot, or removed if the verb had none. Plain hide and show never touch `MUIVerb` |
| Hide a shell extension or modern entry | `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked` | `{CLSID}` | REG_SZ `""` (undocumented: rows carry an Unofficial pill) |
| Show it | same | `{CLSID}` | Value deleted |

**Restore original** (History, or a row's `…` menu) puts back only what
WinTune changes. On a verb's HKCU copy that is `ProgrammaticAccessOnly` and
`MUIVerb`: each returns to its value in the snapshot, or is removed if the
snapshot had none; the copy is then deleted only if WinTune created it and it
is empty (never a tree delete). Anything a program wrote in that key since
(command, icon, other values) stays. A copy that no longer exists is not
recreated. A `Blocked` value and a `WinTune.*` key come back exactly as
snapshotted.

When an Apply brings a slot back to its original (a copy WinTune created is
deleted again, both values equal the snapshot's, or a `Blocked` value is
present or absent as before), its snapshot is dropped and the store saved
again: no Restore original is offered for a state that is already the
original, so a key a program writes later is never touched.

A CLSID found in `HKLM\...\Shell Extensions\Blocked` is hidden for every
account. WinTune cannot change that: the row is shown locked, with its toggle
disabled. The scan lists verbs and handlers under `shell` and
`shellex\ContextMenuHandlers` of `*`, `AllFilesystemObjects`, `Folder`,
`Directory`, `Directory\Background` and `Drive`, in HKCU and HKLM `Classes`.

### Real-machine probe

Script: `scripts/context-menu-probe.ps1` (Windows PowerShell 5.1, unelevated).
It reads menus with `Shell.Application ... ParseName(...).Verbs()` (classic
verbs and shellex handlers; modern entries are not listed), compares labels
with `&` removed, never calls `DoIt()`, and restores every key it touched.
Run on Windows `10.0.26300.0` (Windows 11), French UI.

| Check | Result | Evidence |
| --- | --- | --- |
| 1. Own verb with `MUIVerb` `WinTune probe é'`, `Icon`, `command`, under `Directory`, `Drive` and `*` | PASS | The verb shows in `Verbs()` of a folder named `probe 'é x`, of drive `C:\` and of a temp file. Quotes and accents in the label and path are fine. |
| 2a. HKCU copy of an HKLM `Directory\shell` verb with only `ProgrammaticAccessOnly` = `""` | PASS | `AnyCode` ("Open with Visual Studio") disappears from `Verbs()`. |
| 2b. Same HKCU copy with only `MUIVerb` = `Probe renamed` | PASS | The label changes to `Probe renamed`; the old label is gone; `HKCR\Directory\shell\AnyCode\command` still returns the HKLM command line (HKCU copy has no `command` of its own). |
| 3. `{596AB062-B4D2-4215-9F74-E9109B0A8153}` (Restore previous versions) as a value in HKCU `Blocked` | PASS (new-process read) | Precondition: the verb "Restaurer les versions précédentes" is present in `Verbs()` of `C:\Windows` (read from a new PowerShell process, before the write). After the write, a new PowerShell process no longer lists it and nothing else vanished (19 verbs before, 18 after). A first attempt that read before and after in the same process saw no change (24 and 24): an in-process handler cache hides the effect, so always re-read in a new process. Proves only `Verbs()` from a fresh non-Explorer process; Explorer itself and an Explorer restart are untested. |

Consequences: hiding a system (HKLM) classic verb with a
`ProgrammaticAccessOnly` HKCU copy and renaming it with a `MUIVerb`-only copy
both work without touching HKLM. `Blocked` hides a shellex handler's verb in
a fresh process; whether the open Explorer honours it immediately is still to
be confirmed by the owner (PR 2 / spec section 8).

### Windows entries WinTune can hide

Read on this machine (Windows 11 build 26300), read-only. CLSIDs and their
friendly names come from `HKCR\CLSID`; places come from the `shellex` keys
that reference them. A Clsid entry is hidden by adding its CLSID to HKCU
`Blocked`. A Verbs entry is hidden with a `ProgrammaticAccessOnly` HKCU copy of
the verb.

| id | Mechanism | Places | OS | Presence | Also affects | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `open_in_terminal` | Clsid `{9F156763-7844-4DC4-B2B1-901F640F5155}` | Folder, Background | Any | Packaged COM class registered: `Software\Classes\PackagedCom\ClassIndex\{9F156763-…}` in HKCU or HKLM | none | Not a classic verb. Terminal's AppxManifest declares `desktop5:Verb Id="OpenTerminalHere"` on `Directory` and `Directory\Background`. Read here: `HKLM\Software\Classes\PackagedCom\ClassIndex\{9F156763-7844-4DC4-B2B1-901F640F5155}` exists, default value `Microsoft.WindowsTerminal_1.25.3131.0_x64__8wekyb3d8bbwe`. The `wt.exe` alias is not the evidence: it can exist without the package (or be turned off while Terminal is installed). |
| `share` | Clsid `{E2BF9676-5F8F-435C-97EB-11607A5BEDF7}` | Files, Folder | Any | CLSID registered | none | Friendly name "Ribbon Modern Share Verb". |
| `give_access` | Clsid `{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}` | Files, Folder, Background, Drive | Any | CLSID registered | Sharing tab of folder and drive Properties | Friendly name "Shell extensions for sharing". Also listed under `Directory` and `Drive` `PropertySheetHandlers`. |
| `previous_versions` | Clsid `{596AB062-B4D2-4215-9F74-E9109B0A8153}` | Files, Folder, Drive | Any | CLSID registered | Previous Versions tab of Properties | Friendly name "Previous Versions Property Page". Also listed under `AllFilesystemObjects`, `Directory` and `Drive` `PropertySheetHandlers`. |
| `include_in_library` | Clsid `{3DAD6C5D-2167-4CAE-9914-F99E41C12CFA}` | Folder | Any | CLSID registered | none | Friendly name "Include In Library Sub Context Menu". |
| `pin_to_start` | Clsid `{470C0EBD-5D73-4D58-9CED-E91E22E23282}` | Folder | Any | CLSID registered | Pin to Start on programs and shortcuts | Friendly name "Pin To Start Screen verb handler". Also listed under `exefile`, `mscfile` and the `Launcher.*` classes. |
| `send_to` | Clsid `{7BA4C740-9E81-11CF-99D3-00AA004AE837}` | Files, Folder | Any | CLSID registered | none | Friendly name "Microsoft SendTo Service". |
| `cast_to_device` | Clsid `{7AD84985-87B4-4A16-BE58-8B72A5B390F7}` | Files | Any | CLSID registered | none | Friendly name "Play To menu". |
| `scan_with_defender` | Clsid `{09A47860-11B0-4DA5-AFA5-26D86198A780}` | Files, Folder, Drive | Any | CLSID registered | none | Friendly name "EPP" (Defender); the friendly name is empty on the key itself. |
| `pin_to_quick_access` | Verbs `Folder\pintohome`, `Drive\pintohome` | Folder, Drive | Any | Verb exists | none | Verbs under HKLM `Folder\shell` and `Drive\shell`. |
| `add_to_favorites` | Verbs `*\pintohomefile` | Files | Windows 11 | Verb exists | none | Verb under HKLM `*\shell`. |

### Needs the owner's eyes

These cannot be seen through `Verbs()` and need a look at the real menu (the
steps for hiding and renaming are in the PR 2 checklist below):

- Windows entries (catalog above): each Clsid row hides in the open Explorer,
  or only after an Explorer restart.
- Modern entries ("Open in Terminal", "Share"): whether `Blocked` hides them
  in the new compact menu.
- Where the entries land on Windows 11: only under "Show more options" (the
  classic menu) or also in the new compact menu.
- Side effects of `Blocked` on Properties tabs: blocking "Give access to"
  removes the Sharing tab, and blocking "Restore previous versions" removes
  the Previous Versions tab (a blocked handler can also remove its overlay).
- Every row on Windows 10, where only the classic menu exists.

#### Pre-merge checklist for the Context menu entries

Not run by the developer: the command lines above were checked against the
quoting rules and the tests, but no program was launched. The owner runs this
before merging. Make one WinTune entry per program (VS Code, Windows Terminal,
PowerShell, Command Prompt, and one custom `.exe`, for example any editor that
accepts a folder argument), ticking every place the program allows (Terminal,
PowerShell and Command Prompt cannot be ticked on Files). Click Apply, then use
**Show more options** on Windows 11.

Test locations: a folder named `probe 'é x` (apostrophe, accent, spaces), a
drive root (`C:\`), and a network share (`\\server\share`, or a mapped drive)
if one is available. Expected result of every row: the program opens in that
folder (VS Code and the custom `.exe` open it as the folder; terminals start
with that folder as their current directory, and the prompt shows it).

| Program | Folder (right-click the folder) | Background (empty space inside it) | Background of a drive root (empty space in `C:\`) | Drive (right-click `C:` in This PC) | Files (right-click a file) |
| --- | --- | --- | --- | --- | --- |
| VS Code | opens `probe 'é x` | opens `probe 'é x` | opens `C:\` | opens `C:\` | opens the file |
| Windows Terminal | starts in `probe 'é x` | starts in `probe 'é x` | starts in `C:\` | starts in `C:\` | not offered |
| PowerShell | starts in `probe 'é x` | starts in `probe 'é x` | starts in `C:\` | starts in `C:\` | not offered |
| Command Prompt | starts in `probe 'é x` | starts in `probe 'é x` | starts in `C:\` | starts in `C:\` | not offered |
| Custom `.exe` | receives `probe 'é x` as one argument | receives `probe 'é x` | receives `C:\` | receives `C:\` | receives the file's path |

Repeat the Folder and Background columns for the network share (PowerShell
and Command Prompt use `pushd`, so they should start in a temporary drive
letter that points to the share; the others open the UNC path). Also check
that an entry labelled `Tom & Jerry` shows exactly that text, with no
underlined letter.

Record anything that does not match, with the program, the place and the
folder, before merging.

#### Pre-merge checklist for hiding and renaming existing entries (PR 2)

Not run by the developer: the registry writes are covered by tests and by the
probe above, but no real menu was looked at after a WinTune apply. The owner
runs this before merging, on Windows 11, and on Windows 10 if available. For
each step, check the open Explorer first, then again after an Explorer restart.

1. For each row of the Windows section present on the PC: hide it, Apply
   (confirm the side effects), right-click a folder, a file, the folder
   background and a drive as the row's places say. The entry is gone. Show it
   again: it is back.
2. Hide "Give access to": the Sharing tab of a folder's Properties is gone.
   Hide "Restore previous versions" (or the row for it): the Previous Versions
   tab is gone. Both tabs return on Show.
3. Hide and show one third-party verb (for example from an editor or an
   archiver) and one third-party shell extension. The verb disappears and
   returns in the open Explorer. For the extension, note whether it needs an
   Explorer restart.
4. Open Explorer's new compact menu (Windows 11) and check that "Open in
   Terminal" and "Share" go away when hidden and come back when shown.
5. Rename a third-party verb, Apply: the new label shows in the menu, with
   `&` and accented characters intact. Reset name: the old label returns.
6. In History, Saved originals, Context menu: **Restore original** for each
   entry changed above (an entry already shown again under its own name has
   nothing left to restore and is not listed). Each is back as it was: a
   `Blocked` value WinTune added is gone; a verb's `HKCU` copy is gone if
   WinTune created it, while a program's own per-user key keeps everything
   except `ProgrammaticAccessOnly` and `MUIVerb`, which return to what they
   were before.
7. Check that a CLSID blocked in `HKLM` shows locked with its toggle
   disabled. Creating that `Blocked` value needs an elevated prompt; skip this
   step if the PC has none.

Record anything that differs, with the entry, the place and the Windows
version, before merging.

## Startup apps

The Startup apps module lists the programs that start when a user signs in and
turns each one off or on for the current account. It reads the keys and
folders below and writes only a per-user switch.

### What WinTune reads

| Source | Key or folder | `StartupApproved` sub-key | All users |
| --- | --- | --- | --- |
| Run key (you) | HKCU `Software\Microsoft\Windows\CurrentVersion\Run` | `Run` | No |
| Run key (every account) | HKLM `Software\Microsoft\Windows\CurrentVersion\Run` | `Run` | Yes |
| Run key, 32-bit (every account) | HKLM `SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run` | `Run32` | Yes |
| Startup folder (you) | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup` | `StartupFolder` | No |
| Startup folder (every account) | `%ProgramData%\Microsoft\Windows\Start Menu\Programs\StartUp` | `StartupFolder` | Yes |

The key strings WinTune uses (in `src/startup/mod.rs`, without the hive):

- `Software\Microsoft\Windows\CurrentVersion\Run` (`RUN_KEY`, read in HKCU and
  in HKLM)
- `SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run` (`RUN32_KEY`,
  read in HKLM)
- `Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run`
- `Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32`
- `Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder`

The three `StartupApproved` keys are read in HKCU and in HKLM, and written
only in HKCU. A Run value or a file becomes one row; a per-user item and an
all-users item that share a `StartupApproved` key and a name (ignoring case)
are one row. The folders are read from `%APPDATA%` and `%ProgramData%`, not
from the shell's known-folder API. Every regular file in a Startup folder is
listed, except `desktop.ini` and hidden or system files. Run values that are
not `REG_SZ` or `REG_EXPAND_SZ`, and the default value, are skipped.

Not listed: `RunOnce` keys (they run once, then are removed), packaged
(Store/MSIX) startup tasks, scheduled tasks and services. Task Manager shows
the packaged tasks; WinTune does not.

### What WinTune writes

Only HKCU `StartupApproved\{Run,Run32,StartupFolder}\<name>`, one value per
row, `REG_BINARY`, 12 bytes: a little-endian DWORD flag, then a FILETIME.
`<name>` is the Run value name, or the file name with its extension for a
Startup folder (`Discord.lnk`).

| Action | Value written |
| --- | --- |
| Turn off | `03 00 00 00` followed by the current time as a FILETIME |
| Turn on | The original put back, when it was absent or an on value (`02` or `06`); otherwise `02 00 00 00` followed by 8 zero bytes |
| Restore original | The exact original value, or the value deleted if there was none |

WinTune never edits or deletes a `Run` value, never touches a file in a
Startup folder, and never writes under HKLM. Before the first change to a
switch, its original state is saved in `startup-backups.json` (in
`%ProgramData%\Wintune`, per user, by value path) and a `.reg` export is written, as for the other
modules. When an Apply brings a switch back to its original, its snapshot is
dropped and the store saved again, so no Restore original is offered for a
state that is already the original.

How a row's state is read: no value means on; a `REG_BINARY` of at least 4
bytes whose flag is `2` or `6` means on and `3` or `7` means off; anything
else (another type, fewer than 4 bytes, another flag) is an unknown shape. A
row counts as off when either its HKCU or its HKLM switch is off. Its state is
"Turned off by WinTune" when WinTune holds a snapshot of the switch, else
"Turned off by another program" (Task Manager, Settings, an installer).

A row is shown locked, with its toggle disabled, when:

- the switch cannot be read (`Unreadable`);
- the HKCU or HKLM switch has an unknown shape (`UnknownShape`);
- the HKLM switch is off (`OffForEveryone`): WinTune changes only the current
  account;
- one of its sources has its per-user switch flag set to `false` (see below;
  `AllUsersOnly`).

"Program not found" is shown on a Run command whose program has a folder part
and does not exist; a bare name such as `ms-teams.exe` is never flagged. The
program name shown is the product name from the file's version info, else the
file name without extension.

### Evidence

The sources are documented. The switch is not.

| Claim | Status | Source |
| --- | --- | --- |
| The Run keys (HKCU, HKLM, Wow6432Node), the `RunOnce` keys and the two Startup folders start programs; Task Manager lists and can disable them; a command line is limited to 260 characters; Run values are `REG_SZ` or `REG_EXPAND_SZ` | DOCUMENTED | Microsoft Learn, *Startup apps* (https://learn.microsoft.com/windows/compatibility/startup-apps) |
| HKLM `RunOnce` runs only for administrators; the 260-character limit | DOCUMENTED | Microsoft Learn, *Run and RunOnce registry keys* (https://learn.microsoft.com/windows/win32/setupapi/run-and-runonce-registry-keys) |
| Task Manager and Settings turn an item off by writing a `REG_BINARY` of 12 bytes (flag DWORD plus FILETIME) under HKCU `StartupApproved\{Run,Run32,StartupFolder}`, named like the item; no value means on | OBSERVED | Read-only probe, Windows 11 build 26300 (spec 3.2). Microsoft Learn does not document `StartupApproved`. `Run32` for the 32-bit key is inferred from its name |
| Flag `2` and `6` are on, `3` is off (with the time it was turned off) | OBSERVED | Same probe: `03` seen as the off value, flags 2 and 6 seen as on values |
| Flag `7` is off | FOLKLORE | Repeated in public write-ups, not seen on the probe PC. WinTune accepts it as off, and puts back whatever it found |
| A change takes effect at the next sign-in, with no Explorer restart | FOLKLORE | No Microsoft source found |
| A per-user switch in HKCU turns off an all-users item for that user only | FOLKLORE | An inference: nothing documents it. This is what the flags below and checklist items 2 and 3 are for |

Shapes seen on the development PC (spec 3.4): 14 per-user Run values, 3
all-users Run values, an empty 32-bit Run key, 4 values turned off in HKCU
`StartupApproved` (one of them `03` with a zero time, written by another tool),
one approved value without a Run value, several Run values without an approved
value, and a `desktop.ini` in both Startup folders.

Because the format is undocumented, the Startup apps page carries the
**Unofficial** pill.

### Per-user switch flags

`PER_USER_SWITCH_WORKS` in `src/startup/mod.rs` has one flag per source: does a
per-user switch in HKCU turn items of that source off? A source whose flag is
`false` shows its rows locked ("This app starts for every account...") and
Apply refuses them. The table in `src/startup/mod.rs` is the source of truth
for the values; this page does not repeat them, so it stays true when a flag
changes. Per-user sources are always `true`. The all-users sources start as:
`MachineRun` and `CommonFolder` `true` (until checklist items 2 and 3 are run),
`MachineRun32` `false` (the development PC has no 32-bit item to check it
with).

If checklist item 2 or 3 fails, set the matching entry to `false` in
`src/startup/mod.rs`. That is a one-line change, and the rows of that source
become read-only.

#### Pre-merge checklist (Startup apps)

Not run by the developer: the switch format was read from the registry and the
engine is covered by tests in a sandbox, but no startup item was turned off in
WinTune and then watched across a sign-in. The owner runs this before merging.
For each step, sign out and back in to see whether the program starts. Write
the result next to each item.

- [ ] 1. Per-user Run item. Turn one off in WinTune and Apply. Task Manager's
  Startup tab shows it as Disabled; after signing out and in, it does not
  start. Turn it back on: it starts again. Result: ______
- [ ] 2. All-users Run item (an HKLM `Run` value). Same test. If Task Manager
  does not show Disabled, or the program still starts, set `MachineRun` to
  `false` in `PER_USER_SWITCH_WORKS`. Result: ______
- [ ] 3. Startup folder. First a shortcut in the per-user folder, then one in
  the all-users folder (if there is none, copy any shortcut there, and remove
  it afterwards). Same test. If the all-users one fails, set `CommonFolder` to
  `false`. Result: ______
- [ ] 4. In Task Manager, turn an item off. WinTune shows it as "Turned off by
  another program". Result: ______
- [ ] 5. In History, Saved originals, Startup apps: **Restore original** for an
  item changed above. It is back as it was, and Task Manager agrees. Result:
  ______

Record anything that differs, with the item, its source and the Windows
version, before merging.

## Environment variables

The Environment variables module lists the current user's variables, edits the
user's `Path` as a list, and shows the machine's variables read-only. It reads
the keys below and writes only values of the user's `Environment` key.

### What WinTune reads

| Source | Key | Use |
| --- | --- | --- |
| User variables | `HKCU\Environment` | Every value, raw (references such as `%USERPROFILE%` stay unexpanded), with its registry type. Values that are not `REG_SZ` or `REG_EXPAND_SZ` are listed read-only and never changed; the empty-named default value is skipped |
| System variables | `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment` | Read only, for the "For every account" section and to flag a user `Path` entry that repeats a system one |
| Per-sign-in variables | `HKCU\Volatile Environment` | Never read, never written, never backed up. Windows rebuilds it at each sign-in (`USERPROFILE`, `APPDATA`, ...) |
| The process environment | WinTune's own | Display only: the last place a `%name%` is looked up when a `Path` entry is shown expanded |

A `%name%` in an entry is looked up in the user variables (with the pending
edits), then the system variables, then the process environment, at most 8
levels deep, by a small expander in WinTune (`Resolver`), not by
`ExpandEnvironmentStringsW`. The expansion is used only to show an entry and
to check that its folder exists, never for what is written. Only absolute
local paths (`X:\...`) are checked for existence; UNC and relative entries, and
entries with a `%name%` that cannot be resolved, are never flagged "Folder not
found".

### What WinTune writes

Only values of `HKCU\Environment`, one slot per value name.

| Action | Written |
| --- | --- |
| Add a variable | The value, `REG_EXPAND_SZ` if its text holds a `%name%` reference, else `REG_SZ`. A new `Path` is always `REG_EXPAND_SZ` |
| Edit a variable or the `Path` list | The value, in the type it already had, except that a `REG_SZ` value whose new text holds a `%name%` reference becomes `REG_EXPAND_SZ`. This applies to `Path` like any value |
| Delete a variable | The value deleted. Removing every `Path` entry counts as deleting `Path` |
| Rename a variable | The new name is written, then the old one is deleted |
| Rename that changes only letter case | The old spelling is deleted, then the new one written. The registry keeps the first spelling of a value name, so this is the only way to change it |
| Restore original | The exact original bytes and type, or the value deleted if it did not exist. A value stored under another spelling since is deleted first, so the original spelling comes back |

A `%name%` reference is a `%`, one or more characters other than `%` and `=`,
then a `%`. After the writes, WinTune sends one `WM_SETTINGCHANGE` with the
string "Environment" to every top-level window, through `SendMessageTimeoutW`
with `SMTO_ABORTIFHUNG` and a 5 second timeout, if anything was written (and
after a restore). New programs started from Explorer see the change; windows
that are already open, such as terminals, keep their old values until they are
restarted.

WinTune never writes under `HKLM`, never writes `HKCU\Volatile Environment`,
and never runs `setx`: `setx` stops a value at 1,024 characters and writes a
reference already expanded, so it would lose entries and references.
`SetEnvironmentVariable` changes only WinTune's own process, so it is not used
either.

Validation (any problem refuses the whole Apply, with nothing written):

- A name is trimmed and cannot be empty, contain `=` or a NUL character, or
  exceed 255 characters. Names are unique ignoring case after the change. The
  variable dialog also refuses the name `Path`, which is edited in its own
  section.
- A value cannot be empty (Delete removes a variable), contain a NUL character
  or exceed 32,767 UTF-16 units.
- A `Path` entry cannot contain `;`. Empty entries are dropped when the list is
  joined with `;`, with no trailing `;`.
- A variable changed more than once in one Apply, a text change to a variable
  that is not text, and a change to a variable that vanished since the last
  scan (`Gone`) are refused.
- An Apply that writes `Path` is refused when `HKCU\Environment`'s `Path` is
  no longer the one the page read, type and text compared, absent included
  (`ChangedSinceScan`). The pending list and its confirmation were computed
  from that read, so writing would drop, without naming it, a folder another
  program added since. The page reads `Path` again after the refused Apply,
  and the next Apply asks first, with the warning below and the folders that
  would be removed.

Warnings (asked in a confirmation before Apply, never a refusal): deleting or
renaming `Path`, `TEMP`, `TMP` or a variable whose name starts with `OneDrive`;
a `Path` whose expanded system part, a `;` and the expanded new user part make
more than 8,191 UTF-16 units (measured only when Apply writes `Path`); and a
`Path` that another program changed since the edit started (the Apply writes
the list the user built). A plain add or edit applies without asking: History
undoes it.

Before the first change to a value, its original state is saved in
`environment-backups.json` (in `%ProgramData%\Wintune`, per user, by slot id:
the value path `Environment` and the value name) and a `.reg` export is
written, as for the other modules. A rename's two slots are stored as a group in
the same file (`renames`, next to `users`): **Restore original** on either one,
or on either History row, restores both together, the old name coming back
before the new one goes. When an Apply brings a value back to its original
(same presence, type, bytes and spelling), its snapshot is dropped and the
store saved again, so no Restore original is offered for a state that is
already the original.

Restoring `Path` is exact, but its confirmation compares the original list
with the current one, entry by entry as written, and names the entries the
restore will **remove** (added since, by WinTune or by another program) and
those it will **add back**.

### Evidence

| Claim | Status | Source |
| --- | --- | --- |
| User variables live in `HKEY_USERS\<user>\Environment`, that is `HKCU\Environment` | DOCUMENTED | Microsoft Learn, *Win32_Environment class* (https://learn.microsoft.com/windows/win32/cimwin32prov/win32-environment) |
| System variables live in `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment`; after writing them, broadcast `WM_SETTINGCHANGE` with `lParam` "Environment"; a user-defined variable holds at most 32,767 characters | DOCUMENTED | Microsoft Learn, *Environment Variables* (https://learn.microsoft.com/windows/win32/procthread/environment-variables) |
| The broadcast is a `SendMessageTimeout` to `HWND_BROADCAST` with `wParam` NULL and `lParam` pointing to the string "Environment" | DOCUMENTED | Microsoft Learn, *WM_SETTINGCHANGE message* (https://learn.microsoft.com/windows/win32/winmsg/wm-settingchange) |
| A name cannot contain `=`; a process cannot change the environment of a process it did not start (children get a copy) | DOCUMENTED | Microsoft Learn, *Changing Environment Variables* (https://learn.microsoft.com/windows/win32/procthread/changing-environment-variables) |
| `REG_EXPAND_SZ` holds unexpanded references such as `%PATH%` that are expanded with `ExpandEnvironmentStrings`; `REG_SZ` is a plain string | DOCUMENTED | Microsoft Learn, *Registry value types* (https://learn.microsoft.com/windows/win32/sysinfo/registry-value-types) |
| `setx` stops at 1,024 characters when it assigns a value, and expands the references of an existing variable; the values it sets are for future command windows only | DOCUMENTED | Microsoft Learn, *setx* (https://learn.microsoft.com/windows-server/administration/windows-commands/setx) |
| A command line passed to `cmd.exe` is limited to 8,191 characters, so a longer `Path` can break programs that rely on it | DOCUMENTED | Microsoft Learn, *Command-line string limitation* (https://learn.microsoft.com/troubleshoot/windows-client/shell-experience/command-line-string-limitation) |
| `SMTO_ABORTIFHUNG` and a timeout of about 5,000 ms in the broadcast; Explorer rereads the environment on it, so programs started from Explorer afterwards see the change | FOLKLORE | The usual recipe; the pages above do not give the flag or the timeout |
| A new process gets the system variables, then the user ones; the user `Path` is appended to the system `Path`; a user variable with the name of a system one wins | FOLKLORE | Matches observed behaviour, but the pages above do not state the merge order. WinTune shows the user `Path` and the system `Path` as two lists and does not rely on the order |
| Variable names are case-insensitive | FOLKLORE | Registry value names are case-insensitive, so WinTune compares names ignoring case and keeps the stored spelling |
| Some installers write `Path` as `REG_SZ`, which leaves its `%name%` entries unexpanded; Windows' own dialog writes `REG_EXPAND_SZ` when the text holds a reference | FOLKLORE | Reported by users; this is why WinTune upgrades a `REG_SZ` value to `REG_EXPAND_SZ` when its new text holds a reference |
| The 2,047 figure of old dialogs is not an operating-system limit | FOLKLORE | The 32,767 figure is the documented cap |
| Windows' Environment Variables dialog edits `Path` as a list (one entry per row, split on `;`) | FOLKLORE | Not on the Learn pages read; WinTune mirrors the split and join, not the dialog |

Shapes seen on the development PC (Windows 11, build 26300, read-only probe)
(OBSERVED): 7 user variables (`Path`, `TEMP`, `TMP`, `OneDrive`,
`OneDriveCommercial`, and two tool variables, both `REG_SZ`); `TEMP`, `TMP`,
`OneDrive` and `OneDriveCommercial` are `REG_EXPAND_SZ`. The user `Path` is `REG_EXPAND_SZ`, 1,080 characters, 21
entries, 5 of them with a `%VAR%`, 1 entry whose folder is missing, 1 duplicate
(the same folder written once with `%LOCALAPPDATA%` and once literally) and 1
with a trailing backslash. The system `Path` is `REG_EXPAND_SZ`, 578
characters, 17 entries, with no entry repeated in the user `Path`. No value has
an empty name or a `=` in its name. `HKCU\Volatile Environment` holds
`USERNAME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` and the logon variables.

#### Pre-merge checklist (Environment variables)

Not run by the developer: the engine is covered by tests in a sandbox and the
broadcast is never sent from a test, but no variable was changed in WinTune and
then looked at in a new terminal or in Windows' dialog. The owner runs this
before merging, and records each result next to its item. Use a throw-away
variable name such as `WINTUNE_TEST`.

- [ ] 1. Add a variable in WinTune and Apply. Open a new terminal from Start:
  `echo %WINTUNE_TEST%` shows it. Windows' Environment Variables dialog
  (`sysdm.cpl`, Advanced) shows it too. Result: ______
- [ ] 2. Add a folder to `Path` and Apply. A new terminal finds a program in
  that folder (`where <program>`). The entries that hold a `%VAR%` are still
  unexpanded in Windows' dialog, and `Path` is still `REG_EXPAND_SZ`
  (`reg query HKCU\Environment /v Path`). Result: ______
- [ ] 3. Move an entry up and remove another, then Apply. Windows' dialog shows
  the new order. Result: ______
- [ ] 4. In History, Saved originals, Environment variables: **Restore
  original** for `Path`. The confirmation names the entries that will be
  removed and added back, and Windows' dialog then shows the original list.
  Result: ______
- [ ] 5. Delete the test variable in WinTune and Apply. It is gone in a new
  terminal. Result: ______
- [ ] 6. Edit `Path` in WinTune (move an entry) without applying. Then add a
  folder through Windows' Environment Variables dialog, and press Apply in
  WinTune without leaving the page. WinTune refuses and writes nothing (the
  folder is still in Windows' dialog). After the rescan, Apply again: the
  confirmation says another program changed `Path` and names the new folder
  among those that would be removed. Result: ______

Record anything that differs, with the step and the Windows version, before
merging.

## Mouse & keyboard

The Mouse & keyboard module shows and changes 18 settings of the signed-in
user: pointer, buttons and clicks, wheel, keyboard repeat, and window and menu
timing. It writes nothing to the registry. Every change goes through
`SystemParametersInfoW`, which also tells running programs; the values Windows
keeps under `HKCU\Control Panel` are read only, for a `.reg` safety net.

### What WinTune reads

| Source | Use |
| --- | --- |
| `SystemParametersInfoW` GET actions (the Get column of the table below) | The live value of every setting, at each scan, before each Apply (to snapshot it) and right after each SET (the readback) |
| `GetSystemMetrics(SM_SWAPBUTTON)` | The swapped buttons: there is no `SystemParametersInfoW` GET for them |
| `GetDoubleClickTime` | The double-click time: there is no `SystemParametersInfoW` GET for it |
| `SPI_GETFILTERKEYS` | Whether Filter Keys is on, for the note on the Keyboard section. A failed read counts as off. Read only: WinTune never writes Filter Keys |
| `GetUserNameExW` and `WTSQuerySessionInformationW` | The account check: the account WinTune runs as, read from the process token (not from `%USERNAME%`), against the user signed in to the current session. If they differ, or the check fails, the page is replaced by a banner and nothing is applied |
| `HKCU\Control Panel\Mouse`, `HKCU\Control Panel\Desktop` and `HKCU\Control Panel\Keyboard` | The values directly in each key (not subkeys), read only, for the `.reg` net below |

### What WinTune writes

Nothing directly. WinTune never writes under `HKCU\Control Panel`, never writes
`UserPreferencesMask`, and never calls `SPI_SETTHREADLOCALINPUTSETTINGS`.

Each change is one `SystemParametersInfoW` SET from the table below, always
with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`: the first writes the setting to
the user profile, the second broadcasts `WM_SETTINGCHANGE`. Every SET runs on
the job worker thread (the call waits for the broadcast) and is followed at
once by its GET. If Windows kept another value, that setting is reported as
failed and the next one is still tried. `swap_buttons` is always written last.

### The settings

"ui" means the value goes in `uiParam`; "pv" means it goes in `pvParam`, cast
to a pointer, with `uiParam` 0; "pv array" means `pvParam` points to three
integers. Sending a value in the wrong place silently sets 0 or garbage, which
is why each SET is read back. The default column is what the "Windows default"
menu entry applies; DOCUMENTED, OBSERVED and FOLKLORE are explained under
Evidence.

| Id | Section | Get | Set (action, family) | Page control | Windows default |
| --- | --- | --- | --- | --- | --- |
| `pointer_speed` | Pointer | `SPI_GETMOUSESPEED` 0x0070 | 0x0071 pv | slider 1..20 | 10 (DOCUMENTED) |
| `pointer_precision` | Pointer | `SPI_GETMOUSE` 0x0003 (array) | 0x0004 pv array | toggle | on, `{6, 10, 1}` (FOLKLORE) |
| `pointer_trails` | Pointer | `SPI_GETMOUSETRAILS` 0x005E | 0x005D ui | toggle and length 2..7 | off (FOLKLORE) |
| `pointer_sonar` | Pointer | `SPI_GETMOUSESONAR` 0x101C | 0x101D pv | toggle (show location with Ctrl) | off (FOLKLORE) |
| `pointer_vanish` | Pointer | `SPI_GETMOUSEVANISH` 0x1020 | 0x1021 pv | toggle (hide while typing) | on (FOLKLORE) |
| `snap_to_default` | Pointer | `SPI_GETSNAPTODEFBUTTON` 0x005F | 0x0060 ui | toggle | off (FOLKLORE) |
| `swap_buttons` | Buttons and clicks | `GetSystemMetrics(SM_SWAPBUTTON)` | 0x0021 ui | toggle | off (FOLKLORE) |
| `double_click_time` | Buttons and clicks | `GetDoubleClickTime` | 0x0020 ui | slider 200..900 ms, step 10 | 500 ms (DOCUMENTED) |
| `click_lock` | Buttons and clicks | `SPI_GETMOUSECLICKLOCK` 0x101E | 0x101F pv | toggle | off (FOLKLORE) |
| `click_lock_time` | Buttons and clicks | `SPI_GETMOUSECLICKLOCKTIME` 0x2008 | 0x2009 pv | slider 200..2,200 ms, step 100 | 1,200 ms (DOCUMENTED) |
| `wheel_lines` | Wheel | `SPI_GETWHEELSCROLLLINES` 0x0068 | 0x0069 ui | 1..100 lines, or one screen (`WHEEL_PAGESCROLL`, `u32::MAX`) | 3 (DOCUMENTED) |
| `wheel_chars` | Wheel | `SPI_GETWHEELSCROLLCHARS` 0x006C | 0x006D ui | slider 1..100 | 3 (DOCUMENTED) |
| `scroll_inactive` | Wheel | `SPI_GETMOUSEWHEELROUTING` 0x201C | 0x201D pv | toggle (0 off, 2 on) | on, 2 (DOCUMENTED) |
| `key_repeat_delay` | Keyboard | `SPI_GETKEYBOARDDELAY` 0x0016 | 0x0017 ui | slider 0..3, Short to Long | 1 (FOLKLORE) |
| `key_repeat_rate` | Keyboard | `SPI_GETKEYBOARDSPEED` 0x000A | 0x000B ui | slider 0..31, Slow to Fast | 31 (FOLKLORE) |
| `hover_time` | Windows and menus | `SPI_GETMOUSEHOVERTIME` 0x0066 | 0x0067 ui | slider 100..3,000 ms, step 50 | 400 ms (FOLKLORE) |
| `menu_show_delay` | Windows and menus | `SPI_GETMENUSHOWDELAY` 0x006A | 0x006B ui | slider 0..2,000 ms, step 50 | 400 ms (FOLKLORE) |
| `drag_full_windows` | Windows and menus | `SPI_GETDRAGFULLWINDOWS` 0x0026 | 0x0025 ui | toggle | on (FOLKLORE) |

- Apply refuses a value outside the page range, with nothing written. Restore
  original and Windows default are not checked against it. A live value
  outside a range is shown as it is until the control is moved.
- Precision off writes `{T1, T2, 0}` with the thresholds of that moment (the
  whole array is always read and written). On writes the snapshot's original
  array if its acceleration is not 0, else `{T1, T2, 1}` when both thresholds
  are not 0, else `{6, 10, 1}`.
- Scroll inactive windows off writes 0; on writes the snapshot's original when
  not 0, else 2. A live 1 shows as on, and WinTune writes 1 only to restore an
  original 1.
- Pointer trails off writes 0; a live 1 is off too. ClickLock time is disabled
  while ClickLock is off, because it has no effect then.

### The `.reg` net

Each Apply first writes `reg-backups\<stamp>-mouse-keyboard.reg`: the values
directly in `HKCU\Control Panel\Mouse`, `HKCU\Control Panel\Desktop` and
`HKCU\Control Panel\Keyboard`, as single values, never a key delete-and-recreate
(`Desktop` also holds the wallpaper and `WindowMetrics`). It is a last-resort
net, and **WinTune never imports it**. Importing it by hand also puts back
`UserPreferencesMask`, the bitfield that holds ClickLock, Sonar, Vanish and
many unrelated visual effects, so it reverts visual effects changed since; and
the values take effect only at the next sign-in, not in the running session.
Use **Restore original** in History instead.

Before the first change to a setting, its value, read through its GET, is
saved in `mouse-keyboard-backups.json` (in `%ProgramData%\Wintune`, per user
then setting id, as `{ timestamp, value }`; no registry bytes). If saving it
or the `.reg` fails, nothing is applied. **Restore original** sets that value
through the same call, reads it back, and drops the snapshot only if they
match. When an Apply brings a setting back to its original, its snapshot is
dropped, so no Restore original is offered for a state that is already the
original.

### Evidence

| Claim | Status | Source |
| --- | --- | --- |
| `SystemParametersInfoW` has a GET and a SET action for each setting above, except the swapped buttons and the double-click time, which have no GET action | DOCUMENTED | Microsoft Learn, *SystemParametersInfoW function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-systemparametersinfow) |
| `SPIF_UPDATEINIFILE` writes the new setting to the user profile; `SPIF_SENDCHANGE` broadcasts `WM_SETTINGCHANGE` after updating it | DOCUMENTED | Same page, the `fWinIni` parameter |
| Pointer speed is 1 to 20, default 10 | DOCUMENTED | Same page; *mouse_event function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-mouse_event) |
| Pointer precision is the array `{threshold1, threshold2, acceleration}`: acceleration 0 is off, 1 doubles the speed above threshold 1, 2 doubles it again above threshold 2 | DOCUMENTED | *mouse_event function*, remarks; the array shape on the `SystemParametersInfoW` page |
| The swapped buttons are read with `GetSystemMetrics(SM_SWAPBUTTON)`; `SwapMouseButton` affects all apps and is meant for Control Panel-type apps | DOCUMENTED | Microsoft Learn, *SwapMouseButton function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-swapmousebutton) |
| The double-click time is read with `GetDoubleClickTime`; a value above 5,000 ms is clamped to 5,000; `SetDoubleClickTime(0)` restores 500 | DOCUMENTED | Microsoft Learn, *SetDoubleClickTime function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setdoubleclicktime) and *GetDoubleClickTime function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getdoubleclicktime) |
| Wheel lines default to 3; `WHEEL_PAGESCROLL` (`UINT_MAX`) means one screen at a time; programs must read the setting again on `WM_SETTINGCHANGE` | DOCUMENTED | Microsoft Learn, *About Mouse Input*, "Determining the Number of Scroll Lines" (https://learn.microsoft.com/windows/win32/inputdev/about-mouse-input) |
| Keyboard delay is 0 (about 250 ms) to 3 (about 1 s) and rate is 0 (about 2.5 per second) to 31 (about 30 per second), both depending on the hardware; a rate above 31 is clamped | DOCUMENTED | `SystemParametersInfoW` page |
| ClickLock time defaults to 1,200 ms and has an effect only while ClickLock is on (the "only effective if ClickLock is on" part is not stated on the Learn page); hover time is clamped to the timer limits | OBSERVED | recon notes; default 1,200 ms read from the live value |
| `SPI_[GS]ETMOUSEWHEELROUTING` exists: 0 focus, 1 hybrid, 2 mouse position, the default "starting with Windows 10" | DOCUMENTED | The `SystemParametersInfoA` page (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-systemparametersinfoa). The `SystemParametersInfoW` page lists only 0 and 1, so the two pages disagree |
| Filter Keys is on when `FKF_FILTERKEYSON` is set in the `FILTERKEYS` flags | DOCUMENTED | Microsoft Learn, *FILTERKEYS structure* (https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-filterkeys) |
| The Settings toggle for "Scroll inactive windows" is 2 (on) and 0 (off) | FOLKLORE | The recon read 2 on its machine; no Learn page maps the toggle to a value. WinTune writes 0 or 2 and keeps a live 1 as on |
| Windows' "Enhance pointer precision" toggle writes `{6, 10, 1}` and `{0, 0, 0}` | FOLKLORE | Commonly reported, not on Learn. WinTune keeps the current thresholds when turning it off, so a toggle that reads the thresholds could still show "on" (checklist item 1) |
| `SmoothMouseXCurve` and `SmoothMouseYCurve` exist next to the precision values | OBSERVED | Present on the development PC; Learn does not describe them and WinTune never touches them |
| A SET takes its value in `pvParam` cast to a pointer for the "pv" actions above, and in `uiParam` for the "ui" ones | FOLKLORE | Learn's wording is vague for several of them (the `SPI_SETMOUSEWHEELROUTING` text repeats the GET wording); this is Win32 practice, and the readback after each SET catches a wrong choice |
| `HKCU\Control Panel\Mouse`, `\Desktop` and `\Keyboard` hold the values (`MouseSensitivity`, `DoubleClickSpeed`, `WheelScrollLines`, `KeyboardDelay`, `KeyboardSpeed` and so on), mostly `REG_SZ` numbers; `ActiveWindowTracking`, `ClickLockTime`, `MouseWheelRouting` and `CaretWidth` are `REG_DWORD` | OBSERVED | Seen on the development PC (Windows 11, build 26300), where they agreed with the live GETs wherever a named value exists. Learn never says the "user profile" is `HKCU` and names no value, so WinTune only reads them |
| ClickLock, Sonar and Vanish have no value of their own: they live in `Desktop\UserPreferencesMask`, a shared `REG_BINARY` | FOLKLORE | No named value was found for them on the development PC (`UserPreferencesMask` is present); the bit positions are not on Learn. This is why the mask is never edited or restored by bytes |
| Defaults marked FOLKLORE above are those of a new Windows 11 profile | FOLKLORE | Commonly reported. On the development PC "Hide pointer while typing" was on (the recon had guessed off). Optional checklist item 9 checks them on a fresh account |
| A SET can wait on slow windows because of the broadcast | FOLKLORE | Not timed on Learn; this is why every SET runs on the job worker thread |
| A policy could reset a value at the next sign-in, and Filter Keys, when on, can override the key repeat settings | FOLKLORE | Not on the `SystemParametersInfoW` page; this is why each SET is read back and why the Keyboard section shows a note when Filter Keys is on |

Shapes seen on the development PC (Windows 11, build 26300, read-only probe)
(OBSERVED): pointer speed 10, precision `{6, 10, 1}`, swap off, double-click
500 ms, wheel 3 lines and 3 characters, scroll routing 2, repeat delay 1 and
rate 31, snap off, trails 0, Sonar off, Vanish on, ClickLock off with a time
of 1,200 ms, hover and menu delay 400 ms, drag full windows on, Filter Keys off.

#### Pre-merge checklist (Mouse & keyboard)

Not run by the developer: the engine is covered by tests with a fake
`InputApi` and the `.reg` by tests in a registry sandbox, and no test calls a
real SET. Nothing was changed through WinTune and then looked at in Settings
or Notepad. The owner runs this before merging, on a real PC, with Windows
Settings, Notepad and WinTune, and records each result next to its item. Note
the starting values first (or use History's **Restore all** at the end).

- [ ] 1. Change the pointer speed and turn **Enhance pointer precision** off,
  then Apply. Settings, Bluetooth & devices, Mouse shows the same speed and
  precision off. Turn precision back on in WinTune and Apply: Settings shows
  it on. Result: ______
- [ ] 2. Swap the primary button and Apply. The confirmation warns first.
  After Apply the right button is the primary one. Put it back by clicking the
  toggle with the other button. Result: ______
- [ ] 3. Change the double-click speed and Apply. The "Double-click here to
  test" target lights at the speed you set, and a folder in Explorer opens only
  when you double-click within that time. Result: ______
- [ ] 4. Set the repeat delay to Short and the rate to Slow, then Apply.
  Holding a key repeats the same way in WinTune's test field and in Notepad.
  Result: ______
- [ ] 5. Turn **Scroll inactive windows** off and Apply: with Notepad behind
  another window, the wheel no longer scrolls Notepad when the pointer is over
  it. Turn it on again and Apply: it does. Result: ______
- [ ] 6. Turn **Hide pointer while typing** off and **Show location with Ctrl**
  on, then Apply. In Notepad the pointer stays while you type, and pressing Ctrl
  shows the circles. Result: ______
- [ ] 7. Open History, Saved originals, Mouse & keyboard, and use **Restore
  all**. Settings and the WinTune page show the original values, and the group
  is empty. Also look at the History filter row: with the new Mouse & keyboard
  chip the eight chips no longer fit, so the row scrolls horizontally. Scroll
  it with the mouse wheel or by dragging, and check that every chip, including
  the last one, can be reached and clicked, and that none is cut off.
  Result: ______
- [ ] 8. Change two settings, Apply, then sign out and in. WinTune and Settings
  still show them. Restore them afterwards. Result: ______
- [ ] 9. (Optional) On a fresh local account, compare the page's values with the
  Windows default column above and note any that differ (the FOLKLORE
  defaults). Result: ______

Record anything that differs, with the step and the Windows version, before
merging.

## Visual effects

The Visual effects module shows and changes 18 settings of the signed-in user
in six sections: the UI effects master switch, animations, menus and tooltips,
shadows, text (font smoothing and ClearType) and caret and focus. These are the
checkboxes of Windows' Performance Options dialog that Windows exposes through
`SystemParametersInfoW`, plus a few from the ClearType and Accessibility
settings. The module writes nothing to the registry. Every change goes through
`SystemParametersInfoW`, which also tells running programs; the values Windows
keeps under `HKCU\Control Panel` are read only, for a `.reg` safety net.

### What WinTune reads

| Source | Use |
| --- | --- |
| `SystemParametersInfoW` GET actions (the Get column of the table below) | The live value of every setting, at each scan, before each Apply (to snapshot it) and right after each SET (the readback). Window animation is read through an `ANIMATIONINFO` (`cbSize` 8, `uiParam` 8); any non-zero `iMinAnimate` reads as on |
| `GetUserNameExW` and `WTSQuerySessionInformationW` | The account check, as in Mouse & keyboard: the account WinTune runs as, read from the process token, against the user signed in to the current session. If they differ, or the check fails, the page is replaced by a banner and nothing is applied |
| `HKCU\Control Panel\Desktop` and `HKCU\Control Panel\Desktop\WindowMetrics` | The values directly in each key (not other subkeys), read only, for the `.reg` net below |

### What WinTune writes

Nothing directly. WinTune never writes under `HKCU\Control Panel`, never writes
`UserPreferencesMask` (most of these effects are bits in it, with a layout
Microsoft does not document), and never touches `Explorer\Advanced` or `DWM`.

Each change is one `SystemParametersInfoW` SET from the table below, always
with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE`: the first writes the setting to the
user profile, the second broadcasts `WM_SETTINGCHANGE`. Every SET runs on the
job worker thread and is followed at once by its GET. If Windows kept another
value, that setting is reported as failed and the next one is still tried.

The write order is the table order, except for `ui_effects`: it is written
first when it goes on and last when it goes off, so that each effect is written
while the master is on, in case a SET under a disabled master misbehaves.

### The settings

"ui" means the value goes in `uiParam`; "pv" means it goes in `pvParam`, cast
to a pointer, with `uiParam` 0; "ai" means `pvParam` points to an
`ANIMATIONINFO` and `uiParam` is its size, 8 (for the GET too). Sending a value
in the wrong place silently sets 0 or garbage, which is why each SET is read
back. The gate column is what makes a row disabled on the page while it is off
(the gate's pending value, else its live value). The default column is what the
"Windows default" menu entry applies; DOCUMENTED, OBSERVED and FOLKLORE are
explained under Evidence.

| Id | Section | Get | Set (action, family) | Page control | Gate | Windows default |
| --- | --- | --- | --- | --- | --- | --- |
| `ui_effects` | UI effects | `SPI_GETUIEFFECTS` 0x103E | 0x103F pv | toggle | | on (FOLKLORE) |
| `window_animation` | Animations | `SPI_GETANIMATION` 0x0048 (ai) | 0x0049 ai | toggle | | on (FOLKLORE) |
| `client_area_animation` | Animations | `SPI_GETCLIENTAREAANIMATION` 0x1042 | 0x1043 pv | toggle | | on (FOLKLORE) |
| `combobox_animation` | Animations | `SPI_GETCOMBOBOXANIMATION` 0x1004 | 0x1005 pv | toggle | `ui_effects` | on (FOLKLORE) |
| `listbox_smooth_scroll` | Animations | `SPI_GETLISTBOXSMOOTHSCROLLING` 0x1006 | 0x1007 pv | toggle | `ui_effects` | on (FOLKLORE) |
| `menu_animation` | Menus and tooltips | `SPI_GETMENUANIMATION` 0x1002 | 0x1003 pv | toggle | `ui_effects` | on (FOLKLORE) |
| `menu_fade` | Menus and tooltips | `SPI_GETMENUFADE` 0x1012 | 0x1013 pv | Fade / Slide | `menu_animation` | Fade (FOLKLORE) |
| `selection_fade` | Menus and tooltips | `SPI_GETSELECTIONFADE` 0x1014 | 0x1015 pv | toggle | `ui_effects` | on (FOLKLORE) |
| `tooltip_animation` | Menus and tooltips | `SPI_GETTOOLTIPANIMATION` 0x1016 | 0x1017 pv | toggle | `ui_effects` | on (FOLKLORE) |
| `tooltip_fade` | Menus and tooltips | `SPI_GETTOOLTIPFADE` 0x1018 | 0x1019 pv | Fade / Slide | `tooltip_animation` | Fade (FOLKLORE) |
| `cursor_shadow` | Shadows | `SPI_GETCURSORSHADOW` 0x101A | 0x101B pv | toggle | `ui_effects` | on (FOLKLORE) |
| `drop_shadow` | Shadows | `SPI_GETDROPSHADOW` 0x1024 | 0x1025 pv | toggle | | on (FOLKLORE) |
| `font_smoothing` | Text | `SPI_GETFONTSMOOTHING` 0x004A | 0x004B ui | toggle | | on (FOLKLORE) |
| `font_smoothing_type` | Text | `SPI_GETFONTSMOOTHINGTYPE` 0x200A | 0x200B pv | Standard / ClearType | `font_smoothing` | ClearType, 2 (FOLKLORE) |
| `cleartype_contrast` | Text | `SPI_GETFONTSMOOTHINGCONTRAST` 0x200C | 0x200D pv | slider 1,000..2,200, step 100 | smoothing on and type ClearType | 1,400 (DOCUMENTED) |
| `caret_width` | Caret and focus | `SPI_GETCARETWIDTH` 0x2006 | 0x2007 pv | slider 1..20 px | | 1 (DOCUMENTED) |
| `focus_border_width` | Caret and focus | `SPI_GETFOCUSBORDERWIDTH` 0x200E | 0x200F pv | slider 1..10 px | | 1 (FOLKLORE) |
| `focus_border_height` | Caret and focus | `SPI_GETFOCUSBORDERHEIGHT` 0x2010 | 0x2011 pv | slider 1..10 px | | 1 (FOLKLORE) |

Every GET is `SystemParametersInfoW(action, 0, &mut u32, 0)` except
`window_animation`. The two fade rows are flags (TRUE is Fade, FALSE is Slide).
`font_smoothing_type` is a number: 1 Standard, 2 ClearType.

- **Gates.** Master-gated is the kept members of Learn's list of UI effects
  and nothing more. Drop shadow, window animation and client area animation are
  not gated, though FOLKLORE says some are bits under the master too. A row
  whose gate row is itself disabled is disabled too, with the outer reason
  (Menu fade while UI effects is off), and a gate turned off drops the pending
  choices of the rows it disables. A disabled row says why on hover. Gating is
  a page rule: the engine writes what it is given, and a restore from History
  sets a gated id even when its gate is off.
- **Ranges.** Apply refuses a value outside the page range, with nothing
  written (flags always pass; the type is 1 or 2). The slider step is not
  checked. Restore original and Windows default are not checked against the
  range. A live value outside a range or off a step (a smoothing type of 3, or
  a contrast of 1,250) is shown as it is until the control is moved.
- **Window animation.** Off writes `iMinAnimate` 0 and on writes 1; any
  non-zero live value reads as on and is snapshotted as on.
- **Defaults.** "Windows default" is on for every row, the fades as Fade, and
  ClearType for the type (what Windows ships, not the API's own default of
  Standard). The two presets only fill pending choices: **For appearance**
  turns everything on in sections 1 to 4 with the fades as Fade, and **For
  performance** turns every effect in sections 1 to 4 off and also sets the
  master pending on, so the gated rows are enabled and get their pending off.
  Neither touches Text or Caret and focus.

### The master's companions

Windows does not document whether turning `ui_effects` off overwrites each
gated effect or only gates it, nor whether the gated effects' GETs still report
their own value while the master is off (FOLKLORE: it is a separate bit that
gates the others, which keep theirs). The module is safe in every case, so it
does not depend on checklist item 4, which only records which case happens.

The master's **companions** are the 8 rows gated by `ui_effects`, directly or
through another row: `combobox_animation`, `listbox_smooth_scroll`,
`menu_animation`, `menu_fade`, `selection_fade`, `tooltip_animation`,
`tooltip_fade` and `cursor_shadow`. Beside the six rows the master gates
directly, WinTune counts `tooltip_fade` and `menu_fade`, because
`tooltip_fade` is a member of Learn's UI effects list and `menu_fade` is gated
by `menu_animation`, which the master gates, so either can be cleared or
masked along with the master too.
Cost of the two extra rows: one more snapshot and SET each, which change
nothing when the master only gates.

- **Snapshots.** Any Apply or Restore that touches `ui_effects` first reads
  every companion through its GET and records it, before any write, as a
  companion of the master (`companion_of` in the store). A failed GET refuses
  the job. A row that already has its own snapshot keeps it. Except when the
  master is live off and the job leaves it off: then no companion is read or
  snapshotted, since its GET may read a masked off (see "Master staying off").
- **History.** Companions are not History rows of their own and are not counted
  on Home; the master's row says "also restores {0} effects". A companion the
  user changes directly keeps its first-touch original but becomes its own
  History row.
- **Restore order.** Restoring the master restores its companions with it: when
  the restored master is on, the master first, then the companions; when it is
  off, the companions first, the master last.
- **Master from off to on.** A companion's GET may read off while the master is
  off, whatever its stored value, so a snapshot taken then can be wrong. When
  an Apply turns the master from live off to on, or a Restore sets the master
  on before its companions, every companion snapshotted in that job is read
  again once the master's SET and readback succeed, that value replaces the
  snapshot, and the store is saved again before any gated row is written. This
  is the one exception to "every snapshot GET comes before the first write".
  If the re-read or the save fails, no gated row is written and the report says
  so.
- **Master going off.** In a batch that turns the master off, the snapshot of a
  touched companion is kept even when its readback matches its original, as the
  master's SET (written last) may clear it.
- **Master staying off.** No companion is read, snapshotted or written while
  the master is live off and the job leaves it off. In an Apply that does not
  turn the master on, a touched companion gets no snapshot (its GET may read
  masked) and no SET, and is reported held by the master; the page cannot ask
  for this (the row is disabled), the engine refuses it anyway. An Apply that
  sets such a master off again (a page that saw it on) takes no companion
  snapshot either. A Restore that sets such a master off takes no new
  companion snapshot and writes no companion, even one that has a snapshot:
  each is reported held by the master and keeps its snapshot, to be restored
  once the master is on.
- **Master-off restore.** When the master is live on and a Restore sets it
  off, its companions are written first. After the master's SET off, each
  companion written before it is read again: a value still at its original is
  forgotten, any other keeps its snapshot and is reported held by the master
  (never forgotten on a readback taken before the master went off).
- **HeldByMaster.** When a companion's SET is read back while `ui_effects` is
  live off and the value differs, the issue is "Windows keeps this effect off
  while UI effects is off", not a plain mismatch, and the snapshot is kept. If
  that GET of the master fails, WinTune cannot tell the two apart: the issue is
  "unverified" and the snapshot is kept.
- **When the GETs read off.** A master-off restore leaves the companions
  that are on as held rows: WinTune cannot verify them, so it keeps their
  snapshots and shows them. That is safe and visible, and checklist item 4
  decides whether it is the real behaviour.
- **Forget.** A companion is forgotten on its own when it is restored, or when
  the master is back at its original, after a GET of each; one whose live
  value still differs is kept, as its own History row.

### The `.reg` net

Each Apply first writes `reg-backups\<stamp>-visual-effects.reg`: the values
directly in `HKCU\Control Panel\Desktop` and `HKCU\Control Panel\Desktop\WindowMetrics`,
as single values, never a key delete. It is a last-resort net, and **WinTune
never imports it**. Importing it by hand also puts back `UserPreferencesMask`,
the bitfield that holds these effects, so it reverts Mouse & keyboard's
ClickLock, Sonar and Vanish too; it also reverts the wallpaper and the window
metrics (title and menu fonts); and the values take effect only at the next
sign-in, not in the running session. Use **Restore original** in History
instead. A restore writes no `.reg`.

Before the first change to a setting, its value, read through its GET, is saved
in `visual-effects-backups.json` (in `%ProgramData%\Wintune`, per user then
setting id, as `{ timestamp, value }` plus the optional `companion_of`, which
is left out when empty; no registry bytes). If saving it or the `.reg` fails,
nothing is applied. **Restore original** sets that value through the same
call, reads it back, and drops the snapshot only if they match. When an Apply
brings a setting back to its original, its snapshot is dropped, except the
rows the companion rules above keep.

### Evidence

| Claim | Status | Source |
| --- | --- | --- |
| `SystemParametersInfoW` has a GET and a SET action for each setting above | DOCUMENTED | Microsoft Learn, *SystemParametersInfoW function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-systemparametersinfow), the Desktop, Menu, UI effects, Window and Accessibility parameter tables |
| `SPIF_UPDATEINIFILE` writes the setting to the user profile; `SPIF_SENDCHANGE` broadcasts `WM_SETTINGCHANGE` | DOCUMENTED | Same page, the `fWinIni` parameter |
| Minimize and maximize animation uses an `ANIMATIONINFO` with `uiParam` equal to its size; non-zero `iMinAnimate` is on | DOCUMENTED | Same page (Window table); *ANIMATIONINFO structure* (https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-animationinfo) |
| `SPI_SETUIEFFECTS` FALSE disables all UI effects and TRUE enables them; the UI effects list is combo box, cursor shadow, gradient captions, hot tracking, list box smooth scrolling, menu animation, menu underlines, selection fade, tooltip animation and tooltip fade | DOCUMENTED | Same page, the UI effects table. Menu fade and drop shadow are in the Menu and Desktop tables |
| Menu fade is ignored unless menu animation is on; tooltip fade is meaningful only if tooltip animation is on | DOCUMENTED | Same page |
| Font smoothing on or off takes its value in `uiParam`; smoothing type needs smoothing on; contrast needs type ClearType, valid 1,000..2,200, default 1,400 | DOCUMENTED | Same page; *ClearType antialiasing* (https://learn.microsoft.com/windows/win32/gdi/cleartype-antialiasing), whose sample passes TRUE in `uiParam` |
| Caret width defaults to 1, also its minimum | DOCUMENTED | `SystemParametersInfoW` page, the Window table |
| Client area animation exists as its own setting | DOCUMENTED | `SystemParametersInfoW` page, the Accessibility table; *Client-area animation* (https://learn.microsoft.com/windows/win32/winauto/client-area-animation) |
| Gradient captions are not supported on Windows 10 and later | DOCUMENTED | Microsoft Learn, *GetSysColor function* (https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getsyscolor) |
| Fades and shadows need more than 256 colours (always true today) | DOCUMENTED | `SystemParametersInfoW` page, the UI effects table |
| The values land in `HKCU\Control Panel\Desktop` (`CaretWidth`, `FocusBorderWidth`, `FocusBorderHeight`, `FontSmoothing`, `FontSmoothingType`, `UserPreferencesMask`) and `Desktop\WindowMetrics\MinAnimate`, mostly as `REG_SZ` numbers and `REG_DWORD` for the caret, focus border and smoothing type | OBSERVED | Seen on the development PC (Windows 11, build 26300), where they agreed with the live GETs wherever a named value exists. Learn never says the "user profile" is `HKCU` and names no value, so WinTune only reads them |
| `UserPreferencesMask` was `9E1E078012000000`: the byte with 0x80 matches UI effects on, and a clear bit matches cursor shadow off, as the GETs say | OBSERVED | Same machine. The bit layout is FOLKLORE (not on Learn), which is why the mask is never edited or restored by bytes |
| The live ClearType contrast was 1,200 while `FontSmoothingGamma` read 0, so the registry does not hold the live number | OBSERVED | Same machine. That 0 means "use the tuner's value" is FOLKLORE |
| Seen on the development PC: UI effects, window, client area, combo box, list box, menu, menu fade, selection fade, tooltip, tooltip fade and drop shadow on; cursor shadow off; smoothing on, type 2 (ClearType); caret 1, focus border 1 by 1; `VisualFXSetting` absent | OBSERVED | Read-only probe with `fWinIni` 0, no SET called |
| Whether the master overwrites each gated effect or only gates it, and whether a gated GET still reports its own value while the master is off | FOLKLORE | Not on Learn; commonly reported that the master is a separate bit (bit 31 of `UserPreferencesMask`) that gates the others. This is why the companion rules exist and why checklist item 4 records the real behaviour |
| Which Performance Options checkbox maps to which action, notably "Animate controls and elements inside windows" is client area animation and "Show shadows under windows" is drop shadow | FOLKLORE | No Learn page maps them. Learn ties drop shadow to `CS_DROPSHADOW` windows, menus and tooltips |
| Settings, Accessibility, Visual effects, "Animation effects" may flip several rows at once; the dialog may show "Custom" after a change | FOLKLORE | Commonly reported |
| Windows ships ClearType as the smoothing type, while the API default is Standard | FOLKLORE | Learn gives Standard as the API default; the shipped value is commonly reported |
| Every default marked FOLKLORE above is that of a new Windows 11 profile; the fades default to Fade and the focus border to 1 | FOLKLORE | Commonly reported, not on Learn |
| A SET takes its value in `pvParam` cast to a pointer for the "pv" actions and in `uiParam` for the "ui" ones | FOLKLORE | Learn's wording is vague for several; this is Win32 practice and the readback after each SET catches a wrong choice |
| Caret width 1..20 matches Settings, Accessibility, Text cursor | FOLKLORE | Not on Learn; the focus border range 1..10 is chosen, not documented |
| Some programs read these settings only when they start | FOLKLORE | Not on Learn; this is why the page says some programs show a change only after a restart |

### Not handled here

None of these has a `SystemParametersInfo` action (Explorer or DWM read them,
often only at a restart), and each would need its own registry evidence:

| Item | Where it lives | Status |
| --- | --- | --- |
| Show thumbnails instead of icons | `Explorer\Advanced\IconsOnly` | FOLKLORE |
| Show translucent selection rectangle | `Explorer\Advanced\ListviewAlphaSelect` | FOLKLORE |
| Drop shadows for icon labels on the desktop | `Explorer\Advanced\ListviewShadow` | FOLKLORE |
| Animations in the taskbar | `Explorer\Advanced\TaskbarAnimations` | FOLKLORE |
| Enable Peek | `Software\Microsoft\Windows\DWM\EnableAeroPeek` | FOLKLORE |
| Save taskbar thumbnail previews | `DWM\AlwaysHibernateThumbnails` | FOLKLORE |
| The dialog's radio choice | `Explorer\VisualEffects\VisualFXSetting` | FOLKLORE |
| Transparency effects | `Themes\Personalize\EnableTransparency` | FOLKLORE |

Settings that have an action pair but are left out: gradient captions
(`SPI_[GS]ETGRADIENTCAPTIONS`, ignored by Windows 10 and later, DOCUMENTED),
hot tracking (`SPI_[GS]ETHOTTRACKING`, not in the dialog and its effect under
visual styles is unclear, FOLKLORE), `SPI_[GS]ETCLEARTYPE` (a second ClearType
switch; Learn documents smoothing plus smoothing type, DOCUMENTED), font
smoothing orientation (`SPI_[GS]ETFONTSMOOTHINGORIENTATION`, RGB or BGR
per panel, the ClearType Text Tuner's job; the pair is DOCUMENTED, RGB was
OBSERVED on the development PC), menu underlines
(`SPI_[GS]ETMENUUNDERLINES`, keyboard cues for accessibility, not an
effect; DOCUMENTED, off on the development PC, OBSERVED), "disable
overlapped content" (`SPI_[GS]ETDISABLEOVERLAPPEDCONTENT`, a low-vision
accessibility setting that is not in the dialog; DOCUMENTED, off on the
development PC, OBSERVED) and "Show window contents while dragging" (`SPI_[GS]ETDRAGFULLWINDOWS`, owned
by Mouse & keyboard as `drag_full_windows`; this page links there).

#### Pre-merge checklist (Visual effects)

Not run by the developer: the engine is covered by tests with a fake
`EffectsApi` and the `.reg` by tests in a registry sandbox, and no test calls a
real SET. Nothing was changed through WinTune and then looked at in Performance
Options, Registry Editor or Notepad. The owner runs this before merging, on a
real PC, and records each result next to its item. Note the starting values
first (or use History's **Restore all** at the end).

- [ ] 1. Turn "Animate windows when minimizing and maximizing" off, Apply:
  minimizing Notepad no longer animates, and Performance Options shows the box
  cleared. Turn it on again. Result: ______
- [ ] 2. In a classic menu bar (Registry Editor), menu animation on with Slide,
  Apply: menus slide. Fade: they fade. Menu animation off: they just appear.
  Result: ______
- [ ] 3. Turn "Show shadows under mouse pointer" on, Apply: the pointer has a
  shadow and Performance Options shows it checked. Result: ______
- [ ] 4. Note the gated rows, turn UI effects off, Apply: Registry Editor menus
  no longer animate. After the rescan, record which case happens: the gated rows
  keep their values (the master only gates), or they read off (either cleared:
  the master stored off for them; or reads off: their GETs report off while
  the master is off). Then restore UI effects from History: menus
  animate again and the gated rows show their noted values. What each outcome
  means:
  - The gated rows keep their values (the master only gates): nothing special;
    the extra companion snapshots were harmless, and the restore only brings the
    master back.
  - The rows are cleared: the master's restore brings the effects back
    from its companions; the menus animating again and the gated rows showing
    their noted values is the proof.
  - The rows read off (their GETs report off): a write is reported as "Windows keeps this
    effect off while UI effects is off", and nothing is lost, but a master-off
    restore leaves the companions that are on as held rows (WinTune cannot
    verify them). If rows remain listed under History after the restore, a
    follow-up is needed to model that case; record which rows remained.

  Result: ______
- [ ] 5. Set smoothing type Standard, Apply: Magnifier shows grey text edges, no
  colour fringes. Back to ClearType, contrast 2,200: text is darker. Restore
  both from the rows. Result: ______
- [ ] 6. Caret width 5 and focus border 3 x 3, Apply: Notepad's caret is thick,
  Settings > Accessibility > Text cursor shows 5, and Tab in the Run dialog
  (Win+R) shows a thick focus rectangle. Result: ______
- [ ] 7. For performance, Apply; then History, Saved originals, Visual effects,
  Restore all: the page and Performance Options show the originals; the group is
  empty. Result: ______
- [ ] 8. Change two settings, Apply, sign out and in: WinTune and Performance
  Options still show them. Restore them. Result: ______

Record anything that differs, with the step and the Windows version, before
merging.

## Sources

- Microsoft Learn, *SystemParametersInfoW function* and *SystemParametersInfoA
  function*, *mouse_event function*, *SwapMouseButton function*,
  *SetDoubleClickTime function*, *GetDoubleClickTime function*, *About Mouse
  Input* and *FILTERKEYS structure*: the actions, the flags, the ranges and
  the defaults. Where the profile lives, the `HKCU\Control Panel` value names,
  the bit positions of ClickLock, Sonar and Vanish, the precision toggle's
  values and the default of several settings are not documented there.
- Microsoft Learn, *SystemParametersInfoW function*, *ANIMATIONINFO structure*,
  *ClearType antialiasing*, *Client-area animation* and *GetSysColor function*:
  the Visual effects actions, the three SET families, the master switch and its
  list of effects, the pairs, the ranges and the defaults of caret width and
  contrast. Which checkbox of Performance Options is which action, what the
  master does to the gated effects, the shipped defaults and the registry
  layout are not documented there.
- Microsoft Learn, *Environment Variables*, *Changing Environment Variables*,
  *Win32_Environment class*, *WM_SETTINGCHANGE message*, *Registry value types*,
  *setx* and *Command-line string limitation*: where the variables live, the
  types, the broadcast, the limits and why `setx` is not used. The merge
  order, case-insensitive names and the broadcast flags are not documented
  there.
- Microsoft Learn, *Startup apps* and *Run and RunOnce registry keys*: the
  Run keys, the Startup folders and Task Manager's role. The `StartupApproved`
  switch is undocumented (flagged "Unofficial" in the app).
- Microsoft Learn, *Creating Shortcut Menu Handlers* and *Merged View of
  HKEY_CLASSES_ROOT*: verb keys, `MUIVerb`, `Icon`, `ProgrammaticAccessOnly`,
  and why HKCU entries override HKLM ones in `HKCR`. The `Blocked` key is
  undocumented.
- Microsoft Learn, *Reference for Windows 11 settings* and *Reference for
  Windows 11 and Windows 10 settings*: `TaskbarAl`, `ShowTaskViewButton`,
  `Start_IrisRecommendations`, `Start_Layout`, `ShowRecentList`,
  `ShowAllPinsList`, `Start_TrackDocs`, the File Explorer settings (compact
  view, check boxes, recent and frequent items, full path, hidden files,
  extensions).
- Microsoft Learn, *Manage connections from Windows operating system
  components to Microsoft services*: `Start_TrackProgs`.
- Long-standing Explorer values that Microsoft does not document one by one:
  `HideFileExt`, `Hidden`, `ShowSuperHidden`, `LaunchTo`, `FullPath`,
  `UseCompactMode`, `AutoCheckSelect`, `ShowRecent`, `ShowFrequent`,
  `SearchboxTaskbarMode`.
- Undocumented: the classic context menu CLSID (flagged "Unofficial" in the
  app).

## Needs verification on a real machine

These could not be checked without running WinTune on real Windows 10 and
Windows 11 installs:

- Every tweak's visible effect and its "Visible" column (new window, Explorer
  restart, instantly).
- `SearchboxTaskbarMode` = 1 shows the search icon on both Windows 10 and 11.
- `LaunchTo` = 2 opens Home (Windows 11) / Quick access (Windows 10).
- `classic_context_menu` on the latest Windows 11 builds.
- That `TaskbarAl` and `ShowTaskViewButton` are not blocked on 24H2 (the
  Widgets value `TaskbarDa` is, and was left out for that reason).

Considered and **not** added, because they could not be confirmed in
Microsoft documentation: seconds in the taskbar clock
(`ShowSecondsInSystemClock`), "End task" in the taskbar menu
(`TaskbarDeveloperSettings\TaskbarEndTask`), the per-user advertising ID and
tailored-experiences values, and the `ContentDeliveryManager` tips. Anything
under `Software\Policies` or HKLM is also out of scope for the tweak modules.
