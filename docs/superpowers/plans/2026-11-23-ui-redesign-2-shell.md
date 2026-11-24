# UI redesign, PR 2: Shell — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the app's frame with the new one:
- the grouped navigation sidebar;
- a Home dashboard;
- a Settings page;
- toasts, banners and a progress bar instead of the status bar;
- autosave of `config.json` instead of the "Save Configuration" button.

The module screens (associations, tweaks, icons, rollback) keep their
current look; they are rebuilt in PRs 3 to 5.

**Architecture:**
- **Pages.** `ActiveTab` becomes the page list and gains `Home`,
  `History`, `Settings` and a `ContextMenu` placeholder. Switching pages
  goes through one method, `open_page`, which keeps today's side effects:
  reloading stores, refreshing statuses and resetting Ctrl+F.
- **Frame.** `ui()` draws, in this order:
  - the kit navigation;
  - a progress bar while a job runs;
  - a save-error banner when the last autosave failed;
  - the page;
  - toasts;
  - the confirmation dialog.
- **Notices.** Every `set_status` call becomes `notify(kind, text)`, which
  pushes a toast. The full details of an operation go to the toast's hover
  text and stay in History.
- **Autosave.** `ui()` compares `config` with the last saved copy. It saves
  one second after the last change, and immediately when the window closes.
- **Messages.** Messages produced by the shell are translated: jobs,
  confirmations, reports, the close guard, Home, Settings, the navigation
  and History labels. Messages produced inside the old tab files keep their
  English text until their screen is rebuilt.

**Tech stack:** Rust 1.88, egui/eframe 0.29.1, rfd (folder and file
pickers), the `ui::kit` and `i18n` modules from PR 1.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-redesign-design.md`. Read
§5 (navigation), §6.2 (ActionBar, Toast, Dialog), §6.3 (Home, Settings,
and "Saving" under File associations) and §6.4. This plan is PR 2 of §7.

## Global constraints

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and `cargo test --locked` stay green on every commit.
- One branch: `feat/ui-shell`, from `main`.
- **Engines stay untouched.** This PR only changes `src/ui/`, `i18n/`,
  docs, and two narrow edits:
  - `#[derive(PartialEq)]` on the `src/models.rs` config types;
  - `open_folder` in `AppEnv`.

  `history.jsonl` keeps storing the English module and action labels.
- **Every safety step stays.** None of these may be lost:
  - the confirmation before Remove, Rollback and Restore;
  - the refusal while a job runs or a dialog is open;
  - the block while `backups.json` or `tweaks-backups.json` is
    unreadable;
  - the close guard;
  - "an unreadable `config.json` is never overwritten".
- **No registry action from a keyboard shortcut.**
  - Ctrl+F keeps working.
  - A focused button may still be pressed with Enter or Space.
  - In a confirmation dialog, focus starts on **Cancel**.
- **The navigation, in this order (§5):**
  - Home
  - group "Customize": File associations, Explorer & Taskbar, Start &
    Privacy, Context menu (disabled, hover "Coming soon")
  - group "Tools": Icons, History, Rollback (temporary, removed in PR 5)
  - Settings, pinned at the bottom
- **Texts.**
  - Every text drawn by Home, Settings, History, the navigation, the
    toasts, the progress bar and the dialog comes from `tr.t` or
    `tr.t_args` with a literal key.
  - Each key is added to `i18n/strings.resx` and `i18n/strings.fr.resx`.
  - Sentence case, as short as possible.
  - The only text that may stay untranslated is a raw OS or engine error
    detail, shown after a translated summary.
- **Spacing, colors and sizes come from `kit` tokens.** New screens
  contain no color or size literals. New components go in `src/ui/kit/`.
- **Tests stay headless.** No test depends on system fonts.

## Review focus

These are cases a person will hit that no example in the spec covers.
Each one is pinned by a test in the task that owns it.

1. **Closing the window less than a second after an edit.** The edit must
   be on disk, including when the close waits for a running job (Task 2).
2. **A config save that keeps failing**, for example a read-only
   ProgramData. There must be one banner, not one toast per frame. It
   disappears after a successful retry. An unreadable `config.json` is
   still never written (Task 2).
3. **More than three toasts at once**, for example an apply with errors,
   then "history not saved", then "config not saved". Every error toast
   must stay reachable and dismissable. A timed toast must not expire
   before it was ever drawn (Task 1).
4. **The keyboard with a confirmation dialog open.**
   - Tab and Shift+Tab move only between the dialog's two buttons.
   - Enter on the first focused button cancels.
   - No key reaches the navigation or the page behind the dialog
     (Tasks 1 and 3).
5. **Changing page while a job runs.** The progress bar stays visible on
   every page, and opening a page never reloads a store the job is about
   to replace (Task 4).

---

### Task 1: Kit additions and the PR 1 carry-over

**Files:**
- Modify: `src/ui/kit/feedback.rs`, `src/ui/kit/nav.rs`,
  `src/ui/kit/surfaces.rs`, `src/ui/kit/icons.rs`, `src/ui/kit/mod.rs`
  (re-exports), `src/ui/kit/tests.rs`

**Interfaces:**
- Produces:
  - `Toasts::push_details(&mut self, kind: ToastKind, text: impl Into<String>, details: impl Into<String>)`.
    A non-empty `details` shows as hover text on the toast. `push` stays
    and means "no details".
  - `Toasts::show(&mut self, ctx: &egui::Context, clear_all: &str)`. This
    signature replaces the old `show(ctx)`.
  - `Toasts::latest(&self) -> Option<String>`: the text of the most
    recently pushed toast, followed by `"\n"` and its details when there
    are some. It still returns the toast after it expires; tests use it.
  - `pub fn dialog_buttons(ui: &mut Ui, id: &str, confirm: &str, cancel: &str) -> Option<bool>`
    in `feedback.rs`. It returns `Some(true)` for confirm and `Some(false)`
    for cancel.
  - `pub struct Brand<'a> { pub logo: Option<egui::TextureId>, pub name: &'a str, pub caption: &'a str }`.
    `nav_sidebar` gains a `brand: &Brand<'_>` parameter, second after
    `ctx`. It draws the brand at the top of the sidebar.
  - `pub fn tile(ui: &mut Ui, icon: Icon, title: &str, status: &str, enabled: bool, disabled_hint: Option<&str>) -> Response`
    in `surfaces.rs`: a clickable card used for Home's module cards.
    - It senses clicks only when `enabled`, with a hover fill.
    - When disabled, the text uses `text_disabled` and the hint shows on
      hover.
    - Its `widget_info` is a Button labelled `title`.
- Consumes: the existing kit (`split_row`, `action_button`, tokens).

**Toasts, in order:**
- `born` is set only on the first frame a toast is actually drawn.
- Only drawn toasts can expire.
- When more than `MAX_TOASTS` are queued, a last row shows `+N` (the
  number of hidden toasts) and a `clear_all` link that empties the queue.

**Dialog buttons:**
- The confirm button is primary, the cancel button secondary.
- When neither button has focus, cancel requests it.
- Tab and Shift+Tab are consumed (`consume_key`) and toggle focus between
  the two buttons.
- Escape is not handled here; callers keep handling it.

**`Icon::Start`:** use `'\u{ECA5}'` (Tiles). If
`every_glyph_exists_in_both_icon_fonts` rejects it, use `'\u{F0E2}'`
(GridView). Either way, the glyph must be one the existing test accepts.

- [ ] **Step 1: Write the failing tests** in `src/ui/kit/tests.rs`. Drive
  time with `RawInput { time: Some(t), .. }`.
  - `a_queued_toast_does_not_expire_before_it_is_drawn`
    1. Push 5 Info toasts.
    2. Draw frames at t = 0 and t = 10.
    3. Assert that 2 remain.
    4. Draw at t = 20 and assert that 0 remain.

    Only the 3 drawn toasts expire at t = 10. The 2 hidden ones are first
    drawn at t = 10 and expire by t = 20.
  - `hidden_error_toasts_can_be_cleared`
    1. Push 5 Error toasts and draw a frame.
    2. Click the `clear_all` link, found with `ctx.memory`/layout or with
       a fixed id.
    3. Assert `is_empty()`.
  - `toast_details_are_kept_by_latest`
    1. `push_details(Error, "Applied 1", "Error: x")`.
    2. Assert `latest() == Some("Applied 1\nError: x")`.
  - `dialog_focus_starts_on_cancel_and_tab_stays_inside`
    1. Draw a `dialog` whose body is `dialog_buttons`, with a focusable
       `TextEdit` drawn behind it on the page.
    2. Assert that cancel has focus.
    3. Press Tab and assert that confirm has focus.
    4. Press Tab and assert that cancel has focus.
    5. Press Shift+Tab and assert that confirm has focus.
    6. The page `TextEdit` never has focus.
  - `enter_on_the_initial_focus_cancels`
    1. Draw the dialog.
    2. Press Enter on the next frame.
    3. Assert that the result is `Some(false)`.
  - `a_disabled_tile_does_not_click`
    1. Click a tile drawn with `enabled = false`.
    2. Assert `!clicked()`.
  - `an_enabled_tile_clicks`.
  - `the_nav_draws_the_brand_name`: in `ctx.run` output, a text shape
    equals `brand.name`.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::kit`. Expected: the
  new tests fail (they don't compile or they fail their assertions).
- [ ] **Step 3: Implement** the interfaces above. Update the existing kit
  tests and callers to the new `show` and `nav_sidebar` signatures.
- [ ] **Step 4: Run** `cargo test --locked --lib ui::kit` and
  `cargo clippy --all-targets --locked -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(kit): toast details and overflow, dialog
  buttons with contained focus, tile, nav brand`.

---

### Task 2: Autosave of `config.json`

**Files:**
- Create: `src/ui/autosave.rs`
- Modify: `src/models.rs` (derives only), `src/ui/mod.rs`,
  `src/ui/sidebar.rs` (remove the Save button and the direct
  `save_config` calls), `src/ui/safety_tests.rs`, `src/ui/review2_tests.rs`

**Interfaces:**
- Produces, in `src/ui/autosave.rs`:
  - `pub(super) const AUTOSAVE_DELAY_SECONDS: f64 = 1.0;`
  - `pub(super) struct Autosave` with these fields:
    - `saved: AppConfig`: the last copy written, or the one loaded at
      startup;
    - `seen: AppConfig`: the config as of the last frame;
    - `changed_at: Option<f64>`;
    - `pub error: Option<String>`.
  - `Autosave::new(config: &AppConfig) -> Self`
  - `Autosave::pending(&self) -> bool`, true when `seen != saved`.
  - On `WinTuneApp`:
    - `fn pump_autosave(&mut self, ctx: &egui::Context)`, called each
      frame from `ui()`;
    - `fn flush_config(&mut self)`: saves now if `pending()`;
    - field `autosave: Autosave`.
- **Behaviour of `pump_autosave`:**
  - If `config != seen`: set `seen` to a clone of `config` and
    `changed_at` to `now` (from `ctx.input(|i| i.time)`).
  - When `now - changed_at >= AUTOSAVE_DELAY_SECONDS`: call
    `try_save_config()` and clear `changed_at`.
    - On success: `saved = config.clone()` and `error = None`.
    - On failure: `error = Some(e)`.
  - While `changed_at` is set, `request_repaint_after` the remaining
    delay.
  - **When `config_error` is set, nothing is ever written** and `error`
    stays `None`. Settings' config-error banner covers that case.
- **`flush_config`** is called:
  - when a close request is accepted, meaning no job is running;
  - just before `ViewportCommand::Close` in the close-when-idle path.
- **Remove** `save_config()` and its "Configuration saved." message. Only
  `try_save_config()` remains: autosave and the apply job use it.
  - `set_theme` and `set_language` now only change `config`.
  - The `#[allow(dead_code)]` on `set_language` stays until Task 6 uses
    it.
- **Derives** in `src/models.rs`: add `PartialEq` to `AppConfig` and to
  every type it contains (`ExtensionConfig`, `CustomContextCommand`,
  `DoubleClickMode`…). Add nothing else.
- **The save-error banner** is drawn by `ui()` on every page, above the
  page content, while `autosave.error` is `Some`.
  - It is an Error `kit::banner` with the text
    `t_args("autosave.failed", &[&e])`, for example
    "Settings not saved: {0}".
  - Its action, `t("autosave.retry")`, calls `flush_config()`.
  - The banner is drawn in a `TopBottomPanel::top("autosave_banner")`,
    added before the page panels.

- [ ] **Step 1: Write the failing tests** in `src/ui/autosave.rs`
  (`#[cfg(test)] mod tests`). Each one drives frames with
  `RawInput { time: Some(t), .. }`.
  - `an_edit_is_saved_after_the_delay_not_before`
    1. Change `config.code_executable`.
    2. Draw a frame at t = 0.0, then one at t = 0.5.
    3. Assert that `config.json` doesn't contain the new value yet.
    4. Draw a frame at t = 1.1 and assert that it does.
  - `each_edit_restarts_the_delay`
    1. Edit at t = 0.
    2. Edit again at t = 0.8.
    3. Assert that nothing is written at t = 1.2.
    4. Assert that the second value is written at t = 1.9.
  - `closing_right_after_an_edit_saves_it`
    1. Edit, then draw a frame at t = 0.
    2. Draw `close_request()` at t = 0.1.
    3. Assert that the file contains the edit.
  - `closing_after_a_job_saves_edits_made_during_it`
    1. Run a held `BuildPak` job, as in
       `closing_during_a_job_is_deferred_until_it_finishes`.
    2. Edit and request close.
    3. Release the job and run frames until `Close`.
    4. Assert that the edit is on disk.
  - `an_unreadable_config_is_never_overwritten_by_autosave`
    1. Write `{ broken` to `config.json` and load it.
    2. Set `config_error`, then edit.
    3. Draw frames to t = 5 and request close.
    4. Assert that the file still reads `{ broken` and that
       `autosave.error` is `None`.
  - `a_failed_save_shows_one_banner_until_a_retry_succeeds`
    1. Point `env.config_path` at a path inside a missing, non-creatable
       location, for example a path whose parent is an existing file.
    2. Edit, then draw a frame at t = 2.
    3. Assert `autosave.error.is_some()` and `changed_at.is_none()`.
    4. Draw 5 more frames. Assert that `changed_at` stays `None`: a failure
       doesn't retry on its own.
    5. Point `config_path` at a writable path, call `flush_config()`, and
       assert `error.is_none()`. `flush_config` saves whenever
       `pending()` holds, whatever `changed_at` says.
  - `set_language_with_an_unreadable_config_switches_but_writes_nothing`
    1. Set `config_error`.
    2. Call `set_language("fr")` and draw frames to t = 5.
    3. Assert `tr.code() == "fr"` and that the file is unchanged.

    This is the PR 1 carry-over test.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::autosave`. Expected:
  FAIL.
- [ ] **Step 3: Implement.** Then rewrite the tests that called
  `save_config()`:
  - `an_unreadable_config_is_never_overwritten` becomes the autosave test
    above, so delete the old one.
  - `a_saved_config_reloads_and_leaves_no_temp_file` now uses
    `flush_config()`.
  - `a_refused_config_save_is_reported_after_an_apply` keeps its
    assertion. The apply path still uses `try_save_config`.
- [ ] **Step 4: Run** `cargo test --locked` and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): autosave config.json, flushed on close`.

---

### Task 3: Notices: toasts, progress bar and the kit dialog replace the status bar

**Files:**
- Delete: `src/ui/status_bar.rs`
- Modify:
  - `src/ui/mod.rs`, `src/ui/jobs.rs`, `src/ui/confirm.rs`,
    `src/ui/rule_status.rs`, `src/ui/history_panel.rs`;
  - `src/ui/associations_tab.rs`, `src/ui/tweaks_tab.rs`,
    `src/ui/gallery_tab.rs`, `src/ui/sidebar.rs`: only to change their
    `set_status` calls into `notify`, plus, in `tweaks_tab.rs`, the
    `show_report` call and the `RestoreTweaks` confirmation;
  - the test files that read `status_message`;
  - `i18n/strings.resx`, `i18n/strings.fr.resx`.

**Interfaces:**
- Produces, on `WinTuneApp`:
  - the field `toasts: kit::Toasts`, which replaces `status_message` and
    `status_details`;
  - `fn notify(&mut self, kind: kit::ToastKind, text: impl Into<String>)`;
  - `fn notify_details(&mut self, kind: kit::ToastKind, text: impl Into<String>, details: impl Into<String>)`;
  - `#[cfg(test)] fn last_notice(&self) -> String`, which returns
    `self.toasts.latest().unwrap_or_default()`;
  - `pub(super) enum ReportKind { AppliedRules, RestoredRules, AppliedTweaks, RestoredTweaks }`
    and `fn show_report(&mut self, kind: ReportKind, report: &OperationReport)`.
    These replace `show_report(verb, what, report)`.
  - `RunningJob::title_key(&self) -> &'static str`: the translation key
    of the job's progress text.
- `ui()` calls:
  - `self.toasts.show(ctx, &self.tr.t("toast.clear_all"))` after the page;
  - while `self.job` is `Some`, a `kit::action_bar` in `Busy` state with:
    - the status `tr.t(job.title_key())`;
    - the label `"{label} ({done}/{total})"` when `total > 0`, otherwise
      the empty string;
    - fraction `progress.fraction()`.
- The confirmation dialog draws with `kit::dialog` and
  `kit::dialog_buttons`, with width 440. Escape still cancels.
  `Confirm.title`, `.message` and `.confirm_label` are built with `tr`.
  This includes the `RestoreTweaks` request in `tweaks_tab.rs`, which this
  task translates too.

**What kind of toast each message becomes, and its key:**

| Today's message | Kind | Key (English value = today's text unless noted) |
| --- | --- | --- |
| report with 0 errors | Success | `report.applied_rules` "Applied {0} extension(s)", `report.restored_rules` "Restored {0} extension(s)", `report.applied_tweaks` "Applied {0} tweak(s)", `report.restored_tweaks` "Restored {0} tweak(s)", then "." |
| report with errors | Error, with `details` = the old `status_details` lines | same keys + `report.errors` ", {0} error(s) (first: {1})", `report.warnings` ", {0} warning(s)", `report.reg_backup` " A .reg backup of the previous state was saved." |
| "Answer the open dialog first." | Info | `notice.answer_dialog` |
| "Please wait: {0} is still running." | Info | `notice.still_running` ({0} = `tr.t(title_key)`) |
| "Registry changes are blocked: {0}" / "Rollback is blocked: {0}" | Error | `notice.registry_blocked` / `notice.rollback_blocked` |
| "Nothing applied: fix {0} rule problem(s) first ({1})." | Error, details = every problem, translated with `messages::rule_problem` | `notice.fix_problems` |
| "Set the default code editor before applying associations." | Error | `notice.editor_missing` |
| "Applying file associations…" and the other "…ing" job-start messages | none (the progress bar shows it) | `job.apply_rules`, `job.rollback`, `job.build_pak`, `job.apply_tweaks`, `job.restore_tweaks` |
| "Packed {0} icons: {1} MB -> {2} MB" | Success | `notice.packed` |
| "Pack error: {0}" | Error | `notice.pack_failed` |
| "{0} failed unexpectedly: {1}" | Error | `notice.job_panicked` |
| "History not saved: {0}" | Error, a separate toast | `notice.history_not_saved` |
| "Could not read the rules' status: {0}" | Error | `notice.status_failed` |
| "Cancelled." | Info | `notice.cancelled` |
| "Removed the rule for {0}." / "{0} is no longer in the list." | Success / Info | `notice.rule_removed` / `notice.rule_gone` |
| "An operation is running: WinTune will close as soon as it finishes." | Info | `notice.close_after_job` |
| config unreadable at startup | Error | `notice.config_unreadable` "Using the default rules: {0} Saving is disabled until it is fixed." |
| `backups.json` / `tweaks-backups.json` unreadable | Error | the engine error text as is (raw detail) |
| messages in `associations_tab.rs`, `tweaks_tab.rs`, `gallery_tab.rs` | Error if the text reports a failure, Success if it reports a completed action, otherwise Info | **no key**: keep the English text; PRs 3 to 5 translate it |

The English values must keep today's wording, so the existing assertions
("still running", "dialog", "Packed 2 icons", "Configuration not saved",
"blocked", "invalid"…) still match through `last_notice()`.

- [ ] **Step 1: Write the failing tests.**
  - `a_report_with_errors_is_an_error_toast_with_details`, in `jobs.rs`
    tests:
    1. Apply a rule whose icon export fails, or call `show_report` with a
       hand-built `OperationReport { errors: vec![".b: x".into()], .. }`.
    2. Assert that `last_notice()` contains `"1 error(s)"` and
       `"Error: .b: x"`.
  - `job_start_shows_progress_not_a_toast`
    1. Start a held `BuildPak` job and draw a frame.
    2. Assert that `toasts.is_empty()`.
    3. Assert that the frame output contains a text shape equal to
       `tr.t("job.build_pak")`.

    Add `drawn_texts(&FullOutput) -> Vec<String>` to `test_support.rs`. It
    walks the `ClippedShape`s and returns each `Shape::Text` galley's text,
    recursing into `Shape::Vec`.
  - `the_confirm_dialog_starts_on_cancel_and_enter_cancels`, in
    `confirm.rs` tests:
    1. `request_remove_rule(0)`, then draw a frame.
    2. Press Enter on the next frame.
    3. Assert that `pending_confirm.is_none()` and that the rule is still
       there.
  - `history_not_saved_is_its_own_error_toast`
    1. Point `history_path` at a directory, so the append fails.
    2. Run an apply.
    3. Assert that `toasts.len() >= 2` and that one toast mentions "History
       not saved".
  - Migrate every `app.status_message` / `status_details` read to
    `app.last_notice()`. Keep each assertion's substring.
- [ ] **Step 2: Run** `cargo test --locked --lib ui`. Expected: the new
  tests fail.
- [ ] **Step 3: Implement.**
  1. Delete `status_bar.rs` and `show_status_bar`.
  2. The `History (n)` toggle and the "x / y extensions enabled" counter
     move with the status bar. History becomes a page in Task 4, and Home
     shows the rule counts in Task 5.
  3. Until Task 4, keep `show_history_panel` reachable: temporarily give
     the old sidebar a "History" button.
- [ ] **Step 4: Run** `cargo test --locked` and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): toasts and progress bar replace the
  status bar; kit confirmation dialog`.

---

### Task 4: Navigation and pages

**Files:**
- Create: `src/ui/shell.rs` (navigation, `open_page`, history page)
- Delete: `src/ui/sidebar.rs`
- Modify: `src/ui/mod.rs`, `src/ui/polish.rs` (`search_box_id` for the
  new variants), `src/ui/history_panel.rs` (the panel becomes a page),
  the tests that iterate `ActiveTab::ALL`, and the `.resx` files.

**Interfaces:**
- `ActiveTab` gains `Home`, `History`, `Settings` and `ContextMenu`.
  - `ActiveTab::ALL` lists every **openable** page, in navigation order:
    `Home, Associations, Tweaks, StartPrivacy, Gallery, History, Rollback, Settings`.
  - `ContextMenu` is not in `ALL`.
  - `title()` stays as it is, and returns `""` for the new variants. It is
    the English module label written to `history.jsonl` by
    `tweaks_tab.rs`, and the old tweaks heading uses it. The navigation
    doesn't use it. Its labels are the keys
    `nav.home`, `nav.associations`, `nav.explorer`, `nav.start`,
    `nav.context_menu`, `nav.icons`, `nav.history`, `nav.rollback`,
    `nav.settings`. The group titles are `nav.group.customize` and
    `nav.group.tools`. The disabled hint is `nav.coming_soon`.
- `pub(super) fn open_page(&mut self, page: ActiveTab)`. It moves today's
  `sidebar_nav_button` side effects and keeps their guards:
  - Rollback: reload the backup store, unless a job is running.
  - Associations: request a status refresh when arriving from another
    page.
  - Tweaks and StartPrivacy: reload the tweak store and refresh the
    statuses, unless a job is running.
  - Any change of page: `focus_search = false`.
- `fn show_nav(&mut self, ctx)` calls `kit::nav_sidebar` with:
  - `Brand { logo: self.logo_texture id, name: "WinTune", caption: "v{APP_VERSION}" }`;
  - groups following the Global constraints;
  - badges: the number of pending tweak changes on Tweaks and on
    StartPrivacy (`pending_tweak_changes_in(tweak_groups_of(page)).len()`),
    and `None` elsewhere.

  When the selection changes, `show_nav` calls `open_page`.
- `with_config` opens on `ActiveTab::Home`. Tests that relied on
  Associations being the first page set `active_tab` explicitly.
- `search_box_id` returns `None` for Home, History and Settings. Ctrl+F
  does nothing there.
- The **History page** (temporary, until PR 5) is the old panel's grid in
  the central panel. It has a `kit::page_header` with the title
  `nav.history` and no subtitle, and `kit::empty_state` when there is no
  entry. The labels go through Task 7. Until then, show them as stored.
- Home and Settings draw `kit::empty_state` placeholders until Tasks 5
  and 6.
- Remove the `show_history` field and the bottom panel.

- [ ] **Step 1: Write the failing tests** in `src/ui/shell.rs`:
  - `the_app_opens_on_home`.
  - `the_nav_lists_pages_in_spec_order`. Render a frame, then collect the
    nav labels from `drawn_texts`, filtered to the `nav.*` values in
    English. Assert the order:
    Home, File associations, Explorer & Taskbar, Start & Privacy, Context
    menu, Icons, History, Rollback, Settings.
  - `context_menu_cannot_be_opened`. Click its row (found by its rect, or
    by `widget_info` label through `ctx.memory`) and assert that
    `active_tab` is unchanged.
  - `changing_page_during_a_job_keeps_progress_and_stores`
    1. Start a held `BuildPak` job. `StoreOutcome`'s fields are private
       to `jobs.rs`, so a Rollback job can't be built from here. The
       guards check `is_busy()`, which doesn't depend on the kind of job.
    2. `open_page(Rollback)`, then `open_page(Tweaks)`.
    3. Assert that the backup store and the tweak store were **not**
       reloaded: compare with a sentinel value set before.
    4. Assert that every page in `ActiveTab::ALL` draws
       `tr.t("job.build_pak")`.
  - `pending_tweak_changes_show_as_a_badge`. Set one `tweak_desired`
    entry, render, and assert that a text shape equals `"1"`.
  - Update `every_tab_renders_without_panicking`,
    `tabs_render_with_no_rules_and_no_icons` and
    `the_dialog_renders_on_every_tab` so they cover the new `ALL`.
- [ ] **Step 2: Run** `cargo test --locked --lib ui`. Expected: FAIL.
- [ ] **Step 3: Implement.** Move `set_theme` into `mod.rs`, with
  `#[allow(dead_code)]` until Task 6, then delete `sidebar.rs`. The WSL
  badge, the Refresh Explorer button and the theme picker move to Settings
  in Task 6. Until then they are missing, which is acceptable inside this
  PR.
- [ ] **Step 4: Run** `cargo test --locked` and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): grouped navigation, Home and Settings
  pages, history as a page`.

---

### Task 5: Home dashboard

**Files:**
- Create: `src/ui/home.rs`
- Modify: `src/ui/mod.rs` (dispatch), `i18n/*.resx`

**Interfaces:**
- `fn show_home(&mut self, ctx: &egui::Context)`.
- `pub(super) struct ModuleSummary { pub page: ActiveTab, pub icon: kit::Icon, pub status: String, pub drift: usize }`
  and `fn module_summaries(&self) -> Vec<ModuleSummary>`. It returns one
  summary per Customize page, in navigation order, Context menu last.
  - **Associations**:
    - `status` = `t_args("home.rules_status", &[&enabled, &changed])`,
      "{0} active · {1} changed", where `enabled` counts the enabled
      rules and `changed` counts the rules in `RuleStatus::Drifted`;
    - `drift = changed`.
  - **Tweaks and StartPrivacy**:
    - `status` = `t_args("home.tweaks_status", &[&on, &total])`, "{0} of
      {1} on", over that page's groups;
    - when some tweaks are `Custom`, append
      `t_args("home.custom_suffix", &[&n])`, " · {0} custom";
    - `drift` = the number of `Custom` tweaks.
  - **Context menu**: `status` = `t("nav.coming_soon")`, `drift = 0`.
- `fn drift_target(&self) -> Option<ActiveTab>`: the first summary with
  `drift > 0`, or `None`.
- **Layout**, top to bottom, inside a scrollable central panel:
  1. `page_header` with no breadcrumb, the title `home.title` "Home" and
     the subtitle `home.subtitle` "Everything WinTune changed can be
     undone".
  2. When `drift_target()` is `Some`, a Warning `banner` with
     `t_args("home.drift", &[&total_drift])`, "{0} setting(s) were changed
     by Windows or another app", and the action `home.review` "Review".
     The action calls `open_page(target)`.
  3. The module tiles in a 2-column grid (`kit::tile`). Clicking a tile
     calls `open_page(summary.page)`. The Context menu tile is disabled,
     with the hint `nav.coming_soon`.
  4. `section` "Recent" (`home.recent`) listing the newest 5 history
     entries. Each row shows the time, the module label, the action label
     and the summary, using Task 7's functions. In this task, use the
     stored labels and the summary `t_args("history.ok", &[&n])` /
     `t_args("history.ok_errors", &[&n, &e])`. Show `home.no_recent` when
     there is no entry. A "View all" link (`home.view_all`) opens History.

- [ ] **Step 1: Write the failing tests** in `home.rs`:
  - `summaries_count_enabled_and_changed_rules`. Set up 2 enabled rules and
    1 disabled rule, with `rule_statuses` set by hand to one `Applied` and
    one `Drifted(vec![…])`. Assert that the Associations status equals
    `"2 active · 1 changed"` and that `drift == 1`.
  - `summaries_count_tweaks_on_per_page`. Set
    `tweak_statuses["show_extensions"] = On` and assert that the Tweaks
    status starts with `"1 of "`.
  - `the_drift_banner_opens_the_first_drifted_page`
    1. Set one Custom tweak on StartPrivacy and no rule drift.
    2. Render Home.
    3. Click the banner action (driving `kit::banner` through a click at
       its button, or calling the same code path through a helper).
    4. Assert `active_tab == StartPrivacy`.
  - `no_drift_no_banner`. Assert that the drawn texts don't contain
    `"Review"`.
  - `recent_lists_at_most_five_entries`
    1. Record 7 history entries with distinct actions "A1" to "A7".
    2. Render.
    3. Assert that "A7" to "A3" are drawn and that "A2" is not.
  - `home_renders_with_no_rules_no_history`.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::home`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): Home dashboard with module tiles,
  drift banner and recent operations`.

---

### Task 6: Settings page

**Files:**
- Create: `src/ui/settings.rs`
- Modify:
  - `src/ui/mod.rs`: `AppEnv.open_folder`, dispatch, and removing the
    `#[allow(dead_code)]` on `set_language`;
  - `src/ui/associations_tab.rs`: remove the editor path, Browse and
    Auto-Detect from `show_editor_bar`. "Refresh status" and "Apply
    Checked Extensions" stay.
  - `src/ui/test_support.rs`: `open_folder` records the paths it receives;
  - `i18n/*.resx`.

**Interfaces:**
- `AppEnv` gains `pub open_folder: Arc<dyn Fn(&Path) + Send + Sync>`.
  - In production it spawns `explorer.exe <path>` and ignores spawn
    errors after a toast.
  - In tests it pushes the path into a shared `Arc<Mutex<Vec<PathBuf>>>`
    that `TempIcons` exposes as `opened_folders()`.
- `fn show_settings(&mut self, ctx: &egui::Context)`.
- `pub(super) fn auto_detect_editor(&mut self)`, moved from the
  associations bar. Its two outcomes are a Success toast
  `settings.editor_found` and an Info toast `settings.editor_not_found`.
- `pub(super) fn set_icon_folder(&mut self, path: String)`. It sets
  `config.icon_source_folder` and reloads the gallery.
- **Sections and rows**, in this order. Each is a `kit::section` of
  `kit::SettingRow`s, and each title and description is a key under
  `settings.*`.
  1. When `config_error` is `Some`, an Error `banner` comes first:
     `t_args("settings.config_unreadable", &[&e])`, "config.json can't be
     read: {0} Changes aren't saved until it is fixed."
  2. **Appearance**:
     - Language: a `segmented` over `Tr::languages()`, which calls
       `set_language`;
     - Theme: a `segmented` System / Light / Dark, which calls
       `set_theme`. Remove the `#[allow(dead_code)]` on `set_theme`.
  3. **Associations**: Default code editor. A `TextEdit` bound to
     `config.code_executable`, plus Browse (an `.exe` picker, as today)
     and Auto-detect.
  4. **Icons**: Icon folder. A read-only label with the path and a
     "Change" button. The button opens `rfd::FileDialog::pick_folder()`
     and calls `set_icon_folder`.
  5. **System**:
     - the WSL state as a pill: `settings.wsl_on` "WSL 2 detected", or
       `settings.wsl_off` "No WSL: .sh opens in the editor";
     - "Refresh Explorer" (`settings.refresh_explorer`). It calls
       `(env.notify_shell)()` and shows an Info toast
       `settings.explorer_refreshed`.

       *(This keeps today's SHChangeNotify behaviour. Actually restarting
       explorer.exe from an elevated process is still an open owner
       decision, see the roadmap.)*
  6. **Data**: "Open data folder", which calls
     `(env.open_folder)(&app_data_dir())`.
  7. **About**: the version `t_args("settings.version", &[&APP_VERSION])`
     and a hyperlink to `https://github.com/Sp2ctrale/WinTune`.
- Changes go to `config`. Task 2's autosave writes them, so this page has
  no Save button.

- [ ] **Step 1: Write the failing tests** in `settings.rs`:
  - `choosing_french_switches_the_whole_ui_next_frame`
    1. Open Settings and call `set_language("fr")`.
    2. Render.
    3. Assert that `drawn_texts` contains `Tr::new("fr").t("nav.settings")`
       and not the English `"Settings"`.
  - `editing_the_editor_path_is_autosaved`. Set `code_executable`, then
    draw frames to t = 2. Assert that the file contains it.
  - `the_editor_bar_no_longer_holds_the_path`. Render Associations and
    assert that no drawn text equals `"Default Code Editor:"`.
  - `open_data_folder_uses_the_env`. Call the row's action through a
    helper `open_data_folder()` and assert that `opened_folders()` holds
    `app_data_dir()`.
  - `set_icon_folder_reloads_the_gallery`. Point it at a `TempIcons` dir
    with 2 icons and assert that `gallery.names().len() == 2`.
  - `an_unreadable_config_shows_its_banner_here`. Set `config_error`,
    render Settings, and assert that a drawn text contains
    `"config.json can't be read"`.
  - `settings_renders_in_every_theme_and_language`.
- [ ] **Step 2: Run** `cargo test --locked --lib ui::settings`. Expected:
  FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --locked` and clippy. Expected: PASS.
- [ ] **Step 5: Commit**: `feat(ui): Settings page (language, theme,
  editor, icon folder, system, data, about)`.

---

### Task 7: History labels, the French check, docs

**Files:**
- Modify: `src/i18n/messages.rs`, `src/ui/home.rs`, `src/ui/shell.rs`
  (History page), `src/ui/test_support.rs`, `i18n/*.resx`,
  `ARCHITECTURE.md`, `CONTRIBUTING.md`, `README.md` (the screens section
  only, if it describes the old sidebar or status bar)

**Interfaces:**
- In `src/i18n/messages.rs`:
  - `pub fn history_module(tr: Tr, stored: &str) -> String`:
    - "File associations" maps to `history.module.associations`;
    - "Explorer & Taskbar" maps to `history.module.explorer`;
    - "Start & Privacy" maps to `history.module.start`;
    - any other value is returned **as stored**.
  - `pub fn history_action(tr: Tr, stored: &str) -> String`:
    - "Apply" maps to `history.action.apply`;
    - "Rollback" maps to `history.action.rollback`;
    - "Restore" maps to `history.action.restore`;
    - any other value is returned as stored.
- Home's Recent list and the History page show the labels through these
  functions. They also translate the summary (`history.ok`,
  `history.ok_errors`) and the "Copy .reg path" button
  (`history.copy_reg`).

- [ ] **Step 1: Write the failing tests.**
  - `known_history_labels_are_translated_and_unknown_ones_kept`, in
    `src/i18n/tests.rs`:
    - assert `history_module(fr, "File associations") == fr.t("history.module.associations")`;
    - assert `history_action(fr, "Rollback") == fr.t("history.action.rollback")`;
    - assert `history_module(fr, "Legacy thing") == "Legacy thing"`.
  - `shell_pages_draw_no_english_only_text_in_french`, in
    `src/ui/shell.rs` tests:
    1. Set the language to `fr`, with 2 rules, 1 history entry, one
       Custom tweak and an open confirm dialog.
    2. Render Home, Settings and History.
    3. Collect every drawn text.
    4. For every key whose English value differs from French and is at
       least 4 characters long, assert that the English value is **not**
       drawn verbatim.

    A drawn text equal to a raw detail (a path, a version, an extension,
    or an engine error) is not a key value, so it passes.
- [ ] **Step 2: Run** the two tests. Expected: FAIL. The first fails
  because the functions don't exist yet. The second catches whatever is
  still untranslated.
- [ ] **Step 3: Implement.** Fix every English leak the French test
  reports on these three pages, in the navigation, the toasts and the
  dialog. Then update the docs:
  - **ARCHITECTURE**:
    - the frame (navigation, pages, notices and autosave);
    - `open_page` as the single entry for side effects.
  - **CONTRIBUTING**, "Adding a module": add the navigation entry, the
    Home summary and the nav keys.
- [ ] **Step 4: Run** `cargo fmt --check`,
  `cargo clippy --all-targets --locked -- -D warnings`,
  `cargo test --locked` and `cargo build --release --locked`. Expected:
  all green.
- [ ] **Step 5: Commit**: `feat(i18n): translated history labels, French
  rendering check; docs for the new shell`.

---

## Left to the implementer

- The French wording of each new key, within the copy rules above.
- The exact grid and spacing of Home's tiles, using kit tokens.
- How tests locate a clickable widget: by rect from a previous frame's
  `widget_info`, or by a helper that runs the same code path.
- The order of rows inside a Settings section.
