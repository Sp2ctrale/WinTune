# WinTune UI redesign: design

Date: 2026-11-23. Status: draft, awaiting the owner's approval.

## 1. Intent

The owner finds the current interface visually weak ("pas ouf", with visual
defects he can't name one by one) and wants to **start over visually**. The
engines (registry, tweaks, history, stores) are solid and tested, so they
stay as they are. Only the interface is rebuilt.

The redesign should make WinTune feel like a natural part of Windows 11. It
should also put forward what makes the app different: everything it changes
is tracked, and can be checked and undone.

The app will also become **multilingual**, using `.resx` files so that a new
language can be added without touching code. **English is the default.**

## 2. Goals, non-goals, anti-goals

**Goals**
- A new, consistent visual language: one accent color, real Windows fonts
  and icons, consistent spacing, toggles instead of checkboxes, and short
  texts.
- A new information architecture: a grouped sidebar, a Home dashboard, a
  Settings page, and History taking over the Rollback tab.
- Simpler screens: the essentials first, technical fields folded away.
- English and French shipped, with more languages added by dropping in a
  `.resx` file.
- Every existing behaviour is kept: confirmations, disabled-button reasons,
  Ctrl+F, background jobs with progress, the close guard, live status, and
  per-tab scoping of pending changes.

**Non-goals**
- No change to the engines' logic, stores, file formats or registry
  behaviour. `config.json` only gains a `language` field. **There is one
  exception, decided by the owner**: drift reasons, validation problems
  and the `TweakStatus::Custom` reason become codes (enums with their
  parameters) instead of English sentences, so that the UI can translate
  them. Only the shape of these messages changes, never when or why they
  are produced. The engine tests that check these messages are updated
  to the codes.
- No new customization module. The "Context menu" module (Open in VS Code
  or Terminal on folders) is a separate project that comes after this one.
  The navigation only reserves its place.
- No change of UI technology: it stays egui/eframe 0.29.

**Anti-goals** (the redesign fails if any of these happen, even if it looks
better)
- A safety step is lost: a confirmation, a snapshot before writing, a
  blocked state when a store is unreadable, or a refusal while a job runs.
- A registry action can be triggered by a keyboard shortcut.
- The UI tests can no longer run headless, or an engine test loses what
  it checks. Moving a test's assertion from a sentence to the matching
  code is fine.
- A screen shows untranslated text in French, or crashes on a missing
  translation.

## 3. Constraints

- **egui 0.29 has no native Windows material**, such as the Mica
  background. The look is drawn by egui and should come as close as
  possible to the mockups.
- **Fonts come from Windows.** Text uses `C:\Windows\Fonts\SegUIVar.ttf`
  (Segoe UI Variable, Windows 11), falling back to `segoeui.ttf`. Icons use
  `SegoeIcons.ttf` (Segoe Fluent Icons, Windows 11), falling back to
  `segmdl2.ttf` (Segoe MDL2 Assets, Windows 10). These fonts are **loaded
  at runtime, never bundled**, because their license doesn't allow it. If
  none is found (for example on a CI image), the app falls back to egui's
  default font, and each icon is replaced by a text label or nothing; it
  must never crash.
- **Tests stay headless**: they run `ctx.run` with `ui::test_support`, and
  must not depend on system fonts being present.
- **The app runs elevated (UAC).** Nothing in the redesign changes that.

## 4. Approaches considered

1. **Restyle the current code in place** (theme, fonts, icons, screen by
   screen). This is the fastest, but the layout problems stay; it is not a
   fresh start. Rejected.
2. **Build a layer of components on egui, then rewrite each screen on
   it.** The components are the sidebar, page header, cards, setting rows,
   toggle, status pill, action bar, banner and empty state, along with
   fonts, icons and translations. The engines and their tests are
   untouched. **Chosen by the owner.**
3. **Switch UI technology** (Tauri or Slint). The web rendering would match
   the mockups more closely, but the whole UI would have to be rewritten in
   another language, with a bridge between Rust and the UI, and the
   headless UI tests would be lost. That is too costly for a mostly
   cosmetic gain. Rejected.

## 5. Decisions made by the owner

- **Visual direction A**, "Windows 11 Settings" style, **with the Home
  dashboard of direction C** as the first screen. The mockups are saved in
  [`2026-11-23-ui-redesign-mockups/`](2026-11-23-ui-redesign-mockups/):
  - `1-three-directions.html` shows the three directions;
  - `2-direction-a-home-and-associations.html` shows the chosen
    direction's Home and File associations.

  They set the layout and density. Exact values stay within §6.1.
- **Navigation**, in this order:
  - **Home**
  - group **Customize**: File associations, Explorer & Taskbar, Start &
    Privacy, Context menu (placeholder)
  - group **Tools**: Icons, History
  - **Settings**, pinned at the bottom
- **Rollback becomes part of History**: there is a "Restore original" button
  on each item, plus a full list of saved originals in History. The
  Selective Rollback tab is removed.
- **The editor path moves to Settings**, along with the theme. It is no
  longer at the top of the associations screen.
- **Translations live in `.resx` files, English by default.**
- **Build approach 2**: an egui component layer, then each screen rewritten
  on it.

## 6. Design

### 6.1 Visual language

- **Theme.** There is a light and a dark palette. The setting offers
  *System* (follow Windows), *Light* or *Dark*, and **defaults to System**
  *(builder's call; the owner can override it)*. The current Windows mode
  is read from
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`.
- **Colors.**
  - One accent, Windows blue: `#0078D4`, with `#4CC2FF`-like tones in dark.
  - Neutral surfaces in three levels: page, card, and control or hover.
  - A status set:
    - success: applied, up to date;
    - warning: changed by Windows, custom value;
    - accent: pending;
    - neutral: off, or not applied;
    - danger: error.
- **Shape and space.** Corners are 4 px on controls and 8 px on cards, with
  1 px hairline borders. Spacing follows a 4 px scale: 4, 8, 12, 16, 24 and
  32.
- **Type scale.**

  | Use | Size |
  | --- | --- |
  | Page title | 24 px |
  | Section | 15 px, semibold |
  | Body | 14 px |
  | Secondary | 12 px |

  Sentence case everywhere, and texts as short as possible.
- **Icons** come from the Windows icon font (§3), one per navigation entry
  and per setting row.

### 6.2 Component layer (`src/ui/kit/`)

All screens are built from these components. No screen sets colors or
spacing itself; everything goes through the kit's tokens.

| Component | Role |
| --- | --- |
| `NavSidebar` | App mark, grouped entries (icon, label, selected state, optional count badge), Settings pinned at the bottom |
| `PageHeader` | Breadcrumb (`WinTune › Explorer`), title, one-line subtitle, optional actions on the right |
| `Card` / `Section` | Grouped content on a raised surface |
| `SettingRow` | Icon, title, description, a trailing control (toggle, button or chip), optional status pill, optional `…` menu |
| `Toggle` | An animated on/off switch, with a disabled state and a hover explanation |
| `StatusPill` | Applied, Changed, Pending, Off, Custom, Windows 11, Unofficial |
| `ActionBar` | A bar at the bottom of a page: a status text on the left (for example "n changes pending", "Not applied yet"), secondary and primary actions on the right. While a job runs, it shows the job's progress instead. Each page decides when to show it and what it holds. |
| `Banner` | Info, warning or error, with one action |
| `EmptyState` | Icon, a one-line invitation, an action |
| `Segmented` | A choice between 2 or 3 values (for example the double-click action) |
| `Disclosure` | A folded "Advanced options" section |
| `Toast` | A short message that disappears on its own (for example "3 tweaks applied"). Error toasts stay until dismissed. |
| `Dialog` | The existing confirmation modal, restyled, plus a larger variant for pickers |

The current status bar is removed. Its messages become toasts and banners,
and the full details of each operation stay available in History.

### 6.3 Screens

**Home**
- Title and subtitle: "Everything WinTune changed can be undone".
- A **drift banner** when Windows or another app took back something WinTune
  applied: associations in the *Changed* state, or tweaks in *Custom*. It
  has a "Review" button that opens the right page.
- One **module card** per module, showing its state ("42 active · 2
  changed", "5 of 13 on"). Clicking a card opens its page. The Context menu
  card is shown as "Coming soon" and is disabled.
- **Recent**: the last 5 history entries, and a link to History.

**File associations**
- **List** on the left: a search box (Ctrl+F) and one row per rule. A row
  has the enable checkbox, the icon, the extension, and a status dot
  (green: up to date, orange: changed, empty: not applied, red: invalid).
  "+ Add" sits at the bottom.
- **Detail** of the selected rule:
  - **Header**: icon, "`.rs` · Rust", the editor it opens with, a status
    pill, and the drift reasons when there are any.
  - **On double-click**: a `Segmented` choice between Open in editor and
    Run. Run shows its command fields.
  - **Appearance**: the current icon and a "Change" button that opens an
    **icon picker dialog**. The user no longer jumps to the Icons tab.
  - **Options** as toggles: clean per-user overrides; ignore Windows'
    "Open with" choice (UserChoice), with its known limit explained on
    hover.
  - **Advanced options**, folded by default: ProgID, description,
    PerceivedType, MIME type, run as administrator, custom right-click
    verbs.
  - **ActionBar**, always shown while a rule is selected: Remove rule (with
    confirmation), Restore original (only when a snapshot exists, with
    confirmation), and Apply, which applies this rule. The status text
    gives the rule's state against the registry (up to date, changed, not
    applied).
- **Saving (decided by the owner): autosave.**
  - Every rule edit is written to `config.json` automatically: a field,
    adding or removing a rule, an icon choice, Check or uncheck all. The
    write happens shortly after the last edit (debounced), and also on
    close.
  - There is no Save button and no Discard. "Apply" only writes the
    registry.
  - A failed save shows an error banner, which stays until a save
    succeeds. If the config is unreadable at startup, saving stays
    blocked, as today.
  - The close guard is unchanged: it still only covers pending tweak
    changes and running jobs, since rule edits are never lost.
- **Page header actions**: Apply all enabled, and Check or uncheck all.
- Validation, live status and refresh, disabled reasons and empty states
  all keep their current behaviour.

**Explorer & Taskbar, and Start & Privacy** (one shared implementation, as
today)
- A page header with a "Restore all originals" button (with confirmation,
  scoped to the tab), and a search box (Ctrl+F).
- A `Section` per group, with a `SettingRow` per tweak:
  - the toggle;
  - the title and description;
  - the status pills (Custom, Windows 11, Unofficial with a hover text);
  - when the change becomes visible, as small secondary text;
  - a `…` menu with "Restore original" when a snapshot exists.
- When the tab has pending changes, an `ActionBar` appears with Discard and
  Apply, scoped to the tab.

**Icons**
- A grid of icons with a search box and category chips.
- "Import .ico" and "Build icon pack" sit in the page header.
- The same grid is reused inside the icon picker dialog.

**History**
- **Operations**: the list from `history.jsonl`. It can be filtered by
  module, and each entry can be expanded to show its errors and warnings
  and to copy its `.reg` path.
- **Saved originals**: every snapshot, grouped by module (associations
  from `backups.json`, tweaks from `tweaks-backups.json` for the current
  user). Each one has a "Restore original" button, and **each module
  group** has a "Restore all" button. Both ask for confirmation. There is
  no global "restore everything" button, so each click runs exactly one
  of the existing jobs: `start_rollback` for associations, or the
  tweak restore-all for tweaks. *(Builder's call.)*
- This page replaces the Selective Rollback tab.

**Settings**
- **Language**: English or Français. The language changes immediately,
  without a restart.
- **Theme**: System, Light or Dark.
- **Default code editor**: path, Browse, Auto-detect.
- **Icon folder**: the icon source folder (path, Change). It sets where
  `.ico` imports go and where gallery.pak is built from. The Icons page
  header shows it read-only, with a link to this setting.
- **System**:
  - the WSL state ("WSL 2 detected" or "No WSL: .sh opens in the editor");
    the `.sh` rule detail shows the same hint;
  - a "Restart Explorer" button, which replaces the sidebar's Refresh
    Windows Explorer button.
- **Data folder**: a link that opens `%ProgramData%\Wintune` in Explorer.
- **About**: version and links.

*(The placement of the icon folder, WSL state and Restart Explorer is the
builder's call. No existing control is dropped.)*
- The config file's error state stays: if `config.json` is unreadable, a
  banner says so here, and saving is blocked.

### 6.4 Translations (`.resx`)

- **Files**:
  - `i18n/strings.resx`: English, the neutral reference;
  - `i18n/strings.fr.resx`: French.

  The format is standard .NET ResX: `<data name="key"><value>…</value></data>`.
  A new language is one more `strings.<code>.resx` file.
- **Loading**: the files are embedded at compile time (`include_str!`) and
  parsed once at startup. Parsing XML with `quick-xml` or a small parser is
  the builder's call.
- **API**:
  - `t("nav.home")` returns the text;
  - `t_args("history.applied_n", &[&n])` fills .NET-style placeholders
    (`{0}`, `{1}`).

  A missing key in a translation falls back to English. A key missing even
  in English shows the key itself, and never panics.
- **Language choice**: stored in `config.json` as `language: "en" | "fr"`,
  defaulting to `en` when the field is absent. Older configs therefore keep
  loading.
- **Tests**:
  - every key of each `strings.<code>.resx` exists in English (no orphan
    keys);
  - shipped languages (en, fr) must be complete;
  - every key used in the code exists in English. A `build.rs` step or a
    test that scans the `t("…")` calls is the builder's call.
- **Scope**: every text drawn by the UI is translated, including content
  that comes from the engines:
  - **Tweak catalog**: titles, descriptions, group names and "when it
    becomes visible". Keys are built from the tweak and group ids, for
    example `tweak.<id>.title`. The English text moves out of `tweaks.rs`
    into `strings.resx`, and the test
    `the_registry_reference_lists_every_tweak` keeps working on the ids.
  - **History**: module and action labels map to keys. A label from an
    older `history.jsonl` that is unknown is shown as stored.
  - **Drift reasons, validation problems and the Custom reason**: these
    become codes (see Non-goals), and each code has a key.
  - **The only exception**: raw OS error details (for example "Access is
    denied. (os error 5)"). The UI shows them under a translated summary.
    The French rendering test allows this text and nothing else.

## 7. Delivery

The work ships as a series of PRs, each one green (fmt, clippy with
`-D warnings`, tests) and each one leaving the app usable. Between PRs, new
and old screens may coexist.

1. **Foundations**: kit tokens and components, theme with the System mode,
   runtime font and icon loading with fallbacks, translation system and
   `.resx` files (en and fr) with their tests. Engine messages switch to
   codes, and the tweak catalog's texts move to `strings.resx`.
2. **Shell**: the new sidebar, the Home page, the Settings page (with
   language, theme and editor path moved there), toasts replacing the
   status bar.
3. **File associations** screen, with the icon picker dialog.
4. **The two tweak screens.**
5. **Icons** page and **History** page (with saved originals), then removal
   of the Rollback tab.
6. **Clean-up**: remove dead UI code and old styles, finish the
   translations, update the README and ARCHITECTURE.

**Tests.** Each screen keeps or gains headless tests: rendering in both
languages, key flows (apply, restore with confirmation, pending changes per
tab, picker dialog), and the translation tests. The engine tests stay
unchanged and green. The owner checks the look on a real machine after
PRs 2, 3 and 5.

## 8. Left to the builder

- Exact color values and the type ramp within the rules of §6.1, and how
  the toggle animation is drawn.
- Which XML parser to use for `.resx`, and how key usage is checked.
- How the kit is split into files, and component APIs.
- The order of sections inside a screen, as long as §6.3 is covered.
- Toast timing and position.
- Icon glyph choices (code points of the Windows icon fonts).
