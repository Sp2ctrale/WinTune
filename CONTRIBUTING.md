# Contributing

## Build and check

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

CI runs the same four steps on `windows-latest` (see
`.github/workflows/ci.yml`). A PR must pass all of them.

`cargo test` works from a **non-elevated** terminal. The administrator
manifest is embedded into the `WinTune` binary only, not into the library's
test harness.

## The sandbox rule

Tests must never touch the real `HKLM` or the real `HKCU` settings.

- **Registry.** Registry tests use `regsnap::test_support::Sandbox`, a
  throw-away key under `HKCU\Software\WinTuneTest\<unique>` that is deleted
  when the test ends. Engines take an injectable target (`AssocTarget`,
  `TweakTarget`), and tests point it into the sandbox.
- **UI.** UI tests use `ui::test_support::TempIcons`. It builds the app with
  an `AppEnv` that points at the sandbox and a temp folder, so real apply,
  rollback and restore flows can run.
- **Never launch the app from a test.** `WinTune.exe` asks for elevation.

## Writing tests

- **Write the test first.** Watch it fail for the right reason, then make it
  pass.
- **Test the behaviour, not the code.** Read back the registry, or the store
  on disk, rather than checking which internal function was called.
- **Make sure the test can fail.** When a test passes on the first run, break
  the code on purpose to check that the test catches it, then put the code
  back.

## Adding a tweak or a module

A tweak is one REG_DWORD value or one key under HKCU.

1. **Check the value in Microsoft documentation.** Microsoft Learn's
   *Reference for Windows 11 settings* is a good start. If you cannot
   confirm it, do not add it. If it relies on undocumented behaviour, set
   `unofficial: true`.
2. **Add a `Tweak` to `CATALOG`** in `src/tweaks.rs`: id, title, description,
   group, OS requirement, when the change becomes visible, and the action.
   "Off" must be the Windows default.
3. **Add a row to `docs/registry-reference.md`.** The test
   `the_registry_reference_lists_every_tweak` fails until you do.
4. **Run the tests.** `every_tweak_round_trips_on_off_restore_in_the_sandbox`
   checks on, off and restore for every tweak automatically.

For a new module:

1. **Build the engine on `regsnap`**, with an injectable target, and follow
   the safety model in [ARCHITECTURE.md](ARCHITECTURE.md): validate, snapshot
   on first touch, persist and export `.reg` **before** writing, and report
   every failure.
2. **Give it its own JSON store** through `persist`, and block changes when
   the store cannot be read.
3. **Add an `ActiveTab` variant** (title, sidebar subtitle) and a
   `src/ui/<module>_tab.rs` file. Run long operations through `ui/jobs.rs`,
   ask for confirmation before destructive actions, and record finished jobs
   with `record_history`.
4. **Add headless UI tests** with `TempIcons`.

## Adding a language

1. **Copy `i18n/strings.resx` to `i18n/strings.<code>.resx`**, for example
   `strings.de.resx`. The build picks the file up on its own.
2. **Translate the values**, not the keys. Set `language.name` to the
   language's own name (`Deutsch`): that is what the picker shows.
3. **Keep placeholders.** `{0}`, `{1}` must appear in the translation as in
   English, and a literal brace is written `{{` or `}}`.
4. **Run `cargo test --lib i18n`.** These tests tell you what is missing:
   - `shipped_translations_are_complete`: every English key exists in
     French. It names `fr` explicitly, so add your language to it once you
     ship it as complete.
   - `shipped_translations_have_no_orphan_keys`: no key that English lacks
     (a typo in a key name shows up here).
   - `shipped_translations_use_the_same_placeholders_as_english`: same
     `{n}` placeholders as English, for every language.
   - `every_engine_message_translates_in_every_shipped_language` and
     `every_tweak_and_group_has_texts_in_every_shipped_language` also run
     on every language, so a new file must cover those texts.

A key you leave out falls back to English at run time, so an unfinished
translation still works. A new key in English needs its translations in
the same PR, because the tests above fail until the shipped languages have it.

## Commits and PRs

- Keep one coherent change per PR, and explain what changed and why.
- Squash-merge once CI is green.
- Releases come from `v*` tags (see the README).
