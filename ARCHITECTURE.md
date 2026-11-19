# Architecture

WinTune is a library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`,
window setup only). Everything that touches the registry or the disk lives in
the library, so it can be tested without the GUI and without elevation.

```
                 ┌──────────────────────── ui (egui) ────────────────────────┐
                 │ tabs: associations · tweaks (×2) · gallery · rollback      │
                 │ sidebar · status bar · history panel · confirm dialog      │
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
| `config.json` | `ui` | Saved atomically. If it is unreadable, defaults are used in memory and **saving is refused** |
| `backups.json` | `associations` | Missing means empty. Unreadable means an error, and all association changes are blocked |
| `tweaks-backups.json` | `tweaks` | Same rule as `backups.json`. Entries are keyed by user (HKCU is per user), then by tweak id |
| `history.jsonl` | `history` | Append-only. A damaged line is skipped, never fatal |
| `reg-backups\*.reg` | engines | A `.reg` safety net written before each apply |

`persist::load_json` and `save_json` implement the first three rules. Every
save writes a temp file, then renames it over the old one.

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

- `ui/mod.rs` holds `WinTuneApp`, `ActiveTab` (the sidebar list) and
  `AppEnv`. `AppEnv` carries the targets, store paths, notifications and
  user label, and tests replace it with a sandboxed one.
- **One file per tab.** The two tweak tabs share `tweaks_tab.rs`, and each
  shows its own `TweakGroup`s.
- **Jobs.** Long operations (apply, rollback, tweaks, icon pack) run on a
  background thread through `ui/jobs.rs`. Only one runs at a time, and none
  starts while a confirmation dialog is open. If you close the window during
  a job, it closes once the job finishes.
- **Rule status.** `rule_status.rs` refreshes the live state of the rules on
  a background job at startup, after each job, and on demand.
- **UI tests.** They run headless: `ctx.run` in an `egui::Context` with the
  fixture from `ui/test_support.rs` (temp folder plus registry sandbox).

## Adding a customization module

See [CONTRIBUTING.md](CONTRIBUTING.md#adding-a-tweak-or-a-module).
