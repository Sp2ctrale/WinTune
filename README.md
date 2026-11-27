# WinTune

WinTune is a Windows 10/11 desktop app for customizing the system. Every change it
makes can be undone. It has six modules:

- **File associations**: custom icons, the double-click action (open in your
  editor or run the script), right-click verbs, and Explorer preview metadata.
  It works on the 70 or so developer file types configured by default, and on
  any extension you add.
- **Explorer & Taskbar**: per-user settings such as showing file extensions or
  hidden files, the classic right-click menu on Windows 11, and aligning the
  taskbar to the left.
- **Start & Privacy**: per-user settings for the Start menu (recommendations,
  layout, recently added apps) and for what Windows tracks (app launches,
  recent files).
- **Context menu**: your own entries in Explorer's right-click menu (open a
  folder in VS Code, Windows Terminal, PowerShell, Command Prompt or any
  program), per Windows account. It can also hide or rename entries that are
  already there.
- **Startup apps**: the programs that start when you sign in (Run keys and
  Startup folders), each with a toggle to turn it off or on for your account,
  using the same switch as Task Manager.
- **Environment variables**: your user variables and your `Path` as a list
  (add, edit, remove, reorder, with missing and duplicate folders flagged),
  with the system variables shown read-only. Writes only `HKCU\Environment`
  and tells running programs about the change.

Built with Rust and [egui](https://github.com/emilk/egui). The interface is
available in English and Français, and the theme follows Windows (or can be
forced light or dark).

- [docs/registry-reference.md](docs/registry-reference.md): every key and
  value WinTune can write, how each change is undone, and the values that
  still need checking on a real machine.
- [ARCHITECTURE.md](ARCHITECTURE.md): layers, stores, safety model and UI.
- [CONTRIBUTING.md](CONTRIBUTING.md): checks, the sandbox rule for tests, and
  how to add a tweak or a module.

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
`HKCU\Software\WinTuneTest`, which is removed after each test (the empty
`WinTuneTest` key itself stays, so parallel tests never race on it). They never touch
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
| Compact view (Windows 11) | `UseCompactMode` | 1 | 0 | new windows |
| Item check boxes | `AutoCheckSelect` | 1 | 0 | new windows |
| Hide recent files in Quick access | `…\CurrentVersion\Explorer` → `ShowRecent` | 0 | 1 | new windows |
| Hide frequent folders in Quick access | `…\CurrentVersion\Explorer` → `ShowFrequent` | 0 | 1 | new windows |
| Classic right-click menu (Windows 11, unofficial) | key `HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32` | key present | key removed | Explorer restart |
| Align taskbar to the left (Windows 11) | `TaskbarAl` | 0 | 1 | instantly |
| Hide the Task View button | `ShowTaskViewButton` | 0 | 1 | instantly |
| Hide the taskbar search | `…\CurrentVersion\Search` → `SearchboxTaskbarMode` | 0 | 1 (search icon) | instantly |

The **Start & Privacy** tweaks work the same way. Each one is a single
REG_DWORD value, either under `…\CurrentVersion\Explorer\Advanced` or, where
noted, under `…\CurrentVersion\Start`:

| Tweak | Value | On | Off (Windows default) |
| --- | --- | --- | --- |
| Hide recommendations in Start (Windows 11) | `Start_IrisRecommendations` | 0 | 1 |
| More pins in Start (Windows 11) | `Start_Layout` | 1 | 0 |
| Hide recently added apps (Windows 11) | `…\Start` → `ShowRecentList` | 0 | 1 |
| Show all pins by default (Windows 11) | `…\Start` → `ShowAllPinsList` | 1 | 0 |
| Don't track app launches | `Start_TrackProgs` | 0 | 1 |
| Don't show recent files | `Start_TrackDocs` | 0 | 1 |

These values are documented in Microsoft's Windows settings reference.
Settings that exist only as Group Policy (`HKCU\Software\Policies`) or under
`HKLM` are deliberately left out.

When you turn a tweak off, WinTune writes the Windows default shown in the
table. **Restore original** instead puts back exactly what was there before
WinTune first changed the setting, including "not set". After every change,
WinTune broadcasts `WM_SETTINGCHANGE` so the taskbar picks it up.

HKCU belongs to one account. If WinTune runs elevated under a different
administrator account, the tweaks change that account's settings, not yours.
The page subtitle shows which account is affected.

## Context menu

The **Context menu** page ("My entries") adds entries to Explorer's right-click
menu. Each entry has a label, a program, an optional icon and the places where
it appears: folders, the folder background, drives and files. The program is
VS Code, Windows Terminal, PowerShell, Command Prompt, or any `.exe` you pick.
Windows Terminal, PowerShell and Command Prompt start in a folder, so they
cannot be used on files.

- **Per account.** Entries are written under
  `HKCU\Software\Classes\<Directory|Directory\Background|Drive|*>\shell\WinTune.<id>`
  (no administrator rights needed for that part), and the list is kept in
  `config.json` for the signed-in account only. Another account sees none of
  them.
- **Apply makes the registry match the list.** Entries you removed, or places
  you unticked, are deleted, after their original state is saved. A pill on
  each entry says whether it is applied, not applied or out of date.
- **Windows 11.** The entries appear under **Show more options** (the classic
  menu). The page links to the **Classic right-click menu** tweak of
  Explorer & Taskbar, which makes that menu the default.
- **Folders with unusual names.** A drive root ends in a backslash, which
  would swallow the closing quote, so VS Code, Windows Terminal and custom
  programs get it as `C:\.` on **Drives** and on the **folder background**
  (Explorer also shows the background menu on a drive root); folders and files
  are passed as they are. PowerShell and Command Prompt are started through
  `cmd.exe` so apostrophes in folder names and network folders work; their
  known limit is that `cmd` expands `%VAR%` text inside a folder name. A
  semicolon in a folder name can break Windows Terminal. Labels cannot start
  with `@`, and a custom program must be an `.exe`.
- **Undo.** In **History**, **Saved originals** has a **Context menu** group
  that puts the registry keys back as they were before WinTune first changed
  them. Every apply and restore is logged under "Context menu".

### Entries already in the menu

Under "My entries", two more sections list what the menu already holds.

- **Windows.** Eleven known Windows entries, such as Give access to, Share,
  Include in library, Send to and Open in Terminal, shown when they exist on
  this PC. Each has a short description and, where it applies, a line saying
  what else goes with it (for example "Also removes the Sharing tab...").
- **Other programs.** Entries added by installed programs (verbs and
  shell extensions). Each row names the program that added it. A search box
  (Ctrl+F) filters by name or program. Entries that live in the Windows folder,
  entries whose program file cannot be found, and verbs the installer already
  hides are not listed (unless WinTune hid them). Drivers, such as a graphics
  card's control panel, are the exception: they install into the Windows
  driver store, and their entries are listed when the file names another
  vendor than Microsoft Windows.
- **Hide and show.** Each row has a toggle. Changes stay pending until you
  press Apply (Discard drops them). If Apply fails, the changes stay pending
  so you can try again. Hiding is per account: nothing is written
  under `HKLM`, and another account still sees the entry. A pill shows what
  will change.
- **Unofficial.** Shell extensions and modern entries are hidden through a
  registry key Microsoft does not document, so their rows carry an
  **Unofficial** pill and say "May need an Explorer restart to show". An entry
  blocked for every account on this PC is shown locked, with its toggle
  disabled.
- **Rename.** The `...` menu has **Rename...** and **Reset name** for classic
  entries of other programs only (not for Windows entries or shell
  extensions). Names are 1 to 80 characters and cannot start with `@`.
- **Side effects.** Before Apply, a confirmation lists what else a hide
  takes away, such as the Sharing or Previous Versions tab or Pin to Start on
  programs for the Windows entries,
  and, for a program's shell extension, that it is hidden everywhere the
  program adds it.
- **Undo.** Hidden and renamed entries appear by name in **History**, **Saved
  originals**, **Context menu**, with **Restore original**. Restore original
  puts back only what WinTune changed (the hidden flag and the name); anything
  the program wrote since, such as a new command or icon, is kept. An entry
  shown again under its own name is already back to its original and leaves
  the list. The Home page counts hidden entries.

## Startup apps

The **Startup apps** page lists the classic startup programs Task Manager
reads: the `Run` keys (yours, every account's, and the 32-bit one for every
account) and the Startup folders (yours and every account's). Each row shows
the app's name, its product name, the command or file, and a pill when it
applies to every account. RunOnce keys, Store (packaged) startup tasks,
scheduled tasks and services are not listed.

- **Turn off and on, per account.** Each row has a toggle. Changes stay
  pending until you press Apply (Discard drops them). WinTune writes the same
  `StartupApproved` switch as Task Manager, under `HKCU` only: it never edits
  a `Run` value, never touches a file in a Startup folder and never writes
  under `HKLM`, so an all-users app is turned off for your account alone. No
  confirmation is asked, because Apply is not destructive and History can
  undo it.
- **Takes effect at next sign-in.** The page says so; nothing starts or
  stops while you are signed in.
- **States.** A row is **On**, **Turned off by WinTune** (WinTune holds the
  original switch, so Restore original is offered) or **Turned off by another
  program** (Task Manager, Settings or an installer turned it off). Task
  Manager shows the same state as WinTune.
- **Unofficial.** Microsoft does not document the switch, so the page carries
  an **Unofficial** pill. The format was read from a real Windows 11 PC; see
  [docs/registry-reference.md](docs/registry-reference.md#startup-apps).
- **Locked rows.** A row is shown with its toggle disabled, and says why, when
  its switch cannot be read, is stored in a way WinTune does not know, is
  turned off for every account in `HKLM`, or belongs to an all-users source
  where a per-user switch has not been shown to work (the sources whose flag
  in `PER_USER_SWITCH_WORKS` is `false`).
- **Program not found.** A pill appears when a Run command names a program
  with a folder path and that file does not exist (a leftover from an
  uninstalled app). Bare names such as `ms-teams.exe` are never flagged.
- **Undo.** In **History**, **Saved originals** has a **Startup apps** group
  that puts each switch back exactly as it was, or removes it if there was
  none. Every apply and restore is logged under "Startup apps". The Home tile
  reads "{n} apps, {m} turned off".

## Environment variables

The **Environment variables** page edits the variables of your account, the
ones Windows' own Environment Variables dialog calls "User variables". It has
three sections.

- **Your Path.** The user `Path` as a list, one row per entry, as written
  (`%USERPROFILE%\...` stays unexpanded; hover a row to see it expanded). Each
  row has a `...` menu with Move up, Move down, Edit and Remove, and an **Add a
  folder** field with a Browse button. A **Folder not found** pill marks an
  entry whose local folder is missing, and a **Duplicate** pill marks a later
  copy of a folder already listed earlier in your `Path` or anywhere in the
  system `Path` (case and a trailing backslash are ignored). **Remove missing
  and duplicates** turns those entries into pending removals. Removed entries
  stay listed as "Will be removed" until you Apply or Discard.
- **Your variables.** Every other user variable, by name, with a **Uses %...%**
  pill when it holds references. Add, Edit (name and value in one dialog),
  Delete and Restore original. `Path` cannot be added or renamed here: it has
  its own section.
- **For every account (read only).** The system variables, collapsed by
  default, with the system `Path` as a list. WinTune never writes them: they
  are in `HKLM`.

Ctrl+F filters both sections by name, value or entry.

- **Pending changes.** Changes stay pending until you press Apply (Discard
  drops them), and the action bar counts them ("N changes not applied"; a
  `Path` list edit counts as one). Apply asks first only when it deletes a
  variable, removes a `Path` entry, or carries a warning (deleting or renaming
  `Path`, `TEMP`, `TMP` or a `OneDrive*` variable, a `Path` over 8,191
  characters, or a `Path` another program changed since you started editing). A
  plain add or edit applies at once, since History can undo it. If anything is
  refused or fails, the changes stay pending.
- **Types and references are kept.** WinTune writes the registry directly, so
  an entry with `%VAR%` stays unexpanded and a value keeps its type
  (`REG_SZ` or `REG_EXPAND_SZ`). A `REG_SZ` value whose new text holds a
  `%VAR%` becomes `REG_EXPAND_SZ`, so the reference is expanded when Windows
  builds the environment. A new `Path` is always `REG_EXPAND_SZ`.
- **Why not `setx`.** `setx` stops a value at 1,024 characters and writes the
  references already expanded, so editing a long `Path` with it loses entries.
  WinTune never runs it.
- **Checks.** A name cannot be empty, contain `=` or exceed 255 characters, and
  is unique ignoring case; a value cannot be empty or exceed 32,767 characters
  (Delete removes a variable); a `Path` entry cannot contain `;`. A problem
  refuses the whole Apply, with nothing written. Values that are not text are
  shown read-only.
- **New programs see the change, open terminals do not.** After a write,
  WinTune broadcasts `WM_SETTINGCHANGE` ("Environment"), so programs you start
  from Explorer afterwards see the new values. A terminal, an editor or a
  service that is already running keeps its old environment until it is
  restarted. The page says so.
- **Undo.** In **History**, **Saved originals** has an **Environment
  variables** group, with a row per variable. **Restore original** puts back
  the exact original value and type, or removes the variable if it did not
  exist, then broadcasts again. A rename is restored as one: the old name comes
  back and the new one goes, from either row. Restoring `Path` asks first and
  lists the entries it will remove (added since) and add back. Every apply and
  restore is logged under "Environment variables". The Home tile reads "{n}
  variables, {m} folders in Path".
- **Not covered.** System variables, `HKCU\Volatile Environment` (Windows
  rebuilds it at each sign-in), and the environment of programs that are
  already running.

Details and the evidence for each claim are in
[docs/registry-reference.md](docs/registry-reference.md#environment-variables).

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

Each rule shows its live status: **Up to date** means the registry matches the
rule. **Changed** means the rule was applied but something differs now, for
example another app took the extension, an Explorer `UserChoice` override
exists, a ProgID is missing, or the rule was edited since; hover the status dot to
see why. The status is read in the background at startup, after each apply or
rollback, when you open the page, and when you click **Refresh**.

The interface checks rules as you edit them. A rule with an invalid extension
or ProgID, or an extension listed twice, is flagged in red and blocks Apply.
Disabled rules never block. Removing a rule and rolling back extensions both
ask for confirmation first.

Some names are refused because writing to them could break Windows:
structural `Classes` keys such as `CLSID` or `Directory`, the launch types of
executables such as `exefile`, and extensions such as `.exe`, `.lnk` and
`.dll`. If a ProgID already existed before WinTune (it belongs to another
app), WinTune only overwrites the values it sets, never deletes the key, and
rollback restores it exactly. If a user is signed out during a rollback,
their part of the snapshot is kept. Roll back again once they are signed in.

**Known limit:** Explorer's per-user `UserChoice` key is protected by a hash
that covers the key's metadata. Rollback restores its values, but Windows may
still consider it tampered with, show "An app default was reset" and go back to
its default app. Uncheck **Ignore the "Open with" choice** on a rule to
leave that key alone.

The **History** page lists every operation and, under **Saved originals**,
every snapshot WinTune holds. You can restore one item, or a whole group
(File associations, Explorer & Taskbar, Start & Privacy, Context menu, Startup apps, Environment variables). Each restore asks
first. Restoring a renamed environment variable brings back both names
together. Restoring an extension puts it back to its exact pre-WinTune state. It
also removes every ProgID that WinTune created, including ones you renamed
since. A snapshot is deleted only once it has been fully restored; if the
restore partly fails, the snapshot is kept so you can retry.

## Data files

Everything lives in `%ProgramData%\Wintune`:

| File | Content |
| --- | --- |
| `config.json` | Rules, editor path, folders, theme, language |
| `backups.json` | Original-state snapshots of file associations, used by the rollback |
| `tweaks-backups.json` | Original state of each Explorer & Taskbar tweak, per user |
| `context-menu-backups.json` | Original state of each Context menu key, hidden entry and name, per user |
| `startup-backups.json` | Original state of each Startup apps switch, per user |
| `environment-backups.json` | Original state of each environment variable WinTune changed, per user, plus the pairs of names a rename restores together |
| `history.jsonl` | One line per operation (apply, rollback, restore), shown by the **History** page |
| `reg-backups\*.reg` | `.reg` safety net written before each apply |
| `gallery.pak` | Optional compressed icon pack, built from the icon folder |

Icons used by associations are copied to `%ProgramData%\ico`.

## Code layout

| Module | Role |
| --- | --- |
| `regsnap` | Generic snapshot, restore, delete-tree and `.reg` export. Every module that writes to the registry builds on it |
| `associations` | File-association engine on an injectable `AssocTarget`, split into `target`, `validate`, `store`, `apply`, `rollback`, `status` and `tests` |
| `tweaks` | Explorer & Taskbar tweaks engine (catalog, status, apply, restore) on an injectable `TweakTarget` |
| `backups` | Snapshot store shared by tweaks, the Context menu, Startup apps and Environment variables (per user, per slot), with save-then-export and restore |
| `context_menu` | Context menu engine (entries, programs, catalog of Windows entries, scan of existing entries, apply, status, restore) on an injectable `MenuTarget` |
| `startup` | Startup apps engine (scan of the Run keys and Startup folders, `StartupApproved` switch, apply, restore) on an injectable `StartupTarget` |
| `environment` | Environment variables engine (`path` and `rules` are pure; `scan` reads `HKCU\Environment` and the system key; `engine` applies, restores and broadcasts) on an injectable `EnvTarget` |
| `persist` | JSON stores: a missing file is empty, a corrupt or unreadable file is a `LoadError` (a code the UI translates), saves are atomic |
| `history` | Operation log (`history.jsonl`): append one line per operation, read the most recent |
| `models` | Config and backup data types, `ThemeMode`, plus the default rules |
| `gallery` | Shareable icon library (folder plus in-memory `.pak`), background thumbnail decoding, and the pack builder |
| `tasks` | Background jobs with progress reporting and panic capture |
| `detector` | VS Code, WSL and user-hive detection |
| `clock` | Local timestamps |
| `i18n` | Translations: `.resx` catalogs in `i18n/`, the `Tr` handle, and engine message codes turned into text |
| `ui` | egui interface, one file per page (`home`, `associations_tab`, `tweaks_tab` for both tweak pages, `context_menu_tab`, `startup_tab`, `environment_tab`, `gallery_tab` for Icons, `history_panel`, `settings`), `originals` (History's saved originals), `icon_grid` (the shared icon grid) and `icon_picker` (the dialog that sets a rule's icon), plus `shell` (navigation and action bars), `autosave`, `confirm`, `rule_status`, `jobs`, and `kit/`, the design kit that holds the theme and every control |
| `main.rs` | Window setup only |

Apply, rollback and pack building run on a background thread, a progress bar
shows their progress, and the result appears as a toast. While one of them
runs, the other buttons that change the registry or the pack are disabled.
If you close the window mid-operation, WinTune waits for the operation to
finish before closing.

The editor path, icon folder, theme and language live in the **Settings**
page. Changes there, and to the rules, are saved automatically about a
second after the last edit and always before the window closes; if a save
fails, a banner with a **Retry** button appears.

**Ctrl+F** focuses the search box of the current page, and **Esc** cancels a
confirmation dialog. No keyboard shortcut changes the registry: those actions
always take a click and, when they are destructive, a confirmation. A
disabled button explains on hover why it is disabled.

### Adding a customization module

1. Put the engine in its own library module, on top of `regsnap`, with an
   injectable target like `AssocTarget` so it can be tested in the HKCU
   sandbox.
2. Add a variant to `ui::ActiveTab` and a page in a new
   `src/ui/<module>_tab.rs` file, built from `ui::kit` components. Add its
   navigation entry and Home tile, and its texts in every `.resx` file.
3. Run long operations through `ui::jobs`.

[CONTRIBUTING.md](CONTRIBUTING.md#adding-a-tweak-or-a-module) has the
full steps.

## Releasing

The `Release` workflow (`.github/workflows/release.yml`) runs when a tag such
as `v0.3.0` is pushed:

1. Set `version` in `Cargo.toml` (for example `0.3.0`), commit and merge it.
2. Tag `main` with the same version and push the tag:
   `git tag v0.3.0 && git push origin v0.3.0`.
3. The workflow checks that the tag matches `Cargo.toml`, then runs fmt,
   clippy and the tests, and builds the release binary. It creates a **draft**
   GitHub release with `WinTune.exe` and its SHA-256.
4. Review the draft on GitHub and publish it.

The binary is not code-signed yet, so SmartScreen may warn on first launch.

## Roadmap

- **Phase 2 (done)**: UI split into modules; apply, rollback, pack build and
  icon decoding moved off the UI thread, with progress.
- **Phase 3 (in progress)**: done so far are each rule's live status,
  validation in the UI, confirmations for destructive actions and the
  operation history. Still to do: a diff preview before applying, and config
  import/export.
- **Customization modules**: Explorer & Taskbar, Start & Privacy, Context
  menu, Startup apps and Environment variables are built on the snapshot engine. More can follow, built on the same
  snapshot and rollback engine.
- **Phase 4**: the release workflow (draft releases from tags) is done. Still to do: an installer and code signing.
