# WinTune

WinTune is a Windows 10/11 desktop app for customizing the system. Every change it
makes can be undone. It has two modules:

- **File associations**: custom icons, the double-click action (open in your
  editor or run the script), right-click verbs, and Explorer preview metadata.
  It works on the 70 or so developer file types configured by default, and on
  any extension you add.
- **Explorer & Taskbar**: per-user settings such as showing file extensions or
  hidden files, the classic right-click menu on Windows 11, and aligning the
  taskbar to the left.

Built with Rust and [egui](https://github.com/emilk/egui).

## Requirements

- Windows 10 or 11
- Administrator rights. The binary asks for elevation (UAC) because file
  associations are written machine-wide under `HKLM`.
- To build: Rust 1.88 or newer, plus the MSVC toolchain and Windows SDK, which
  provide `rc.exe` for embedding the icon and manifest.

## Build, test, run

```bash
cargo build --release       # target/release/WinTune.exe
cargo test                  # works from a non-elevated terminal
cargo clippy --all-targets -- -D warnings
```

The tests that touch the registry run in a throw-away sandbox under
`HKCU\Software\WinTuneTest`, which is removed after each test. They never touch
`HKLM`, and they do not need elevation, because the administrator manifest is
embedded only into the `WinTune` binary and not into the test harness.

## What it changes

For each enabled rule (for example `.rs` → `CustomRustFile`), WinTune:

| Key | Change |
| --- | --- |
| `HKLM\SOFTWARE\Classes\<ProgID>` | Created with a description, `DefaultIcon`, `shell\open\command`, an optional `runas` verb and optional custom verbs |
| `HKLM\SOFTWARE\Classes\<ext>` | Default value set to the ProgID, plus `PerceivedType` and `Content Type`. The previous ProgID is kept in `OpenWithProgids` |
| `HKU\<SID>\Software\Classes\<ext>` | Optionally deleted (per-user override) |
| `HKU\<SID>\…\Explorer\FileExts\<ext>` | Optionally deleted (Explorer `UserChoice`) |

Then it calls `SHChangeNotify` so Explorer refreshes its icons.

The **Explorer & Taskbar** tweaks change only the current user's settings
(HKCU). Unless noted, each one is a single REG_DWORD value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced`:

| Tweak | Value | On | Off (Windows default) | Visible |
| --- | --- | --- | --- | --- |
| Show file extensions | `HideFileExt` | 0 | 1 | new windows |
| Show hidden files | `Hidden` | 1 | 2 | new windows |
| Show protected system files | `ShowSuperHidden` | 1 | 0 | new windows |
| Open Explorer to This PC | `LaunchTo` | 1 | 2 | new windows |
| Full path in the title bar | `…\Explorer\CabinetState` → `FullPath` | 1 | 0 | Explorer restart |
| Classic right-click menu (Windows 11, unofficial) | key `HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32` | key present | key removed | Explorer restart |
| Align taskbar to the left (Windows 11) | `TaskbarAl` | 0 | 1 | instantly |
| Hide the Task View button | `ShowTaskViewButton` | 0 | 1 | instantly |
| Hide the taskbar search | `…\CurrentVersion\Search` → `SearchboxTaskbarMode` | 0 | 1 (search icon) | instantly |

When you turn a tweak off, WinTune writes the Windows default shown in the
table. **Restore original** instead puts back exactly what was there before
WinTune first changed the setting, including "not set". After every change,
WinTune broadcasts `WM_SETTINGCHANGE` so the taskbar picks it up.

HKCU belongs to one account. If WinTune runs elevated under a different
administrator account, the tweaks change that account's settings, not yours.
The tab header shows which account is affected.

## Safety and undo

Before anything is written:

1. Every rule is validated: the extension must look like `.xyz` and the ProgID
   must contain no path characters.
2. The original state of every key that will change is snapshotted without loss:
   every value type, including `REG_NONE` and `REG_EXPAND_SZ` default values,
   and every subkey. The snapshot is taken the first time WinTune touches an
   extension, and the first time it touches each ProgID and each user hive.
   Snapshots are saved to `backups.json` before the registry is modified.
3. The current state is exported to a timestamped `.reg` file. Double-clicking
   that file in Explorer restores it, even without WinTune.

If saving the snapshots or the `.reg` file fails, nothing is applied. If
`backups.json` cannot be read, all registry changes are blocked, so the next
save cannot overwrite your snapshots.

Each rule shows its live status: **Applied** means the registry matches the
rule. **Changed** means the rule was applied but something differs now, for
example another app took the extension, an Explorer `UserChoice` override
exists, a ProgID is missing, or the rule was edited since; hover the badge to
see why. The status is read in the background at startup, after each apply or
rollback, when you open the tab, and when you click **Refresh status**.

The interface checks rules as you edit them. A rule with an invalid extension
or ProgID, or an extension listed twice, is flagged in red and blocks Apply.
Disabled rules never block. Removing a rule and rolling back extensions both
ask for confirmation first.

The **Selective Rollback** tab restores chosen extensions to their exact
pre-WinTune state. It also removes every ProgID that WinTune created, including
ones you renamed since. A snapshot is deleted only once it has been fully
restored; if the restore partly fails, the snapshot is kept so you can retry.

## Data files

Everything lives in `%ProgramData%\Wintune`:

| File | Content |
| --- | --- |
| `config.json` | Rules, editor path, folders, theme |
| `backups.json` | Original-state snapshots of file associations, used by the rollback |
| `tweaks-backups.json` | Original state of each Explorer & Taskbar tweak, per user |
| `reg-backups\*.reg` | `.reg` safety net written before each apply |
| `gallery.pak` | Optional compressed icon pack, built from the icon folder |

Icons used by associations are copied to `%ProgramData%\ico`.

## Code layout

| Module | Role |
| --- | --- |
| `regsnap` | Generic snapshot, restore, delete-tree and `.reg` export. Every module that writes to the registry builds on it |
| `associations` | File-association engine (apply, rollback and live status) on an injectable `AssocTarget` |
| `tweaks` | Explorer & Taskbar tweaks engine (catalog, status, apply, restore) on an injectable `TweakTarget` |
| `persist` | JSON stores: a missing file is empty, a corrupt file is an error, saves are atomic |
| `models` | Config and backup data types, plus the default rules |
| `gallery` | Shareable icon library (folder plus in-memory `.pak`), background thumbnail decoding, and the pack builder |
| `tasks` | Background jobs with progress reporting and panic capture |
| `detector` | VS Code, WSL and user-hive detection |
| `clock` | Local timestamps |
| `theme` | Fluent-style dark and light theme |
| `ui` | egui interface, one file per tab (`associations_tab`, `tweaks_tab`, `gallery_tab`, `rollback_tab`) plus `sidebar`, `status_bar`, `confirm`, `rule_status` and `jobs` |
| `main.rs` | Window setup only |

Apply, rollback and pack building run on a background thread, and the status
bar shows their progress. While one of them runs, the other buttons that
change the registry or the pack are disabled. If you close the window
mid-operation, WinTune waits for the operation to finish before closing.

### Adding a customization module

1. Put the engine in its own library module, on top of `regsnap`, with an
   injectable target like `AssocTarget` so it can be tested in the HKCU
   sandbox.
2. Add a variant to `ui::ActiveTab` (title and sidebar subtitle) and a
   `show_*_tab` method in a new `src/ui/<module>_tab.rs` file.
3. Run long operations through `ui::jobs`.

## Roadmap

- **Phase 2 (done)**: UI split into modules; apply, rollback, pack build and
  icon decoding moved off the UI thread, with progress.
- **Phase 3 (in progress)**: done so far are each rule's live status,
  validation in the UI and confirmations for destructive actions. Still to do:
  a diff preview before applying, an operation log, and config import/export.
- **Customization modules**: Explorer & Taskbar is the second module. More can
  follow, built on the same snapshot and rollback engine.
- **Phase 4**: installer, code signing and release workflow.
