//! Test fixture shared by the UI tests: an app wired to a temp folder and
//! the HKCU registry sandbox (never HKLM, HKU or %ProgramData%).

use super::{AppEnv, WinTuneApp};
use crate::associations::AssocTarget;
use crate::models::AppConfig;
use crate::regsnap::test_support::Sandbox;
use eframe::egui::{self, FullOutput, RawInput, ViewportCommand, ViewportEvent, ViewportId};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Temp folder (icons, config, backups) + HKCU registry sandbox. The app
/// built from it never touches HKLM, HKU or %ProgramData%.
pub(super) struct TempIcons {
    pub(super) dir: PathBuf,
    pub(super) sandbox: Sandbox,
    opened: Arc<Mutex<Vec<PathBuf>>>,
    /// How many times the app announced an environment change (never sent).
    broadcasts: Arc<AtomicUsize>,
}

pub(super) const TEST_HIVE: &str = "S-1-5-21-test";

impl TempIcons {
    pub(super) fn env(&self) -> AppEnv {
        let base = self.sandbox.path.clone();
        AppEnv {
            assoc_target: Arc::new(move || AssocTarget {
                machine_root: RegKey::predef(HKEY_CURRENT_USER),
                machine_root_name: "HKEY_CURRENT_USER".into(),
                machine_classes: format!("{}\\Machine\\Classes", base),
                users_root: RegKey::predef(HKEY_CURRENT_USER),
                users_root_name: "HKEY_CURRENT_USER".into(),
                user_hives: vec![format!("{}\\Users\\{}", base, TEST_HIVE)],
            }),
            backup_store_path: self.dir.join("backups.json"),
            reg_backup_dir: self.dir.join("reg-backups"),
            config_path: self.dir.join("config.json"),
            notify_shell: || {},
            tweak_target: {
                let base = format!("{}\\HKCU", self.sandbox.path);
                Arc::new(move || crate::tweaks::TweakTarget {
                    root: RegKey::predef(HKEY_CURRENT_USER),
                    root_name: "HKEY_CURRENT_USER".into(),
                    base: base.clone(),
                })
            },
            tweak_store_path: self.dir.join("tweaks-backups.json"),
            notify_settings: || {},
            is_windows_11: true,
            user_label: "TEST\\user".into(),
            history_path: self.dir.join("history.jsonl"),
            data_dir: self.dir.join("data"),
            open_folder: {
                let opened = self.opened.clone();
                Arc::new(move |path: &Path| opened.lock().unwrap().push(path.to_path_buf()))
            },
            menu_target: {
                let sandbox = self.sandbox.path.clone();
                Arc::new(move || {
                    let at = |hive: &str| crate::backups::RegTarget {
                        root: RegKey::predef(HKEY_CURRENT_USER),
                        root_name: "HKEY_CURRENT_USER".into(),
                        base: format!("{}\\{}", sandbox, hive),
                    };
                    crate::context_menu::MenuTarget {
                        user: at("HKCU"),
                        machine: at("HKLM"),
                    }
                })
            },
            menu_store_path: self.dir.join("context-menu-backups.json"),
            startup_target: {
                let sandbox = self.sandbox.path.clone();
                let (user_folder, common_folder) =
                    (self.startup_folder(false), self.startup_folder(true));
                Arc::new(move || {
                    let at = |hive: &str| crate::backups::RegTarget {
                        root: RegKey::predef(HKEY_CURRENT_USER),
                        root_name: "HKEY_CURRENT_USER".into(),
                        base: format!("{}\\{}", sandbox, hive),
                    };
                    crate::startup::StartupTarget {
                        user: at("HKCU"),
                        machine: at("HKLM"),
                        user_folder: Some(user_folder.clone()),
                        common_folder: Some(common_folder.clone()),
                    }
                })
            },
            startup_store_path: self.dir.join("startup-backups.json"),
            startup_switch_works: |s| s != crate::startup::Source::MachineRun32,
            env_target: {
                let sandbox = self.sandbox.path.clone();
                Arc::new(move || {
                    let at = |hive: &str| crate::backups::RegTarget {
                        root: RegKey::predef(HKEY_CURRENT_USER),
                        root_name: "HKEY_CURRENT_USER".into(),
                        base: format!("{}\\{}", sandbox, hive),
                    };
                    crate::environment::EnvTarget {
                        user: at("HKCU"),
                        machine: at("HKLM"),
                    }
                })
            },
            env_store_path: self.dir.join("environment-backups.json"),
            notify_environment: {
                let broadcasts = self.broadcasts.clone();
                Arc::new(move || {
                    broadcasts.fetch_add(1, Ordering::SeqCst);
                })
            },
            process_var: |_| None,
            // Never created: programs under it would count as Windows' own.
            windows_dir: self.dir.join("Windows"),
            resolve_indirect: |_| None,
            product_name: |_| None,
            programs: {
                let exe = |name: &str| Some(self.dir.join("programs").join(name));
                crate::context_menu::Programs {
                    vscode: exe("code.exe"),
                    terminal: exe("wt.exe"),
                    powershell: exe("powershell.exe"),
                    cmd: exe("cmd.exe"),
                }
            },
        }
    }

    pub(super) fn app(&self, ctx: &egui::Context) -> WinTuneApp {
        self.app_with(ctx, self.config())
    }

    pub(super) fn app_with(&self, ctx: &egui::Context, config: AppConfig) -> WinTuneApp {
        WinTuneApp::with_config(ctx, config, false, self.env())
    }

    /// Path of a key under the sandbox's machine `Classes`.
    pub(super) fn classes(&self, rel: &str) -> String {
        format!("{}\\Machine\\Classes\\{}", self.sandbox.path, rel)
    }

    pub(super) fn new(names: &[&str]) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wintune-ui-{}-{}", std::process::id(), nanos));
        fs::create_dir_all(dir.join("icons")).unwrap();
        fs::create_dir_all(dir.join("programs")).unwrap();
        fs::create_dir_all(dir.join("startup-user")).unwrap();
        fs::create_dir_all(dir.join("startup-common")).unwrap();
        for exe in ["code.exe", "wt.exe", "powershell.exe", "cmd.exe"] {
            fs::write(dir.join("programs").join(exe), b"x").unwrap();
        }
        for name in names {
            let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
            let img = ico::IconImage::from_rgba_data(32, 32, vec![200; 32 * 32 * 4]);
            icon.add_entry(ico::IconDirEntry::encode(&img).unwrap());
            let file = fs::File::create(dir.join("icons").join(format!("{}.ico", name))).unwrap();
            icon.write(file).unwrap();
        }
        let sandbox = Sandbox::new();
        // The test user is "signed in": its hive root exists.
        sandbox
            .root
            .create_subkey(sandbox.key(&format!("Users\\{}", TEST_HIVE)))
            .expect("create test user hive");
        Self {
            dir,
            sandbox,
            opened: Arc::default(),
            broadcasts: Arc::default(),
        }
    }

    /// Folders the app asked Explorer to open (nothing is really opened).
    pub(super) fn opened_folders(&self) -> Vec<PathBuf> {
        self.opened.lock().unwrap().clone()
    }

    pub(super) fn config(&self) -> AppConfig {
        let path = |p: &Path| p.to_string_lossy().into_owned();
        AppConfig {
            icon_source_folder: path(&self.dir.join("icons")),
            icon_destination_folder: path(&self.dir.join("installed")),
            pak_file_path: path(&self.dir.join("gallery.pak")),
            ..AppConfig::default()
        }
    }
}

impl Drop for TempIcons {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

pub(super) fn frame(app: &mut WinTuneApp, ctx: &egui::Context, input: RawInput) -> FullOutput {
    ctx.run(input, |ctx| app.ui(ctx))
}

/// A store error, for tests that only need one to be set.
pub(super) fn corrupt(file: &str, detail: &str) -> crate::persist::LoadError {
    crate::persist::LoadError::Corrupt {
        path: file.into(),
        detail: detail.into(),
    }
}

pub(super) fn close_request() -> RawInput {
    let mut input = RawInput::default();
    input
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    input
}

pub(super) fn commands(out: &FullOutput) -> Vec<ViewportCommand> {
    out.viewport_output
        .get(&ViewportId::ROOT)
        .map(|v| v.commands.clone())
        .unwrap_or_default()
}

/// A valid "open in editor" rule using the fixture icon `foo`.
pub(super) fn rule(ext: &str) -> crate::models::ExtensionConfig {
    let stem = ext.trim_start_matches('.').to_uppercase();
    crate::models::ExtensionConfig {
        enabled: true,
        extension: ext.to_string(),
        prog_id: format!("Custom{}File", stem),
        description: format!("{} File", stem),
        icon_name: "foo".to_string(),
        reset_existing: false,
        reset_user_choice: false,
        mode: crate::models::DoubleClickMode::OpenInEditor,
        launcher_command: None,
        elevated_command: None,
        perceived_type: "text".to_string(),
        content_type: "text/plain".to_string(),
        preserve_existing_properties: true,
        extra_verbs: Vec::new(),
    }
}

impl TempIcons {
    /// App whose rules are exactly `exts` and whose editor is set, ready to
    /// apply into the sandbox.
    pub(super) fn app_with_rules(&self, ctx: &egui::Context, exts: &[&str]) -> WinTuneApp {
        let mut config = self.config();
        config.code_executable = r"C:\Editor\editor.exe".to_string();
        config.extensions = exts.iter().map(|e| rule(e)).collect();
        self.app_with(ctx, config)
    }

    /// Path of a key in the sandbox's per-user (HKCU) area.
    pub(super) fn hkcu(&self, rel: &str) -> String {
        format!("{}\\HKCU\\{}", self.sandbox.path, rel)
    }

    pub(super) fn key_exists(&self, path: &str) -> bool {
        self.sandbox.root.open_subkey(path).is_ok()
    }

    /// The sandbox's machine side (`{sandbox}\HKLM`): the app only reads it,
    /// tests seed HKLM-like keys there.
    pub(super) fn machine_key(&self) -> RegKey {
        let (key, _) = self
            .sandbox
            .root
            .create_subkey(format!("{}\\HKLM", self.sandbox.path))
            .expect("create sandbox HKLM");
        key
    }

    /// A third-party machine verb on `class` (e.g. `Directory`), labelled
    /// `label`, run by a program outside the Windows folder.
    pub(super) fn seed_machine_verb(&self, class: &str, verb: &str, label: &str) {
        let key = format!(r"Software\Classes\{class}\shell\{verb}");
        let (k, _) = self.machine_key().create_subkey(&key).unwrap();
        k.set_value("MUIVerb", &label).unwrap();
        let (command, _) = k.create_subkey("command").unwrap();
        let exe = format!(r#""C:\Apps\{verb}.exe" "%1""#);
        command.set_value("", &exe).unwrap();
    }

    /// Registers a CLSID on the machine side (enough for a catalog entry).
    pub(super) fn seed_machine_clsid(&self, clsid: &str) {
        let key = format!(r"Software\Classes\CLSID\{clsid}");
        let (k, _) = self.machine_key().create_subkey(&key).unwrap();
        k.set_value("", &"Seeded").unwrap();
    }

    /// The Startup folder of this account or (`common`) of every account.
    pub(super) fn startup_folder(&self, common: bool) -> PathBuf {
        self.dir.join(if common {
            "startup-common"
        } else {
            "startup-user"
        })
    }

    /// A `REG_SZ` Run value `name` under `RUN_KEY`, on the sandbox's machine
    /// side (`machine`) or the user side.
    pub(super) fn seed_run(&self, machine: bool, name: &str, command: &str) {
        self.seed_run_in(machine, crate::startup::RUN_KEY, name, command);
    }

    /// Like `seed_run`, under any Run key (`RUN_KEY` or `RUN32_KEY`).
    pub(super) fn seed_run_in(&self, machine: bool, key: &str, name: &str, command: &str) {
        let (k, _) = if machine {
            self.machine_key().create_subkey(key)
        } else {
            self.sandbox.root.create_subkey(self.hkcu(key))
        }
        .expect("create Run key");
        k.set_value(name, &command).unwrap();
    }

    /// Data of this account's `StartupApproved\Run` value `name`.
    pub(super) fn user_switch(&self, name: &str) -> Option<Vec<u8>> {
        let key = self
            .sandbox
            .root
            .open_subkey(self.hkcu(&crate::startup::Approved::Run.key()))
            .ok()?;
        key.get_raw_value(name).ok().map(|v| v.bytes)
    }

    /// A text variable under the sandbox's `Environment` key, on the machine
    /// side (`machine`, under `SYSTEM_ENV_KEY`) or the user side. `kind` is
    /// the registry type: 1 (`REG_SZ`) or 2 (`REG_EXPAND_SZ`).
    pub(super) fn seed_env(&self, machine: bool, name: &str, kind: u32, value: &str) {
        let vtype = match kind {
            1 => winreg::enums::RegType::REG_SZ,
            2 => winreg::enums::RegType::REG_EXPAND_SZ,
            other => panic!("seed_env: kind {other} is not text"),
        };
        let (k, _) = if machine {
            self.machine_key()
                .create_subkey(crate::environment::SYSTEM_ENV_KEY)
        } else {
            self.sandbox
                .root
                .create_subkey(self.hkcu(crate::environment::USER_ENV_KEY))
        }
        .expect("create Environment key");
        let bytes = crate::environment::encode_text(value);
        k.set_raw_value(name, &winreg::RegValue { bytes, vtype })
            .unwrap();
    }

    /// A user variable of any registry type.
    #[allow(dead_code)] // The page (Task 6) uses it; remove then.
    pub(super) fn seed_env_raw(&self, name: &str, value: winreg::RegValue) {
        let (k, _) = self
            .sandbox
            .root
            .create_subkey(self.hkcu(crate::environment::USER_ENV_KEY))
            .expect("create Environment key");
        k.set_raw_value(name, &value).unwrap();
    }

    /// Type and decoded text of this account's variable `name`.
    pub(super) fn user_env(&self, name: &str) -> Option<(u32, String)> {
        let key = self
            .sandbox
            .root
            .open_subkey(self.hkcu(crate::environment::USER_ENV_KEY))
            .ok()?;
        let value = key.get_raw_value(name).ok()?;
        Some((
            value.vtype as u32,
            crate::context_menu::fileinfo::decode_utf16le(&value.bytes),
        ))
    }

    /// Environment change announcements the app made so far.
    pub(super) fn broadcast_count(&self) -> usize {
        self.broadcasts.load(Ordering::SeqCst)
    }
}

/// Runs frames until no environment scan is pending or running.
pub(super) fn run_until_env_scanned(app: &mut WinTuneApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while (app.env_scan_needed || app.env_scan_job.is_some()) && Instant::now() < deadline {
        frame(app, ctx, RawInput::default());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !app.env_scan_needed && app.env_scan_job.is_none(),
        "the environment scan never finished"
    );
}

/// Runs frames until no startup scan is pending or running.
pub(super) fn run_until_startup_scanned(app: &mut WinTuneApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while (app.startup_scan_needed || app.startup_scan_job.is_some()) && Instant::now() < deadline {
        frame(app, ctx, RawInput::default());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !app.startup_scan_needed && app.startup_scan_job.is_none(),
        "the startup scan never finished"
    );
}

/// Runs frames until no scan of the right-click menu is pending or running.
pub(super) fn run_until_scanned(app: &mut WinTuneApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while (app.menu_scan_needed || app.menu_scan_job.is_some()) && Instant::now() < deadline {
        frame(app, ctx, RawInput::default());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !app.menu_scan_needed && app.menu_scan_job.is_none(),
        "the scan never finished"
    );
}

/// Asks for a scan and waits for its result.
pub(super) fn scan_now(app: &mut WinTuneApp, ctx: &egui::Context) {
    app.request_menu_scan();
    run_until_scanned(app, ctx);
}

pub(super) fn run_until_idle(app: &mut WinTuneApp, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.is_busy() && Instant::now() < deadline {
        frame(app, ctx, RawInput::default());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.is_busy(), "job did not finish");
}

/// Every text drawn in a frame, in paint order.
pub(super) fn drawn_texts(out: &FullOutput) -> Vec<String> {
    fn walk(shape: &egui::epaint::Shape, texts: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(t) => texts.push(t.galley.text().to_string()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, texts)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut texts);
    }
    texts
}

/// Center of the first text drawn as exactly `text`, to aim a click at it.
pub(super) fn text_center(out: &FullOutput, text: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::epaint::Shape, text: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| find(&c.shape, text))
}

/// Centers of every text drawn as exactly `text`, in paint order.
pub(super) fn text_centers(out: &FullOutput, text: &str) -> Vec<egui::Pos2> {
    fn walk(shape: &egui::epaint::Shape, text: &str, found: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                found.push(t.pos + t.galley.rect.center().to_vec2());
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, text, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    out.shapes
        .iter()
        .for_each(|c| walk(&c.shape, text, &mut found));
    found
}

/// A window tall enough to show a whole page without scrolling.
pub(super) fn tall_input(events: Vec<egui::Event>) -> RawInput {
    RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1180.0, 4000.0),
        )),
        events,
        ..Default::default()
    }
}

/// How a test builds each frame's input from its events.
pub(super) type InputFn = fn(Vec<egui::Event>) -> RawInput;

/// Hover, press, release at `pos`, in a tall window.
pub(super) fn click_at(app: &mut WinTuneApp, ctx: &egui::Context, pos: egui::Pos2) {
    click_at_with(app, ctx, tall_input, pos);
}

/// Hover, press, release at `pos`, with the frames' input built by `input`.
pub(super) fn click_at_with(
    app: &mut WinTuneApp,
    ctx: &egui::Context,
    input: InputFn,
    pos: egui::Pos2,
) {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(app, ctx, input(vec![egui::Event::PointerMoved(pos)]));
    frame(app, ctx, input(vec![button(true)]));
    frame(app, ctx, input(vec![button(false)]));
}

/// Moves onto `pos` and rests there long enough for a tooltip, and returns
/// the texts drawn, in a tall window. egui shows no tooltip while the last
/// click is more recent than the last move, and it only sees a move over a
/// few closely spaced frames.
pub(super) fn hover_texts(
    app: &mut WinTuneApp,
    ctx: &egui::Context,
    pos: egui::Pos2,
) -> Vec<String> {
    hover_texts_with(app, ctx, tall_input, pos)
}

/// `hover_texts` with the frames' input built by `input`.
pub(super) fn hover_texts_with(
    app: &mut WinTuneApp,
    ctx: &egui::Context,
    input: InputFn,
    pos: egui::Pos2,
) -> Vec<String> {
    let away = pos - egui::vec2(0.0, 200.0);
    let mut texts = Vec::new();
    for i in 0..20 {
        let at = if i < 2 { away } else { pos };
        let mut raw = input(vec![egui::Event::PointerMoved(at)]);
        if i >= 6 {
            raw.time = Some(ctx.input(|i| i.time) + 0.1);
        }
        texts = drawn_texts(&frame(app, ctx, raw));
    }
    texts
}

/// Center of the widget of exactly `size` on the row of tweak `id`, or
/// `None` when the row has none. The row's description sits on the row's
/// middle line, as do its trailing widgets; rows are at least 48 px apart.
pub(super) fn row_widget(
    app: &mut WinTuneApp,
    ctx: &egui::Context,
    id: &str,
    size: egui::Vec2,
) -> Option<egui::Pos2> {
    let out = frame(app, ctx, tall_input(vec![]));
    let description = crate::i18n::messages::tweak_description(app.tr, id);
    let at = text_center(&out, &description).unwrap_or_else(|| panic!("row {id} not drawn"));
    ctx.viewport_for(ViewportId::ROOT, |v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, widgets)| widgets.iter())
            .filter(|w| w.rect.size() == size && w.rect.center().x > at.x)
            .map(|w| w.rect.center())
            .filter(|c| (c.y - at.y).abs() < 24.0)
            .min_by(|a, b| (a.y - at.y).abs().total_cmp(&(b.y - at.y).abs()))
    })
}

/// Holds a pak build until the returned sender is used (or dropped).
pub(super) fn hold_job(app: &mut WinTuneApp) -> std::sync::mpsc::Sender<()> {
    use super::jobs::RunningJob;
    use crate::tasks::BackgroundJob;
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    app.job = Some(RunningJob::BuildPak(BackgroundJob::spawn(
        "Building gallery.pak",
        || {},
        move |_| {
            let _ = release_rx.recv();
            Ok(crate::gallery::PakStats {
                count: 0,
                bytes_before: 0,
                bytes_after: 0,
            })
        },
    )));
    release_tx
}
