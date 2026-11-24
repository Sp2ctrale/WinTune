use super::rule_status::{status_badge, status_details};
use super::{kit, ActiveTab, WinTuneApp};
use crate::associations::{find_rule_problems, validate_extension, RuleProblem, RuleStatus};
use crate::i18n::messages;
use crate::models::{CustomContextCommand, DoubleClickMode, ExtensionConfig};
use crate::theme::{accent_button, card_frame, draw_badge, get_palette};
use eframe::egui::{self, Margin, RichText};
use kit::{space, text_size, PillKind};

/// Height of a row of the rule list.
const ROW_HEIGHT: f32 = 36.0;
/// Height of the buttons of the page header.
const BUTTON_HEIGHT: f32 = 32.0;

/// What the rule list shows for a rule, from the registry check and the
/// validation of the rule itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuleState {
    /// The status has not been computed yet (or the rule was just renamed).
    Checking,
    NotApplied,
    UpToDate,
    Changed,
    Invalid,
}

/// Color of the status dot (and of the detail's pill); `None` is a ring.
pub(super) fn rule_state_pill(state: RuleState) -> Option<PillKind> {
    match state {
        RuleState::UpToDate => Some(PillKind::Success),
        RuleState::Changed => Some(PillKind::Warning),
        RuleState::Invalid => Some(PillKind::Danger),
        RuleState::NotApplied | RuleState::Checking => None,
    }
}

impl WinTuneApp {
    pub(super) fn show_associations_tab(&mut self, ctx: &egui::Context) {
        self.show_extension_list(ctx);
        self.show_extension_details(ctx);
    }

    /// `Invalid` wins; otherwise the status read for the rule's extension.
    /// The list uses `rule_state_in` to validate once per frame; the rule
    /// detail uses this one.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn rule_state(&self, index: usize) -> RuleState {
        self.rule_state_in(index, &find_rule_problems(&self.config.extensions))
    }

    fn rule_state_in(&self, index: usize, problems: &[(usize, RuleProblem)]) -> RuleState {
        if problems.iter().any(|(i, _)| *i == index) {
            return RuleState::Invalid;
        }
        match self
            .config
            .extensions
            .get(index)
            .and_then(|r| self.rule_statuses.get(&r.extension))
        {
            None => RuleState::Checking,
            Some(RuleStatus::NotApplied) => RuleState::NotApplied,
            Some(RuleStatus::Applied) => RuleState::UpToDate,
            Some(RuleStatus::Drifted(_)) => RuleState::Changed,
        }
    }

    fn rule_state_label(&self, state: RuleState) -> String {
        match state {
            RuleState::Checking => self.tr.t("assoc.state.checking"),
            RuleState::NotApplied => self.tr.t("assoc.state.not_applied"),
            RuleState::UpToDate => self.tr.t("assoc.state.up_to_date"),
            RuleState::Changed => self.tr.t("assoc.state.changed"),
            RuleState::Invalid => self.tr.t("assoc.state.invalid"),
        }
    }

    /// Breadcrumb, title and the page's bulk actions. Drawn at the top of
    /// the detail column.
    fn associations_header(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let tr = self.tr;
        let p = kit::current_palette(&ctx);
        let busy = self.is_busy();
        let disabled_reason = self.apply_disabled_reason();
        let all_checked = self.config.extensions.iter().all(|r| r.enabled);
        let name = tr.t("nav.associations");
        let subtitle = tr.t("assoc.subtitle");
        let toggle_label = if all_checked {
            tr.t("assoc.uncheck_all")
        } else {
            tr.t("assoc.check_all")
        };
        let apply_label = tr.t("assoc.apply_all");
        let refresh_label = tr.t("assoc.refresh");
        let refresh_hint = tr.t("assoc.refresh_hint");

        let mut apply = false;
        let mut toggle = false;
        let mut refresh = false;
        kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |ui| {
            let primary = egui::Button::new(
                RichText::new(&apply_label)
                    .size(text_size::BODY)
                    .color(p.on_accent),
            )
            .fill(p.accent)
            .min_size(egui::vec2(0.0, BUTTON_HEIGHT));
            let resp = ui.add_enabled(disabled_reason.is_none(), primary);
            let resp = match &disabled_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            apply = resp.clicked();

            let secondary = |text: &str| {
                egui::Button::new(RichText::new(text).size(text_size::BODY))
                    .min_size(egui::vec2(0.0, BUTTON_HEIGHT))
            };
            toggle = ui.add(secondary(&toggle_label)).clicked();
            refresh = ui
                .add_enabled(!busy, secondary(&refresh_label))
                .on_hover_text(&refresh_hint)
                .on_disabled_hover_text(&refresh_hint)
                .clicked();
        });

        if apply {
            let rules = self.config.extensions.clone();
            self.start_apply(&ctx, rules);
        }
        if toggle {
            for rule in &mut self.config.extensions {
                rule.enabled = !all_checked;
            }
        }
        if refresh {
            self.request_status_refresh();
        }
    }

    fn add_extension_from_input(&mut self) {
        let mut ext = self.new_ext_input.trim().to_lowercase();
        if ext.is_empty() {
            return;
        }
        if !ext.starts_with('.') {
            ext = format!(".{}", ext);
        }
        if self
            .config
            .extensions
            .iter()
            .any(|e| e.extension.eq_ignore_ascii_case(&ext))
        {
            let text = self.tr.t_args("assoc.exists", &[&ext]);
            self.notify(kit::ToastKind::Info, text);
            return;
        }
        let clean = ext.trim_start_matches('.').to_uppercase();
        let default_icon = ext.trim_start_matches('.').to_string();
        let candidate = ExtensionConfig {
            enabled: true,
            extension: ext.clone(),
            prog_id: format!("Custom{}File", clean),
            description: format!("{} Source File", clean),
            icon_name: default_icon,
            reset_existing: false,
            reset_user_choice: true,
            mode: DoubleClickMode::OpenInEditor,
            launcher_command: None,
            elevated_command: None,
            perceived_type: "text".to_string(),
            content_type: "text/plain".to_string(),
            preserve_existing_properties: true,
            extra_verbs: Vec::new(),
        };
        if let Err(e) = validate_extension(&candidate) {
            let why = messages::rule_problem(self.tr, &e);
            let text = self.tr.t_args("assoc.add_invalid", &[&ext, &why]);
            self.notify(kit::ToastKind::Error, text);
            return;
        }
        self.config.extensions.push(candidate);
        self.selected_ext_index = self.config.extensions.len() - 1;
        self.new_ext_input.clear();
    }

    fn show_extension_list(&mut self, ctx: &egui::Context) {
        let p = kit::current_palette(ctx);
        let tr = self.tr;

        egui::SidePanel::left("extensions_master_list")
            .resizable(true)
            .default_width(300.0)
            .frame(egui::Frame::none().fill(p.page).inner_margin(Margin {
                left: space::L,
                right: space::L,
                top: space::XL,
                bottom: space::L,
            }))
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    // The add row sits at the bottom; the list takes the rest.
                    let mut add = false;
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 80.0).max(60.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_ext_input)
                                .hint_text(tr.t("assoc.add_hint"))
                                .desired_width(width),
                        );
                        add = ui.button(tr.t("assoc.add")).clicked();
                    });
                    if add {
                        self.add_extension_from_input();
                    }
                    ui.add_space(space::S);

                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.ext_search_query)
                                .id(super::polish::search_box_id(ActiveTab::Associations)
                                    .expect("search box"))
                                .hint_text(tr.t("assoc.search"))
                                .desired_width(f32::INFINITY),
                        );
                        self.focus_search_if_requested(&search);
                        ui.add_space(space::S);
                        self.rule_rows(ctx, ui);
                    });
                });
            });
    }

    /// The scrolling list of rules, or what to show instead of it.
    fn rule_rows(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let p = kit::current_palette(ctx);
        let tr = self.tr;
        let query = self.ext_search_query.trim().to_lowercase();
        let visible: Vec<usize> = self
            .config
            .extensions
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                query.is_empty()
                    || e.extension.to_lowercase().contains(&query)
                    || e.description.to_lowercase().contains(&query)
                    || e.icon_name.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();

        if self.config.extensions.is_empty() {
            kit::empty_state(ui, kit::Icon::Associations, &tr.t("assoc.empty"), None);
            return;
        }
        if visible.is_empty() {
            let text = tr.t_args("assoc.no_match", &[&self.ext_search_query.trim()]);
            ui.label(
                RichText::new(text)
                    .size(text_size::BODY)
                    .color(p.text_secondary),
            );
            return;
        }

        let problems = find_rule_problems(&self.config.extensions);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for idx in visible {
                    let state = self.rule_state_in(idx, &problems);
                    let hover = match state {
                        RuleState::Invalid => problems
                            .iter()
                            .find(|(i, _)| *i == idx)
                            .map(|(_, problem)| messages::rule_problem(tr, problem))
                            .unwrap_or_default(),
                        _ => self.rule_state_label(state),
                    };
                    let tex = self
                        .gallery
                        .texture(ctx, &self.config.extensions[idx].icon_name);
                    let selected = self.selected_ext_index == idx;

                    let (rect, row) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        egui::Sense::click(),
                    );
                    let fill = if selected {
                        Some(p.accent.gamma_multiply(0.14))
                    } else if row.hovered() {
                        Some(p.surface_hover)
                    } else {
                        None
                    };
                    if let Some(fill) = fill {
                        ui.painter().rect_filled(
                            rect,
                            egui::Rounding::same(kit::radius::CONTROL),
                            fill,
                        );
                    }
                    let mut content = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect.shrink2(egui::vec2(space::S, 0.0)))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    let rule = &mut self.config.extensions[idx];
                    content.checkbox(&mut rule.enabled, "");
                    match tex {
                        Some(t) => {
                            content.image((t.id(), egui::vec2(20.0, 20.0)));
                        }
                        None => {
                            // Keep rows aligned while the icon decodes.
                            content
                                .allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                        }
                    }
                    content.add(
                        egui::Label::new(
                            RichText::new(&rule.extension)
                                .size(text_size::BODY)
                                .color(p.text),
                        )
                        .selectable(false),
                    );
                    content.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        kit::status_dot(ui, rule_state_pill(state), Some(&hover));
                    });
                    if row.clicked() {
                        self.selected_ext_index = idx;
                    }
                }
            });
    }

    fn show_extension_details(&mut self, ctx: &egui::Context) {
        let palette = get_palette(self.ui_theme);
        let busy = self.is_busy();

        Self::page_panel(ctx, |ui| {
            self.associations_header(ui);
            if self.selected_ext_index >= self.config.extensions.len() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Select a rule on the left, or add one, to edit it here.")
                            .color(palette.text_secondary),
                    );
                });
                return;
            }

            let current_tex = self.gallery.texture(
                ctx,
                &self.config.extensions[self.selected_ext_index].icon_name,
            );
            let has_backup = self
                .backup_store
                .find(&self.config.extensions[self.selected_ext_index].extension)
                .is_some();

            let problem = find_rule_problems(&self.config.extensions)
                .into_iter()
                .find(|(i, _)| *i == self.selected_ext_index)
                .map(|(_, p)| p.to_string());

            let status = self
                .rule_statuses
                .get(&self.config.extensions[self.selected_ext_index].extension)
                .cloned();

            let mut delete_current = false;
            let mut open_gallery_picker = false;
            let mut apply_single = false;

            egui::ScrollArea::vertical().show(ui, |ui| {
                let ext_cfg = &mut self.config.extensions[self.selected_ext_index];

                card_frame(self.ui_theme).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(t) = current_tex {
                            ui.image((t.id(), egui::vec2(52.0, 52.0)));
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.heading(
                                    RichText::new(format!(
                                        "{} — {}",
                                        ext_cfg.extension, ext_cfg.description
                                    ))
                                    .size(20.0),
                                );
                                if has_backup {
                                    draw_badge(ui, "Snapshot Saved", palette.success);
                                }
                                status_badge(ui, self.ui_theme, status.as_ref());
                            });
                            ui.label(
                                RichText::new(format!(
                                    "ProgID: {}  |  Icon: {}.ico",
                                    ext_cfg.prog_id, ext_cfg.icon_name
                                ))
                                .color(palette.text_secondary),
                            );
                            status_details(ui, self.ui_theme, status.as_ref());
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !busy && problem.is_none(),
                                    accent_button("Apply This Extension", self.ui_theme),
                                )
                                .clicked()
                            {
                                apply_single = true;
                            }
                            if ui.button("Remove").clicked() {
                                delete_current = true;
                            }
                        });
                    });
                });

                ui.add_space(10.0);
                identity_card(
                    ui,
                    ext_cfg,
                    self.ui_theme,
                    problem.as_deref(),
                    &mut open_gallery_picker,
                );
                ui.add_space(10.0);
                behavior_card(ui, ext_cfg, self.ui_theme);
                ui.add_space(10.0);
                verbs_card(ui, ext_cfg, self.ui_theme);
            });

            if open_gallery_picker {
                let extension = self.config.extensions[self.selected_ext_index]
                    .extension
                    .clone();
                self.open_icon_picker(&extension);
            }

            if apply_single {
                let mut single = self.config.extensions[self.selected_ext_index].clone();
                single.enabled = true;
                self.start_apply(ctx, vec![single]);
            }

            if delete_current {
                self.request_remove_rule(self.selected_ext_index);
            }
        });
    }
}

fn identity_card(
    ui: &mut egui::Ui,
    ext_cfg: &mut ExtensionConfig,
    theme: crate::theme::ThemeMode,
    problem: Option<&str>,
    open_gallery_picker: &mut bool,
) {
    card_frame(theme).show(ui, |ui| {
        ui.label(
            RichText::new("Identity & File Properties")
                .size(15.0)
                .strong(),
        );
        ui.add_space(6.0);

        egui::Grid::new("ext_identity_grid")
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                ui.label("File Extension:");
                ui.text_edit_singleline(&mut ext_cfg.extension);
                ui.end_row();

                ui.label("Custom ProgID:");
                ui.text_edit_singleline(&mut ext_cfg.prog_id);
                ui.end_row();

                ui.label("Type Description:");
                ui.text_edit_singleline(&mut ext_cfg.description);
                ui.end_row();

                ui.label("Assigned Icon:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut ext_cfg.icon_name);
                    if ui.button("Pick from Gallery...").clicked() {
                        *open_gallery_picker = true;
                    }
                });
                ui.end_row();

                ui.label("PerceivedType (Preview Pane):");
                ui.text_edit_singleline(&mut ext_cfg.perceived_type);
                ui.end_row();

                ui.label("MIME Content Type:");
                ui.text_edit_singleline(&mut ext_cfg.content_type);
                ui.end_row();
            });

        if let Some(problem) = problem {
            ui.add_space(6.0);
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("This rule cannot be applied: {}", problem),
            );
        }
    });
}

fn behavior_card(ui: &mut egui::Ui, ext_cfg: &mut ExtensionConfig, theme: crate::theme::ThemeMode) {
    card_frame(theme).show(ui, |ui| {
        ui.label(
            RichText::new("Registry Behavior & Execution")
                .size(15.0)
                .strong(),
        );
        ui.add_space(6.0);

        ui.checkbox(
            &mut ext_cfg.preserve_existing_properties,
            "Preserve native extension properties (OpenWithProgids, ShellNew, Shellex)",
        );
        ui.checkbox(
            &mut ext_cfg.reset_existing,
            "Clean per-user HKCU\\Software\\Classes overrides",
        );
        ui.checkbox(
            &mut ext_cfg.reset_user_choice,
            "Reset Explorer UserChoice override (FileExts)",
        )
        .on_hover_text(
            "Removes the per-user \"Open with… / Always\" choice so WinTune's association is used.\n\
             Rollback puts the choice back, but Windows protects it with a hash: it may still \
             report \"An app default was reset\" and fall back to its default app.",
        );

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Double-Click Action:").strong());
            ui.radio_value(
                &mut ext_cfg.mode,
                DoubleClickMode::OpenInEditor,
                "Open in Code Editor",
            );
            ui.radio_value(
                &mut ext_cfg.mode,
                DoubleClickMode::ExecuteScript,
                "Execute Script (Put Editor under Right-Click 'Edit')",
            );
        });

        if ext_cfg.mode == DoubleClickMode::ExecuteScript {
            ui.add_space(6.0);
            let launcher_str = ext_cfg
                .launcher_command
                .get_or_insert_with(|| "\"%1\" %*".to_string());
            ui.horizontal(|ui| {
                ui.label("Launch Command (open):");
                ui.text_edit_singleline(launcher_str);
            });

            let mut has_elevated = ext_cfg.elevated_command.is_some();
            if ui
                .checkbox(
                    &mut has_elevated,
                    "Enable 'Run as Administrator' context menu verb (runas)",
                )
                .changed()
            {
                ext_cfg.elevated_command = has_elevated.then(|| "cmd.exe /C \"%1\" %*".to_string());
            }
            if let Some(elev_cmd) = &mut ext_cfg.elevated_command {
                ui.horizontal(|ui| {
                    ui.label("Administrator Command (runas):");
                    ui.text_edit_singleline(elev_cmd);
                });
            }
        }
    });
}

fn verbs_card(ui: &mut egui::Ui, ext_cfg: &mut ExtensionConfig, theme: crate::theme::ThemeMode) {
    let palette = get_palette(theme);
    card_frame(theme).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Custom Context Menu Verbs (Right-Click)")
                    .size(15.0)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ Add Context Verb").clicked() {
                    ext_cfg.extra_verbs.push(CustomContextCommand {
                        verb_key: "open_terminal".to_string(),
                        menu_label: "Open in Windows Terminal".to_string(),
                        command: "wt.exe -d \"%V\"".to_string(),
                    });
                }
            });
        });

        if ext_cfg.extra_verbs.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("No extra context menu actions configured.")
                    .color(palette.text_secondary),
            );
        }

        let mut remove_verb_idx = None;
        for (v_idx, verb) in ext_cfg.extra_verbs.iter_mut().enumerate() {
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Registry Key:");
                    ui.add(egui::TextEdit::singleline(&mut verb.verb_key).desired_width(110.0));
                    ui.label("Menu Label:");
                    ui.add(egui::TextEdit::singleline(&mut verb.menu_label).desired_width(180.0));
                    if ui.button("Remove").clicked() {
                        remove_verb_idx = Some(v_idx);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Command:");
                    ui.text_edit_singleline(&mut verb.command);
                });
            });
        }
        if let Some(idx) = remove_verb_idx {
            ext_cfg.extra_verbs.remove(idx);
        }
    });
}

/// Problems of the enabled rules only: a disabled rule is never applied, so
/// it must not block the others.
pub(super) fn problems_of_enabled(tr: crate::i18n::Tr, rules: &[ExtensionConfig]) -> Vec<String> {
    let enabled: Vec<ExtensionConfig> = rules.iter().filter(|r| r.enabled).cloned().collect();
    find_rule_problems(&enabled)
        .into_iter()
        .map(|(_, p)| messages::rule_problem(tr, &p))
        .collect()
}

impl WinTuneApp {
    /// Why "Apply all enabled" is blocked: problems of enabled rules.
    pub(super) fn apply_blockers(&self) -> Vec<String> {
        problems_of_enabled(self.tr, &self.config.extensions)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::ui::kit::PillKind;

    #[test]
    fn invalid_enabled_rule_blocks_apply_and_writes_nothing() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".a b"]);

        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);

        assert!(!app.is_busy(), "apply started despite an invalid rule");
        assert!(app.last_notice().contains(".a b"), "{}", app.last_notice());
        assert!(!fx.key_exists(&fx.classes(".foo")));
        assert!(!app.apply_blockers().is_empty());
    }

    #[test]
    fn duplicate_enabled_rules_block_apply() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".FOO"]);

        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);

        assert!(!app.is_busy());
        assert!(
            app.last_notice().contains("more than once"),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn disabled_invalid_rule_does_not_block_apply() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".a b"]);
        app.config.extensions[1].enabled = false;
        assert!(app.apply_blockers().is_empty());

        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);

        assert!(fx.key_exists(&fx.classes(".foo")), "{}", app.last_notice());
    }

    #[test]
    fn adding_an_invalid_extension_is_refused() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.new_ext_input = "my ext".to_string();

        app.add_extension_from_input();

        assert_eq!(app.config.extensions.len(), 1);
        let prefix = app.tr.t_args("assoc.add_invalid", &[&".my ext", &""]);
        assert!(
            app.last_notice().starts_with(&prefix),
            "{}",
            app.last_notice()
        );
        assert_eq!(app.new_ext_input, "my ext", "input kept so it can be fixed");
    }

    #[test]
    fn adding_a_duplicate_extension_says_so() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.new_ext_input = "FOO".to_string();

        app.add_extension_from_input();

        assert_eq!(app.config.extensions.len(), 1);
        assert_eq!(app.last_notice(), app.tr.t_args("assoc.exists", &[&".foo"]));
    }

    fn page_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1180.0, 760.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        let out = frame(app, ctx, page_input(vec![]));
        let pos = text_center(&out, text).unwrap_or_else(|| panic!("no text {text:?} drawn"));
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(app, ctx, page_input(vec![egui::Event::PointerMoved(pos)]));
        frame(app, ctx, page_input(vec![button(true)]));
        frame(app, ctx, page_input(vec![button(false)]));
    }

    #[test]
    fn rule_state_maps_statuses_and_problems() {
        use crate::associations::RuleStatus;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b", ".c", ".d", ".x y"]);
        app.rule_statuses.clear();
        app.rule_statuses.insert(".a".into(), RuleStatus::Applied);
        app.rule_statuses
            .insert(".b".into(), RuleStatus::Drifted(vec![]));
        app.rule_statuses
            .insert(".c".into(), RuleStatus::NotApplied);
        app.rule_statuses.insert(".x y".into(), RuleStatus::Applied);

        let states: Vec<RuleState> = (0..5).map(|i| app.rule_state(i)).collect();
        assert_eq!(
            states,
            [
                RuleState::UpToDate,
                RuleState::Changed,
                RuleState::NotApplied,
                RuleState::Checking,
                RuleState::Invalid
            ]
        );
        assert_eq!(
            rule_state_pill(RuleState::UpToDate),
            Some(PillKind::Success)
        );
        assert_eq!(rule_state_pill(RuleState::Changed), Some(PillKind::Warning));
        assert_eq!(rule_state_pill(RuleState::Invalid), Some(PillKind::Danger));
        assert_eq!(rule_state_pill(RuleState::NotApplied), None);
        assert_eq!(rule_state_pill(RuleState::Checking), None);
    }

    #[test]
    fn check_all_then_uncheck_all() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b", ".c"]);
        app.active_tab = ActiveTab::Associations;
        app.config.extensions[1].enabled = false;

        let check = app.tr.t("assoc.check_all");
        click_text(&mut app, &ctx, &check);
        assert!(app.config.extensions.iter().all(|r| r.enabled));

        let uncheck = app.tr.t("assoc.uncheck_all");
        click_text(&mut app, &ctx, &uncheck);
        assert!(app.config.extensions.iter().all(|r| !r.enabled));
    }

    #[test]
    fn a_filtered_out_selection_stays_in_the_detail() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.active_tab = ActiveTab::Associations;
        app.selected_ext_index = 1;
        app.ext_search_query = "a".to_string();

        let out = frame(&mut app, &ctx, page_input(vec![]));

        let texts = drawn_texts(&out);
        assert!(
            texts.iter().any(|t| t.starts_with(".b")),
            "detail lost the selection: {texts:?}"
        );
    }

    #[test]
    fn the_list_shows_the_empty_state_without_rules() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[]);
        app.active_tab = ActiveTab::Associations;

        let out = frame(&mut app, &ctx, page_input(vec![]));

        assert!(drawn_texts(&out).contains(&app.tr.t("assoc.empty")));
    }

    #[test]
    fn a_search_without_match_says_so() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a"]);
        app.active_tab = ActiveTab::Associations;
        app.ext_search_query = "zzz".to_string();

        let out = frame(&mut app, &ctx, page_input(vec![]));

        assert!(drawn_texts(&out).contains(&app.tr.t_args("assoc.no_match", &[&"zzz"])));
    }

    #[test]
    fn adding_a_valid_extension_selects_it() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.new_ext_input = "Zig".to_string();

        app.add_extension_from_input();

        assert_eq!(app.config.extensions.len(), 2);
        assert_eq!(app.config.extensions[1].extension, ".zig");
        assert_eq!(app.selected_ext_index, 1);
        assert!(app.new_ext_input.is_empty());
    }
}
