# UI redesign, PR 4: the two tweak screens — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the "Explorer & Taskbar" and "Start & Privacy" screens
on the kit. Each has:
- a page header with "Restore all originals";
- a search box;
- one section per group, with one setting row per tweak;
- a `…` menu per row;
- an action bar owned by the page, holding Discard and Apply, scoped to
  the tab.

Every text is translated.

**Architecture:**
- **Shared code.** Both screens stay one implementation in
  `src/ui/tweaks_tab.rs`. Today's tab-scoping helpers are kept as they
  are: `tweak_groups_of`, `pending_tweak_changes_in`,
  `discard_tweak_changes`, `request_restore_all_tweaks`, and the jobs.
- **Layout.** `show_tweaks_tab` draws, in this order:
  1. the page's action bar (`page_action_bar` from PR 3);
  2. a `CentralPanel` with the page header, the search box, an optional
     blocked banner, and the group sections.
- **Action bar.** `ActiveTab::owns_action_bar` becomes true for
  `Tweaks` and `StartPrivacy`. The shell then stops drawing its own
  progress bar on those pages.
- **Overflow menu.** A new kit widget gives each row a `…` menu.

**Tech stack:** Rust 1.88, egui/eframe 0.29.1, the `ui::kit` and `i18n`
modules.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-redesign-design.md`. Read
§6.2 (SettingRow with its `…` menu, StatusPill, Toggle, ActionBar), §6.3
"Explorer & Taskbar, and Start & Privacy", and §6.4 ("when it becomes
visible" is part of the tweak catalog's texts). This plan is PR 4 of §7.

## Global constraints

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and `cargo test --locked` stay green on every commit.
- One branch: `feat/ui-tweaks`, from `main`.
- **Engines stay untouched.** This PR only changes `src/ui/`, `i18n/` and
  docs. It does not change `src/tweaks.rs`, `src/models.rs` or the store
  formats.
- **Every safety step stays:**
  - the confirmation before "Restore original" and "Restore all
    originals";
  - the refusal while a job runs;
  - the block while `tweaks-backups.json` is unreadable;
  - Windows 11-only tweaks are never sent on Windows 10;
  - each tab only applies, discards or restores the tweaks of its own
    groups.
- **No registry action from a keyboard shortcut.**
  - Ctrl+F focuses the search box.
  - A focused control may still be pressed with Enter or Space.
- **Texts.**
  - Every text drawn by these pages comes from `tr.t` / `tr.t_args` with
    a literal key, added to both `i18n/strings.resx` and
    `i18n/strings.fr.resx`.
  - New keys start with `tweaks.` or `refresh.`.
  - Tweak titles, descriptions and group names go through
    `i18n::messages`.
  - The only untranslated text allowed is a raw OS or engine error
    detail, shown after a translated summary.
  - Sentence case, as short as possible.
- **Spacing, colors and sizes come from `kit` tokens.** New code contains
  no color literals. New reusable widgets go in `src/ui/kit/`.
- **Tests stay headless.** No test depends on system fonts.

## Review focus

These are cases a person will hit that no example in the spec covers.
Each one is pinned by a test in the task that owns it.

1. **Pending changes on both tabs.**
   - The user flips a setting on Explorer & Taskbar, then one on Start &
     Privacy.
   - Each page's bar shows only its own count.
   - Discard on one page leaves the other page's change pending.
   - Apply on one page writes only that page's tweaks (Task 3).
2. **A tweak in the Custom state.**
   - It shows the Custom pill, with the translated reason on hover.
   - Its toggle shows off.
   - Flipping it either way is a pending change.
   - Discard brings it back to "no change" (Tasks 2 and 3).
3. **Windows 10 with Windows 11-only tweaks.**
   - The toggle is disabled, with `tweaks.win11_only` on hover.
   - The Windows 11 pill says it is unavailable.
   - A forced `tweak_desired` entry is never counted as pending nor
     sent (Task 2).
4. **An unreadable `tweaks-backups.json`.**
   - An error banner shows the translated summary and the raw detail.
   - Every toggle is disabled with the blocked reason.
   - The bar's Apply and "Restore all originals" are disabled with the
     reason, and nothing is written (Tasks 2 and 3).
5. **A job running while a tweak page is open.**
   - The page's bar shows the progress and no Discard or Apply.
   - Toggles and the `…` menu's "Restore original" are disabled.
   - Only one action-bar panel is drawn (Task 3).

---

### Task 1: Kit overflow menu, and translated shared reasons

**Files:**
- Modify: `src/ui/kit/controls.rs`, `src/ui/kit/mod.rs` (re-export),
  `src/ui/kit/tests.rs`, `src/ui/polish.rs`, `src/ui/tweaks_tab.rs` (the
  notices in `start_apply_tweaks` and `start_restore_tweaks` only),
  `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `pub struct MenuItem<'a> { pub label: &'a str, pub enabled: bool, pub disabled_hint: Option<&'a str> }`
    in `controls.rs`.
  - `pub fn overflow_menu(ui: &mut Ui, id_salt: &str, tooltip: &str, items: &[MenuItem<'_>]) -> Option<usize>`
    in `controls.rs`.
    - It draws a small `…` button: the `Icon::More` glyph, or the text
      `…` without the icon font. Its hover text and `widget_info` label
      are `tooltip`.
    - Clicking it opens a popup with one entry per item, using
      `egui::menu::menu_button` or `popup_below_widget`.
    - It returns the index of the clicked, enabled item, then the popup
      closes.
    - A disabled item is greyed out, shows its hint on hover, and never
      returns.
  - `busy_or_blocked` becomes `pub(super)`, so `tweaks_tab.rs` can use it
    (Task 2).
  - `busy_or_blocked`, `tweak_apply_disabled_reason`,
    `tweak_restore_all_disabled_reason` and `rollback_disabled_reason`
    (polish.rs) return translated text:
    - busy reuses `notice.still_running` with the job's `job.name.*`
      key, as `assoc_apply_blocked` does;
    - blocked is `tweaks.blocked` ("Changes are blocked: {0}");
    - nothing to apply is `tweaks.nothing_to_apply`;
    - nothing to restore is `tweaks.nothing_to_restore`;
    - rollback with an empty selection is `rollback.tick_one`.
  - The toasts in `start_apply_tweaks` and `start_restore_tweaks` use
    `tweaks.blocked` and `tweaks.nothing_to_apply`.
- Consumes: kit tokens, `Icon::More`, `icons::icon_text`.

**Keys** (en / fr):
- `tweaks.blocked`: "Changes are blocked: {0}" / "Les modifications sont
  bloquées : {0}"
- `tweaks.nothing_to_apply`: "Nothing to apply: turn a setting on or off
  first." / "Rien à appliquer : activez ou désactivez d'abord un
  réglage."
- `tweaks.nothing_to_restore`: "Nothing to restore: WinTune hasn't
  changed these settings." / "Rien à restaurer : WinTune n'a modifié
  aucun de ces réglages."
- `rollback.tick_one`: "Tick at least one extension to restore." /
  "Cochez au moins une extension à restaurer."

- [ ] **Step 1: Write the failing tests:**
  - In `src/ui/kit/tests.rs`:
    - `the_overflow_menu_returns_the_clicked_item`. Click `…`, then
      click the second item's text. Expect `Some(1)`.
    - `a_disabled_menu_item_never_returns`. Same steps, with item 0
      disabled. Clicking it returns `None`, and the popup stays open or
      closes without returning.
  - In `src/ui/polish.rs` and `src/ui/tweaks_tab.rs`, update the
    existing reason and notice tests to compare with
    `tr.t`/`tr.t_args` of the keys above, instead of English fragments.
    Examples: `an_unreadable_snapshot_file_blocks_tweak_changes`, and
    the polish tests that use "Tick" and "blocked".
- [ ] **Step 2: Run** `cargo test --locked --lib ui::kit ui::polish ui::tweaks_tab`.
  Expected: the new kit tests fail (the widget doesn't exist).
- [ ] **Step 3: Implement** the interfaces and keys above.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(kit): overflow menu; translated tweak and
  rollback reasons`

### Task 2: Tweak page body — header, search, sections, rows

**Files:**
- Modify: `src/ui/tweaks_tab.rs` (rewrite `show_tweaks_tab` and
  `tweak_row`), `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces:
  - `pub(super) fn refresh_label(tr: Tr, r: Refresh) -> String` and
    `pub(super) fn refresh_hint(tr: Tr, r: Refresh) -> String` in
    `tweaks_tab.rs`. They map `Live`, `NewWindows` and `ExplorerRestart`
    to the `refresh.*` keys below.
  - `pub(super) fn tweak_toggle_disabled_reason(&self, tweak: &Tweak) -> Option<String>`.
    The first matching reason wins:
    1. the busy reason;
    2. the blocked reason (both from `busy_or_blocked`, Task 1);
    3. `tweaks.win11_only` when the tweak isn't available.
- Consumes: `kit::page_header`, `kit::section`, `kit::SettingRow`
  (`hover: None`), `kit::toggle_enabled`, `kit::pill`,
  `kit::overflow_menu` and `kit::banner` (Task 1 and earlier),
  `messages::{tweak_title, tweak_description, group_title,
  custom_reason}`.

**Page** (`CentralPanel`, kit page margins, vertical scroll):
1. **`page_header`.**
   - Breadcrumb `["WinTune", nav title]`, where the nav title is
     `nav.explorer` or `nav.start`. The title is the same nav title.
   - Subtitle `tweaks.subtitle` ("Settings of {0}. The first change of
     each one saves the original, so it can be restored."), where `{0}`
     is `env.user_label`.
   - One action: **Restore all originals** (`tweaks.restore_all`,
     "Restore all originals ({0})"). It is disabled with
     `tweak_restore_all_disabled_reason` on hover, and calls
     `request_restore_all_tweaks(tab)`.
2. **Search box.** It keeps the existing `search_box_id(ActiveTab::Tweaks)`
   and `focus_search_if_requested`, with hint `tweaks.search`. It gets a
   Clear button (`tweaks.clear`) when not empty.
3. **Blocked banner.** When `tweak_store_error` is `Some(e)`, a
   `kit::banner(Error, t_args("tweaks.store_unreadable", [e]), None)`.
4. **One `kit::section` per group** of the tab, titled with
   `messages::group_title`. Groups with no visible tweak are skipped.
   When nothing matches, the page shows `t_args("tweaks.no_match",
   [query])` as secondary text.
5. **One `SettingRow` per tweak**:
   - `title`: the tweak title. `description`: the tweak description,
     followed on a second line by the refresh label in small secondary
     text, for example "Visible: in new windows".
   - **Trailing, right to left:**
     1. `kit::toggle_enabled(wanted, tweak_toggle_disabled_reason)`.
        Its change rule is today's: flipping back to the registry value
        of a non-Custom tweak removes the entry from `tweak_desired`.
     2. The `…` `overflow_menu`, only when `has_backup`. It has one
        item, `tweaks.restore_original`, disabled with the busy or
        blocked reason. Choosing it calls
        `request_restore_tweaks(vec![id])`.
     3. The pills:
        - **Custom** (`tweaks.pill.custom`, Warning). Its hover is
          `t_args("tweaks.custom_hint", [custom_reason])` ("Set to a
          value WinTune doesn't write ({0})").
        - **Windows 11** (`tweaks.pill.win11`, Neutral). Its hover is
          `tweaks.win11_hint` when available, else `tweaks.win11_only`.
        - **Unofficial** (`tweaks.pill.unofficial`, Warning). Its hover
          is `tweaks.unofficial_hint`.
        - **Pending** (Accent): `tweaks.pill.will_on` or
          `tweaks.pill.will_off`, when the row is pending.
   - The refresh hint (`refresh_hint`) is the hover text of the refresh
     line.

**Ruling on `CustomReason::Unreadable`:** the engine's text ("Cannot read
…: <os error>") stays as the raw detail inside `custom.unreadable`. The
key's English and French values become a translated summary followed by
the detail: "Can't read this setting: {0}" / "Impossible de lire ce
réglage : {0}". This is what the global constraint allows, and the
engine stays untouched.

**Keys** (en / fr):
- `tweaks.subtitle` (above) / "Réglages de {0}. La première modification
  de chacun enregistre l'original, pour pouvoir le restaurer."
- `tweaks.restore_all` (above) / "Restaurer tous les originaux ({0})"
- `tweaks.search`: "Search settings" / "Rechercher un réglage"
- `tweaks.clear`: "Clear" / "Effacer"
- `tweaks.no_match`: "No setting matches \"{0}\"." / "Aucun réglage ne
  correspond à « {0} »."
- `tweaks.store_unreadable`: "The saved originals can't be read, so
  changes are blocked: {0}" / "Les originaux enregistrés sont illisibles,
  les modifications sont donc bloquées : {0}"
- `tweaks.restore_original`: "Restore original" / "Restaurer l'original"
- `tweaks.more`: "More" / "Plus"
- `tweaks.pill.custom`: "Custom" / "Personnalisé"
- `tweaks.custom_hint` (above) / "Valeur que WinTune n'écrit pas ({0})"
- `tweaks.pill.win11`: "Windows 11" / "Windows 11"
- `tweaks.win11_hint`: "Windows 11 only." / "Windows 11 uniquement."
- `tweaks.win11_only`: "Not available on this version of Windows." /
  "Non disponible sur cette version de Windows."
- `tweaks.pill.unofficial`: "Unofficial" / "Non officiel"
- `tweaks.unofficial_hint`: "Relies on undocumented Windows behaviour
  that an update could change." / "Repose sur un comportement non
  documenté de Windows, qu'une mise à jour peut changer."
- `tweaks.pill.will_on`: "Will turn on" / "Sera activé"
- `tweaks.pill.will_off`: "Will turn off" / "Sera désactivé"
- `refresh.live`: "Visible: now" / "Visible : tout de suite"
- `refresh.new_windows`: "Visible: in new windows" / "Visible : dans les
  nouvelles fenêtres"
- `refresh.explorer_restart`: "Visible: after an Explorer restart" /
  "Visible : après redémarrage de l'Explorateur"
- `refresh.live_hint`: "Visible immediately." / "Visible
  immédiatement."
- `refresh.new_windows_hint`: "Visible in newly opened Explorer windows,
  or press F5." / "Visible dans les nouvelles fenêtres de l'Explorateur,
  ou appuyez sur F5."
- `refresh.explorer_restart_hint`: "Visible after restarting Explorer
  (Task Manager › Windows Explorer › Restart) or signing out." /
  "Visible après redémarrage de l'Explorateur (Gestionnaire des tâches ›
  Explorateur Windows › Redémarrer) ou une déconnexion."
- `custom.unreadable`: change its values as ruled above.

- [ ] **Step 1: Write the failing tests** in `tweaks_tab.rs`, on a
  1180×1600 screen and with three-frame clicks:
  - `clicking_a_toggle_marks_the_change_pending`. Click the
    `show_extensions` row's toggle (to find it, use the toggle right of
    the title's `text_center`, or a `widget_info` search). Assert that
    `pending_tweak_changes_in(groups)` has one entry and that
    `tweaks.pill.will_on` is drawn.
  - `a_custom_tweak_shows_its_reason_and_either_choice_is_pending`:
    1. Write `HideFileExt = 7` in the sandbox and refresh the statuses.
    2. Assert that `tweaks.pill.custom` is drawn and that its hover
       contains `custom_reason`.
    3. Set `tweak_desired[id] = false` and assert one pending change.
    4. Set it to `true` and assert one pending change.
  - `windows_11_only_toggles_are_disabled_on_windows_10`. With
    `is_windows_11 = false`, hovering the `taskbar_left` toggle shows
    `tweaks.win11_only`, and clicking it leaves `tweak_desired` empty.
  - `an_unreadable_store_shows_a_banner_and_locks_the_toggles`:
    1. Break `tweaks-backups.json`.
    2. Assert that `t_args("tweaks.store_unreadable", …)`'s text is
       drawn.
    3. Clicking a toggle leaves `tweak_desired` empty.
  - `restore_original_in_the_menu_asks_first`:
    1. Apply `show_hidden`.
    2. Open the row's `…` menu and click `tweaks.restore_original`.
    3. Assert that `pending_confirm` is `RestoreTweaks(["show_hidden"])`.
  - `the_menu_is_absent_without_a_snapshot`. No `…` is drawn when the
    user has no backup.
  - `refresh_labels_cover_every_variant`. Each `Refresh` maps to a
    non-empty, distinct label and hint.
  - Keep and adapt the existing tests:
    - `the_tab_renders_on_windows_10_and_11`;
    - search;
    - `restore_all_only_targets_the_tweaks_of_the_current_tab`;
    - `a_search_without_match_shows_nothing_and_still_renders`, which
      now asserts that the `tweaks.no_match` text is drawn.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::tweaks_tab`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the page and the keys. Remove
  `crate::theme` imports (`accent_button`, `card_frame`, `draw_badge`,
  `get_palette`) from this file, along with the old header panel.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): tweak pages on the kit — sections,
  setting rows, pills and the … menu`

### Task 3: The tweak pages' action bar

**Files:**
- Modify: `src/ui/mod.rs` (`owns_action_bar`), `src/ui/tweaks_tab.rs`,
  `src/ui/shell.rs` (tests only, if the progress-bar loop needs
  adjusting), `i18n/strings.resx`, `i18n/strings.fr.resx`

**Interfaces:**
- Produces: `owns_action_bar` returns true for `Associations`, `Tweaks`
  and `StartPrivacy`.
- Consumes: `page_action_bar(ctx, status: Option<&str>, actions)`
  (shell.rs, PR 3), `pending_tweak_changes_in`, `discard_tweak_changes`,
  `start_apply_tweaks`, `tweak_apply_disabled_reason`.

**Bar** (drawn first in `show_tweaks_tab`, before the `CentralPanel`):
- **Status:** `t_args("tweaks.pending_n", [n])` ("{0} changes pending" /
  "{0} modification(s) en attente"), where `n` is this tab's pending
  count. It is `None` when `n == 0`, so the bar shows only while a job
  runs.
- **Actions, right to left:**
  - **Apply** (`tweaks.apply`, primary). It is disabled with
    `tweak_apply_disabled_reason` on hover, and calls
    `start_apply_tweaks(ctx)`.
  - **Discard** (`tweaks.discard`). It calls
    `discard_tweak_changes(tab)`.

**Keys** (en / fr):
- `tweaks.pending_n` (above)
- `tweaks.apply`: "Apply" / "Appliquer"
- `tweaks.discard`: "Discard" / "Annuler les modifications"

- [ ] **Step 1: Write the failing tests** in `tweaks_tab.rs`:
  - `the_bar_appears_with_a_pending_change_and_applies_only_this_tab`:
    1. On Explorer & Taskbar, flip `show_extensions`.
    2. On Start & Privacy, flip `no_app_launch_tracking`.
    3. Draw each page and assert that its bar shows `tweaks.pending_n`
       with 1.
    4. Click the bar's Apply on Start & Privacy and run the job to the
       end.
    5. Assert that the Start value was written, `HideFileExt` was not,
       and `show_extensions` is still pending.
  - `discard_in_the_bar_only_clears_this_tab`. The same setup, then
    Discard on Explorer & Taskbar. Its count goes to 0 and the bar is
    gone (no `tweaks.discard` drawn). Start & Privacy still has 1.
  - `while_a_job_runs_the_tweak_bar_shows_progress_and_no_actions`:
    - Hold a job, using the `hold_job` pattern from the associations
      tests.
    - The job title is drawn exactly once, and `tweaks.apply` and
      `tweaks.discard` are not.
    - The row toggles are disabled with the busy reason (hover one).
  - `an_unreadable_store_disables_apply_with_the_reason`. With a pending
    change and a broken store, the bar's Apply hover shows the
    `tweaks.blocked` text. Clicking it writes nothing.
  - Keep the existing `other_pages_still_show_the_shell_progress_bar`
    (shell.rs) green: the tweak pages now draw the progress through
    their own bar, exactly once.
- [ ] **Step 2: Run** `cargo test --locked -- ui::tweaks_tab ui::shell`.
  Expected: FAIL.
- [ ] **Step 3: Implement** the bar and `owns_action_bar`.
- [ ] **Step 4: Run** the full suite and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): tweak pages own their action bar
  (Discard / Apply, scoped to the tab)`

### Task 4: French rendering check and docs

**Files:**
- Modify: `src/ui/shell.rs` (the French test), `ARCHITECTURE.md`

- [ ] **Step 1: Extend** `shell_pages_draw_no_english_only_text_in_french`.
  Draw `ActiveTab::Tweaks` and `ActiveTab::StartPrivacy` with:
  - a pending change on each page, so the bar shows;
  - one tweak in a Custom state (`HideFileExt = 7`);
  - one applied tweak with a backup, with its `…` menu opened by
    clicking `…`;
  - one frame with `is_windows_11 = false`.

  Assert positively that these texts were drawn:
  - `tweaks.pending_n`;
  - `tweaks.pill.custom`;
  - `tweaks.restore_original`;
  - a `refresh.*` label.

  The existing no-English-only assertion covers everything drawn.
- [ ] **Step 2: Run** `cargo test --locked --lib shell_pages_draw_no_english_only_text_in_french`.
  Expected: PASS. Fix any French value or literal it reports.
- [ ] **Step 3: Update ARCHITECTURE.md.**
  - The tweak pages are now kit pages, with the `…` menu and their own
    action bar.
  - `owns_action_bar` now covers three pages.
- [ ] **Step 4: Run** `cargo fmt --check`, clippy, `cargo test --locked`
  and `cargo build --release --locked`. Expected: all green.
- [ ] **Step 5: Commit**: `test(i18n): French check covers the tweak
  pages; docs`

## Rulings made while writing this plan

- **`CustomReason::Unreadable` keeps the engine's sentence as raw
  detail** behind a translated summary. Making it a code would mean
  changing `tweak_status`'s error type in the engine. Cost if wrong: a
  small engine change in PR 6.
- **Rows keep a Pending pill** ("Will turn on" / "Will turn off"),
  besides the bar's count. The current screen shows it, and the spec
  keeps every existing behaviour. Cost if wrong: dropping one pill.
- **The `…` menu holds only "Restore original"**, and is drawn only when
  a snapshot exists, as the spec says. Cost if wrong: an always-present
  menu with a disabled item.
- **The search box sits under the page header**, not inside its actions
  row. This gives room for the hint on narrow windows. Cost if wrong:
  moving one widget.
