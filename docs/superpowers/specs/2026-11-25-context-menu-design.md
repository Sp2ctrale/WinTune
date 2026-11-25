# WinTune "Context menu" module: design

Date: 2026-11-25. Status: reviewed by a builder check, awaiting the owner's
approval. Calls the designer made without the owner are marked
*(Designer's call)*.

## 1. Intent

WinTune is growing from a file-association tool into a toolbox of reversible
Windows customizations. The next module is the **right-click menu** of File
Explorer, because it is one of the things people most often want to change.

The owner has no personal pain here: they already have "Open with Code"
and "Open in Terminal" on folders and find them very handy. The module is for
WinTune's users in general. It does two jobs:

1. **Add your own entries**, such as "Open with <program>" on folders.
2. **Manage the entries that already exist**: hide them and, where Windows
   allows it, rename them.

Like everything in WinTune, every change is tracked and can be undone from
History, back to exactly what was there before.

## 2. Goals, non-goals, anti-goals

**Goals**
- A user can add an entry that opens a chosen program on the clicked folder,
  folder background, drive or file, with a label and an icon.
- A user can hide well-known Windows entries from a curated list with real
  names, and hide entries that other programs installed, found by scanning
  the registry.
- A user can rename classic entries (those whose label lives in the
  registry), where the real-machine check in section 7 confirms it works.
- Every change is per user (the current account), snapshotted before it is
  written, and restorable from History.
- The page follows the redesign: kit components, translated texts (English
  and French), the page-owned Discard/Apply bar, headless UI tests.

**Non-goals**
- **No entries in the Windows 11 short menu.** Entries added through the
  registry show under "Show more options" on Windows 11. Reaching the short
  menu needs a COM `IExplorerCommand` DLL in a signed sparse package, which
  is far too heavy for this module. The page says so and links to the
  existing "Classic context menu" tweak (Explorer & Taskbar page).
- **No advanced entry options**: no free command-line arguments, no "Run as
  administrator", no "Shift only" (extended) entries, no submenus. The owner
  chose the simple version.
- **No per-extension entries.** Adding a verb to one file type is the File
  associations module's job.
- **No renaming of modern entries** (packaged apps' `IExplorerCommand`
  entries such as "Open in Terminal"): Windows gives them their label in
  code. They can only be hidden.
- **No scan of third-party packaged (MSIX) entries.** Finding them means
  reading app manifests. Only the ones in the curated Windows list are
  covered.
- No machine-wide (HKLM) writes unless the owner decides so after the
  real-machine check (section 7, open question 1).

**Anti-goals** (the module fails if any of these happen)
- A change can't be undone, or "Restore original" doesn't bring back the
  exact previous state (including "absent").
- WinTune deletes or rewrites a key another program owns, instead of adding
  a reversible marker next to it.
- An added entry breaks on a path with spaces, apostrophes or non-ASCII
  characters.
- The scan makes the page slow to open or freezes the UI.
- An entry is shown as hideable but hiding it silently does nothing.

## 3. Constraints

- Same stack and rules as the rest of the app: Rust 1.88, egui/eframe 0.29,
  `regsnap` for snapshots, `persist` for stores, `ui/jobs.rs` for long work.
- The safety model in ARCHITECTURE.md applies unchanged: validate, snapshot
  on first touch, persist the store and export a `.reg` **before** writing,
  report every failure, block writing when the store can't be read.
- **Only registry mechanisms confirmed in Microsoft documentation**, or
  flagged `unofficial` in the UI, as the classic context menu tweak already
  is. Every mechanism used gets a row in `docs/registry-reference.md`.
- Tests run unelevated, in the `HKCU\Software\WinTuneTest` sandbox, through
  an injectable target (as tweaks and associations do).
- WinTune runs elevated. HKCU is the account WinTune runs as; the page
  subtitle names it, like the tweak pages do.

## 4. Approaches considered

- **A. One module with its own engine (chosen by the owner).** A new
  `src/context_menu.rs` engine on `regsnap`, its own snapshot store, its own
  page with three sections. Adding, hiding and renaming touch the same keys
  (the `shell` and `shellex` keys under `*`, `Directory`, `Drive`…), so one
  engine sees all of them and gives one restore story. Cost: the largest
  amount of new code.
- **B. Reuse the two existing engines.** Windows entries as tweaks, added
  entries as a new association rule type. Less new code, but tweaks are a
  fixed catalog and third-party entries differ on every PC, so both engines
  would be bent out of shape.
- **C. Two pages.** Hide/rename on a "Context menu" page, adding entries on
  the File associations page. Users would look for the right-click menu in
  two places, and the associations page is already full.

## 5. Decisions made by the owner

- The module both adds entries and manages existing ones.
- Existing entries: **the mix**. Windows entries come from a curated list
  with real names; third-party entries come from a registry scan, shown with
  their label, where they appear, and the program they come from when it
  can be found.
- Added entries: **the simple version**. A label, a program, where it
  appears, and an icon. No arguments, no admin, no Shift-only, no submenus.
- Approach A.
- Entries whose `Blocked` hide has side effects are **kept, with a warning**
  that names the side effect (section 6.4).

## 6. Design

### 6.1 Where entries live (Microsoft Learn: *Creating Shell Extension Handlers*, *Creating Shortcut Menu Handlers*, *Merged View of HKEY_CLASSES_ROOT*)

The menu is built from `HKEY_CLASSES_ROOT`, the merge of
`HKCU\Software\Classes` (per user) over `HKLM\Software\Classes` (machine).
The module works on these object types:

| Place in the UI | Key under `Classes` | Placeholder |
|---|---|---|
| Folder | `Directory` | `%1` |
| Folder background | `Directory\Background` | `%V` |
| Drive | `Drive` | `%1` |
| Files | `*` | `%1` |

Two kinds of existing entries:
- **Static verbs**: `<type>\shell\<verb>`, label in the default value or
  `MUIVerb`, command in `<verb>\command`. Readable, renamable.
- **Handlers**: `<type>\shellex\ContextMenuHandlers\<name>` whose default
  value (or name) is a CLSID of a COM DLL. The label comes from the DLL at
  run time, so the UI shows the handler name and the DLL's product name.
  Also `AllFilesystemObjects` and `Folder` handlers, which show on folders
  and files.

Modern entries (packaged apps, `IExplorerCommand`) are not in these keys and
are only reachable through the curated list, by CLSID.

### 6.2 Adding an entry

An added entry is stored in `config.json`, in a new `context_menu` section
with serde defaults so old configs load. **The list is kept per user**
(keyed like the snapshot store), because the entries are written to that
user's HKCU: another admin account running WinTune sees and applies only its
own list. *(Designer's call, after the builder check.)*

**Pending-change model** *(designer's call)*: the same as File associations
for added entries, the same as the tweak pages for existing entries.
- Added entries: `config.json` holds what the user wants. Edits are
  autosaved like rules; Apply writes them to the registry. Each entry shows
  its live state (applied, not applied, changed outside WinTune), like rule
  status. Closing without applying keeps the edits, still marked not
  applied. Restoring an added entry's original from History deletes its keys
  but leaves it in the list, marked not applied; deleting it from the list
  is a separate, explicit action.
- Hide/show and rename of existing entries: held in memory as pending
  changes until Apply; Discard clears them; the registry is the truth.
- Discard therefore only clears pending hide/show/rename changes; it never
  touches `config.json`.

Each entry has:

- `label`: the menu text, 1 to 80 characters, not blank.
- `program`: a preset (`vscode`, `terminal`, `powershell`, `cmd`) or a
  custom `.exe` path.
- `places`: a non-empty subset of {folder, background, drive, files}.
- `icon`: by default the program's own icon; can be changed to another
  `.ico`/`.exe`/`.dll` path.
- an internal id, stable across renames.

**Writing.** Each entry is written per place as
`HKCU\Software\Classes\<type>\shell\WinTune.<id>` with `MUIVerb` = label,
`Icon` = icon path, and `command` default = the command line. The `WinTune.`
prefix marks the key as ours. Removing an entry deletes its keys; its
snapshot (the "absent" original) makes this a normal restore.

**Commands** (the builder confirms each on a real machine, section 7):
- custom program: `"<exe>" "%1"` (`"%V"` for background);
- `vscode`: the path from the existing `detector::resolve_vscode_executable`,
  same shape as a custom program;
- `terminal`: `wt.exe -d "<placeholder>"`;
- `powershell`: the same command Windows itself uses for "Open PowerShell
  window here" (`Set-Location -LiteralPath '%V'`), so apostrophes in paths
  are handled the Windows way;
- `cmd`: `cmd.exe /s /k pushd "<placeholder>"`.

Presets whose program isn't found are listed disabled, with the reason on
hover. Validation (as enum codes, translated by the UI): empty label, label
too long, no place chosen, program not found, icon not found, two entries
with the same label on the same place.

### 6.3 Hiding and renaming existing entries

- **Static verbs: hide** with a `ProgrammaticAccessOnly` REG_SZ value
  (documented: the verb is never shown in a menu). **Rename** with a
  `MUIVerb` value. The goal is to write these per user, in an
  `HKCU\Software\Classes` copy of the verb's key, so a verb installed in
  HKLM is hidden only for this account. Whether that HKCU copy hides the
  HKLM verb is **not settled by the documentation** (the merged-view rules
  describe immediate subkeys only); the first plan task checks it on a real
  machine (section 7).
- **Handlers and modern entries: hide** by adding their CLSID to
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked`.
  This is **undocumented**, so those rows carry the existing "Unofficial"
  pill. Handlers can't be renamed.
- WinTune never deletes or renames another program's key. It adds or sets a
  value or a `Blocked` entry, and the snapshot records the state before.
  When the verb is a per-user install (its key is already in HKCU) and
  already has a `MUIVerb`, Rename overwrites it after the snapshot, so
  Restore original puts the program's label back. *(Designer's call.)*
- A program's updater can rewrite its key and remove WinTune's marker. The
  scan then shows the entry as "changed outside WinTune", like drift on the
  other pages, and Apply can set it again.

### 6.4 The curated Windows list

A static catalog like the tweak catalog: id, translation keys for name and
description, mechanism (verb or CLSID), the places it shows, OS requirement.
Candidates, each kept only if the builder can confirm its key or CLSID on a
real Windows 11 and Windows 10 machine and document it in
`docs/registry-reference.md`:

- Open in Terminal (modern entry);
- Share;
- Give access to;
- Restore previous versions;
- Include in library;
- Pin to Start;
- Send to;
- Edit with Photos / Edit with Clipchamp (modern entries, if their CLSIDs
  are stable).

A scanned handler whose CLSID is in the curated list is shown only once, in
the Windows section.

**Side effects of `Blocked` (owner's decision: keep, with a warning).**
`Blocked` turns off the whole extension behind a CLSID, everywhere, not just
one menu item. Likely examples, to confirm in the real-machine check: hiding
"Give access to" also removes the **Sharing** tab of folder Properties, and
hiding "Restore previous versions" also removes the **Previous Versions**
tab. These entries stay in the list, and each catalog row carries an
"also affects" line naming the side effect found during the check (shown on
the row, and in the confirmation when the change is applied). Every hidden
third-party handler gets a general line: it is hidden everywhere the program
adds it, not only in the places listed.

### 6.5 The scan

- Reads `shell` and `shellex\ContextMenuHandlers` under `*`,
  `AllFilesystemObjects`, `Folder`, `Directory`, `Directory\Background` and
  `Drive`, in both HKCU and HKLM `Classes`, through the injectable target.
- Skips `WinTune.*` keys (shown in "My entries"), canonical verbs Windows
  draws itself (`open`, `explore`, `find`, `opennew`, `print`, `runas`…),
  and entries already in the curated list.
- For each entry: label (verb label, or handler name), places, origin (the
  product name of the command's `.exe` or the handler's DLL from its version
  information, or the file name when there is none), current state (shown,
  hidden by WinTune, hidden by something else).
- Runs on a background job, like the association rule status, at page open
  and after each apply; never under a running job. The page shows a loading
  state meanwhile.
- An entry WinTune can't hide reliably (for example a verb whose key it
  can't read) is listed with a disabled toggle and the reason on hover.

### 6.6 Snapshots, apply and History

- **Store**: `%ProgramData%\Wintune\context-menu-backups.json`, per user then
  per touched key or `Blocked` value, like `tweaks-backups.json`. Unreadable
  store → the page and the History group show the error banner and block
  writing, as for tweaks.
- **Apply**: pending changes (added/edited/removed entries, toggled hides,
  renames) are applied by one job, after the snapshot and `.reg` export.
  Changes show on the next right-click for verbs; `Blocked` changes may need
  an Explorer restart, and those rows say so (`Refresh::ExplorerRestart`).
- **History**: the operation log gets the module label `Context menu`
  (`history.jsonl` stays English, translated when drawn). Saved originals
  gain a "Context menu" group with Restore original and Restore all, through
  the existing confirmation.

### 6.7 The page

The disabled "Context menu" navigation entry and Home tile become real.
Home shows e.g. "3 entries added, 2 hidden".

```
Context menu
For <account>. On Windows 11 these entries appear under "Show more
options". [Use the classic menu]  (link to the Explorer & Taskbar page)

My entries                                           [+ Add entry]
  Open with VS Code     Folder, Background        …
  Open in PowerShell    Background                …
  ┌ detail of the selected entry ───────────────────────────┐
  │ Label      [ Open with VS Code ]                        │
  │ Program    [ VS Code ▾ ]                                 │
  │ Appears on [x] Folder [x] Background [ ] Drive [ ] Files │
  │ Icon       VS Code's icon  [Change]                      │
  └──────────────────────────────────────────────────────────┘

Windows
  Open in Terminal      Folder, Background   Unofficial  [on]
  Share                 Files                Unofficial  [on]
  …

Other programs                                   [search]
  7-Zip                 Files, Folder   7-Zip    Unofficial  [on]
  Open with Code        Folder          VS Code              [on]  …
  …
                                      [Discard]  [Apply]
```

- Toggles read "shown" when on; turning one off hides the entry.
- The `…` menu holds Rename (static verbs only) and Restore original when a
  snapshot exists.
- Ctrl+F focuses the "Other programs" search.
- The layout is the builder's within the kit; the sketch fixes the content
  and order, not pixels.

## 7. Delivery

Two PRs, the owner checks each on a real machine before merging:

1. **Engine + "My entries".** Real-machine check first (the plan's Task 1,
   results written into `docs/registry-reference.md`):
   - do `MUIVerb`, `Icon` and the four command shapes work on folder,
     background, drive and files, with spaces, apostrophes and accents;
   - does an HKCU copy of an HKLM verb's key with `ProgrammaticAccessOnly`
     hide it, and does `MUIVerb` in that copy rename it, without breaking
     the verb's command;
   - does the `Blocked` key hide handlers and modern entries per user, what
     refresh it needs, and what else each curated CLSID turns off (tabs in
     Properties, other menus), so the warnings name it.
   Then the engine, the store, the page with "My entries", Home and History.
2. **Managing existing entries.** The curated Windows list, the scan, the
   two sections, Rename.

## 8. Open questions for the owner (after the real-machine check)

1. If an HKCU copy **can't** hide or rename a machine-wide (HKLM) verb: drop
   hide/rename for those verbs (shown disabled with a reason), or write
   into HKLM, which affects every account on the PC?

## 9. Left to the builder

- Exact data types, enum names, file split inside the engine and UI.
- Which canonical verbs to skip in the scan (beyond the list above).
- How origin names are read from version information.
- Icons and spacing within the kit, and the texts' exact wording (English
  and French), following the redesign's tone.
- Which curated entries survive the real-machine check.
