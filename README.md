# WinTune

WinTune is a Windows 10/11 desktop app for customizing the system. Every change it
makes can be undone. The first module manages **file associations**: custom icons,
the double-click action (open in your editor or run the script), right-click verbs,
and Explorer preview metadata. It works on the 70 or so developer file types
configured by default, and on any extension you add.

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

The **Selective Rollback** tab restores chosen extensions to their exact
pre-WinTune state. It also removes every ProgID that WinTune created, including
ones you renamed since. A snapshot is deleted only once it has been fully
restored; if the restore partly fails, the snapshot is kept so you can retry.

## Data files

Everything lives in `%ProgramData%\Wintune`:

| File | Content |
| --- | --- |
| `config.json` | Rules, editor path, folders, theme |
| `backups.json` | Original-state snapshots used by the rollback |
| `reg-backups\*.reg` | `.reg` safety net written before each apply |
| `gallery.pak` | Optional compressed icon pack, built from the icon folder |

Icons used by associations are copied to `%ProgramData%\ico`.

## Code layout

| Module | Role |
| --- | --- |
| `regsnap` | Generic snapshot, restore, delete-tree and `.reg` export. Every module that writes to the registry builds on it |
| `associations` | File-association engine (apply and rollback) on an injectable `AssocTarget` |
| `models` | Config and backup data types, plus the default rules |
| `gallery` | Icon gallery (folder or memory-mapped `.pak`) and pack builder |
| `detector` | VS Code, WSL and user-hive detection |
| `clock` | Local timestamps |
| `theme` | Fluent-style dark and light theme |
| `main.rs` | egui user interface |

## Roadmap

- **Phase 2**: split `main.rs` into UI modules, and move long operations (apply,
  pack build, icon decoding) to a worker thread with progress.
- **Phase 3**: show each rule's live status (applied, changed since, not
  applied), preview the diff before applying, validate input in the UI, confirm
  destructive actions, add a log panel, and import/export the config.
- **More Windows customization modules**, built on the same snapshot and
  rollback engine.
- **Phase 4**: installer, code signing and release workflow.
