# UI redesign, PR 3: File associations — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the File associations screen on the kit:
- a rule list on the left;
- the selected rule's detail on the right;
- an icon picker dialog that replaces the jump to the Icons tab;
- an action bar owned by the page.

Every text on the screen is translated.

**Architecture:**
- **Layout.** `show_associations_tab` draws, in this order:
  1. the page's action bar (`page_action_bar`, Task 4);
  2. the list (a kit-styled `SidePanel`, Task 3);
  3. the detail (`CentralPanel` with `page_header` and sections, Task 4).
- **Icon picker.** It is a kit `dialog` with a search box, category
  choice, and the icon grid. The grid is extracted from `gallery_tab.rs`
  into `src/ui/icon_grid.rs`, so the Icons page and the picker share it.
  The picker finds its rule again by extension, never by index.
- **Action bar.** There is one action bar per frame. The shell draws its
  progress bar only for pages that don't own one. The associations page
  owns its bar, and that bar shows the job's progress while a job runs.
- **Saving.** Nothing new is needed: PR 2's autosave already saves every
  change to `config.extensions`. This PR only makes sure that every edit,
  including the picker's, goes through `config`.

**Tech stack:** Rust 1.88, egui/eframe 0.29.1, the `ui::kit` and `i18n`
modules from PRs 1 and 2.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-redesign-design.md`. Read
§6.2 (Segmented, Disclosure, ActionBar, Dialog), §6.3 "File associations"
and "Icons" (the shared grid), and §6.4. This plan is PR 3 of §7.

## Global constraints

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and `cargo test --locked` stay green on every commit.
- One branch: `feat/ui-associations`, from `main`.
- **Engines stay untouched.** This PR only changes `src/ui/`, `i18n/` and
  docs. It does not change `src/associations/`, `src/models.rs` or the
  store formats.
- **Every safety step stays:**
  - the confirmation before Remove rule and Restore original;
  - the refusal while a job runs;
  - the block while `backups.json` is unreadable;
  - the disabled Apply, with its reason shown on hover, when an enabled
    rule is invalid or the editor is missing;
  - "an unreadable `config.json` is never overwritten".
- **No registry action from a keyboard shortcut.**
  - Ctrl+F focuses the rule search box.
  - A focused button may still be pressed with Enter or Space.
- **Texts.**
  - Every text drawn by the associations page and the picker comes from
    `tr.t` / `tr.t_args` with a literal key, added to both
    `i18n/strings.resx` and `i18n/strings.fr.resx`.
  - New keys start with `assoc.` or `picker.`.
  - Drift reasons and rule problems go through `i18n::messages`
    (`drift_reason`, `rule_problem`).
  - User data (extensions, descriptions, ProgIDs, commands, icon names,
    paths) is shown as is.
  - Sentence case, as short as possible.
- **Spacing, colors and sizes come from `kit` tokens.** New code contains
  no color literals. New reusable widgets go in `src/ui/kit/`.
- **Don't open egui popups inside a kit dialog** (no `ComboBox`, menu or
  context menu). They draw below the dialog layer.
- **Tests stay headless.** No test depends on system fonts.

## Review focus

These are cases a person will hit that no example in the spec covers.
Each one is pinned by a test in the task that owns it.

1. **Renaming a rule's extension in Advanced options.**
   - The status is keyed by extension, so a renamed rule must not keep
     the old rule's "Up to date".
   - It shows "Checking status" until a fresh status arrives, and a
     refresh is requested.
   - A rename that makes the rule invalid or a duplicate turns its dot
     red and disables Apply with the reason (Task 4).
2. **The selected rule disappears.**
   - When it is removed, or is the last one, the detail shows the empty
     state with no panic, and the selection moves to a valid rule.
   - When the search filter hides it, the detail keeps showing it
     (Tasks 3 and 4).
3. **Choosing an icon after the picker's rule is gone or renamed.** The
   picker remembers the extension. If no rule has it any more, nothing is
   changed and an Info toast says so. It never writes into another rule
   (Task 2).
4. **An empty or missing icon folder.** The picker shows an empty state
   with the folder path, and Cancel and Escape close it. The rule's
   current icon may be missing from the gallery; the header then shows a
   blank 48 px slot (Tasks 2 and 4).
5. **A job running while the page is open.**
   - The page's bar shows the job's progress and draws no Apply, Remove
     or Restore.
   - No second action bar panel is drawn in the same frame.
   - The other pages still show the shell's progress bar (Task 4).

---

### Task 1: Kit additions and the PR 2 carry-over

**Files:**
- Modify: `src/ui/kit/controls.rs`, `src/ui/kit/mod.rs` (re-exports),
  `src/ui/kit/tests.rs`

**Interfaces:**
- Produces:
  - `pub fn status_dot(ui: &mut Ui, kind: Option<PillKind>, hover: Option<&str>) -> Response`
    in `controls.rs`.
    - It is an 8 px circle in a 16×16 slot: filled with the kind's
      palette color, or a 1 px `border`-colored ring when `kind` is
      `None` (not applied).
    - `hover`, when given, is the hover text.
  - `segmented` and `disclosure` keep their signatures. They gain:
    - a 2 px accent focus ring around the focused option or header, the
      same stroke the nav rows and tiles use (width `2.0`);
    - `widget_info`: each segmented option is `WidgetType::RadioButton`
      with its label and selected state, and the disclosure header is
      `WidgetType::CollapsingHeader` with its title.
  - The segmented options are focusable with Tab. Space or Enter on a
    focused option selects it.
- Consumes: the existing kit tokens and `current_palette`.

- [ ] **Step 1: Write the failing tests** in `src/ui/kit/tests.rs`:
  - `a_focused_segmented_option_paints_a_focus_ring`
    - Same pattern as `a_focused_nav_row_paints_a_focus_ring`.
    - No 2 px stroke before Tab, and one after it.
  - `space_on_a_focused_segmented_option_selects_it`
    - Options `[(1, "A"), (2, "B")]`, value `1`.
    - Press Tab twice, then Space.
    - Expect value `2` and `changed()` on that frame.
  - `a_focused_disclosure_header_paints_a_focus_ring`
  - `status_dot_draws_a_ring_when_not_applied`
    - `status_dot(ui, None, None)` paints a circle with a stroke and a
      transparent fill.
    - `Some(PillKind::Success)` paints a filled circle.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::kit`. Expected: the
  four new tests fail.
- [ ] **Step 3: Implement** the interfaces above.
- [ ] **Step 4: Run** `cargo test --locked --lib ui::kit` and clippy.
  Expected: PASS.
- [ ] **Step 5: Commit**: `feat(kit): status dot; focus rings and
  accessibility info on segmented and disclosure`

### Task 2: Shared icon grid and the icon picker dialog

**Files:**
- Create: `src/ui/icon_grid.rs` (the grid), `src/ui/icon_picker.rs` (the
  dialog state and drawing)
- Modify: `src/ui/gallery_tab.rs` (uses the grid and loses the "selecting
  icon for" mode), `src/ui/mod.rs` (fields, module declarations, draws
  the picker), `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - In `icon_grid.rs`:
    `pub(super) fn icon_grid(ui: &mut egui::Ui, ctx: &egui::Context, gallery: &IconGallery, names: &[String], current: Option<&str>, id_salt: &str) -> Option<String>`.
    - It returns the clicked icon's name.
    - It keeps PR 2's fixed 130×98 cells (`GALLERY_CELL`, `GALLERY_GAP`
      and `gallery_cell_rect` move here) and `show_rows` virtualisation.
    - It uses `ScrollArea::vertical().id_salt(id_salt)`, so the page and
      the picker don't share scroll state.
    - Cards use kit tokens: `surface` fill, 1 px `border`, `radius`
      card. The `current` icon's card has a 2 px accent border.
    - Each cell's `widget_info` is a Button labelled with the icon name.
    - Names are truncated, as in PR 2.
  - In `icon_picker.rs`:
    `pub(super) struct IconPicker { pub extension: String, pub query: String, pub category: IconCategory }`.
  - `WinTuneApp` gains `icon_picker: Option<IconPicker>`. The
    `assign_icon_to_selected_ext` field is removed, along with the Icons
    page's "Selecting icon for" badge and its assign-on-click.
  - `pub(super) fn open_icon_picker(&mut self, extension: &str)`. It
    opens with `category = IconCategory::FilesOnly`, an empty query, and
    focus requested on the search box.
  - `pub(super) fn show_icon_picker(&mut self, ctx: &egui::Context)`. It
    is called in `ui()` right after `show_confirm_dialog`, and does
    nothing when `icon_picker` is `None`.
    - It draws `kit::dialog(ctx, "icon_picker", title, 720.0, …)`.
    - Inside: a search `TextEdit`, a `kit::segmented` with the 5
      categories, the grid with a max height of 420, and a Cancel
      button.
    - With no names to show, it draws `kit::empty_state` with the text
      `picker.empty` and the icon folder path. Without a filter, the
      text is `picker.no_icons`.
    - Escape and Cancel close the picker.
  - `fn choose_icon(&mut self, name: &str)` (private, `icon_picker.rs`):
    - It finds the rule by the picker's `extension`
      (`eq_ignore_ascii_case`).
    - If found, it sets `icon_name` and pushes a Success toast
      `picker.assigned` ("{1} now uses the {0} icon.").
    - If not found, it pushes an Info toast reusing `notice.rule_gone`.
    - Either way, it closes the picker.
  - Ctrl+F does nothing while the picker is open (`handle_shortcuts`
    returns early, as for `pending_confirm`).
- Consumes: `kit::dialog`, `kit::segmented`, `kit::empty_state`,
  `IconGallery::filter_names`, `IconGallery::texture`.

**Keys** (en / fr):
- `picker.title`: "Choose an icon" / "Choisir une icône"
- `picker.search`: "Search icons" / "Rechercher une icône"
- `picker.cat.all`: "All" / "Toutes"
- `picker.cat.files`: "Files" / "Fichiers"
- `picker.cat.folders`: "Folders" / "Dossiers"
- `picker.cat.light`: "Light" / "Claires"
- `picker.cat.clones`: "Clones" / "Clones"
- `picker.empty`: "No icon matches." / "Aucune icône ne correspond."
- `picker.no_icons`: "No icons in {0}" / "Aucune icône dans {0}"
- `picker.assigned`: "{1} now uses the {0} icon." / "{1} utilise
  maintenant l'icône {0}."
- `picker.cancel`: "Cancel" / "Annuler"

- [ ] **Step 1: Write the failing tests** in `icon_picker.rs` (use
  `TempIcons::new(&["foo", "bar"])` and `app_with_rules(&ctx, &[".foo"])`):
  - `choosing_an_icon_sets_it_on_the_rule_and_closes`
    1. `open_icon_picker(".foo")`.
    2. Draw a frame on a 1180×760 screen, then click the `bar` tile
       (`text_center(&out, "bar")`, three-frame click).
    3. Assert `config.extensions[0].icon_name == "bar"` and
       `icon_picker.is_none()`.
  - `the_picker_choice_is_autosaved`
    1. Pick `bar` as above.
    2. Run frames at `time` 0 to 1.5 s.
    3. Assert that `config.json` on disk holds `"icon_name": "bar"`.
  - `choosing_an_icon_for_a_removed_rule_changes_nothing`
    1. Open for `.foo`.
    2. Rename the rule's extension to `.baz`.
    3. Call `choose_icon("bar")`.
    4. Assert `icon_name` is unchanged, the picker is closed, and
       `last_notice()` contains `.foo`.
  - `escape_closes_the_picker`
  - `an_empty_icon_folder_shows_the_empty_state`
    1. Use `TempIcons::new(&[])`.
    2. Open the picker and draw a frame.
    3. `drawn_texts` contains `t_args("picker.no_icons", [folder])`.
  - `ctrl_f_does_nothing_while_the_picker_is_open`: `focus_search` stays
    `false`.
  - In `gallery_tab.rs`, update
    `gallery_columns_stay_aligned_with_a_long_name` to the shared grid
    (constant paths only), and add
    `clicking_an_icon_on_the_icons_page_changes_no_rule`.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::icon_picker ui::gallery_tab`.
  Expected: FAIL (the module doesn't exist).
- [ ] **Step 3: Implement** the interfaces above. Move the grid code out
  of `gallery_tab.rs` instead of copying it.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): icon picker dialog over a shared icon
  grid`

### Task 3: Rule list and page header

**Files:**
- Modify: `src/ui/associations_tab.rs` (rewrite the list, the header bar
  and `add_extension_from_input`), `src/ui/polish.rs` (translate
  `apply_disabled_reason`), `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `pub(super) enum RuleState { Checking, NotApplied, UpToDate, Changed, Invalid }`
    in `associations_tab.rs`.
  - `pub(super) fn rule_state(&self, index: usize) -> RuleState`.
    - `Invalid` wins when `find_rule_problems` lists the index.
    - Otherwise the state is read from `rule_statuses` by extension:
      `None` gives `Checking`, `NotApplied` gives `NotApplied`,
      `Applied` gives `UpToDate`, and `Drifted(_)` gives `Changed`.
  - `pub(super) fn rule_state_pill(state: RuleState) -> Option<PillKind>`:
    - `UpToDate` gives `Success`;
    - `Changed` gives `Warning`;
    - `Invalid` gives `Danger`;
    - `NotApplied` and `Checking` give `None`.

    Task 4 uses the same mapping for its pill.
  - `apply_disabled_reason` returns translated text. It reuses
    `notice.still_running` (with `job.name.*`) and
    `notice.registry_blocked`, and adds `assoc.blocked.editor` and
    `assoc.blocked.problems` ("Fix these rules first:\n{0}").
- Consumes: `kit::page_header`, `kit::status_dot`, `kit::empty_state`,
  `polish::search_box_id`, `focus_search_if_requested`.

**The list** (`SidePanel::left("extensions_master_list")`, `page` fill,
kit margins, `resizable(true)`, default width 300):
- A search box (Ctrl+F, hint `assoc.search`). It filters on the
  extension, the description and the icon name, as today.
- One row per rule, 36 px high. Each row has:
  - the `enabled` checkbox;
  - a 20 px icon (a blank slot while it decodes);
  - the extension;
  - the `status_dot` of `rule_state_pill`, with hover text: the problem
    when invalid, otherwise the state label.

  Clicking a row selects it.
- A bottom row with a text box (hint `assoc.add_hint`) and a `+ Add`
  button (`assoc.add`).
- When there are no rules, `kit::empty_state(ui, Icon::Associations,
  &t("assoc.empty"), None)`. When the search matches nothing, show
  `t_args("assoc.no_match", [query])` as secondary text.

**Page header** (top of the detail column, drawn by Task 4's central
panel through `fn associations_header(&mut self, ui)` defined here):
- Breadcrumb `["WinTune", t("nav.associations")]`, title
  `nav.associations`, subtitle `assoc.subtitle`.
- Actions, right to left:
  - **Apply all enabled** (`assoc.apply_all`, primary). It is disabled
    with `apply_disabled_reason` on hover, and calls `start_apply` with
    all rules, as today.
  - **Check all / Uncheck all** (`assoc.check_all` /
    `assoc.uncheck_all`). This is one button: it reads "Check all" when
    any rule is unchecked, and checking all then unchecks all.
  - **Refresh** (`assoc.refresh`, hover `assoc.refresh_hint`). It is
    disabled while busy and calls `request_status_refresh`.

**Adding a rule:** the toasts in `add_extension_from_input` use
`assoc.exists` ("{0} is already in the list.") and `assoc.add_invalid`
("Can't add {0}: {1}", where `{1}` is `messages::rule_problem` of the
validation error).

- [ ] **Step 1: Write the failing tests** in `associations_tab.rs`:
  - `rule_state_maps_statuses_and_problems`. Rules `.a`, `.b`, `.c`,
    `.d` and `.x y`, with statuses `.a: Applied`, `.b:
    Drifted(vec![])`, `.c: NotApplied` and `.d` missing. Expect
    `UpToDate`, `Changed`, `NotApplied`, `Checking` and `Invalid`.
  - `check_all_then_uncheck_all`. With one rule unchecked, click the
    header's `assoc.check_all` (by `text_center`) and assert all are
    enabled. Click again (the label is now `assoc.uncheck_all`) and
    assert none are.
  - `a_filtered_out_selection_stays_in_the_detail`. Select `.b`, search
    "a", draw a frame, and assert that the detail title still names
    `.b`.
  - `the_list_shows_the_empty_state_without_rules`.
  - Update the existing `adding_an_invalid_extension_is_refused` and
    `polish.rs` tests to assert on translated keys
    (`tr.t_args("assoc.add_invalid", …)` or the key's English prefix)
    instead of English literals.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::associations_tab ui::polish`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the list, the header and the translated
  messages.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): associations rule list and page
  header on the kit`

### Task 4: Rule detail and the page-owned action bar

**Files:**
- Modify: `src/ui/associations_tab.rs` (detail, sections),
  `src/ui/shell.rs` (`page_action_bar`), `src/ui/mod.rs` (the shell
  progress bar only for pages without their own bar), `src/ui/confirm.rs`
  (`request_restore_rule`), `src/ui/rule_status.rs` (delete
  `status_badge` and `status_details` once unused), `i18n/strings.resx`,
  `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `impl ActiveTab { pub(super) fn owns_action_bar(self) -> bool }`. It
    is `true` for `Associations` only; PR 4 adds the tweak pages.
  - `pub(super) fn page_action_bar(&self, ctx: &egui::Context, status: Option<&str>, actions: impl FnOnce(&mut egui::Ui))`
    in `shell.rs`:
    - while a job runs, it draws `kit::action_bar` in the `Busy` state,
      with the same label and title as the shell's bar today;
    - otherwise, when `status` is `Some`, it draws the `Idle` state with
      `actions`;
    - otherwise it draws nothing.
  - In `ui()`, the shell's busy bar is drawn only when
    `!self.active_tab.owns_action_bar()`.
  - `pub(super) fn request_restore_rule(&mut self, extension: &str)` in
    `confirm.rs`. It builds the same `Confirm` as `request_rollback`,
    with `ConfirmAction::Rollback(vec![extension])` and the existing
    `confirm.rollback.*` keys.
  - `pub(super) fn apply_rule_disabled_reason(&self, index: usize) -> Option<String>`
    in `polish.rs`, next to `apply_disabled_reason`. It gives the bar's
    Apply rule below.
  - `fn editor_display_name(path: &str) -> String`:
    - a file stem of `code` gives `VS Code`, and `code - insiders` gives
      `VS Code Insiders` (case-insensitive);
    - any other path gives its file stem;
    - an empty path gives `""`.
- Consumes: `rule_state`, `rule_state_pill`, `associations_header`
  (Task 3), `open_icon_picker` (Task 2), `kit::segmented`,
  `kit::disclosure`, `kit::toggle`, `kit::SettingRow`, `kit::section`,
  `kit::pill`, `messages::drift_reason`, `messages::rule_problem`.

**Detail column** (`CentralPanel`, kit page margins, vertical scroll).
Without a selection, it shows `kit::empty_state(ui, Icon::Associations,
&t("assoc.select"), None)`. Otherwise it shows, in order:

1. **`associations_header`** (Task 3).
2. **Rule header.**
   - A 48 px icon, or a blank slot.
   - The title `"{ext} · {description}"`.
   - One secondary line:
     - Open in editor: `assoc.opens_with` ("Opens with {0}"), or
       `assoc.no_editor` when the name is empty;
     - Run: `assoc.runs` ("Runs the file · Edit opens {0}").
   - The state pill (`rule_state_pill`, label `assoc.state.*`).
   - A neutral `assoc.original_saved` pill when `backup_store` has the
     extension.
   - When the state is `Changed`, each drift reason on its own secondary
     line, through `messages::drift_reason`.
   - When it is `Invalid`, the problem in `danger` color.
   - For `.sh`, the WSL hint `settings.wsl_on` / `settings.wsl_off`.
3. **On double-click** (`kit::section`, `assoc.double_click`).
   - A `segmented` choice between `assoc.mode.editor` and
     `assoc.mode.run`.
   - Run adds a `SettingRow` with the launch command `TextEdit`
     (`assoc.launch_command`). It defaults to `"%1" %*`, as today.
4. **Appearance** (`assoc.appearance`). A `SettingRow` with the icon
   name, and a `assoc.change_icon` button that calls
   `open_icon_picker(ext)`.
5. **Options** (`assoc.options`). Two `SettingRow`s with toggles:
   - `reset_existing` (`assoc.clean_overrides`, with a description);
   - `reset_user_choice` (`assoc.ignore_open_with`). Its known limit
     (Windows may report "An app default was reset") is the row's
     **hover text** (`assoc.ignore_open_with_hint`), as the spec asks.
     It is today's hover text, translated.
6. **Advanced options** (`kit::disclosure`, id `assoc_advanced`, title
   `assoc.advanced`, closed by default). It holds `TextEdit` rows for:
   - extension;
   - ProgID;
   - description;
   - PerceivedType;
   - MIME type.

   It also holds:
   - the `preserve_existing_properties` toggle;
   - in Run mode, the "Run as administrator" toggle and its command
     (today's defaults);
   - the custom right-click verbs (add, edit and remove, with today's
     defaults for a new verb).

   When the extension `TextEdit` reports `changed()`, call
   `request_status_refresh`.

**Action bar** (`page_action_bar`, drawn before the list and the detail
panels):
- **Status**, from `rule_state`:
  - `assoc.bar.checking`;
  - `assoc.bar.not_applied` ("Not applied yet");
  - `assoc.bar.up_to_date`;
  - `assoc.bar.changed`;
  - `assoc.bar.invalid` ("Can't be applied: {0}").
- **Actions**, right to left:
  - **Apply** (primary, `assoc.apply`). It applies a clone of this rule
    with `enabled = true`, as today. It is disabled with
    `apply_rule_disabled_reason(index)` on hover. That reason is the
    same as `apply_disabled_reason`, but ignores the problems of
    *other* rules:
    - the busy and blocked reasons stay the same;
    - an empty editor gives `assoc.blocked.editor`;
    - this rule's own problem gives `assoc.blocked.this_rule`.
  - **Restore original** (`assoc.restore`). It is drawn only when a
    snapshot exists, and calls `request_restore_rule`.
  - **Remove rule** (`assoc.remove`). It calls `request_remove_rule`.
- With no rule selected, `status` is `None`. The bar then only appears
  during a job.

Delete `identity_card`, `behavior_card`, `verbs_card` and the old detail
code. Delete `status_badge` and `status_details` from `rule_status.rs` if
nothing else uses them.

- [ ] **Step 1: Write the failing tests** in `associations_tab.rs`:
  - `apply_in_the_bar_applies_only_this_rule`
    1. Use rules `.foo` and `.bar`, with `.foo` disabled and selected.
    2. Click `assoc.apply` and run the job to the end.
    3. Assert that `.foo`'s key exists and `.bar`'s does not.
  - `another_rules_problem_does_not_block_this_rules_apply`
    1. Use rules `.foo` and `.x y`, with `.foo` selected.
    2. Assert that the bar's Apply is enabled.
    3. With `.x y` selected, assert that it is disabled and that its
       hover reason is `assoc.blocked.this_rule`.
  - `restore_original_asks_first_and_is_hidden_without_a_snapshot`
    1. Without a backup, `assoc.restore` is not drawn.
    2. After applying `.foo`, it is drawn.
    3. Clicking it sets `pending_confirm` to
       `Rollback(vec![".foo"])`, and nothing runs.
  - `renaming_the_extension_requests_a_status_refresh`
    1. Statuses `.foo: Applied`.
    2. Set the extension to `.foo2` through `config` and draw a frame.
    3. Assert `rule_state(0) == Checking`.
    4. Typing into the Advanced extension box sets
       `status_refresh_needed`.
  - `removing_the_last_rule_shows_the_select_empty_state`
  - `while_a_job_runs_the_page_bar_shows_progress_and_no_actions`
    1. Hold a job, as in the PR 2 shell tests.
    2. `drawn_texts` contains the job's `job.*` title.
    3. It contains none of `assoc.apply`, `assoc.remove` or
       `assoc.restore`.
    4. Exactly one panel with id `wintune_action_bar` was laid out:
       check `ctx.memory(|m| m.areas()…)`, or count the action bar's
       status texts, which must appear once.
  - `other_pages_still_show_the_shell_progress_bar`. Update the existing
    `shell.rs` test that loops over `ActiveTab::ALL`, so that it still
    passes for every page.
  - `editor_display_name_maps_code_to_vs_code`:
    - `C:\x\Code.exe` gives `VS Code`;
    - `C:\x\Code - Insiders.exe` gives `VS Code Insiders`;
    - `C:\x\notepad++.exe` gives `notepad++`;
    - `""` gives `""`.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::associations_tab ui::shell`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the detail, the bar and `page_action_bar`.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): associations rule detail and
  page-owned action bar`

### Task 5: French rendering check and docs

**Files:**
- Modify: `src/ui/shell.rs` (the French test), `ARCHITECTURE.md`,
  `CONTRIBUTING.md` (only where they describe the associations screen, the
  Icons tab assign flow, or the action bar)

**Interfaces:**
- Consumes: everything above.

- [ ] **Step 1: Extend** `shell_pages_draw_no_english_only_text_in_french`:
  - Add `ActiveTab::Associations` to the drawn pages, with these rules:
    - `.a`, selected, whose status is `Drifted` with one reason;
    - `.b` with status `Applied`;
    - `.x y`, invalid.
  - Open the Advanced disclosure (set its memory state, or click its
    header).
  - Then draw once more with `open_icon_picker(".a")`.
  - The existing assertion stays: no English-only text is drawn.
- [ ] **Step 2: Run** `cargo test --locked --lib shell_pages_draw_no_english_only_text_in_french`.
  Expected: PASS once every key has its French text. Fix any key it
  reports.
- [ ] **Step 3: Update the docs.**
  - The associations screen layout.
  - The icon picker replaces "pick from gallery".
  - The rule for action bars: one bar per frame, owned by the page when
    `owns_action_bar` is set.
- [ ] **Step 4: Run** `cargo fmt --check`, clippy, `cargo test --locked`
  and `cargo build --release --locked`. Expected: all green.
- [ ] **Step 5: Commit**: `test(i18n): French check covers associations and
  the icon picker; docs`

## Rulings made while writing this plan

- **The Icons page no longer assigns icons on click.** The picker
  replaces that flow (spec: "The user no longer jumps to the Icons
  tab"). Cost if wrong: re-adding one click handler.
- **"Check or uncheck all" is one button whose label flips.** Cost if
  wrong: splitting it into two buttons.
- **Run as administrator and "preserve existing properties" live in
  Advanced options.** The spec lists the first there, and drops no
  existing control. Cost if wrong: moving one row.
- **The 5 icon categories use `segmented`**, not new "chips". Cost if
  wrong: a new kit widget in PR 5.
- **The extension stays editable, in Advanced options.** It was editable
  before, and a rename re-requests the status. Cost if wrong: making it
  read-only.
