# UI redesign, PR 5: Icons page, History page, Rollback tab removed — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- Rebuild the **Icons** page on the kit.
- Turn **History** into the place where every saved original can be
  restored, with:
  - the list of operations, filterable and with expandable entries;
  - the saved originals, grouped by module, with "Restore original" on
    each item and "Restore all" on each group.
- Remove the **Selective Rollback** tab.

Every text is translated.

**Architecture:**
- **Icons page.** `src/ui/gallery_tab.rs` becomes a kit page:
  - a `page_header` with Import .ico and Build icon pack;
  - a read-only folder line with a link to Settings;
  - a search box and a `segmented` category choice;
  - the shared `icon_grid`, which on this page is drawn non-interactive
    (no hand cursor, no click sense).
- **History page.** `src/ui/history_panel.rs` keeps the operations list
  and the logging. A new `src/ui/originals.rs` draws the "Saved
  originals" section and builds its confirmations. Every restore click
  goes through an existing confirmation and runs exactly one existing
  job:
  - `start_rollback` for associations;
  - `start_restore_tweaks` for a tweak module.
- **Rollback removal.** `ActiveTab::Rollback`, its nav item,
  `rollback_tab.rs` and `rollback_selection` go. Opening History reloads
  `backups.json` and `tweaks-backups.json`, as opening Rollback did, and
  never under a running job.

**Tech stack:** Rust 1.88, egui/eframe 0.29.1, the `ui::kit` and `i18n`
modules.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-redesign-design.md`. Read:
- §5 (Rollback becomes part of History);
- §6.3 "Icons" and "History";
- §6.3 "Settings" (the Icons page header shows the icon folder read-only,
  with a link to Settings);
- §6.4.

This plan is PR 5 of §7.

## Global constraints

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and `cargo test --locked` stay green on every commit.
- One branch: `feat/ui-history`, from `main`.
- **Engines stay untouched.** This PR only changes `src/ui/`, `i18n/` and
  docs. It does not change `src/associations/`, `src/tweaks.rs`,
  `src/history.rs`, `src/gallery.rs` or the store formats.
- **Every safety step stays:**
  - a confirmation before every restore;
  - the refusal while a job runs;
  - the block while `backups.json` or `tweaks-backups.json` is
    unreadable;
  - tweak restores only ever touch the current user's snapshots.
- **There is no global "restore everything" button.** Each click runs
  exactly one existing job (spec §6.3 History).
- **No registry action from a keyboard shortcut.** Ctrl+F focuses the
  page's search box (Icons).
- **Texts.**
  - Every text drawn by these pages comes from `tr.t` / `tr.t_args` with
    a literal key, added to both `i18n/strings.resx` and
    `i18n/strings.fr.resx`.
  - New keys start with `icons.`, `history.` or `originals.`.
  - User data (extensions, ProgIDs, paths, icon names, timestamps) and
    the stored English module/action labels of an unknown, older
    `history.jsonl` entry are shown as is.
  - Sentence case, as short as possible.
- **Spacing, colors and sizes come from `kit` tokens.** New code contains
  no color literals. New reusable widgets go in `src/ui/kit/`.
- **Tests stay headless.** No test depends on system fonts.

## Review focus

These are cases a person will hit that no example in the spec covers.
Each one is pinned by a test in the task that owns it.

1. **One store unreadable, the other fine.**
   - When `backups.json` is broken, the associations group shows the
     error and its restore buttons are disabled with the reason.
   - The tweak groups still work.
   - The reverse case holds too (Task 3).
2. **A snapshot whose tweak id no longer exists in the catalog**, for
   example a tweak removed in a later version.
   - It is listed under a "Other" group with its raw id.
   - Its restore still goes through `request_restore_tweaks`.
   - It never panics (Task 3).
3. **Restoring while a job runs.** Every restore button is disabled with
   the busy reason, and the confirmation can't start a second job
   (Task 3).
4. **An older `history.jsonl` whose module label is unknown.** It is
   shown as stored, and the module filter's "All" still lists it
   (Task 2).
5. **An empty icon folder, or a search with no match, on the Icons
   page.** The folder case shows an empty state with the folder path and
   a link to Settings. The search case shows a no-match message, and
   nothing crashes (Task 1).

---

### Task 1: The Icons page on the kit

**Files:**
- Modify: `src/ui/gallery_tab.rs`, `src/ui/icon_grid.rs` (an
  `interactive` flag), `src/ui/icon_picker.rs` (pass `true`),
  `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `icon_grid(ui, ctx, gallery, names, current, id_salt, interactive: bool) -> Option<String>`.
    - With `interactive = false`, the cells sense hover only: no hand
      cursor, no hover fill, no focus. It always returns `None`.
    - The picker passes `true`.
  - `fn import_icons(&mut self)` and `reload_gallery` keep their roles.
    Their toasts become `icons.folder_error` ("Can't create the icon
    folder: {0}") and `icons.imported` ("Imported {0} of {1} .ico
    files.").
- Consumes: `kit::page_header`, `kit::segmented`, `kit::empty_state`,
  `kit::primary_button` / `secondary_button`, `open_page`,
  `polish::search_box_id(ActiveTab::Gallery)`,
  `focus_search_if_requested`, and `start_build_pak` (unchanged; it is
  refused while busy).

**Page** (`page_panel`):
1. **`page_header`.**
   - Breadcrumb `["WinTune", t("nav.icons")]`, title `nav.icons`,
     subtitle `icons.subtitle` ("Icons used by your file associations").
   - Actions, right to left:
     - **Build icon pack** (`icons.build`, primary), disabled while
       busy with the busy reason;
     - **Import .ico** (`icons.import`, secondary).
2. **Folder line.** `t_args("icons.folder", [folder])` ("Folder: {0}")
   as secondary text, followed by a link-style button `icons.change_in_settings`
   that calls `open_page(ActiveTab::Settings)`. The folder is no longer
   editable here.
3. **Search box**, with hint `icons.search`, plus a `kit::segmented`
   with the 5 categories. It reuses the picker's keys
   (`picker.cat.all`, `.files`, `.folders`, `.light`, `.clones`).
4. **The grid**, `interactive = false`, with id salt `gallery_page`.
   - With no icons at all, it shows `kit::empty_state(Icon::Icons,
     t_args("icons.empty", [folder]), Some(t("icons.import")))`. Its
     action calls `import_icons`.
   - With icons but no match, it shows `t_args("icons.no_match",
     [query])` as secondary text.

**Keys** (en / fr):
- `icons.subtitle`: "Icons used by your file associations" / "Icônes
  utilisées par vos associations de fichiers"
- `icons.build`: "Build icon pack" / "Créer le pack d'icônes"
- `icons.import`: "Import .ico" / "Importer des .ico"
- `icons.folder`: "Folder: {0}" / "Dossier : {0}"
- `icons.change_in_settings`: "Change in Settings" / "Modifier dans les
  paramètres"
- `icons.search`: "Search icons" / "Rechercher une icône"
- `icons.empty`: "No icons in {0}" / "Aucune icône dans {0}"
- `icons.no_match`: "No icon matches \"{0}\"." / "Aucune icône ne
  correspond à « {0} »."
- `icons.folder_error`: "Can't create the icon folder: {0}" /
  "Impossible de créer le dossier d'icônes : {0}"
- `icons.imported`: "Imported {0} of {1} .ico files." / "{0} fichier(s)
  .ico importé(s) sur {1}."

- [ ] **Step 1: Write the failing tests** in `gallery_tab.rs`:
  - `icons_page_tiles_do_not_react`. Hovering a tile on the Icons page
    sets no hand cursor (`out.platform_output.cursor_icon` stays
    `Default`). Clicking it changes nothing, and the page draws no focus
    ring after Tab.
  - `the_picker_grid_still_clicks`. Keep the picker tests green.
  - `change_in_settings_opens_settings`. Clicking the
    `icons.change_in_settings` button makes `active_tab == Settings`.
  - `an_empty_folder_offers_import`. With `TempIcons::new(&[])`, the
    `icons.empty` text with the folder is drawn, and so is the
    `icons.import` action.
  - `a_search_without_match_says_so`.
  - Keep `gallery_columns_stay_aligned_with_a_long_name` and
    `clicking_an_icon_on_the_icons_page_changes_no_rule`.
- [ ] **Step 2: Run** `cargo test --locked -- ui::gallery_tab ui::icon_picker`.
  Expected: FAIL.
- [ ] **Step 3: Implement.** Remove `crate::theme` imports from
  `gallery_tab.rs`.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): Icons page on the kit; the page's
  grid only browses`

### Task 2: History — operations list

**Files:**
- Modify: `src/ui/history_panel.rs`, `src/ui/mod.rs` (a
  `history_filter` field), `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub(super) enum HistoryFilter { All, Associations, Explorer, Start }`
    with `fn matches(self, stored_module: &str) -> bool`:
    - `All` matches anything, including unknown labels;
    - the others match the stored English labels `"File associations"`,
      `"Explorer & Taskbar"` and `"Start & Privacy"`.
  - `WinTuneApp.history_filter: HistoryFilter`, defaulting to `All`.
  - `fn show_operations(&mut self, ui: &mut egui::Ui)`, the section
    drawn by `show_history_page` (Task 3 adds the second section below
    it).
- Consumes: `kit::section`, `kit::segmented`, `kit::disclosure`,
  `kit::empty_state`, `messages::{history_module, history_action}`.

**Section `history.operations`** ("Operations"):
- **Filter.** A `segmented` choice between `history.filter.all`,
  `history.module.associations`, `history.module.explorer` and
  `history.module.start`.
- **Entries.** One `kit::disclosure` per matching entry, closed by
  default, with an id salt built from the entry's index and time. Its
  title reads `"{time} · {module} · {action} · {summary}"`, where the
  summary is today's `history.ok` / `history.ok_errors`.
- **Opened entry:**
  - the succeeded items (`history.succeeded`, "Done: {0}"), joined with
    ", ";
  - each error, in `danger`;
  - each warning, in `warning`;
  - when `reg_backup` is set, the path as secondary text plus a
    `history.copy_reg` button, which copies it.
- **Empty states:**
  - no entries at all: `kit::empty_state(Icon::History,
    t("history.empty"), None)`;
  - none matching the filter: secondary text `history.no_match`.

**Keys** (en / fr):
- `history.operations`: "Operations" / "Opérations"
- `history.filter.all`: "All" / "Tout"
- `history.succeeded`: "Done: {0}" / "Effectué : {0}"
- `history.no_match`: "No operation for this module." / "Aucune
  opération pour ce module."

- [ ] **Step 1: Write the failing tests** in `history_panel.rs`:
  - `the_filter_keeps_only_the_chosen_module`. Record one associations
    entry and one tweaks entry. Set `history_filter = Explorer` and
    check that only the Explorer entry's title is drawn.
  - `an_unknown_module_is_shown_as_stored_under_all`. Insert an entry
    with module `"Legacy thing"`. It is drawn under `All` and hidden
    under `Associations`.
  - `opening_an_entry_shows_its_errors_and_reg_path`. Use an entry
    with one error and a `reg_backup`. Click its disclosure header and
    assert that the error text and the path are drawn.
  - Keep the existing logging tests.
- [ ] **Step 2: Run** `cargo test --locked -- ui::history_panel`.
  Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): History operations — module filter
  and expandable entries`

### Task 3: History — saved originals

**Files:**
- Create: `src/ui/originals.rs`
- Modify: `src/ui/history_panel.rs` (calls the section),
  `src/ui/mod.rs` (module declaration), `src/ui/shell.rs` (`open_page`
  reloads both stores when opening History and not busy),
  `src/ui/confirm.rs` (`request_restore_all_rules`), `i18n/strings.resx`,
  `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `pub(super) enum OriginalsModule { Associations, Explorer, Start, Other }`.
    Each tweak id maps to a module through `find_tweak` and
    `tweak_groups_of`. An unknown id maps to `Other`.
  - `fn show_originals(&mut self, ui: &mut egui::Ui)` in
    `originals.rs`.
  - `pub(super) fn request_restore_all_rules(&mut self)` in
    `confirm.rs`. It builds the same `Confirm` as `request_rollback`,
    with every extension in `backup_store`, through the existing
    `confirm_rollback` helper. It does nothing when the store is empty.
  - `open_page(ActiveTab::History)` reloads `backup_store` and
    `tweak_store` when `!is_busy()`.
- Consumes: `request_restore_rule(ext)` (PR 3),
  `request_restore_tweaks(ids)`, `user_backups_in` (make it
  `pub(super)`), `busy_or_blocked`, `kit::section`, `kit::SettingRow`,
  `kit::banner`, the kit buttons, and `messages::tweak_title`.

**Section `originals.title`** ("Saved originals"), with the subtitle
line `originals.subtitle` ("What Windows had before WinTune changed it.
Restoring puts it back."):
- **One `kit::section` per module that has snapshots**, in the order
  Associations, Explorer & Taskbar, Start & Privacy, Other. The titles
  are `history.module.*`, and `originals.other` for Other.
- **Section header action: Restore all** (`originals.restore_all`,
  "Restore all ({0})").
  - For Associations it calls `request_restore_all_rules`.
  - For a tweak module it calls `request_restore_tweaks(ids of that
    module)`.
  - It is disabled with `busy_or_blocked(the module's store error)`.
- **Rows.**
  - **Associations:** `SettingRow { title: ext, description:
    t_args("originals.assoc_desc", [original ProgID or
    t("originals.none"), timestamp]) }`. Its trailing button
    `originals.restore` calls `request_restore_rule(ext)`.
  - **Tweaks:** the title is the tweak title, or the raw id for Other.
    The description is `t_args("originals.saved_on", [timestamp])`.
    The button calls `request_restore_tweaks(vec![id])`.
  - Every restore button is disabled with the same busy-or-blocked
    reason as its section.
- **When a store is unreadable**, its module sections are replaced by a
  `kit::banner(Error, t_args("originals.unreadable", [file name, e]))`,
  and no restore buttons are drawn for that store.
- **When no snapshot exists at all**, the section shows
  `kit::empty_state(Icon::Restore, t("originals.empty"), None)`.

**Keys** (en / fr):
- `originals.title`: "Saved originals" / "Originaux enregistrés"
- `originals.subtitle` (above) / "Ce que Windows avait avant que WinTune
  le modifie. Restaurer le remet en place."
- `originals.restore_all`: "Restore all ({0})" / "Tout restaurer ({0})"
- `originals.restore`: "Restore original" / "Restaurer l'original"
- `originals.assoc_desc`: "Original: {0} · saved {1}" / "Original : {0}
  · enregistré le {1}"
- `originals.saved_on`: "Saved {0}" / "Enregistré le {0}"
- `originals.none`: "none" / "aucun"
- `originals.other`: "Other" / "Autres"
- `originals.unreadable`: "{0} can't be read, so its restores are
  blocked: {1}" / "{0} est illisible, ses restaurations sont donc
  bloquées : {1}"
- `originals.empty`: "Nothing saved yet. WinTune saves the original the
  first time it changes something." / "Rien d'enregistré pour l'instant.
  WinTune enregistre l'original la première fois qu'il modifie quelque
  chose."

- [ ] **Step 1: Write the failing tests** in `originals.rs`:
  - `each_module_lists_its_snapshots_and_restore_asks_first`:
    1. Apply the `.foo` rule, `show_hidden` (Explorer) and
       `no_app_launch_tracking` (Start).
    2. Open History and draw.
    3. Assert that the three module titles and their item titles are
       drawn.
    4. Clicking `.foo`'s restore sets `pending_confirm` to
       `Rollback([".foo"])`. Nothing runs.
  - `restore_all_on_a_tweak_module_only_targets_that_module`. Clicking
    Explorer's Restore all sets `RestoreTweaks(["show_hidden"])`.
  - `restore_all_on_associations_targets_every_rule`. Use two applied
    rules; the result is `Rollback` with both, sorted.
  - `a_broken_backups_file_blocks_only_the_associations_group`:
    1. Break `backups.json` and reload.
    2. Assert that the `originals.unreadable` banner is drawn.
    3. The tweak group's restore still sets `pending_confirm`.
  - `a_broken_tweak_store_blocks_only_the_tweak_groups`.
  - `an_unknown_tweak_id_is_listed_under_other`:
    1. Insert a backup for id `"gone_tweak"` into
       `tweak_store.users[user]`.
    2. Assert that `originals.other` and `"gone_tweak"` are drawn.
    3. Clicking its restore sets `RestoreTweaks(["gone_tweak"])`.
  - `while_a_job_runs_every_restore_is_disabled`. Hold a job, then
    hover a restore button: the busy reason shows. A click sets no
    `pending_confirm`.
  - `opening_history_reloads_the_stores_unless_busy`. Mirror the old
    Rollback reload test in shell.rs.
  - `with_no_snapshot_the_section_says_so`.
- [ ] **Step 2: Run** `cargo test --locked -- ui::originals ui::shell`.
  Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): History saved originals — restore
  per item and per module`

### Task 4: Remove the Rollback tab

**Files:**
- Delete: `src/ui/rollback_tab.rs`
- Modify:
  - `src/ui/mod.rs`: `ActiveTab::Rollback`, `ALL`, `title`, the page
    match and `rollback_selection`;
  - `src/ui/shell.rs`: the nav item and `open_page`;
  - `src/ui/confirm.rs`: `request_rollback` is replaced by
    `request_restore_all_rules` and `request_restore_rule`;
  - `src/ui/polish.rs`: drop `rollback_disabled_reason` and the
    `rollback.tick_one` usage;
  - `src/ui/jobs.rs`: the `rollback_selection.clear()` line;
  - every test that names `ActiveTab::Rollback` or
    `rollback_selection`;
  - `i18n/strings.resx` and `i18n/strings.fr.resx`: remove keys that
    nothing uses any more (`nav.rollback`, `rollback.tick_one`), only
    if the key-usage test confirms they are unused.

**Interfaces:**
- Produces: the navigation, in the spec §5 order: Home; Customize (File
  associations, Explorer & Taskbar, Start & Privacy, Context menu);
  Tools (Icons, History); Settings at the bottom.
- Consumes: Task 3's History restores.

- [ ] **Step 1: Write the failing test** in `shell.rs`:
  `the_nav_has_no_rollback_entry_and_follows_the_spec_order`. Draw the
  nav and collect its labels in order. They equal the §5 list, read
  from `nav.*` keys.
- [ ] **Step 2: Run** `cargo test --locked -- ui::shell`. Expected:
  FAIL.
- [ ] **Step 3: Remove the tab.**
  - Port every Rollback-only test that still pins a behaviour to the
    History restores. Examples: the reload-not-under-job test, and the
    confirmation tests in `confirm.rs`.
  - Delete the tests whose behaviour is gone, such as the selection
    tests.
  - In the report, list each deleted test and why.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): History replaces the Rollback tab`

### Task 5: French rendering check and docs

**Files:**
- Modify: `src/ui/shell.rs` (the French test), `ARCHITECTURE.md`,
  `README.md` (only where it mentions the Rollback tab or the old Icons
  page)

- [ ] **Step 1: Extend** `shell_pages_draw_no_english_only_text_in_french`.
  Draw `ActiveTab::Gallery`, with icons and with a no-match search.
  Draw `ActiveTab::History` with:
  - one opened operation entry that has an error and a `.reg` path;
  - saved originals in Associations, Explorer & Taskbar, Start &
    Privacy and Other;
  - an unreadable `tweaks-backups.json` banner.

  Assert positively that these were drawn:
  - `icons.build`;
  - `history.operations`;
  - `originals.title`;
  - `originals.restore_all`;
  - `originals.unreadable`.
- [ ] **Step 2: Run** `cargo test --locked --lib shell_pages_draw_no_english_only_text_in_french`.
  Expected: PASS. Fix any French value or literal it reports.
- [ ] **Step 3: Update the docs.**
  - ARCHITECTURE.md: the History page (operations plus saved
    originals), the Rollback tab being gone, and the Icons page as a
    browse-only kit page.
  - README.md: any mention of the Rollback tab.
- [ ] **Step 4: Run** `cargo fmt --check`, clippy, `cargo test --locked`
  and `cargo build --release --locked`. Expected: all green.
- [ ] **Step 5: Commit**: `test(i18n): French check covers Icons and
  History; docs`

## Rulings made while writing this plan

- **The Icons page's folder becomes read-only, with a link to
  Settings,** as the spec says. Editing it moves to Settings, which
  already has it. Cost if wrong: one text box back.
- **Tweak snapshots of a removed tweak go under "Other"**, with their
  raw id, instead of being hidden. Cost if wrong: hiding one group.
- **The order of History's two sections is Operations, then Saved
  originals.** The spec lists them in that order. Cost if wrong:
  swapping two calls.
- **History gets no action bar.** Its actions are per row and per group.
  The shell's progress bar still shows during a job. Cost if wrong: an
  `owns_action_bar` entry.
