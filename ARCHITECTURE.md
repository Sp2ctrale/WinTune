# Architecture

WinTune is a library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`,
window setup only). Everything that touches the registry or the disk lives in
the library, so it can be tested without the GUI and without elevation.

```
                 ┌──────────────────────── ui (egui) ────────────────────────┐
                 │ pages: home · assoc. · tweaks (×2) · menu · icons · history│
                 │ navigation · toasts · progress bar · history · dialog      │
                 │ jobs ── tasks::BackgroundJob (one at a time, with progress)│
                 └───────────────┬───────────────────────────┬────────────────┘
                                 │ AppEnv (injectable)        │
          ┌──────────────────────┴───────┐        ┌──────────┴───────────┐
          │ associations (HKLM + HKU)    │        │ tweaks · context_menu│
          │ AssocTarget                  │        │ (HKCU) TweakTarget / │
          │                              │        │ MenuTarget           │
          └──────────────┬───────────────┘        └──────────┬───────────┘
                         └──────────── regsnap ──────────────┘
                     snapshot / restore / delete-tree / .reg export
                                         │
                          persist · history · clock (files)
```

## Layers

- **`regsnap`**: generic, lossless registry primitives. `snapshot_key` and
  `restore_key` work on whole trees; `snapshot_value` and `restore_value`
  work on single values. They keep every value type, including `REG_NONE`
  and typed default values. An unreadable key is an error, never "missing".
  `export_reg_full` writes `.reg` files that delete then recreate a key, so
  importing one restores the exact state.
- **Engines**: `associations`, `tweaks`, `context_menu`, `startup` and `environment`. Each one works on
  an injectable target (`AssocTarget`, `TweakTarget`, `MenuTarget`, `StartupTarget`, `EnvTarget`). Production uses the real hives;
  tests point the target at `HKCU\Software\WinTuneTest\<unique>`.
- **Stores**: small JSON files under `%ProgramData%\Wintune`.

| File | Owner | Rule |
| --- | --- | --- |
| `config.json` | `ui` | Saved atomically. If it is unreadable, defaults are used in memory and **saving is refused** (the file is never overwritten) |
| `backups.json` | `associations` | Missing means empty. Unreadable means an error, and all association changes are blocked |
| `tweaks-backups.json` | `tweaks` | Same rule as `backups.json`. Entries are keyed by user (HKCU is per user), then by tweak id |
| `context-menu-backups.json` | `context_menu` | Same rule as `backups.json`. Snapshots are keyed by user, then by slot id: a key path (a `WinTune.<id>` verb key, or a verb's HKCU copy, snapshotted whole) or a `Blocked` CLSID value. Stores written by the first Context menu release (bare key snapshots) still load |
| `startup-backups.json` | `startup` | Same rule as `backups.json`. Snapshots are keyed by user, then by slot id: one `StartupApproved` value of HKCU (a value path and name) |
| `environment-backups.json` | `environment` | Same rule as `backups.json`. Snapshots are keyed by user, then by slot id: one value of HKCU `Environment` (the value path `Environment` and the variable name). Next to `users` it holds `renames`, per user: groups of slot ids (a rename's two slots) restored together. A store without `renames` loads |
| `history.jsonl` | `history` | Append-only. A damaged line is skipped, never fatal |
| `reg-backups\*.reg` | engines | A `.reg` safety net written before each apply |

`persist::load_json` and `save_json` implement the first three rules. Every
save writes a temp file, then renames it over the old one.

`load_json` fails with a `LoadError`, a code and not a sentence:
`Corrupt { path, detail }` (the file is not valid JSON for the store) or
`Unreadable { path, detail }` (the OS refused to read it). `detail` is the
raw serde or OS text. `Display` is an English sentence kept for logs and
reports. The UI shows that English only inside report details (for example
the "Configuration not saved" warning); everywhere else
`i18n::messages::load_error` turns the code into a translated sentence
(`load.corrupt`, `load.unreadable`) around the raw detail.

## Context menu engine

`context_menu` (`src/context_menu/`) lets a user add entries to Explorer's
right-click menu. The list lives in `config.json` under `context_menu.users`,
keyed by account, so an account only ever sees and edits its own entries.

- **Model** (`mod.rs`): a `MenuEntry` has an id, a label, a `ProgramChoice`
  (VS Code, Terminal, PowerShell, Cmd or a custom `.exe`), a set of `Place`s
  (folders, folder background, drives, files) and an optional icon.
  `entry_problems` validates the whole list and returns codes
  (`EntryProblem`), which `i18n/messages.rs` turns into text.
- **Programs** (`programs.rs`): `Programs` holds the detected executables and
  `command_line` builds the `command` value. Every placeholder is quoted. A
  drive root ends in a backslash, so Drive passes `"%1."` (`-d "%V."` for
  Terminal) and the folder background, which Explorer also shows on a drive
  root, passes `"%V\."` (`-d "%V\."`), so the closing quote survives. The
  PowerShell and Command Prompt presets go through `cmd.exe` (`pushd`) so
  apostrophes in folder names and network folders work. Presets are checked
  against the disk every time they are resolved, not only at startup.
- **Registry** (`engine.rs`): one verb key per entry and place,
  `HKCU\Software\Classes\<Directory|Directory\Background|Drive|*>\shell\WinTune.<id>`,
  with `MUIVerb`, `Icon` and a `command` subkey. `apply_entries` makes the
  registry match the list: it snapshots every key it is about to create,
  change or delete (in `context-menu-backups.json`, plus a `.reg` export),
  then writes, and deletes the keys of removed entries and unticked places.
  Only keys named `WinTune.*` are ever touched. `entry_status` is read-only
  and says Applied, Not applied or Changed (with the places that differ);
  `orphan_keys` finds keys of entries no longer in the list; `restore_keys`
  puts snapshots back.
- **Page** (`ui/context_menu_tab.rs`): a list of entries with a state pill, an
  editor for the selected one, a Windows 11 banner linking to the Classic
  context menu tweak, and an action bar showing what is not applied. Apply and
  restore run as a job (`ui/jobs.rs`) and are logged under "Context menu".
  History's Saved originals has a Context menu group that restores through
  `restore_keys`, and the operations filter has a matching chip.
- **Windows 11.** Entries written this way appear under "Show more options".
- **Shared snapshots** (`src/backups.rs`): used by tweaks and the Context
  menu. A `RegTarget` is the hive (or test sandbox) to work on, a `Slot` is one
  whole key or one value of a key, a `Snapshot` is its original state, a `Backup` adds a timestamp, and
  `UserBackups` holds them per user and slot id. `take_snapshot` and
  `restore_snapshot` read and put back one slot; `save_then_export` stores the
  snapshots, then writes the `.reg`, before any write; `restore_ids` restores
  a set of slots.
- **Catalog** (`catalog.rs`): 11 curated Windows entries, each a Clsid (hidden
  through `Blocked`) or a Verbs entry (hidden through a
  `ProgrammaticAccessOnly` copy), with the text of what else it affects.
- **Scan** (`existing.rs`, `origin.rs`, `fileinfo.rs`): a read-only pass over
  `shell` and `shellex\ContextMenuHandlers` under `*`, `AllFilesystemObjects`,
  `Folder`, `Directory`, `Directory\Background` and `Drive`, in HKCU and HKLM
  `Classes`. It skips `WinTune.*` verbs, the canonical verbs (open, opennew,
  explore, find, print, printto, runas, runasuser, openas, properties),
  catalog entries and anything run from the Windows folder, except a file in
  `System32\DriverStore` whose product name is not Windows' own (checked in
  any language: it contains both "Microsoft" and "Windows"; no name counts as
  Windows'). A verb the
  installer already hides (`ProgrammaticAccessOnly` in the machine copy, or
  `LegacyDisable`) is skipped unless WinTune holds a snapshot of it. An entry whose
  program file cannot be found is skipped too (fail closed) unless WinTune
  holds a snapshot of it. Verbs are grouped by verb name and origin file,
  handlers by CLSID. The origin is the product name from the file's version
  info, else the file name without extension (`fileinfo.rs`).
- **Scan job** (`ui/menu_existing_state.rs`): the scan runs as a background job
  at startup, when the page opens and after every other job, and never while
  a job runs, so a page never reads half-written keys. Rows are drawn only
  when no scan is running. Pending hides and renames are cleared by an Apply
  without errors; after a refused or partial one they stay, and each finished
  scan drops the pending choices its rows already match.
- **Hide and rename writes** (`engine.rs`, `apply_menu`, one pass together
  with the added entries): hide a verb writes `ProgrammaticAccessOnly` on an
  HKCU copy of the verb key; show removes it and deletes the copy if WinTune
  created it and it is empty. Rename writes `MUIVerb` on the HKCU copy. Hide a
  shell extension or modern entry writes its CLSID under HKCU
  `...\Shell Extensions\Blocked`. A CLSID blocked in HKLM `Blocked` is shown
  locked. Nothing is written under HKLM. When an Apply brings a slot back to
  its snapshot (the copy WinTune created is gone, or `ProgrammaticAccessOnly`
  and `MUIVerb` equal the snapshot's, or the `Blocked` value is present or
  absent as before), the snapshot is forgotten and the store saved again.
- **Restore of existing entries** (`engine.rs`, `restore_keys`): a verb's
  HKCU copy is not replayed as a whole key. Only `ProgrammaticAccessOnly` and
  `MUIVerb` return to the snapshot (removed when it had none); the copy is
  deleted (`delete_subkey`, never a tree delete) only if WinTune created it and
  it is now empty, and is never recreated once gone. Everything else in it is
  the program's and stays. `WinTune.*` keys and `Blocked` values are restored
  exactly. `restore_ids` (`backups.rs`) stays generic: it takes the per-slot
  restore as a closure.
- **Catalog presence**: a Clsid row shows when `CLSID\<clsid>` exists, Open in
  Terminal when its packaged COM class (`PackagedCom\ClassIndex\<clsid>`)
  exists, a Verbs row when one of its verb keys exists (HKCU or HKLM).
- **Page** (`ui/context_menu_existing.rs`, `ui/menu_existing_state.rs`): the
  Windows and Other programs sections. Toggles and renames stay pending
  (`menu_pending`) until Apply; the confirmation lists side effects.

## Startup apps engine

`startup` (`src/startup/`) lists what starts at sign-in and turns each item off
or on for the current account. It is the smallest writer: one value per item,
HKCU only. `HKLM`, `Run` values and Startup folder files are never touched.

- **Sources** (`mod.rs`, `Source`): HKCU `Run`, HKLM `Run`, HKLM
  `WOW6432Node\...\Run`, the per-user Startup folder and the all-users one.
  `StartupTarget` carries the user and machine `RegTarget`s and the two folder
  paths (from `%APPDATA%` and `%ProgramData%`; a missing variable skips that
  folder). Tests point it at `HKCU\Software\WinTuneTest\<unique>` and a temp
  folder.
- **One row per approved value** (`scan.rs`): `Source::approved()` maps a
  source to its `StartupApproved` sub-key (`Run`, `Run32` or `StartupFolder`).
  Values of the same sub-key with the same name, ignoring case, are one
  `StartupItem` with several `Origin`s (a per-user and an all-users item can
  share a switch). The row id is `<run|run32|folder>:<lowercased name>`; its
  slot is the switch value. Rows are sorted by display name. Name and program
  come from the first origin. The program is the product name of the command's
  executable, else its file stem; a command with a folder part whose program is
  missing sets `program_missing`. Run values that are not strings, the default
  value and unreadable keys give no rows. A Startup folder lists regular files
  except `desktop.ini` and hidden or system files.
- **Switch format** (`mod.rs`): a `REG_BINARY` of at least 4 bytes whose first
  DWORD is the flag. No value is `Absent` (on); `2` and `6` are `On`; `3` and
  `7` are `Off`; anything else, a wrong type or fewer than 4 bytes is
  `Unknown`. `off_value(filetime)` is `03 00 00 00` plus the FILETIME;
  `on_value()` is `02 00 00 00` plus 8 zero bytes. Each is 12 bytes.
- **State and lock** (`scan.rs`): the item is off when its HKCU or HKLM switch
  is off. `StartupState` is `On`, `Off` (off and the store holds a snapshot of
  the switch) or `OffElsewhere` (off, no snapshot). The lock is the first that
  applies, in this order: `Unreadable` (a switch cannot be read), `UnknownShape`
  (HKCU or HKLM switch of unknown format), `OffForEveryone` (HKLM switch off),
  `AllUsersOnly` (a source of the row has its flag at `false`). The HKLM switch
  rules apply whatever the row's sources.
- **Flag table** (`mod.rs`): `PER_USER_SWITCH_WORKS`, one `(Source, bool)` per
  source: does a per-user switch turn that source's items off? Initial values:
  `UserRun` true, `MachineRun` true, `MachineRun32` false, `UserFolder` true,
  `CommonFolder` true. The all-users values rest on an inference (see the
  Evidence table in the registry reference), so the pre-merge checklist
  decides them; a failed check sets the entry to `false`. The scan, the engine
  and `AppEnv` take the table as a `fn(Source) -> bool`, so tests do not change
  when a flag does.
- **Apply** (`engine.rs`, `apply_startup`): like `apply_menu`. Keep the first
  change per slot (ids compare ignoring case); validate every change against
  the registry as it is now (`problem`: a source whose flag is false, an
  unreadable or unknown HKCU switch, an unknown or off HKLM switch), and if any
  fails, refuse the whole Apply with nothing written. Then snapshot each
  switch on first touch (`take_snapshot`, `record_first`), `save_then_export`
  (the store, then a `.reg`), then write HKCU only. Off writes `off_value(now)`.
  On puts back the original when it was absent or an on value, else writes
  `on_value()`; with no recorded original it fails that item. A switch found
  back at its original after the writes has its snapshot forgotten and the
  store saved again. The report lists errors and successes; detail lines stay
  English sentences, as in the other engines.
- **Restore** (`restore_startup`): `restore_ids` with `restore_snapshot` per
  slot: the exact original value, or the value deleted if there was none, and
  the snapshot is dropped only after it succeeded.
- **Page** (`ui/startup_tab.rs`, `ui/startup_state.rs`): the scan runs as a
  background job at startup, when the page opens and after every other job.
  Rows stay visible during a rescan; the loading line shows only before the
  first scan. Toggles stay pending (`startup_pending`, by row id) until Apply;
  a search box filters by name and program. Apply asks no confirmation, because
  it is non-destructive and History can undo it; Restore asks. A locked row's
  toggle is disabled with its reason. The page carries an Unofficial pill
  because the switch is undocumented.

## Environment variables engine

`environment` (`src/environment/`) edits the current user's variables and
`Path`. It writes only values of `HKCU\Environment`: never `HKLM`, never
`HKCU\Volatile Environment`, and it never runs `setx`.

- **Target** (`mod.rs`): `EnvTarget` carries a `user` and a `machine`
  `RegTarget`. Only `user` is ever written; `machine` (HKLM) is read, and
  `Volatile Environment` is named by a constant only for a guard test. Each
  variable is one `Slot::Value { path: "Environment", name }`; its id is the
  join key between row, pending change, store and History.
- **Pure parts** (`path.rs`, `rules.rs`): no registry, no disk. `path.rs`
  holds `Resolver` (a small `%name%` expander: user variables, then system,
  then a process lookup passed in as a function, at most 8 levels), `has_reference`,
  `split_entries` and `join_entries` (split on `;`, drop empty entries, keep the
  rest verbatim), `analyze_entries` (missing folder and duplicate flags, the
  later copy being the duplicate), `removed_entries` (a multiset difference by
  exact text) and `PathEdit`, the list being edited with its moved and added
  marks. `rules.rs` holds `EnvChange` (add, edit, delete, rename), the type rule
  `kind_to_write`, the checks `validate_changes` (returns `EnvProblem` codes),
  the warnings `change_warnings`, and `var_change` and `path_change`, which turn
  pending edits into changes.
- **Type rule** (`kind_to_write`): an existing value keeps its type, except a
  `REG_SZ` whose new text holds a `%name%` reference, which becomes
  `REG_EXPAND_SZ`. A new value is `REG_EXPAND_SZ` if it holds a reference, else
  `REG_SZ`; a new `Path` is always `REG_EXPAND_SZ`. Values are read raw, so a
  reference is never expanded on its way back to the registry.
- **Scan** (`scan.rs`, `scan_environment`): read-only. `read_vars` reads the
  user and machine keys raw with their types (a missing key is empty; a value
  that is not text has no text; the empty-named default value is skipped),
  marks which user variables hold a snapshot, and checks which `Path` entries
  exist on disk, but only for absolute local paths whose references all
  resolve. `resolver_vars` is the one mapping from variables to the resolver,
  used by the scan and the page.
- **Validation** (`validate_changes`): any problem refuses the whole Apply with
  nothing written: a bad name or value, a name taken (ignoring case, after the
  change), a change to a variable that vanished (`Gone`) or is not text
  (`NotText`), and a variable changed twice (`Twice`; a name freed by a delete
  cannot be reused in the same Apply). Warnings are not refusals: the page asks
  for a confirmation.
- **Apply** (`engine.rs`, `apply_environment`): like `apply_startup`. Validate
  against the registry as it is now; snapshot each slot on first touch
  (`take_snapshot`, `record_first`), under the spelling stored in the registry;
  `save_then_export` (the store, then a `.reg`); then write in change order.
  A rename links its two slots (`EnvBackupStore::link`). A slot found back at
  its original (same presence, type, bytes and spelling) has its snapshot
  forgotten, and the store is saved again. The report lists every error and
  success.
- **Rename and case-only rename** (`write_change`, `write_exact`): a rename
  writes the new name, then deletes the old one, so a crash between the two
  leaves both. The registry keeps the first spelling of a value name, so
  `write_exact` deletes a value stored under another spelling and then writes
  it: that is how a rename that changes only case works, and how a restore
  brings an old spelling back. A crash between that delete and the write loses
  that one variable until Restore or the `.reg` puts it back.
- **Rename pairs** (`EnvBackupStore`): the store is `UserBackups` (flattened)
  plus `renames`, per user, groups of slot ids. `backups::save_then_export`
  takes `&impl Serialize` for that. `with_partners` adds the other members of
  a group to a restore, and `prune` drops groups whose snapshots are gone.
- **Restore** (`restore_environment`): the ids plus their rename partners go
  through `restore_ids` with `restore_exact`: the old name comes back before
  the new one goes, then the snapshot is dropped. `restore_ids` saves only the
  snapshots, so the engine saves the full store again right after; if
  WinTune died between the two saves, the other pairs would restore one half
  at a time.
- **Broadcast** (`EnvOptions::broadcast`): a function injected through
  `AppEnv`, called once after an Apply that wrote something, and once after a
  restore that succeeded. `broadcast_environment` sends `WM_SETTINGCHANGE` with
  "Environment" through `SendMessageTimeoutW` (`HWND_BROADCAST`,
  `SMTO_ABORTIFHUNG`, 5 s). Tests pass a counter and never send it.
- **Page** (`ui/environment_tab.rs`, `ui/environment_state.rs`): the scan runs
  as a background job at startup, when the page opens and after every other
  job. Pending edits live in the state (`PathEdit` for `Path`, one `VarEdit`
  per other variable) until Apply; a `Path` list edit counts as one change.
  Apply asks first only when it deletes a variable, removes a `Path` entry or
  carries a warning (deleting or renaming `Path`, `TEMP`, `TMP`, `OneDrive*`;
  a merged `Path` over 8,191 UTF-16 units; `Path` changed by another program
  since the edit started). The confirmation lives in `ui/confirm.rs`. Restore
  asks always, adds the rename partners, and for `Path` names the entries it
  removes and adds back (`removed_entries` on the entries as written). A
  refused or partial Apply keeps the pending changes. An unreadable
  `HKCU\Environment` shows a banner in place of the sections. Engine reasons
  are enums (`EnvProblem`, `EnvWarning`) translated in `i18n/messages.rs`.

## Safety model (every module that writes to the registry)

1. **Validate.** Values that would hit the wrong place are refused: path
   characters, reserved names, duplicates.
2. **Snapshot on first touch.** The original state of each key or value is
   recorded once, the first time WinTune touches it, and kept until restored.
3. **Persist, then write.** The snapshot store and a `.reg` export are written
   **before** any registry change. If either fails, nothing is applied.
4. **Report every failure.** Nothing is swallowed. A snapshot is dropped only
   after a restore fully succeeded.

## UI

- `ui/mod.rs` holds `WinTuneApp`, `ActiveTab` (the navigation list) and
  `AppEnv`. `AppEnv` carries the targets, store paths, notifications and
  user label, and tests replace it with a sandboxed one.
- **The frame** (`ui/shell.rs`). A grouped navigation on the left, one page
  in the middle, and two kinds of notice: toasts for results and a progress
  bar while a job runs. The pages are Home, File associations, Explorer &
  Taskbar, Start & Privacy, Context menu, Startup apps, Environment variables, Icons, History and Settings. Home (`home.rs`) shows one tile per
  module (the Startup apps tile reads "{n} apps, {m} turned off", counting
  every item that is not On; the Environment variables tile reads "{n} variables, {m} folders in Path",
  counting `Path` among the variables), a drift banner and the recent operations. Settings (`settings.rs`)
  and History (`history_panel.rs`) are pages too. `config.json` is saved by `autosave.rs`
  one second after the last edit, and always before the window closes.
- **`open_page` is the single entry** for switching page. It reloads the
  stores and refreshes statuses (never under a running job), so every
  navigation click, Home tile and "Review" link goes through it. Do not set
  `active_tab` directly. It also closes any open popup.
- **History labels.** `history.jsonl` stores English labels. They are
  translated only when drawn, by `history_module` and `history_action` in
  `i18n/messages.rs`; unknown labels are shown as stored.
- **Startup apps modules.** `startup_tab.rs` draws the page,
  `startup_state.rs` holds the scan job and the pending choices, the apply
  and restore jobs live in `jobs.rs` (`start_startup_apply`,
  `start_startup_restore`, `finish_startup_job`), and `history_panel.rs` and
  `originals.rs` add the History filter and the Saved originals group.
- **Environment variables modules.** `environment_tab.rs` draws the page,
  `environment_state.rs` holds the scan job and the pending edits, the apply
  and restore jobs live in `jobs.rs`, and `history_panel.rs` and `originals.rs`
  add the History filter and the Saved originals group (a rename's two rows
  restore together).
- **One file per page.** The two tweak tabs share `tweaks_tab.rs`, and each
  shows its own `TweakGroup`s. They are kit pages: a section per group, a
  `SettingRow` per tweak with its state pill, and a `…` menu on rows that
  have a saved original (it holds "Restore original").
- **Jobs.** Long operations (apply, rollback, tweaks, icon pack) run on a
  background thread through `ui/jobs.rs`. Only one runs at a time, and none
  starts while a confirmation dialog is open. If you close the window during
  a job, it closes once the job finishes.
- **Rule status.** `rule_status.rs` refreshes the live state of the rules on
  a background job at startup, after each job, and on demand.
- **UI tests.** They run headless: `ctx.run` in an `egui::Context` with the
  fixture from `ui/test_support.rs` (temp folder plus registry sandbox).

## Translations (`src/i18n/`)

- **Catalogs** are `.resx` files in `i18n/`: `strings.resx` is English and
  `strings.<code>.resx` is any other language. `build.rs` scans that folder
  and generates the language table (English first, then by code), so a new
  file is picked up with no code change. They are embedded in the binary and
  parsed once (`resx.rs`).
- **`Tr`** is a small copyable handle on one language. `tr.t(key)` looks the
  key up in the language, then in English, then returns the key itself, so
  a missing translation never panics or shows a blank. `tr.t_args(key, &[..])`
  fills `{0}`, `{1}` placeholders (`{{` and `}}` are literal braces in `t_args` only, not in `t`).
  `Tr::languages()` lists the shipped codes with each language's own name
  (the `language.name` key).
- **Engines return codes, not text.** `RuleProblem`, `DriftReason`,
  `CustomReason` and `LoadError` are enums. `i18n/messages.rs` turns them
  into text in the chosen language (a raw OS or serde detail stays as it is,
  inside a translated sentence), and also holds the tweak titles, descriptions and group
  titles (looked up by `TweakGroup` id).
- **Tests** (`src/i18n/tests.rs`) keep the catalogs honest: every shipped
  language is complete (a missing key would silently fall back to English), no language has keys English lacks, placeholders match English,
  every engine message and tweak text exists in every shipped language, and
  every key used in the code exists in English.

## UI kit (`src/ui/kit/`)

The kit is the only place that decides how things look. **New screens never set
colors, sizes or fonts**: they call kit controls and read tokens.

- **Tokens** (`tokens.rs`): the `Palette` for light and dark (opaque colors,
  in the spirit of Windows 11 Settings), plus `space`, `radius` and
  `text_size`.
- **Styles** (`style.rs`): the theme lives here. `install_styles` builds one
  egui style per theme, once. `set_theme_mode` maps `ThemeMode` (System,
  Light, Dark; the type is in `src/models.rs`, because `config.json` stores
  it) to egui's
  theme preference, so System follows Windows and switching rebuilds nothing.
  `current_palette` and `resolved` give screens the theme in use.
- **Fonts** (`fonts.rs`): Segoe UI Variable (or `segoeui`) and `seguisb`
  (semibold) are loaded from the Windows fonts folder at runtime, never
  bundled. Each file is checked to be a valid font before egui sees it, and
  anything missing keeps egui's built-in font. `FontReport` says what loaded.
- **Icons** (`icons.rs`): the `Icon` enum, drawn from Segoe Fluent Icons, or
  Segoe MDL2 Assets on older Windows. Without an icon font the caller gets
  `None` and draws text.
- **Components**: `controls.rs` (toggle, pill, status dot, segmented,
  disclosure, the `…` overflow menu with its `OVERFLOW_BUTTON` size, and the
  shared `primary_button` / `secondary_button` at `BUTTON_HEIGHT`),
  `surfaces.rs` (card, section, `SettingRow` and `show_with_note`, banner,
  empty state), `nav.rs`
  (navigation sidebar, page header), `feedback.rs` (action bar, toasts,
  dialog).

The File associations screen is built on the kit. It has a rule list on
the left (search, add, state dots) and the selected rule's detail on the
right, with the less common settings under Advanced options. Choosing an
icon opens the icon picker (`icon_picker.rs`), a dialog over a shared icon
grid. The user no longer goes to the Icons page to assign an icon.

A page body goes in `WinTuneApp::page_panel` (the central panel with the
kit's margins) and starts with `kit::page_header`.

One action bar is drawn per frame. A page with `ActiveTab::owns_action_bar`
set draws its own bar (`page_action_bar` in `shell.rs`), and the shell draws
only the job's progress bar for the others. Three pages do: File associations, Explorer & Taskbar and
Start & Privacy. A tweak page's bar counts only its own pending changes, and
Discard and Apply touch only its own groups. Toasts sit above that bar.

The Icons page (`gallery_tab.rs`) is a browse-only kit page: a search box,
category chips and a grid of tiles. Tiles do not react. The icon folder is
shown read-only, with a link to Settings, where it is edited.

The History page has two sections, in this order. **Saved originals**
(`originals.rs`) lists the snapshots in `backups.json` and
`tweaks-backups.json`, grouped by module (a snapshot of a tweak that no
longer exists goes under "Other", with its raw id; it is listed but cannot be
restored, because the engine rejects an unknown id). Each item, and each
group, has a restore button that asks first and then runs one existing job
(`start_rollback` or `start_restore_tweaks`). Each group has its own
"Restore all", but there is no global restore-everything button. Opening
History reloads both stores, unless a job runs. An unreadable store disables
only its own groups. **Operations** (`history_panel.rs`) is the module filter
and one expandable row per entry of `history.jsonl`: the 20 most recent
matching entries, and a "Show all" button for the rest. There is no Rollback
tab: History replaced it. The file-association rollback itself is still an
engine operation (`associations::rollback`), run from History.

Every screen is on the kit. The old `theme.rs` helpers are gone.

## Adding a customization module

See [CONTRIBUTING.md](CONTRIBUTING.md#adding-a-tweak-or-a-module).
