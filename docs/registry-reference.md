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

Record anything that differs, with the step and the Windows version, before
merging.

## Sources

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
