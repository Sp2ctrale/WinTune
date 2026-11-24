# Architecture

WinTune is a library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`,
window setup only). Everything that touches the registry or the disk lives in
the library, so it can be tested without the GUI and without elevation.

```
                 ┌──────────────────────── ui (egui) ────────────────────────┐
                 │ pages: home · associations · tweaks (×2) · icons · history │
                 │ navigation · toasts · progress bar · history · dialog      │
                 │ jobs ── tasks::BackgroundJob (one at a time, with progress)│
                 └───────────────┬───────────────────────────┬────────────────┘
                                 │ AppEnv (injectable)        │
          ┌──────────────────────┴───────┐        ┌──────────┴───────────┐
          │ associations (HKLM + HKU)    │        │ tweaks (HKCU)        │
          │ AssocTarget                  │        │ TweakTarget          │
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
- **Engines**: `associations` and `tweaks`. Each one works on an injectable
  target (`AssocTarget`, `TweakTarget`). Production uses the real hives;
  tests point the target at `HKCU\Software\WinTuneTest\<unique>`.
- **Stores**: small JSON files under `%ProgramData%\Wintune`.

| File | Owner | Rule |
| --- | --- | --- |
| `config.json` | `ui` | Saved atomically. If it is unreadable, defaults are used in memory and **saving is refused** (the file is never overwritten) |
| `backups.json` | `associations` | Missing means empty. Unreadable means an error, and all association changes are blocked |
| `tweaks-backups.json` | `tweaks` | Same rule as `backups.json`. Entries are keyed by user (HKCU is per user), then by tweak id |
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
  Taskbar, Start & Privacy, Icons, History and Settings. A Context menu entry
  is shown disabled ("coming soon"). Home (`home.rs`) shows one tile per
  module, a drift banner and the recent operations. Settings (`settings.rs`)
  and History (`history_panel.rs`) are pages too. `config.json` is saved by `autosave.rs`
  one second after the last edit, and always before the window closes.
- **`open_page` is the single entry** for switching page. It reloads the
  stores and refreshes statuses (never under a running job), so every
  navigation click, Home tile and "Review" link goes through it. Do not set
  `active_tab` directly. It also closes any open popup.
- **History labels.** `history.jsonl` stores English labels. They are
  translated only when drawn, by `history_module` and `history_action` in
  `i18n/messages.rs`; unknown labels are shown as stored.
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
