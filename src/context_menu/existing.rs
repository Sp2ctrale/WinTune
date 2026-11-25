//! Read-only scan of the entries already in the Explorer right-click menu:
//! the Windows ones the catalog knows, and third-party verbs and handlers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use winreg::enums::*;
use winreg::RegKey;

use super::catalog::{catalog, catalog_clsid, CatalogItem, CatalogMechanism, Presence};
use super::{MenuTarget, Place, KEY_PREFIX};
use crate::backups::{RegTarget, Slot, Snapshot, UserBackups};
use crate::regsnap::RegistryKeySnapshot;
use crate::tweaks::OsRequirement;

/// Per-user list of shell extensions Explorer must not load (value name =
/// CLSID). The same path under HKLM blocks them for every user.
pub const BLOCKED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";

/// Classes under `Software\Classes` whose verbs and handlers are scanned.
pub const SCAN_CLASSES: [&str; 6] = [
    "*",
    "AllFilesystemObjects",
    "Folder",
    "Directory",
    r"Directory\Background",
    "Drive",
];

/// Where an entry registered on `class` shows up.
pub fn class_places(class: &str) -> &'static [Place] {
    match class {
        "*" => &[Place::Files],
        "AllFilesystemObjects" => &[Place::Files, Place::Folder],
        "Folder" => &[Place::Folder, Place::Drive],
        "Directory" => &[Place::Folder],
        r"Directory\Background" => &[Place::Background],
        "Drive" => &[Place::Drive],
        _ => &[],
    }
}

/// One verb key: `Software\Classes\<class>\shell\<verb>`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VerbRef {
    pub class: String,
    pub verb: String,
}

impl VerbRef {
    /// The key of this verb's HKCU copy (relative to the user root).
    pub fn user_key(&self) -> String {
        format!(r"Software\Classes\{}\shell\{}", self.class, self.verb)
    }
}

/// How an existing entry is hidden or renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mechanism {
    /// Classic verbs, through an HKCU copy of each key.
    Verbs(Vec<VerbRef>),
    /// A shell extension, through a value under [`BLOCKED_KEY`].
    Clsid(String),
}

impl Mechanism {
    /// What hiding or renaming snapshots first.
    pub fn slots(&self) -> Vec<Slot> {
        match self {
            Mechanism::Verbs(refs) => refs.iter().map(|r| Slot::Key(r.user_key())).collect(),
            Mechanism::Clsid(clsid) => vec![Slot::Value {
                path: BLOCKED_KEY.to_string(),
                name: clsid.clone(),
            }],
        }
    }

    /// Only a verb has a label of its own to change.
    pub fn renamable(&self) -> bool {
        matches!(self, Mechanism::Verbs(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingState {
    Shown,
    HiddenByWinTune,
    HiddenElsewhere,
}

/// Why WinTune cannot change an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locked {
    /// Hidden for every user (HKLM `Blocked`); only an administrator undoes it.
    MachineWide,
    /// A key exists but cannot be read.
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExistingEntry {
    /// `win:<catalog id>`, `verb:<name>|<origin file>` or `clsid:<CLSID>`.
    pub id: String,
    /// The name shown in the menu. For a catalog row it is the catalog id:
    /// the UI shows the translated name from `catalog` instead.
    pub label: String,
    /// Verbs only: the label Reset name returns to.
    pub original_label: Option<String>,
    /// The catalog id of a Windows entry.
    pub catalog: Option<&'static str>,
    pub mechanism: Mechanism,
    pub places: BTreeSet<Place>,
    /// Product name (else file name) of the program behind the entry.
    pub origin: Option<String>,
    pub state: ExistingState,
    pub locked: Option<Locked>,
    /// WinTune holds an original state for one of the mechanism's slots.
    pub has_snapshot: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    /// Catalog entries present on this machine, in catalog order.
    pub windows: Vec<ExistingEntry>,
    /// Third-party verbs and handlers, by label.
    pub others: Vec<ExistingEntry>,
}

impl Scan {
    fn all(&self) -> impl Iterator<Item = &ExistingEntry> {
        self.windows.iter().chain(&self.others)
    }

    pub fn find(&self, id: &str) -> Option<&ExistingEntry> {
        self.all().find(|e| e.id == id)
    }

    /// The row one of whose slots has this id (case-insensitive).
    pub fn by_slot(&self, slot_id: &str) -> Option<&ExistingEntry> {
        self.all().find(|e| {
            e.mechanism
                .slots()
                .iter()
                .any(|s| s.id().eq_ignore_ascii_case(slot_id))
        })
    }
}

pub struct ScanOptions<'a> {
    /// Key of this user's snapshots in the store.
    pub user: &'a str,
    /// Programs under this folder are Windows' own and never listed.
    pub windows_dir: PathBuf,
    pub is_windows_11: bool,
    pub terminal_installed: bool,
    /// Resolves an `@file,-id` label (`fileinfo::resolve_indirect`).
    pub indirect: &'a dyn Fn(&str) -> Option<String>,
    /// A program's product name (`fileinfo::product_name`).
    pub product_name: &'a dyn Fn(&Path) -> Option<String>,
}

/// Lists the existing entries and their state. Read-only.
pub fn scan(target: &MenuTarget, store: &UserBackups, opts: &ScanOptions) -> Scan {
    let mut others = scan_verbs(target, store, opts);
    others.extend(scan_handlers(target, store, opts));
    others.sort_by(|a, b| (a.label.to_lowercase(), &a.id).cmp(&(b.label.to_lowercase(), &b.id)));
    Scan {
        windows: scan_catalog(target, store, opts),
        others,
    }
}

/// Verbs Explorer or every program understands: never listed.
const CANONICAL_VERBS: [&str; 10] = [
    "open",
    "opennew",
    "explore",
    "find",
    "print",
    "printto",
    "runas",
    "runasuser",
    "openas",
    "properties",
];

const PROGRAMMATIC_ACCESS_ONLY: &str = "ProgrammaticAccessOnly";
const LEGACY_DISABLE: &str = "LegacyDisable";

// ---- Registry reading ------------------------------------------------------

/// `rel` on one side: `Ok(None)` when it does not exist.
fn open(t: &RegTarget, rel: &str) -> Result<Option<RegKey>, String> {
    match t.root.open_subkey_with_flags(t.path(rel), KEY_READ) {
        Ok(key) => Ok(Some(key)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot read {rel}: {e}")),
    }
}

fn has_value(key: &RegKey, name: &str) -> bool {
    key.get_raw_value(name).is_ok()
}

fn text_of(key: &RegKey, name: &str) -> Option<String> {
    key.get_value::<String, _>(name).ok()
}

/// The user and machine copies of one key, read the way Explorer merges
/// them: a value set in the user copy wins.
struct Merged {
    user: Option<RegKey>,
    machine: Option<RegKey>,
    /// The first side that exists but cannot be read.
    error: Option<String>,
}

impl Merged {
    fn open(target: &MenuTarget, rel: &str) -> Merged {
        let user = open(&target.user, rel);
        let machine = open(&target.machine, rel);
        let error = user.as_ref().err().or(machine.as_ref().err()).cloned();
        Merged {
            user: user.ok().flatten(),
            machine: machine.ok().flatten(),
            error,
        }
    }

    fn exists(&self) -> bool {
        self.user.is_some() || self.machine.is_some() || self.error.is_some()
    }

    fn text(&self, name: &str) -> Option<String> {
        match &self.user {
            Some(key) if has_value(key, name) => text_of(key, name),
            _ => self.machine.as_ref().and_then(|key| text_of(key, name)),
        }
    }

    fn has(&self, name: &str) -> bool {
        [&self.user, &self.machine]
            .into_iter()
            .flatten()
            .any(|key| has_value(key, name))
    }
}

/// Subkey names of `rel` on both sides, once each (case-insensitive), with
/// the machine spelling first.
fn subkey_names(target: &MenuTarget, rel: &str) -> Vec<String> {
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for side in [&target.machine, &target.user] {
        if let Ok(Some(key)) = open(side, rel) {
            for name in key.enum_keys().flatten() {
                names.entry(name.to_lowercase()).or_insert(name);
            }
        }
    }
    names.into_values().collect()
}

/// The text of a REG_SZ / REG_EXPAND_SZ value in a key snapshot.
fn snapshot_text(snapshot: &RegistryKeySnapshot, name: &str) -> Option<String> {
    let (kind, bytes) = snapshot.values.get(name)?;
    if *kind != REG_SZ as u32 && *kind != REG_EXPAND_SZ as u32 {
        return None;
    }
    Some(super::fileinfo::decode_utf16le(bytes))
}

// ---- Labels ----------------------------------------------------------------

/// Menu text as shown: `&&` is a literal `&`, a single `&` marks the
/// accelerator and is not displayed.
fn strip_accelerators(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
        } else if chars.peek() == Some(&'&') {
            chars.next();
            out.push('&');
        }
    }
    out
}

/// `MUIVerb`, else the default value, else the verb name; an indirect string
/// is resolved (kept raw when it cannot be).
fn verb_label(
    muiverb: Option<String>,
    default: Option<String>,
    verb: &str,
    indirect: &dyn Fn(&str) -> Option<String>,
) -> String {
    let Some(raw) = [muiverb, default]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
    else {
        return verb.to_string();
    };
    let raw = raw.trim();
    let text = match raw.starts_with('@') {
        true => indirect(raw).unwrap_or_else(|| raw.to_string()),
        false => raw.to_string(),
    };
    let label = strip_accelerators(&text).trim().to_string();
    if label.is_empty() {
        verb.to_string()
    } else {
        label
    }
}

// ---- Origin files ----------------------------------------------------------

/// `%NAME%` replaced by the variable's value; unknown names stay as written.
fn expand_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) if !name.is_empty() => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            // Keep the closing `%`: it may open the next variable.
            _ => {
                out.push('%');
                out.push_str(name);
                rest = &after[end..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The program of a command line: the first quoted token, else the text up
/// to the first `.exe`, else the first word; variables expanded.
fn command_program(command: &str) -> Option<String> {
    let command = command.trim();
    let program = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next().unwrap_or_default()
    } else if let Some(end) = command.to_ascii_lowercase().find(".exe") {
        &command[..end + 4]
    } else {
        command.split_whitespace().next().unwrap_or_default()
    };
    let program = expand_env(program.trim());
    (!program.is_empty()).then_some(program)
}

/// A bare file name (`rundll32.exe`) as Windows finds it: in `System32` or
/// the Windows folder itself. Anything else is returned unchanged.
fn locate(file: String, windows_dir: &Path) -> String {
    if file.contains(['\\', '/']) {
        return file;
    }
    let mut names = vec![file.clone()];
    if Path::new(&file).extension().is_none() {
        names.push(format!("{file}.exe"));
    }
    let dirs = [windows_dir.join("System32"), windows_dir.to_path_buf()];
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
        .map(|found| found.display().to_string())
        .unwrap_or(file)
}

/// `file` is `dir` or inside it (case-insensitive, whole components only).
fn under_dir(file: &str, dir: &Path) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let file = norm(file);
    let dir = norm(&dir.to_string_lossy());
    !dir.is_empty()
        && file
            .strip_prefix(&dir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

fn is_braced_guid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 38
        && bytes[0] == b'{'
        && bytes[37] == b'}'
        && bytes[1..37].iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// The DLL behind a CLSID (`InprocServer32`, user then machine), expanded.
fn clsid_server(target: &MenuTarget, clsid: &str) -> Option<String> {
    let key = Merged::open(
        target,
        &format!(r"Software\Classes\CLSID\{clsid}\InprocServer32"),
    );
    let server = expand_env(key.text("")?.trim().trim_matches('"'));
    (!server.is_empty()).then_some(server)
}

/// What a row shows as its origin: the product name, else the file name.
fn origin_name(file: &str, opts: &ScanOptions) -> String {
    let path = Path::new(file);
    (opts.product_name)(path).unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.to_string())
    })
}

// ---- State -----------------------------------------------------------------

/// Fills `has_snapshot`, `state` and `locked` of a row.
fn settle(
    mut entry: ExistingEntry,
    hidden: bool,
    machine_wide: bool,
    error: Option<String>,
    store: &UserBackups,
    user: &str,
) -> ExistingEntry {
    entry.has_snapshot = entry
        .mechanism
        .slots()
        .iter()
        .any(|slot| store.has_backup(user, &slot.id()));
    entry.state = match (hidden, entry.has_snapshot) {
        (false, _) => ExistingState::Shown,
        (true, true) => ExistingState::HiddenByWinTune,
        (true, false) => ExistingState::HiddenElsewhere,
    };
    entry.locked = error
        .map(Locked::Unreadable)
        .or(machine_wide.then_some(Locked::MachineWide));
    entry
}

/// A row with no state yet; [`settle`] fills it.
fn row(id: String, label: String, mechanism: Mechanism, places: BTreeSet<Place>) -> ExistingEntry {
    ExistingEntry {
        id,
        label,
        original_label: None,
        catalog: None,
        mechanism,
        places,
        origin: None,
        state: ExistingState::Shown,
        locked: None,
        has_snapshot: false,
    }
}

/// A CLSID row, hidden through `Blocked`.
fn settle_clsid(
    entry: ExistingEntry,
    clsid: &str,
    error: Option<String>,
    target: &MenuTarget,
    store: &UserBackups,
    user: &str,
) -> ExistingEntry {
    let blocked = Merged::open(target, BLOCKED_KEY);
    let in_user = blocked.user.as_ref().is_some_and(|k| has_value(k, clsid));
    let in_machine = blocked
        .machine
        .as_ref()
        .is_some_and(|k| has_value(k, clsid));
    let error = error.or(blocked.error);
    settle(entry, in_user || in_machine, in_machine, error, store, user)
}

// ---- Verbs -----------------------------------------------------------------

/// What one verb key says once both copies are merged.
struct VerbRead {
    label: String,
    original_label: String,
    file: Option<String>,
    /// The HKCU copy has `ProgrammaticAccessOnly`: how WinTune hides a verb.
    hidden_in_user: bool,
    /// Hidden by whoever registered it: `ProgrammaticAccessOnly` in the
    /// machine copy (in the user copy when there is no machine one), or
    /// `LegacyDisable` anywhere.
    hidden_by_installer: bool,
    error: Option<String>,
}

fn read_verb(
    target: &MenuTarget,
    store: &UserBackups,
    opts: &ScanOptions,
    v: &VerbRef,
) -> VerbRead {
    let rel = v.user_key();
    let key = Merged::open(target, &rel);
    let command = Merged::open(target, &format!(r"{rel}\command"));
    let label = verb_label(key.text("MUIVerb"), key.text(""), &v.verb, opts.indirect);

    // What Reset name returns to: the HKCU copy as WinTune first found it,
    // over the machine key.
    let snapshot = store
        .backups_of(opts.user)
        .and_then(|b| b.get(&rel))
        .map(|b| &b.snapshot);
    let (saved_verb, saved_default) = match snapshot {
        Some(Snapshot::Key(k)) if k.exists => (snapshot_text(k, "MUIVerb"), snapshot_text(k, "")),
        _ => (None, None),
    };
    // A user-only verb that WinTune never touched is its own original.
    let base = match (snapshot, &key.machine) {
        (None, None) => &key.user,
        _ => &key.machine,
    };
    let base_text = |name: &str| base.as_ref().and_then(|k| text_of(k, name));
    let original_label = verb_label(
        saved_verb.or_else(|| base_text("MUIVerb")),
        saved_default.or_else(|| base_text("")),
        &v.verb,
        opts.indirect,
    );

    let delegate = || {
        command
            .text("DelegateExecute")
            .map(|c| c.trim().to_string())
            .filter(|c| is_braced_guid(c))
            .and_then(|clsid| clsid_server(target, &clsid))
    };
    let file = command
        .text("")
        .and_then(|c| command_program(&c))
        .or_else(delegate)
        .map(|f| locate(f, &opts.windows_dir));

    let user_has = |name| key.user.as_ref().is_some_and(|k| has_value(k, name));
    let hidden_in_user = user_has(PROGRAMMATIC_ACCESS_ONLY);
    let original_pao = match &key.machine {
        Some(machine) => has_value(machine, PROGRAMMATIC_ACCESS_ONLY),
        None => hidden_in_user,
    };
    VerbRead {
        label,
        original_label,
        file,
        hidden_in_user,
        hidden_by_installer: original_pao || key.has(LEGACY_DISABLE),
        error: key.error.or(command.error),
    }
}

/// Verbs the catalog lists as Windows entries.
fn catalog_covers_verb(class: &str, verb: &str) -> bool {
    catalog().iter().any(|item| match item.mechanism {
        CatalogMechanism::Verbs(list) => list
            .iter()
            .any(|(c, v)| c.eq_ignore_ascii_case(class) && v.eq_ignore_ascii_case(verb)),
        CatalogMechanism::Clsid(_) => false,
    })
}

fn skipped_verb_name(verb: &str) -> bool {
    let lower = verb.to_lowercase();
    lower.starts_with(&KEY_PREFIX.to_lowercase()) || CANONICAL_VERBS.contains(&lower.as_str())
}

/// One verb row while the classes are walked.
struct VerbGroup {
    entry: ExistingEntry,
    refs: Vec<VerbRef>,
    all_hidden: bool,
    error: Option<String>,
}

fn scan_verbs(target: &MenuTarget, store: &UserBackups, opts: &ScanOptions) -> Vec<ExistingEntry> {
    let mut groups: BTreeMap<String, VerbGroup> = BTreeMap::new();
    for class in SCAN_CLASSES {
        let shell = format!(r"Software\Classes\{class}\shell");
        for verb in subkey_names(target, &shell) {
            if skipped_verb_name(&verb) || catalog_covers_verb(class, &verb) {
                continue;
            }
            let v = VerbRef {
                class: class.to_string(),
                verb,
            };
            let read = read_verb(target, store, opts, &v);
            if read
                .file
                .as_deref()
                .is_some_and(|f| under_dir(f, &opts.windows_dir))
            {
                continue;
            }
            if read.hidden_by_installer && !store.has_backup(opts.user, &v.user_key()) {
                continue;
            }
            let file = read.file.clone().unwrap_or_default();
            let id = format!("verb:{}|{}", v.verb.to_lowercase(), file.to_lowercase());
            let group = groups.entry(id.clone()).or_insert_with(|| {
                let mut entry = row(
                    id,
                    read.label.clone(),
                    Mechanism::Verbs(Vec::new()),
                    BTreeSet::new(),
                );
                entry.original_label = Some(read.original_label.clone());
                entry.origin = read.file.as_deref().map(|f| origin_name(f, opts));
                VerbGroup {
                    entry,
                    refs: Vec::new(),
                    all_hidden: true,
                    error: None,
                }
            });
            group.entry.places.extend(class_places(class));
            group.all_hidden &= read.hidden_in_user;
            group.error = group.error.take().or(read.error);
            group.refs.push(v);
        }
    }
    groups
        .into_values()
        .map(|mut g| {
            g.entry.mechanism = Mechanism::Verbs(g.refs);
            settle(g.entry, g.all_hidden, false, g.error, store, opts.user)
        })
        .collect()
}

// ---- Handlers --------------------------------------------------------------

/// One handler row while the classes are walked.
struct HandlerGroup {
    /// The first handler key's name, trimmed.
    name: String,
    places: BTreeSet<Place>,
    error: Option<String>,
}

fn scan_handlers(
    target: &MenuTarget,
    store: &UserBackups,
    opts: &ScanOptions,
) -> Vec<ExistingEntry> {
    let mut groups: BTreeMap<String, HandlerGroup> = BTreeMap::new();
    for class in SCAN_CLASSES {
        let handlers = format!(r"Software\Classes\{class}\shellex\ContextMenuHandlers");
        for name in subkey_names(target, &handlers) {
            let key = Merged::open(target, &format!(r"{handlers}\{name}"));
            let from_value = key.text("").map(|t| t.trim().to_string());
            let Some(clsid) = from_value
                .filter(|t| is_braced_guid(t))
                .or_else(|| Some(name.trim().to_string()).filter(|t| is_braced_guid(t)))
                .map(|c| c.to_uppercase())
            else {
                continue;
            };
            if catalog_clsid(&clsid).is_some() {
                continue;
            }
            let group = groups.entry(clsid).or_insert_with(|| HandlerGroup {
                name: name.trim().to_string(),
                places: BTreeSet::new(),
                error: None,
            });
            group.places.extend(class_places(class));
            group.error = group.error.take().or(key.error);
        }
    }

    let mut rows = Vec::new();
    for (clsid, group) in groups {
        let file = clsid_server(target, &clsid).map(|f| locate(f, &opts.windows_dir));
        if file
            .as_deref()
            .is_some_and(|f| under_dir(f, &opts.windows_dir))
        {
            continue;
        }
        let class_key = Merged::open(target, &format!(r"Software\Classes\CLSID\{clsid}"));
        let label = class_key
            .text("")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or(group.name);
        let mechanism = Mechanism::Clsid(clsid.clone());
        let mut entry = row(format!("clsid:{clsid}"), label, mechanism, group.places);
        entry.origin = file.as_deref().map(|f| origin_name(f, opts));
        let error = group.error.or(class_key.error);
        rows.push(settle_clsid(entry, &clsid, error, target, store, opts.user));
    }
    rows
}

// ---- Catalog ---------------------------------------------------------------

fn os_fits(item: &CatalogItem, opts: &ScanOptions) -> bool {
    match item.os {
        OsRequirement::Any => true,
        OsRequirement::Windows11 => opts.is_windows_11,
    }
}

fn scan_catalog(
    target: &MenuTarget,
    store: &UserBackups,
    opts: &ScanOptions,
) -> Vec<ExistingEntry> {
    let mut rows = Vec::new();
    for item in catalog() {
        if !os_fits(item, opts) {
            continue;
        }
        // Verbs: only the keys that exist, in either root.
        let refs: Vec<VerbRef> = match item.mechanism {
            CatalogMechanism::Verbs(list) => list
                .iter()
                .map(|(class, verb)| VerbRef {
                    class: class.to_string(),
                    verb: verb.to_string(),
                })
                .filter(|v| Merged::open(target, &v.user_key()).exists())
                .collect(),
            CatalogMechanism::Clsid(_) => Vec::new(),
        };
        let present = match (item.presence, item.mechanism) {
            (Presence::TerminalInstalled, _) => opts.terminal_installed,
            (Presence::ClsidRegistered, CatalogMechanism::Clsid(clsid)) => {
                Merged::open(target, &format!(r"Software\Classes\CLSID\{clsid}")).exists()
            }
            (Presence::ClsidRegistered, CatalogMechanism::Verbs(_)) => false,
            (Presence::VerbExists, _) => !refs.is_empty(),
        };
        if !present {
            continue;
        }
        let places = item.places.iter().copied().collect();
        let (id, label) = (format!("win:{}", item.id), item.id.to_string());
        let mut entry = match item.mechanism {
            CatalogMechanism::Clsid(clsid) => {
                let entry = row(id, label, Mechanism::Clsid(clsid.to_string()), places);
                settle_clsid(entry, clsid, None, target, store, opts.user)
            }
            CatalogMechanism::Verbs(_) => {
                let copies: Vec<_> = refs
                    .iter()
                    .map(|v| open(&target.user, &v.user_key()))
                    .collect();
                let error = copies.iter().find_map(|c| c.as_ref().err().cloned());
                let hidden = !copies.is_empty()
                    && copies.iter().all(
                        |c| matches!(c, Ok(Some(k)) if has_value(k, PROGRAMMATIC_ACCESS_ONLY)),
                    );
                let entry = row(id, label, Mechanism::Verbs(refs), places);
                settle(entry, hidden, false, error, store, opts.user)
            }
        };
        entry.catalog = Some(item.id);
        rows.push(entry);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regsnap::test_support::Sandbox;

    const APP_GUID: &str = "{23170F69-40C1-278A-1000-000100020000}";

    struct Env {
        sb: Sandbox,
        dir: PathBuf,
    }

    impl Env {
        fn new() -> Self {
            let sb = Sandbox::new();
            let dir = std::env::temp_dir().join(format!(
                "wintune-scan-{}",
                sb.path.rsplit('\\').next().unwrap()
            ));
            std::fs::create_dir_all(dir.join(r"Windows\System32")).unwrap();
            Self { sb, dir }
        }

        fn target(&self) -> MenuTarget {
            let side = |name: &str| RegTarget {
                root: RegKey::predef(HKEY_CURRENT_USER),
                root_name: "HKEY_CURRENT_USER".into(),
                base: format!("{}\\{}", self.sb.path, name),
            };
            MenuTarget {
                user: side("HKCU"),
                machine: side("HKLM"),
            }
        }

        fn windows(&self) -> PathBuf {
            self.dir.join("Windows")
        }

        /// A program path under the temp folder (never created).
        fn file(&self, rel: &str) -> String {
            self.dir.join(rel).display().to_string()
        }

        fn set(&self, side: &str, rel: &str, name: &str, value: &str) {
            let path = format!("{}\\{}\\{}", self.sb.path, side, rel);
            let (k, _) = self.sb.root.create_subkey(path).unwrap();
            k.set_value(name, &value).unwrap();
        }

        fn machine(&self, rel: &str, name: &str, value: &str) {
            self.set("HKLM", rel, name, value);
        }

        fn user(&self, rel: &str, name: &str, value: &str) {
            self.set("HKCU", rel, name, value);
        }

        /// A machine verb on `class` running `command`.
        fn verb(&self, class: &str, verb: &str, label: &str, command: &str) {
            let key = format!(r"Software\Classes\{class}\shell\{verb}");
            if !label.is_empty() {
                self.machine(&key, "MUIVerb", label);
            }
            self.machine(&format!(r"{key}\command"), "", command);
        }

        /// A machine handler on `class` whose server is `server`.
        fn handler(&self, class: &str, name: &str, clsid: &str, server: &str) {
            let key = format!(r"Software\Classes\{class}\shellex\ContextMenuHandlers\{name}");
            self.machine(&key, "", clsid);
            let server_key = format!(r"Software\Classes\CLSID\{clsid}\InprocServer32");
            self.machine(&server_key, "", server);
        }

        fn opts(&self) -> ScanOptions<'static> {
            ScanOptions {
                user: "A",
                windows_dir: self.windows(),
                is_windows_11: true,
                terminal_installed: false,
                indirect: &|_| None,
                product_name: &|_| None,
            }
        }

        fn scan(&self, store: &UserBackups) -> Scan {
            scan(&self.target(), store, &self.opts())
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn places(list: &[Place]) -> BTreeSet<Place> {
        list.iter().copied().collect()
    }

    fn key_snapshot(values: &[(&str, &str)]) -> Snapshot {
        let mut snapshot = RegistryKeySnapshot {
            exists: true,
            ..Default::default()
        };
        for (name, text) in values {
            let bytes = text
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            snapshot
                .values
                .insert(name.to_string(), (REG_SZ as u32, bytes));
        }
        Snapshot::Key(snapshot)
    }

    #[test]
    fn a_third_party_verb_is_listed_once_with_its_places_and_origin() {
        let env = Env::new();
        let exe = env.file(r"App\app.exe");
        env.verb(
            "Directory",
            "AppVerb",
            "Open with &App",
            &format!("\"{exe}\" \"%1\""),
        );
        env.verb(
            r"Directory\Background",
            "AppVerb",
            "Open with &App",
            &format!("\"{exe}\" \"%V\""),
        );
        let scan = env.scan(&UserBackups::default());
        assert!(scan.windows.is_empty(), "{:?}", scan.windows);
        assert_eq!(scan.others.len(), 1, "{:?}", scan.others);
        let row = &scan.others[0];
        assert_eq!(row.id, format!("verb:appverb|{}", exe.to_lowercase()));
        assert_eq!(row.label, "Open with App");
        assert_eq!(row.places, places(&[Place::Folder, Place::Background]));
        assert_eq!(row.origin.as_deref(), Some("app"));
        assert_eq!(row.state, ExistingState::Shown);
        assert_eq!(row.locked, None);
        assert!(row.mechanism.renamable());
        assert_eq!(row.catalog, None);
        assert_eq!(row.original_label.as_deref(), Some("Open with App"));
        let key = r"Software\Classes\Directory\shell\AppVerb";
        assert_eq!(scan.by_slot(key).map(|e| &e.id), Some(&row.id));
        assert_eq!(scan.find(&row.id), Some(row));
    }

    #[test]
    fn a_handler_is_grouped_by_clsid_and_named_from_its_clsid() {
        let env = Env::new();
        let dll = env.file(r"7-Zip\7-zip.dll");
        env.handler("*", " 7-Zip", APP_GUID, &dll);
        env.handler("Directory", " 7-Zip", APP_GUID, &dll);
        env.machine(
            &format!(r"Software\Classes\CLSID\{APP_GUID}"),
            "",
            "7-Zip Shell Extension",
        );
        // No CLSID name: the handler key's name, trimmed.
        let other = "{11111111-2222-3333-4444-555555555555}";
        env.handler("Drive", "  Other Tool ", other, &env.file("other.dll"));
        // The CLSID can be the key name itself, in any case.
        let lower = "{aaaaaaaa-2222-3333-4444-555555555555}";
        let key = format!(r"Software\Classes\Folder\shellex\ContextMenuHandlers\{lower}");
        env.machine(&key, "", "");
        let server = format!(r"Software\Classes\CLSID\{lower}\InprocServer32");
        env.machine(&server, "", &env.file("named.dll"));
        // No parseable CLSID: skipped.
        let bad = r"Software\Classes\*\shellex\ContextMenuHandlers\Broken";
        env.machine(bad, "", "not a guid");

        let scan = env.scan(&UserBackups::default());
        assert_eq!(scan.others.len(), 3, "{:?}", scan.others);
        let row = scan.find(&format!("clsid:{APP_GUID}")).unwrap();
        assert_eq!(row.label, "7-Zip Shell Extension");
        assert_eq!(row.places, places(&[Place::Files, Place::Folder]));
        assert_eq!(row.origin.as_deref(), Some("7-zip"));
        assert_eq!(row.mechanism, Mechanism::Clsid(APP_GUID.into()));
        assert!(!row.mechanism.renamable());
        assert_eq!(row.original_label, None);
        let row = scan.find(&format!("clsid:{other}")).unwrap();
        assert_eq!(row.label, "Other Tool");
        assert_eq!(row.places, places(&[Place::Drive]));
        let upper = lower.to_uppercase();
        let row = scan.find(&format!("clsid:{upper}")).unwrap();
        assert_eq!(row.places, places(&[Place::Folder, Place::Drive]));
        let slot = format!("{BLOCKED_KEY}\\\\{upper}");
        assert_eq!(scan.by_slot(&slot).map(|e| &e.id), Some(&row.id));
    }

    #[test]
    fn windows_own_entries_are_not_listed() {
        let env = Env::new();
        let windows = env.windows().display().to_string();
        let guid = "{BBBBBBBB-2222-3333-4444-555555555555}";
        let shell32 = format!(r"{windows}\System32\shell32.dll");
        env.handler("*", "Sys", guid, &shell32);
        // Unquoted, other case: up to `.exe`.
        let exe = format!(r"{}\SYSTEM32\tool.exe %1", windows.to_uppercase());
        env.verb("Directory", "SysVerb", "Sys", &exe);
        // A bare name found in System32.
        std::fs::write(env.windows().join(r"System32\bare.exe"), b"x").unwrap();
        env.verb("Drive", "BareVerb", "Bare", "bare.exe \"%1\"");
        // A DelegateExecute server under the Windows folder.
        let delegate = "{CCCCCCCC-2222-3333-4444-555555555555}";
        let key = r"Software\Classes\Folder\shell\Delegated\command";
        env.machine(key, "DelegateExecute", delegate);
        let server = format!(r"Software\Classes\CLSID\{delegate}\InprocServer32");
        env.machine(&server, "", &shell32);
        let scan = env.scan(&UserBackups::default());
        assert!(scan.others.is_empty(), "{:?}", scan.others);

        // A sibling folder whose name starts the same is not Windows.
        let sibling = format!(r"{windows}2\x.exe");
        env.verb("Directory", "Sibling", "Sibling", &format!("\"{sibling}\""));
        let scan = env.scan(&UserBackups::default());
        assert_eq!(scan.others.len(), 1, "{:?}", scan.others);
        assert_eq!(scan.others[0].label, "Sibling");
    }

    #[test]
    fn canonical_wintune_and_installer_hidden_verbs_are_skipped() {
        let env = Env::new();
        let exe = format!("\"{}\" \"%1\"", env.file("app.exe"));
        env.verb("Directory", "open", "Open", &exe);
        env.verb("Directory", "Properties", "Props", &exe);
        env.user(
            r"Software\Classes\Directory\shell\WinTune.e1",
            "MUIVerb",
            "Mine",
        );
        env.verb("Directory", "Quiet", "Quiet", &exe);
        env.machine(
            r"Software\Classes\Directory\shell\Quiet",
            "ProgrammaticAccessOnly",
            "",
        );
        env.verb("Drive", "Legacy", "Legacy", &exe);
        env.machine(r"Software\Classes\Drive\shell\Legacy", "LegacyDisable", "");
        let mut store = UserBackups::default();
        assert!(env.scan(&store).others.is_empty());

        // Once WinTune holds a snapshot of it, an installer-hidden verb is listed.
        let quiet = r"Software\Classes\Directory\shell\Quiet";
        store.record_first("A", quiet, "x", key_snapshot(&[]));
        let scan = env.scan(&store);
        assert_eq!(scan.others.len(), 1, "{:?}", scan.others);
        assert_eq!(scan.others[0].label, "Quiet");
        assert!(scan.others[0].has_snapshot);
    }

    #[test]
    fn catalog_entries_go_to_windows_only_when_present() {
        let env = Env::new();
        let previous = "{596AB062-B4D2-4215-9F74-E9109B0A8153}";
        env.machine(
            &format!(r"Software\Classes\CLSID\{previous}"),
            "",
            "Previous",
        );
        let key = format!(
            r"Software\Classes\AllFilesystemObjects\shellex\ContextMenuHandlers\{previous}"
        );
        env.machine(&key, "", "");
        let terminal = "{9F156763-7844-4DC4-B2B1-901F640F5155}";
        env.handler(
            r"Directory\Background",
            "WindowsTerminal",
            terminal,
            &env.file("terminal.dll"),
        );
        env.verb("*", "pintohomefile", "", "x");
        let mut opts = env.opts();
        opts.terminal_installed = false;
        opts.is_windows_11 = false;
        let scan = scan(&env.target(), &UserBackups::default(), &opts);
        let ids: Vec<&str> = scan.windows.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["win:previous_versions"]);
        assert!(scan.others.is_empty(), "{:?}", scan.others);
        let row = &scan.windows[0];
        assert_eq!(row.catalog, Some("previous_versions"));
        assert_eq!(row.label, "previous_versions");
        assert_eq!(row.mechanism, Mechanism::Clsid(previous.into()));
        assert_eq!(
            row.places,
            places(&[Place::Files, Place::Folder, Place::Drive])
        );
        assert_eq!(row.state, ExistingState::Shown);

        let mut opts = env.opts();
        opts.terminal_installed = true;
        let all = super::scan(&env.target(), &UserBackups::default(), &opts);
        let ids: Vec<&str> = all.windows.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "win:open_in_terminal",
                "win:previous_versions",
                "win:add_to_favorites"
            ]
        );
        let favorites = all.find("win:add_to_favorites").unwrap();
        let fav = VerbRef {
            class: "*".into(),
            verb: "pintohomefile".into(),
        };
        assert_eq!(favorites.mechanism, Mechanism::Verbs(vec![fav]));
        assert_eq!(favorites.state, ExistingState::Shown);

        // A catalog verb is hidden by its HKCU copy, like any verb.
        let copy = r"Software\Classes\*\shell\pintohomefile";
        env.user(copy, "ProgrammaticAccessOnly", "");
        let all = super::scan(&env.target(), &UserBackups::default(), &opts);
        let favorites = all.find("win:add_to_favorites").unwrap();
        assert_eq!(favorites.state, ExistingState::HiddenElsewhere);
    }

    #[test]
    fn command_lines_paths_and_guids_are_parsed() {
        assert_eq!(strip_accelerators("&Open && go"), "Open & go");
        let program = |c: &str| command_program(c);
        assert_eq!(
            program(r#""C:\A b\x.exe" "%1""#).as_deref(),
            Some(r"C:\A b\x.exe")
        );
        assert_eq!(
            program(r"C:\A b\X.EXE %1").as_deref(),
            Some(r"C:\A b\X.EXE")
        );
        assert_eq!(program("rundll32 x.dll,Y").as_deref(), Some("rundll32"));
        assert_eq!(program("  "), None);
        let root = std::env::var("SystemRoot").expect("SystemRoot is always set");
        assert_eq!(expand_env(r"%SystemRoot%\x"), format!(r"{root}\x"));
        assert_eq!(
            expand_env("%WINTUNE_NOT_A_VARIABLE%%SystemRoot%"),
            format!("%WINTUNE_NOT_A_VARIABLE%{root}")
        );
        assert_eq!(expand_env("100% sure"), "100% sure");
        let windows = Path::new(r"C:\Windows\");
        assert!(under_dir(r"c:\windows\System32\x.dll", windows));
        assert!(under_dir(r"C:/Windows/x.exe", windows));
        assert!(!under_dir(r"C:\Windows2\x.exe", windows));
        assert!(!under_dir(r"D:\C\Windows\x.exe", windows));
        assert!(is_braced_guid("{596ab062-B4D2-4215-9F74-E9109B0A8153}"));
        assert!(!is_braced_guid("596AB062-B4D2-4215-9F74-E9109B0A8153"));
        assert!(!is_braced_guid("{596AB062-B4D2-4215-9F74-E9109B0A815G}"));
    }

    #[test]
    fn hidden_states_follow_blocked_and_the_store() {
        let env = Env::new();
        let b = "{BBBBBBBB-0000-0000-0000-000000000001}";
        let c = "{CCCCCCCC-0000-0000-0000-000000000002}";
        env.handler("*", "B", b, &env.file("b.dll"));
        env.handler("*", "C", c, &env.file("c.dll"));
        env.user(BLOCKED_KEY, b, "");
        env.machine(BLOCKED_KEY, c, "");
        let mut store = UserBackups::default();

        let scan = env.scan(&store);
        let row_b = scan.find(&format!("clsid:{b}")).unwrap();
        assert_eq!(row_b.state, ExistingState::HiddenElsewhere);
        assert_eq!(row_b.locked, None);
        assert!(!row_b.has_snapshot);
        let row_c = scan.find(&format!("clsid:{c}")).unwrap();
        assert_eq!(row_c.state, ExistingState::HiddenElsewhere);
        assert_eq!(row_c.locked, Some(Locked::MachineWide));

        let slot = Slot::Value {
            path: BLOCKED_KEY.into(),
            name: b.into(),
        };
        let absent = Snapshot::Value(Default::default());
        store.record_first("A", &slot.id(), "x", absent.clone());
        let row_b = env
            .scan(&store)
            .find(&format!("clsid:{b}"))
            .cloned()
            .unwrap();
        assert_eq!(row_b.state, ExistingState::HiddenByWinTune);
        assert!(row_b.has_snapshot);
        // Another user's snapshot does not count.
        let mut other = UserBackups::default();
        other.record_first("B", &slot.id(), "x", absent);
        let row_b = env
            .scan(&other)
            .find(&format!("clsid:{b}"))
            .cloned()
            .unwrap();
        assert_eq!(row_b.state, ExistingState::HiddenElsewhere);
    }

    #[test]
    fn an_hkcu_copy_renames_and_hides_a_machine_verb() {
        let env = Env::new();
        let exe = env.file("app.exe");
        env.verb(
            "Directory",
            "AppVerb",
            "Open with App",
            &format!("\"{exe}\""),
        );
        let copy = r"Software\Classes\Directory\shell\AppVerb";
        env.user(copy, "MUIVerb", "Renamed");
        let mut store = UserBackups::default();

        let row = env.scan(&store).others[0].clone();
        assert_eq!(row.label, "Renamed");
        assert_eq!(row.original_label.as_deref(), Some("Open with App"));
        assert_eq!(row.state, ExistingState::Shown);

        env.user(copy, "ProgrammaticAccessOnly", "");
        let row = env.scan(&store).others[0].clone();
        assert_eq!(row.state, ExistingState::HiddenElsewhere);

        // The snapshot holds the HKCU copy's original name.
        store.record_first("A", copy, "x", key_snapshot(&[("MUIVerb", "Mine")]));
        let row = env.scan(&store).others[0].clone();
        assert_eq!(row.state, ExistingState::HiddenByWinTune);
        assert_eq!(row.original_label.as_deref(), Some("Mine"));
    }

    #[test]
    fn labels_resolve_indirect_strings_and_drop_accelerators() {
        let env = Env::new();
        let command = |name: &str| format!("\"{}\"", env.file(&format!("{name}.exe")));
        env.verb("Directory", "Ind", "@x.dll,-1", &command("ind"));
        env.verb(
            "Directory",
            "Unresolved",
            "@y.dll,-2",
            &command("unresolved"),
        );
        env.verb("Directory", "Amp", "", &command("amp"));
        env.machine(r"Software\Classes\Directory\shell\Amp", "", "A && B");
        env.verb("Directory", "Bare", "", &command("bare"));
        let mut opts = env.opts();
        opts.indirect = &|text| (text == "@x.dll,-1").then(|| "Resolved &Name".to_string());
        let scan = scan(&env.target(), &UserBackups::default(), &opts);
        let labels: BTreeSet<&str> = scan.others.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            BTreeSet::from(["Resolved Name", "@y.dll,-2", "A & B", "Bare"])
        );
    }

    #[test]
    fn origin_uses_the_product_name_and_expands_variables() {
        let env = Env::new();
        let root = std::env::var("SystemRoot").expect("SystemRoot is always set");
        env.verb(
            "Drive",
            "Env",
            "Env",
            r#""%SystemRoot%\..\Tools\env.exe" "%1""#,
        );
        let mut opts = env.opts();
        // Not under the temp Windows folder, so it is listed.
        opts.product_name = &|path| Some(path.display().to_string());
        let scan = scan(&env.target(), &UserBackups::default(), &opts);
        let expected = format!(r"{root}\..\Tools\env.exe");
        assert_eq!(scan.others[0].origin.as_deref(), Some(expected.as_str()));
        assert_eq!(
            scan.others[0].id,
            format!("verb:env|{}", expected.to_lowercase())
        );
    }
}
