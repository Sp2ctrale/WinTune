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
| `open_in_terminal` | Clsid `{9F156763-7844-4DC4-B2B1-901F640F5155}` | Folder, Background | Any | Terminal installed | none | Not a classic verb. Terminal's AppxManifest declares `desktop5:Verb Id="OpenTerminalHere"` on `Directory` and `Directory\Background`. |
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

These cannot be seen through `Verbs()` and need a look at the real menu:

- Windows entries (catalog above): each Clsid row hides in the open Explorer,
  or only after an Explorer restart.
- "Open in Terminal" and "Share" are modern entries: check that `Blocked`
  hides them in the new compact menu.
- The two Properties tabs: blocking "Give access to" removes the Sharing tab,
  and blocking "Restore previous versions" removes the Previous Versions tab.
- Every row on Windows 10, where only the classic menu exists.

- Where the entries land on Windows 11: only under "Show more options" (the
  classic menu) or also in the new compact menu.
- Modern entries (for example "Open in Terminal") and whether `Blocked`
  hides them.
- Side effects of `Blocked` on Properties tabs (a blocked handler can also
  remove its tab or overlay).

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

## Sources

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
