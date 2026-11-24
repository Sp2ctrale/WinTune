//! The Settings page: language, theme, editor, icon folder, system, data and
//! about. Every change goes to `config`; the autosave writes it.

use super::{kit, WinTuneApp, APP_VERSION};
use crate::detector::resolve_vscode_executable;
use crate::i18n::Tr;
use crate::theme::ThemeMode;
use eframe::egui;

const REPO_URL: &str = "https://github.com/Sp2ctrale/WinTune";

/// Choices made while drawing, applied once the frame's borrows end (the
/// dialogs and effects need the whole app).
#[derive(Default)]
struct Picked {
    language: Option<&'static str>,
    theme: Option<ThemeMode>,
    browse_editor: bool,
    auto_detect: bool,
    change_icons: bool,
    refresh_explorer: bool,
    open_data: bool,
}

impl WinTuneApp {
    pub(super) fn show_settings(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let mut picked = Picked::default();
        let languages = Tr::languages();
        let language_options: Vec<(&'static str, &str)> = languages
            .iter()
            .map(|(code, name)| (*code, name.as_str()))
            .collect();
        let theme_labels = [
            tr.t("settings.theme.system"),
            tr.t("settings.theme.light"),
            tr.t("settings.theme.dark"),
        ];
        let theme_options = [
            (ThemeMode::System, theme_labels[0].as_str()),
            (ThemeMode::Light, theme_labels[1].as_str()),
            (ThemeMode::Dark, theme_labels[2].as_str()),
        ];
        let mut language = tr.code();
        let mut theme = self.config.theme;
        let title = tr.t("nav.settings");
        let palette = kit::current_palette(ctx);

        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &[], &title, None, |_| {});
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(e) = &self.config_error {
                    kit::banner(
                        ui,
                        kit::BannerKind::Error,
                        &tr.t_args("settings.config_unreadable", &[e]),
                        None,
                    );
                    ui.add_space(kit::space::L);
                }

                kit::section(ui, &tr.t("settings.appearance"), |ui| {
                    kit::SettingRow {
                        icon: Some(kit::Icon::Settings),
                        title: &tr.t("settings.language.title"),
                        description: Some(&tr.t("settings.language.desc")),
                    }
                    .show(ui, |ui| {
                        if kit::segmented(ui, &mut language, &language_options).changed() {
                            picked.language = Some(language);
                        }
                    });
                    kit::SettingRow {
                        icon: Some(kit::Icon::Settings),
                        title: &tr.t("settings.theme.title"),
                        description: Some(&tr.t("settings.theme.desc")),
                    }
                    .show(ui, |ui| {
                        if kit::segmented(ui, &mut theme, &theme_options).changed() {
                            picked.theme = Some(theme);
                        }
                    });
                });
                ui.add_space(kit::space::L);

                kit::section(ui, &tr.t("settings.associations"), |ui| {
                    kit::SettingRow {
                        icon: Some(kit::Icon::Associations),
                        title: &tr.t("settings.editor.title"),
                        description: Some(&tr.t("settings.editor.desc")),
                    }
                    .show(ui, |ui| {
                        if ui.button(tr.t("settings.browse")).clicked() {
                            picked.browse_editor = true;
                        }
                        if ui.button(tr.t("settings.auto_detect")).clicked() {
                            picked.auto_detect = true;
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.config.code_executable)
                                .hint_text("C:\\...\\Code.exe"),
                        );
                    });
                });
                ui.add_space(kit::space::L);

                kit::section(ui, &tr.t("settings.icons"), |ui| {
                    kit::SettingRow {
                        icon: Some(kit::Icon::Icons),
                        title: &tr.t("settings.icon_folder.title"),
                        description: Some(&tr.t("settings.icon_folder.desc")),
                    }
                    .show(ui, |ui| {
                        if ui.button(tr.t("settings.change")).clicked() {
                            picked.change_icons = true;
                        }
                        ui.label(
                            egui::RichText::new(&self.config.icon_source_folder)
                                .color(palette.text_secondary),
                        );
                    });
                });
                ui.add_space(kit::space::L);

                kit::section(ui, &tr.t("settings.system"), |ui| {
                    kit::SettingRow {
                        icon: Some(kit::Icon::Associations),
                        title: &tr.t("settings.wsl.title"),
                        description: Some(&tr.t("settings.wsl.desc")),
                    }
                    .show(ui, |ui| {
                        if self.wsl_detected {
                            kit::pill(ui, &tr.t("settings.wsl_on"), kit::PillKind::Success);
                        } else {
                            kit::pill(ui, &tr.t("settings.wsl_off"), kit::PillKind::Neutral);
                        }
                    });
                    kit::SettingRow {
                        icon: Some(kit::Icon::Explorer),
                        title: &tr.t("settings.refresh_explorer.title"),
                        description: Some(&tr.t("settings.refresh_explorer.desc")),
                    }
                    .show(ui, |ui| {
                        if ui.button(tr.t("settings.refresh_explorer")).clicked() {
                            picked.refresh_explorer = true;
                        }
                    });
                });
                ui.add_space(kit::space::L);

                kit::section(ui, &tr.t("settings.data"), |ui| {
                    kit::SettingRow {
                        icon: Some(kit::Icon::History),
                        title: &tr.t("settings.data_folder.title"),
                        description: Some(&tr.t("settings.data_folder.desc")),
                    }
                    .show(ui, |ui| {
                        if ui.button(tr.t("settings.open_data_folder")).clicked() {
                            picked.open_data = true;
                        }
                    });
                });
                ui.add_space(kit::space::L);

                kit::section(ui, &tr.t("settings.about"), |ui| {
                    ui.label(tr.t_args("settings.version", &[&APP_VERSION]));
                    ui.hyperlink_to(tr.t("settings.repo_link"), REPO_URL);
                });
            });
        });

        self.apply_picked(ctx, picked);
    }

    fn apply_picked(&mut self, ctx: &egui::Context, picked: Picked) {
        if let Some(code) = picked.language {
            self.set_language(code);
        }
        if let Some(mode) = picked.theme {
            self.set_theme(ctx, mode);
        }
        if picked.browse_editor {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Executable", &["exe"])
                .pick_file()
            {
                self.config.code_executable = file.to_string_lossy().into_owned();
            }
        }
        if picked.auto_detect {
            self.auto_detect_editor();
        }
        if picked.change_icons {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.set_icon_folder(dir.to_string_lossy().into_owned());
            }
        }
        if picked.refresh_explorer {
            (self.env.notify_shell)();
            let text = self.tr.t("settings.explorer_refreshed");
            self.notify(kit::ToastKind::Info, text);
        }
        if picked.open_data {
            self.open_data_folder();
        }
    }

    pub(super) fn auto_detect_editor(&mut self) {
        match resolve_vscode_executable(None) {
            Some(found) => {
                self.config.code_executable = found;
                let text = self.tr.t("settings.editor_found");
                self.notify(kit::ToastKind::Success, text);
            }
            None => {
                let text = self.tr.t("settings.editor_not_found");
                self.notify(kit::ToastKind::Info, text);
            }
        }
    }

    /// Points the gallery at another folder of `.ico` files.
    pub(super) fn set_icon_folder(&mut self, path: String) {
        self.config.icon_source_folder = path;
        self.gallery
            .reload(&self.config.icon_source_folder, &self.config.pak_file_path);
    }

    /// Opens the data folder in Explorer, creating it first: a first run
    /// has written nothing there yet.
    pub(super) fn open_data_folder(&mut self) {
        let dir = self.env.data_dir.clone();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            let text = self.tr.t("settings.data_folder_failed");
            self.notify_details(kit::ToastKind::Error, text, e.to_string());
            return;
        }
        (self.env.open_folder)(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::*;
    use egui::RawInput;

    fn at(t: f64) -> RawInput {
        RawInput {
            time: Some(t),
            ..Default::default()
        }
    }

    #[test]
    fn choosing_french_switches_the_whole_ui_next_frame() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Settings);
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(drawn_texts(&out).contains(&"Settings".to_string()));

        app.set_language("fr");
        let out = frame(&mut app, &ctx, RawInput::default());
        let texts = drawn_texts(&out);
        assert!(texts.contains(&Tr::new("fr").t("nav.settings")));
        assert!(!texts.contains(&"Settings".to_string()), "{texts:?}");
    }

    #[test]
    fn editing_the_editor_path_is_autosaved() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Settings);
        frame(&mut app, &ctx, at(0.0));
        app.config.code_executable = r"C:\Tools\ed.exe".into();
        for t in [0.5, 1.0, 1.5, 2.0] {
            frame(&mut app, &ctx, at(t));
        }
        let saved = std::fs::read_to_string(fx.dir.join("config.json")).unwrap();
        assert!(saved.contains("ed.exe"), "{saved}");
    }

    #[test]
    fn the_editor_bar_no_longer_holds_the_path() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_page(ActiveTab::Associations);
        let out = frame(&mut app, &ctx, RawInput::default());
        let texts = drawn_texts(&out);
        assert!(!texts.contains(&"Default Code Editor:".to_string()));
        assert!(!texts.contains(&"Auto-Detect".to_string()));
        assert!(!texts.contains(&"Browse...".to_string()));
        assert!(texts.contains(&"Refresh status".to_string()), "{texts:?}");
    }

    #[test]
    fn open_data_folder_uses_the_env() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.open_data_folder();
        assert_eq!(fx.opened_folders(), vec![fx.dir.join("data")]);
        assert!(fx.dir.join("data").is_dir(), "created before opening");
    }

    #[test]
    fn open_data_folder_reports_a_folder_that_cannot_be_created() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        // A file where the folder should be.
        std::fs::write(fx.dir.join("blocker"), "a file").unwrap();
        app.env.data_dir = fx.dir.join("blocker").join("data");

        app.open_data_folder();

        assert!(fx.opened_folders().is_empty());
        assert!(
            app.last_notice()
                .starts_with(&app.tr.t("settings.data_folder_failed")),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn set_icon_folder_reloads_the_gallery() {
        let fx = TempIcons::new(&["rust", "python"]);
        let ctx = egui::Context::default();
        let mut config = fx.config();
        config.icon_source_folder = String::new();
        let mut app = fx.app_with(&ctx, config);
        assert_eq!(app.gallery.names().len(), 0);
        app.set_icon_folder(fx.dir.join("icons").to_string_lossy().into_owned());
        assert_eq!(app.gallery.names().len(), 2);
        assert!(app.config.icon_source_folder.ends_with("icons"));
    }

    #[test]
    fn an_unreadable_config_shows_its_banner_here() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.config_error = Some("bad json".into());
        app.open_page(ActiveTab::Settings);
        let out = frame(&mut app, &ctx, RawInput::default());
        assert!(
            drawn_texts(&out)
                .iter()
                .any(|t| t.contains("config.json can't be read") && t.contains("bad json")),
            "{:?}",
            drawn_texts(&out)
        );
    }

    #[test]
    fn auto_detect_reports_either_outcome() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        app.auto_detect_editor();
        let last = app.last_notice();
        assert!(
            last == app.tr.t("settings.editor_found")
                || last == app.tr.t("settings.editor_not_found"),
            "{last}"
        );
    }

    #[test]
    fn settings_renders_in_every_theme_and_language() {
        use crate::theme::ThemeMode;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        for (code, _) in Tr::languages() {
            for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
                let mut app = fx.app(&ctx);
                app.set_language(code);
                app.set_theme(&ctx, mode);
                app.wsl_detected = mode == ThemeMode::Dark;
                app.open_page(ActiveTab::Settings);
                for _ in 0..3 {
                    let out = frame(&mut app, &ctx, RawInput::default());
                    let texts = drawn_texts(&out);
                    assert!(
                        texts.contains(&app.tr.t("settings.appearance")),
                        "{code} {mode:?}"
                    );
                }
            }
        }
    }
}
