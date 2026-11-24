# UI redesign, PR 6: clean-up — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the redesign.
- Remove the dead UI code and the old styles.
- Turn the last English engine messages the UI shows (snapshot and config
  file load errors) into codes that the UI translates.
- Pay off the small debts the PR 2–5 reviews carried over.
- Bring README, ARCHITECTURE and CONTRIBUTING up to date.

**Architecture:** There is no new screen. Each task is a self-contained
clean-up with its own tests. The only engine-facing change is the
load-error type of `persist::load_json`. It follows the same pattern as
PR 1's drift reasons:
- an enum carries the parameters;
- its `Display` gives exactly today's English sentence, so reports, history
  and logs don't change;
- the UI translates it through `i18n::messages`.

**Stacking:** This branch (`feat/ui-cleanup`) starts from `feat/ui-history`
(PR 5, open and unmerged). It is opened as a **draft PR** with base
`feat/ui-history`. Once PR 5 is squash-merged it is rebased onto `main`
(`git rebase --onto main <PR 5 tip>`) and retargeted to `main`.

**Tech stack:** Rust 1.88, egui/eframe 0.29.1, the `ui::kit` and `i18n`
modules.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-redesign-design.md`. Read
§2 (non-goals: only the *shape* of messages may change, never when or why
they are produced), §6.4 (the only untranslated text allowed is a raw OS
error detail) and §7 item 6. The carry-over list is in the PR 2–5 reviews,
summarised in each task below.

## Global constraints

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and `cargo test --locked` stay green on every commit.
- **Engines keep their behaviour.** Engine changes are limited to:
  - `persist::load_json`'s error type and the store loaders that return
    it (Task 2), whose `Display` must stay byte-identical;
  - one message's wording in `src/associations/apply.rs`;
  - one doc comment in `src/models.rs`.

  Engine tests keep every assertion. Only the assertions about the
  *shape* of these messages may change, to the code.
- **Every safety step stays:**
  - confirmations;
  - snapshot before writing;
  - blocked state while a store is unreadable;
  - "an unreadable `config.json` is never overwritten";
  - refusal while a job runs;
  - the close guard.
- **Texts.**
  - Every text the UI draws comes from a literal key in both `.resx`
    files.
  - Only raw OS or serde error details may stay untranslated, after a
    translated summary.
- **No visual change** except where a task says so. This PR is a
  clean-up.
- **Tests stay headless.**

## Review focus

1. **A corrupt `backups.json`, `tweaks-backups.json` or `config.json` at
   startup, in French.**
   - The toast and the banners read as French sentences around the raw
     detail.
   - Writing stays blocked.
   - `config.json` is never overwritten (Task 2).
2. **A tweak restore job.** It must still show the "Restoring" progress
   title and be logged as "Restore" once the job kind no longer comes
   from the English job title (Task 1).
3. **A `…` menu or other popup open when a Home tile or banner switches
   page.** The popup must close, like it does for a nav click (Task 3).
4. **The icon picker opened for a rule whose icon is a folder or light
   variant.** It opens on the category that shows that icon (Task 4).
5. **Many rules.** Rule validation runs once per frame, not once per row
   or per widget. The results are the same (Task 4).

---

### Task 1: Dead code and old styles

**Files:**
- Modify:
  - `src/theme.rs`: keep only `ThemeMode`, or move it to `src/models.rs`
    and delete `theme.rs`. Your call; record it in the report.
  - `src/ui/jobs.rs`.
  - `src/ui/tweaks_tab.rs`.
  - `src/ui/mod.rs`: the stale labels in `ActiveTab::title()`.
  - `src/ui/kit/tokens.rs`.
  - `src/ui/kit/controls.rs`.
  - every importer of the removed items.
  - `i18n/*.resx`: drop keys nothing uses, if the key-usage test still
    passes.

**What goes:**
- **Old theme helpers.** `FluentColors`, `get_palette`, `card_frame`,
  `sidebar_frame`, `accent_button` and `draw_badge` are unused since PR 5.
  Delete them.
- **Job kind from the English title.**
  - `RunningJob::Tweaks(job, module)` decides "restore" vs "apply" with
    `j.title().starts_with("Restoring")` (jobs.rs ~56 and ~78).
  - Replace that with an explicit kind carried by the variant, for
    example `RunningJob::Tweaks { job, module, restore: bool }` or a
    `TweakJobKind` enum.
  - The engine job titles stay English and internal.
- **`ActiveTab::title()`.** It returns labels used only as the stored
  history module names. Rename it to `history_module(self) ->
  &'static str`, return only the stored labels ("File associations",
  "Explorer & Taskbar", "Start & Privacy"), and make it `Option` or
  `unreachable!` for pages with no module. Keep `history.jsonl` labels
  byte-identical.
- **kit/tokens.rs.** `SEARCH_WIDTH` sits between the "/// Spacing scale"
  doc line and `pub mod space`. Move it above the doc line so `space`
  keeps its doc comment.
- **kit/controls.rs.** The `PillKind` → palette color `match` is written
  in both `pill` and `status_dot`. Extract
  `fn kind_color(p: &Palette, kind: PillKind) -> Color32`.

- [ ] **Step 1: Write the failing tests:**
  - `a_tweak_restore_job_is_titled_and_logged_as_restore`. Run
    `start_restore_tweaks`. The progress title uses
    `job.restore_tweaks` and the history action is `"Restore"`. Pin the
    behaviour first: run it on the current code, where it should pass.
    Then change the code, and the test must stay green.
  - `history_module_labels_are_the_stored_ones`. For Associations,
    Tweaks and StartPrivacy, the labels are exactly the three stored
    strings.
- [ ] **Step 2: Implement** the removals and refactors. `cargo build`
  must show no dead-code warnings.
- [ ] **Step 3: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 4: Commit**: `refactor(ui): remove the old theme helpers;
  job kind and history labels without English matching`

### Task 2: Load errors as codes

**Files:**
- Modify: `src/persist.rs`, `src/associations/store.rs`, `src/tweaks.rs`
  (the `load_tweak_store` signature only), `src/i18n/messages.rs`, the
  UI's store and config error fields and every banner or toast that
  shows them (`src/ui/mod.rs`, `settings.rs`, `tweaks_tab.rs`,
  `originals.rs`, `polish.rs`, `autosave.rs` as found),
  `src/associations/apply.rs` (one message), `src/models.rs` (one doc
  comment), `i18n/*.resx`

**Interfaces:**
- Produces:
  - In `persist.rs`:
    `#[derive(Debug, Clone, PartialEq)] pub enum LoadError { Corrupt { path: PathBuf, detail: String }, Unreadable { path: PathBuf, detail: String } }`.
  - `impl Display for LoadError`. It gives exactly today's two English
    sentences:
    - `"{path} is unreadable ({detail}). It was left untouched: fix or move it before applying changes."`
    - `"Cannot read {path}: {detail}"`
  - `load_json<T>(path) -> Result<T, LoadError>`. `save_json` is
    unchanged.
  - `load_backup_store` and `load_tweak_store` return `Result<_,
    LoadError>`.
  - The UI fields `backup_store_error`, `tweak_store_error` and
    `config_error` become `Option<LoadError>`.
  - Anything that needs the English sentence (history, reports) uses
    `.to_string()`.
  - `pub fn load_error(tr: Tr, e: &LoadError) -> String` in
    `messages.rs`, with two keys:
    - `load.corrupt`: "{0} can't be read ({1}). It was left untouched:
      fix or move it before applying changes." / "{0} est illisible
      ({1}). Il n'a pas été modifié : corrigez-le ou déplacez-le avant
      d'appliquer des changements."
    - `load.unreadable`: "Can't read {0}: {1}" / "Impossible de lire
      {0} : {1}"

    `{0}` is the path and `{1}` the raw detail.
- Every UI place that showed a store or config error string now shows
  `messages::load_error`. This covers the startup toast, the Settings
  banner, the tweak page banner, the History banners and the
  blocked-reason texts.
- `apply.rs`: "… (may be partially applied, use Rollback to restore it)"
  becomes "… (may be partially applied: restore it from History)". Update
  its test if one asserts the old text.
- `models.rs:48`: the doc comment "shown in the Rollback tab" becomes
  "shown in History".

- [ ] **Step 1: Write the failing tests:**
  - `load_error_display_is_unchanged`, in persist tests. It checks both
    variants' `to_string()` against the exact old sentences, using a
    corrupt file and a directory path, as the existing persist tests do.
  - `a_corrupt_store_reads_in_french`, in a UI test.
    1. Break `backups.json`.
    2. Set French and draw History and Settings.
    3. Assert that the drawn banner equals `messages::load_error(tr,
       &e)`, and that no English sentence from the old text ("It was
       left untouched") is drawn.
  - Keep every existing "unreadable store blocks writing" and "corrupt
    config is never overwritten" test green.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run** the full suite and clippy. Expected: PASS. This
  includes `shell_pages_draw_no_english_only_text_in_french`, which now
  also sees the translated banners.
- [ ] **Step 4: Commit**: `refactor(persist): load errors as codes,
  translated by the UI; no more Rollback-tab wording`

### Task 3: Shared test helpers and popups on page change

**Files:**
- Modify: `src/ui/test_support.rs`, every test module that copies these
  helpers (`associations_tab.rs`, `tweaks_tab.rs`, `originals.rs`,
  `history_panel.rs`, `gallery_tab.rs`, `shell.rs`, `icon_picker.rs`,
  `kit/tests.rs` as applicable), and `src/ui/shell.rs` (`open_page`)

**What changes:**
- **Shared helpers.** Move the duplicated helpers into
  `test_support.rs`: `hold_job` (returns the release sender),
  `click_at`, `hover_texts`, `centers`/`text_centers`, `row_widget`, and
  the tall-screen input. Delete the local copies and keep each test's
  assertions unchanged. Kit tests can't reach `ui::test_support`; leave
  `kit/test_input.rs` as the kit's own helper file.
- **Popups on page change.** Today only the nav click closes egui popups
  (shell.rs `show_nav`). Move the close into `open_page` itself, so Home
  tiles, the drift banner's Review button and Settings links close
  popups too. `open_page` has no `ctx`, so use one of these:
  - set a `close_popups_next_frame: bool` that `ui()` handles at the
    start of the next frame;
  - pass `ctx` to `open_page`.

  Pick one and record it in the report.

- [ ] **Step 1: Write the failing test:**
  `an_open_menu_closes_when_a_home_tile_opens_another_page`.
  1. Open a `…` menu on Explorer & Taskbar.
  2. Go Home through the nav.
  3. Click the Start & Privacy tile, then the Explorer tile.
  4. The menu item `tweaks.restore_original` is never drawn.
- [ ] **Step 2: Implement.** The helper move must not change any
  assertion. List every moved helper and every file touched in the
  report.
- [ ] **Step 3: Run** the full suite and clippy. Expected: PASS, with the
  same test count plus the new test.
- [ ] **Step 4: Commit**: `test(ui): shared test helpers; popups close on
  every page change`

### Task 4: Associations polish carried over from PR 3

**Files:**
- Modify: `src/ui/associations_tab.rs`, `src/ui/polish.rs`,
  `src/ui/icon_picker.rs`, `src/gallery.rs` (read-only use; no change
  unless a helper to classify one name is missing, in which case add a
  pure `IconCategory::of(name)` there with a unit test)

**What changes:**
- **Validation once per frame.** `find_rule_problems` runs up to 5 times
  per frame for the selected rule, and the "this index's problem" lookup
  is written 3 times. Compute the problems once per frame in
  `show_associations_tab`. Pass them to the bar, the header and
  `apply_rule_disabled_reason`, or cache them per frame behind one
  helper. Keep every result identical.
- **Rule rows.** They get the kit focus ring (2 px accent, as the nav
  rows have) and a `widget_info` label: the extension plus the state
  label.
- **Picker category.** The picker opens on the category of the rule's
  current icon. A file icon opens Files, a folder-* icon Folders, a
  *_light icon Light and a *.clone icon Clones. An unknown or empty
  icon opens All.
- **`editor_display_name`.** Keep the UI copy (PR 3 ruling: the
  engine's version returns English "Editor" for an empty path). Add a
  comment pointing to `detector.rs` so the two stay in sync.

- [ ] **Step 1: Write the failing tests:**
  - `the_picker_opens_on_the_current_icons_category`. Use the icon
    names `folder-src`, `rust_light`, `rust.clone`, `rust` and `""`.
  - `a_focused_rule_row_paints_a_focus_ring`. Use the same pattern as
    the kit focus-ring tests.
  - `rule_problems_are_computed_once_per_frame`. Count calls through a
    test-only counter, or assert through a structural test that the
    helper is called once. Pick the less intrusive option and explain
    it in the report.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 4: Commit**: `perf(ui): validate rules once per frame; rule
  row focus ring; picker opens on the icon's category`

### Task 5: Docs final pass and full French check

**Files:**
- Modify: `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, and
  `src/ui/shell.rs` (the French test, if a surface is still missing)

**What changes:**
- **Docs.** Read all three documents against the code, and fix anything
  stale from the redesign:
  - the module list and the page list;
  - how to add a page (kit, `owns_action_bar`, translation keys);
  - how to add a language (a `strings.<code>.resx` file);
  - the load-error codes;
  - the theme now living in the kit.

  Keep their style: short sentences, plain words.
- **French check.** It must draw every page: Home, Associations with the
  picker, both tweak pages, Icons, History and Settings. It must also
  draw a corrupt-store banner, now translated by Task 2.
- **Final checks.** `cargo fmt --check`, clippy, `cargo test --locked`
  and `cargo build --release --locked` must all pass.

- [ ] **Step 1:** Update the docs, and the French test if needed.
- [ ] **Step 2: Run** all checks. Expected: all green.
- [ ] **Step 3: Commit**: `docs: README, ARCHITECTURE and CONTRIBUTING
  after the redesign`

## Rulings made while writing this plan

- **Load errors become codes.** This extends the spec's message-code
  exception (§2) to `persist::load_json`. The PR 2 review asked for it,
  and §6.4 allows only raw OS details untranslated. `Display` stays
  identical, so reports and logs don't change. Cost if wrong: revert one
  error type.
- **Not in this PR (owner decisions, kept in the project notes):**
  - a "Forget snapshot" action for unknown tweaks;
  - a real Explorer restart in Settings;
  - an edit made less than 1 s before a failed close being lost;
  - re-clicking the current nav item not reloading stores;
  - `CustomReason::Unreadable`'s engine sentence.
