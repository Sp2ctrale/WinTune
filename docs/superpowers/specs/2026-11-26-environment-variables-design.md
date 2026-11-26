# Environment variables module: design

Date: 2026-11-26. Status: design approved by delegation. The owner asked the
builder to choose the module and make the calls, and to ask only when the
owner is really needed. Every call made that way is marked **(call)**, with
what it costs if it is wrong. Evidence: `recon-envvars.md`, with each claim
tagged DOCUMENTED / OBSERVED / FOLKLORE, summarised in section 3.

## 1. Intent

WinTune is a toolbox of per-user Windows customisations, each one
snapshotted and undoable from History. The next module edits the user's
environment variables, and above all the user's `Path`, as a list. Windows'
own dialog is small, hard to find, shows `Path` as one long line or a modal
list, and has no undo. Power users and developers edit `Path` often, and a
mistake (a lost entry, a truncated value from `setx`) is painful. WinTune's
safety model fits well: pending changes, a snapshot and a `.reg` backup
before any write, and Restore from History. **(call)** This module was
chosen next because it is a classic power-user need, its registry location
and refresh message are documented by Microsoft, and it touches HKCU only.

## 2. Goals, non-goals, anti-goals

Goals:
- List the user's variables (`HKCU\Environment`) with their values; add,
  edit, rename and delete them.
- Edit the user's `Path` as a list: add, edit, remove and reorder entries;
  flag entries whose folder is missing, duplicates (also against the system
  `Path`), and show each entry expanded.
- A "Remove missing and duplicates" action on `Path`.
- Show the system variables (`HKLM`) read-only, so the user sees what their
  `Path` is appended to.
- Apply tells running programs that the environment changed. Undo from a
  row and from History; a Home tile.

Non-goals:
- Editing system variables (HKLM). **(call)** Same rule as every module.
- `HKCU\Volatile Environment` (set by Windows at each sign-in): not shown,
  never touched.
- Changing the environment of programs already running (Windows cannot;
  DOCUMENTED that a process cannot change a non-child's environment).
- Import or export of variable sets.

Anti-goals:
- Losing an unexpanded `%VAR%` reference, or changing a value's registry
  type when the user did not change the value (`setx` does both: it
  truncates at 1024 characters and expands references; DOCUMENTED).
- Writing anything under HKLM, or anything in `Volatile Environment`.
- A `Path` edit that silently drops an entry.

## 3. What Windows does

- **Locations:** system variables in `HKLM\SYSTEM\CurrentControlSet\
  Control\Session Manager\Environment` (DOCUMENTED, "Environment
  Variables"); user variables in `HKEY_USERS\<user>\Environment`, that is
  `HKCU\Environment` (DOCUMENTED, `Win32_Environment` class).
- **Types:** `REG_EXPAND_SZ` holds a value with unexpanded `%VAR%`
  references, `REG_SZ` a plain one (DOCUMENTED). Installers sometimes write
  `Path` as `REG_SZ` (FOLKLORE).
- **Refresh:** broadcast `WM_SETTINGCHANGE` with `lParam` = `"Environment"`
  through `SendMessageTimeout(HWND_BROADCAST, ...)`, `wParam` NULL
  (DOCUMENTED). The shell picks it up, so programs started afterwards from
  Explorer see the change. Running consoles keep their old environment.
  `SMTO_ABORTIFHUNG` and a 5-second timeout: FOLKLORE, the usual recipe.
- **Merge:** a new process gets the system variables, then the user ones;
  the user `Path` is appended to the system `Path`; another user variable
  with the same name as a system one wins (FOLKLORE, matches observed
  behaviour).
- **Limits:** a variable's value is at most 32,767 characters (DOCUMENTED).
  A name cannot contain `=` (DOCUMENTED). `cmd.exe` cannot handle a
  command line, so a `Path` it reads, over 8,191 characters (DOCUMENTED for
  the command line). Names are case-insensitive (FOLKLORE, registry value
  names are).
- **Shapes seen here (OBSERVED):** 7 user variables (`Path`, `TEMP`, `TMP`,
  `OneDrive`, `OneDriveCommercial`, two app variables). User `Path`:
  `REG_EXPAND_SZ`, 1,080 characters, 21 entries, 5 with `%VAR%`, 1 missing
  folder, 1 duplicate (same folder written once with `%LOCALAPPDATA%` and
  once literally), one with a trailing backslash. System `Path`: 17 entries.

## 4. Approaches considered

- **A. WinTune's engine on the registry (chosen).** Write the values in
  `HKCU\Environment` with the same safety model as the other modules, then
  broadcast. Keeps types and references exactly; undoable.
- **B. Shell out to `setx`.** Truncates at 1024 characters and expands
  references (DOCUMENTED). Rejected.
- **C. `SetEnvironmentVariable`.** Changes only WinTune's own process
  (DOCUMENTED). Rejected.

## 5. Design

### 5.1 Model

- Each user variable is one `backups::Slot::Value { path: "Environment",
  name }`. The slot id is the join key between row, pending change, store
  and History.
- Values are read raw (never expanded) with their type. Expansion
  (`ExpandEnvironmentStringsW`, or the same rules) is used only to display
  `Path` entries and to check whether a folder exists.
- A rename is a delete of the old name plus a write of the new one, in the
  same Apply (two slots).

### 5.2 Writing types **(call)**

- An existing value keeps its type when the user edits it, except that a
  `REG_SZ` value whose new text contains a `%name%` reference becomes
  `REG_EXPAND_SZ`. Cost if wrong: a value written as `REG_SZ` on purpose
  with a literal `%` pair changes type.
- A new variable is `REG_EXPAND_SZ` if its text contains a `%name%`
  reference, else `REG_SZ`. A new `Path` is always `REG_EXPAND_SZ`.
- A `%name%` reference means `%` + one or more characters other than `%`
  and `=` + `%`.

### 5.3 Validation (any problem refuses the whole Apply)

- Name: trimmed, not empty, no `=`, no NUL, at most 255 characters
  **(call)**; unique, ignoring case, among the user variables after the
  change.
- Value: not empty **(call: an empty value is refused, Delete is the way
  to remove a variable)**; at most 32,767 characters; no NUL.
- `Path` entries: an empty entry is dropped when the list is joined with
  `;`; an entry containing `;` is refused.
- Warnings (shown in the confirmation before Apply, not refusals):
  deleting or renaming `Path`, `TEMP`, `TMP` or a `OneDrive*` variable; a
  `Path` (user + system joined) longer than 8,191 characters.

### 5.4 Apply and Restore

Validate, snapshot on first touch (`record_first`), save the store
(`environment-backups.json`) and a `.reg` before any write, write, report
every failure, forget a slot back at its original (same presence, type and
bytes), then broadcast `WM_SETTINGCHANGE` "Environment" once if anything
was written. Restore from a row or History uses `backups::restore_ids` +
`restore_snapshot`, then broadcasts. The broadcast is injected (a function
in the app environment) so tests never send it. An unreadable store blocks
Apply and Restore.

- **Restoring `Path` (call):** the restore is exact (the original bytes and
  type), but its confirmation compares the original list with the current
  one and names the entries that will be **removed** (in the current `Path`,
  not in the original: added since by WinTune or by another program) and
  **added back**. Nothing is dropped silently. Cost if wrong: a user who
  wanted to keep an installer's folder must re-add it, but was told.
- **Renamed variables (call):** a rename's two slots are one undo unit.
  Restore original on the renamed row, and on either History row, restores
  both slots together (the old name comes back, the new one goes). The
  pairing is recorded with the snapshots. Cost if wrong: none in safety,
  only that a user cannot restore one half alone.

### 5.5 Page

Nav: "Environment variables" in the Customize group, after Startup apps.
Header note: "New programs see your changes. Windows that are already open,
such as terminals, keep the old values until you restart them."

- **Your Path** section (first, the main use): one row per entry, in
  order: the entry as written; its expanded form on hover when it differs;
  pills "Folder not found", "Duplicate" (same expanded folder, ignoring case
  and a trailing backslash, earlier in the user `Path` or anywhere in the
  system `Path`; the later copy is flagged). Each row: Move up, Move down,
  Edit, Remove. An "Add a folder" field with a Browse button (folder
  picker: `rfd::FileDialog::pick_folder`, as in `src/ui/settings.rs`; no
  new crate). A
  "Remove missing and duplicates" button that turns those entries into
  pending removals. If the user has no `Path`, the section offers to add
  the first folder.
- **Your variables** section: every user variable except `Path`, sorted by
  name: name, value (shortened, full on hover), a pill "Uses %…%" for
  `REG_EXPAND_SZ`. Edit (name and value in one dialog), Delete, Restore
  original (when a snapshot exists). An "Add a variable" button.
- **For every account (read only)** section, collapsed by default: the
  system variables, with the system `Path` as a list.
- Pending changes are shown on the rows (pills "Will be added / changed /
  removed", moved entries marked), the action bar shows "N changes not
  applied", Discard, Apply. A refused or partial Apply keeps the pending
  changes; a clean one clears them. If the registry changes under a pending
  edit (another program rewrote `Path`), the next scan shows the new value
  and the pending change is re-checked; **(call)** a pending `Path` edit is
  kept as the full wanted list, so Apply writes that list; the confirmation
  says "Path was changed by another program since you started editing" when
  the current value differs from the one the edit started from. Cost if
  wrong: an entry another program added meanwhile is dropped if the user
  confirms.
- Search (Ctrl+F) filters both sections by name, value or entry.
- History: module "Environment variables", filter chip, Saved originals
  group naming rows by variable name. Home tile: "{0} variables, {1} folders
  in Path". English and French.

## 6. Constraints

- Rust 1.88, egui/eframe 0.29.1, winreg 0.52, windows-sys 0.59; no new
  crate unless a folder picker needs one (prefer none).
- Tests touch only `HKCU\Software\WinTuneTest` and temp folders; the
  registry targets and the broadcast are injected.
- Engine reasons the page shows are enums translated in
  `i18n/messages.rs`; every text is a key in both `.resx` files.
- Follow CONTRIBUTING.md "For a new module" and the Startup apps module's
  layout (engine in `src/environment/`, UI state, tab, jobs, History,
  Home), copying the per-module pattern.

## 7. Delivery

One PR (`feat/envvars-1`). The owner's real-machine checklist, written into
`docs/registry-reference.md`:
1. Add a variable in WinTune, Apply, open a new terminal from Start:
   `echo %NAME%` shows it. Windows' Environment Variables dialog shows it.
2. Add a folder to `Path`, Apply: a new terminal finds a program in it
   (`where <program>`). Entries with `%VAR%` are unchanged in the dialog
   (still unexpanded) and `Path` is still `REG_EXPAND_SZ`
   (`reg query HKCU\Environment /v Path`).
3. Move an entry up, remove one, Apply: the dialog shows the new order.
4. Restore `Path` from History: the dialog shows the original.
5. Delete the test variable: gone in a new terminal.

## 8. Open questions for the owner

None.

## 9. Left to the builder

Already settled by the builder check, as the builder's calls: removing every
`Path` entry counts as deleting `Path` (with its warning); user values that
are not text are shown read-only; pending edits to other variables are used
when expanding `Path` entries for display; an entry whose `%VAR%` cannot be
expanded is not flagged "Folder not found"; a rename writes the new name
before deleting the old one; a rename that changes only letter case is
handled. Also:

- Module layout, enum and key names, wording in English and French, the
  nav icon.
- How the Path list editor and the variable edit dialog are drawn within
  the kit.
- Whether `ExpandEnvironmentStringsW` or a small expander is used for
  display (it must use the current user's values).
