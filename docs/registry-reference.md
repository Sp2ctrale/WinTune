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

## Sources

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
