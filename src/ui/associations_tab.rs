use super::{kit, ActiveTab, WinTuneApp};
use crate::associations::{find_rule_problems, validate_extension, RuleProblem, RuleStatus};
use crate::i18n::messages;
use crate::models::{CustomContextCommand, DoubleClickMode, ExtensionConfig};
use eframe::egui::{self, Margin, RichText};
use kit::{primary_button, secondary_button, space, text_size, PillKind};

/// Height of a row of the rule list.
const ROW_HEIGHT: f32 = 36.0;
/// Size of the rule's icon in the detail header.
const ICON_SIZE: f32 = 48.0;
/// Width of the text boxes of the rule detail.
const FIELD_WIDTH: f32 = 260.0;
/// Launch command of a new "Run" rule.
const DEFAULT_LAUNCH_COMMAND: &str = "\"%1\" %*";
/// Command of a newly enabled "Run as administrator" entry.
const DEFAULT_ADMIN_COMMAND: &str = "cmd.exe /C \"%1\" %*";

/// Id of a row of the rule list (tests focus it).
fn rule_row_id(index: usize) -> egui::Id {
    egui::Id::new(("assoc_rule_row", index))
}

#[cfg(test)]
thread_local! {
    /// How often `rule_problems` ran on this thread (tests reset and read it).
    static RULE_PROBLEM_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Id of the extension box in Advanced options (tests aim at it).
fn advanced_extension_id() -> egui::Id {
    egui::Id::new("assoc_advanced_extension")
}

/// Short name of the code editor, from its path: `VS Code` for Code.exe,
/// else the file stem; `""` when no editor is set.
///
/// A UI copy of the engine's naming in `src/detector.rs` (the engine's
/// version returns the English "Editor" for an empty path, which the UI must
/// not show). Keep the two in sync when an editor is added or renamed.
fn editor_display_name(path: &str) -> String {
    let stem = std::path::Path::new(path.trim())
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match stem.to_lowercase().as_str() {
        "code" => "VS Code".to_string(),
        "code - insiders" => "VS Code Insiders".to_string(),
        _ => stem,
    }
}

fn secondary_line(ui: &mut egui::Ui, p: &kit::Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .size(text_size::SECONDARY)
            .color(p.text_secondary),
    );
}

/// A setting row whose control is a text box.
fn text_row(
    ui: &mut egui::Ui,
    title: &str,
    value: &mut String,
    id: Option<egui::Id>,
) -> egui::Response {
    kit::SettingRow {
        icon: None,
        title,
        description: None,
        hover: None,
    }
    .show(ui, |ui| {
        let mut edit = egui::TextEdit::singleline(value).desired_width(FIELD_WIDTH);
        if let Some(id) = id {
            edit = edit.id(id);
        }
        ui.add(edit)
    })
}

/// A setting row whose control is a toggle.
fn toggle_row(
    ui: &mut egui::Ui,
    title: &str,
    description: Option<&str>,
    on: &mut bool,
) -> egui::Response {
    kit::SettingRow {
        icon: None,
        title,
        description,
        hover: None,
    }
    .show(ui, |ui| kit::toggle(ui, on))
}

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
        // Validated once; the bar, the list and the detail all read this.
        let problems = self.rule_problems();
        // Before the panels, so the bar spans the whole page width.
        self.rule_action_bar(ctx, &problems);
        self.show_extension_list(ctx, &problems);
        self.show_extension_details(ctx, &problems);
    }

    /// The problems of all rules, with their indexes. The one place the page
    /// validates; called once per frame.
    pub(super) fn rule_problems(&self) -> Vec<(usize, RuleProblem)> {
        #[cfg(test)]
        RULE_PROBLEM_CALLS.with(|c| c.set(c.get() + 1));
        find_rule_problems(&self.config.extensions)
    }

    /// `Invalid` wins; otherwise the status read for the rule's extension.
    #[cfg(test)]
    pub(super) fn rule_state(&self, index: usize) -> RuleState {
        self.rule_state_in(index, &self.rule_problems())
    }

    /// Why the rule at `index` cannot be applied, translated.
    fn rule_problem_text(&self, index: usize, problems: &[(usize, RuleProblem)]) -> Option<String> {
        problems
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, problem)| messages::rule_problem(self.tr, problem))
    }

    /// The page's bar: the selected rule's state and its actions, or the
    /// job's progress while one runs.
    fn rule_action_bar(&mut self, ctx: &egui::Context, problems: &[(usize, RuleProblem)]) {
        let tr = self.tr;
        let index = self.selected_ext_index;
        let Some(rule) = self.config.extensions.get(index) else {
            self.page_action_bar(ctx, None, |_| {});
            return;
        };
        let extension = rule.extension.clone();
        let has_backup = self.backup_store.find(&extension).is_some();
        let status = match self.rule_state_in(index, problems) {
            RuleState::Checking => tr.t("assoc.bar.checking"),
            RuleState::NotApplied => tr.t("assoc.bar.not_applied"),
            RuleState::UpToDate => tr.t("assoc.bar.up_to_date"),
            RuleState::Changed => tr.t("assoc.bar.changed"),
            RuleState::Invalid => tr.t_args(
                "assoc.bar.invalid",
                &[&self.rule_problem_text(index, problems).unwrap_or_default()],
            ),
        };
        let disabled_reason = self.apply_rule_disabled_reason(index, problems);
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("assoc.apply");
        let restore_label = tr.t("assoc.restore");
        let remove_label = tr.t("assoc.remove");

        let (mut apply, mut restore, mut remove) = (false, false, false);
        // Right to left: Apply sits at the far right.
        self.page_action_bar(ctx, Some(&status), |ui| {
            let resp = ui.add_enabled(disabled_reason.is_none(), primary_button(p, &apply_label));
            let resp = match &disabled_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            apply = resp.clicked();
            if has_backup {
                restore = ui.add(secondary_button(&restore_label)).clicked();
            }
            remove = ui.add(secondary_button(&remove_label)).clicked();
        });

        if apply {
            let mut single = self.config.extensions[index].clone();
            single.enabled = true;
            self.start_apply(ctx, vec![single]);
        }
        if restore {
            self.request_restore_rule(&extension);
        }
        if remove {
            self.request_remove_rule(index);
        }
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
        let all_checked =
            !self.config.extensions.is_empty() && self.config.extensions.iter().all(|r| r.enabled);
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
            let resp = ui.add_enabled(disabled_reason.is_none(), primary_button(p, &apply_label));
            let resp = match &disabled_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            apply = resp.clicked();

            toggle = ui.add(secondary_button(&toggle_label)).clicked();
            refresh = ui
                .add_enabled(!busy, secondary_button(&refresh_label))
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

    fn show_extension_list(&mut self, ctx: &egui::Context, problems: &[(usize, RuleProblem)]) {
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
                        self.rule_rows(ctx, ui, problems);
                    });
                });
            });
    }

    /// The scrolling list of rules, or what to show instead of it.
    fn rule_rows(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        problems: &[(usize, RuleProblem)],
    ) {
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

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for idx in visible {
                    let state = self.rule_state_in(idx, problems);
                    let state_label = self.rule_state_label(state);
                    let hover = match state {
                        RuleState::Invalid => {
                            self.rule_problem_text(idx, problems).unwrap_or_default()
                        }
                        _ => state_label.clone(),
                    };
                    let tex = self
                        .gallery
                        .texture(ctx, &self.config.extensions[idx].icon_name);
                    let selected = self.selected_ext_index == idx;

                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        egui::Sense::hover(),
                    );
                    let row = ui.interact(rect, rule_row_id(idx), egui::Sense::click());
                    let row_label =
                        format!("{}, {}", self.config.extensions[idx].extension, state_label);
                    row.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            selected,
                            row_label.clone(),
                        )
                    });
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
                    // The ring goes over the fill, so it stays visible.
                    if row.has_focus() {
                        ui.painter().rect_stroke(
                            rect.shrink(1.0),
                            egui::Rounding::same(kit::radius::CONTROL),
                            egui::Stroke::new(2.0_f32, p.accent),
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

    fn show_extension_details(&mut self, ctx: &egui::Context, problems: &[(usize, RuleProblem)]) {
        Self::page_panel(ctx, |ui| {
            self.associations_header(ui);
            let index = self.selected_ext_index;
            if index >= self.config.extensions.len() {
                let text = self.tr.t("assoc.select");
                kit::empty_state(ui, kit::Icon::Associations, &text, None);
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.rule_header(ui, index, problems);
                    ui.add_space(space::L);
                    self.rule_sections(ui, index);
                });
        });
    }

    /// Icon, title, what a double-click does, the state and its details.
    fn rule_header(&self, ui: &mut egui::Ui, index: usize, problems: &[(usize, RuleProblem)]) {
        let tr = self.tr;
        let ctx = ui.ctx().clone();
        let p = kit::current_palette(&ctx);
        let rule = &self.config.extensions[index];
        let state = self.rule_state_in(index, problems);
        let editor = editor_display_name(&self.config.code_executable);
        let behavior = match rule.mode {
            DoubleClickMode::OpenInEditor if editor.is_empty() => tr.t("assoc.no_editor"),
            DoubleClickMode::OpenInEditor => tr.t_args("assoc.opens_with", &[&editor]),
            DoubleClickMode::ExecuteScript if editor.is_empty() => tr.t("assoc.runs_no_editor"),
            DoubleClickMode::ExecuteScript => tr.t_args("assoc.runs", &[&editor]),
        };
        let has_backup = self.backup_store.find(&rule.extension).is_some();
        let tex = self.gallery.texture(&ctx, &rule.icon_name);

        ui.horizontal_top(|ui| {
            let icon = egui::vec2(ICON_SIZE, ICON_SIZE);
            match tex {
                Some(t) => {
                    ui.image((t.id(), icon));
                }
                None => {
                    // Missing from the gallery: keep the slot.
                    ui.allocate_exact_size(icon, egui::Sense::hover());
                }
            }
            ui.add_space(space::M);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = space::XS;
                ui.label(
                    RichText::new(format!("{} · {}", rule.extension, rule.description))
                        .font(kit::fonts::semibold(&ctx, text_size::SECTION))
                        .color(p.text),
                );
                secondary_line(ui, p, &behavior);
                ui.horizontal(|ui| {
                    let kind = rule_state_pill(state).unwrap_or(PillKind::Neutral);
                    kit::pill(ui, &self.rule_state_label(state), kind);
                    if has_backup {
                        kit::pill(ui, &tr.t("assoc.original_saved"), PillKind::Neutral);
                    }
                });
                match state {
                    RuleState::Changed => {
                        if let Some(RuleStatus::Drifted(reasons)) =
                            self.rule_statuses.get(&rule.extension)
                        {
                            for reason in reasons {
                                secondary_line(ui, p, &messages::drift_reason(tr, reason));
                            }
                        }
                    }
                    RuleState::Invalid => {
                        if let Some(problem) = self.rule_problem_text(index, problems) {
                            ui.label(
                                RichText::new(problem)
                                    .size(text_size::SECONDARY)
                                    .color(p.danger),
                            );
                        }
                    }
                    RuleState::Checking | RuleState::NotApplied | RuleState::UpToDate => {}
                }
                if rule.extension.eq_ignore_ascii_case(".sh") {
                    let wsl = if self.wsl_detected {
                        tr.t("settings.wsl_on")
                    } else {
                        tr.t("settings.wsl_off")
                    };
                    secondary_line(ui, p, &wsl);
                }
            });
        });
    }

    /// The rule's settings: double-click, appearance, options, advanced.
    fn rule_sections(&mut self, ui: &mut egui::Ui, index: usize) {
        let tr = self.tr;
        let mut open_picker = false;
        let mut extension_edited = false;
        let rule = &mut self.config.extensions[index];

        kit::section(ui, &tr.t("assoc.double_click"), |ui| {
            ui.add_space(space::S);
            let mut run = rule.mode == DoubleClickMode::ExecuteScript;
            let editor_label = tr.t("assoc.mode.editor");
            let run_label = tr.t("assoc.mode.run");
            let options = [(false, editor_label.as_str()), (true, run_label.as_str())];
            if kit::segmented(ui, &mut run, &options).changed() {
                rule.mode = if run {
                    DoubleClickMode::ExecuteScript
                } else {
                    DoubleClickMode::OpenInEditor
                };
            }
            ui.add_space(space::S);
            if rule.mode == DoubleClickMode::ExecuteScript {
                let command = rule
                    .launcher_command
                    .get_or_insert_with(|| DEFAULT_LAUNCH_COMMAND.to_string());
                text_row(ui, &tr.t("assoc.launch_command"), command, None);
            }
        });

        kit::section(ui, &tr.t("assoc.appearance"), |ui| {
            let change = tr.t("assoc.change_icon");
            let description = (!rule.icon_name.is_empty()).then_some(rule.icon_name.as_str());
            kit::SettingRow {
                icon: None,
                title: &tr.t("assoc.icon"),
                description,
                hover: None,
            }
            .show(ui, |ui| {
                open_picker = ui.add(secondary_button(&change)).clicked();
            });
        });

        kit::section(ui, &tr.t("assoc.options"), |ui| {
            toggle_row(
                ui,
                &tr.t("assoc.clean_overrides"),
                Some(&tr.t("assoc.clean_overrides_desc")),
                &mut rule.reset_existing,
            );
            // Windows may still say "An app default was reset": the hint
            // covers the whole row.
            let hint = tr.t("assoc.ignore_open_with_hint");
            kit::SettingRow {
                icon: None,
                title: &tr.t("assoc.ignore_open_with"),
                description: None,
                hover: Some(&hint),
            }
            .show(ui, |ui| kit::toggle(ui, &mut rule.reset_user_choice));
        });

        kit::disclosure(ui, "assoc_advanced", &tr.t("assoc.advanced"), false, |ui| {
            extension_edited = text_row(
                ui,
                &tr.t("assoc.field.extension"),
                &mut rule.extension,
                Some(advanced_extension_id()),
            )
            .changed();
            text_row(ui, &tr.t("assoc.field.prog_id"), &mut rule.prog_id, None);
            text_row(
                ui,
                &tr.t("assoc.field.description"),
                &mut rule.description,
                None,
            );
            text_row(
                ui,
                &tr.t("assoc.field.perceived_type"),
                &mut rule.perceived_type,
                None,
            );
            text_row(
                ui,
                &tr.t("assoc.field.content_type"),
                &mut rule.content_type,
                None,
            );
            toggle_row(
                ui,
                &tr.t("assoc.preserve"),
                Some(&tr.t("assoc.preserve_desc")),
                &mut rule.preserve_existing_properties,
            );
            if rule.mode == DoubleClickMode::ExecuteScript {
                let mut elevated = rule.elevated_command.is_some();
                if toggle_row(ui, &tr.t("assoc.run_as_admin"), None, &mut elevated).changed() {
                    rule.elevated_command = elevated.then(|| DEFAULT_ADMIN_COMMAND.to_string());
                }
                if let Some(command) = &mut rule.elevated_command {
                    text_row(ui, &tr.t("assoc.admin_command"), command, None);
                }
            }
            verb_rows(ui, tr, rule);
        });

        if open_picker {
            self.open_icon_picker(index);
        }
        if extension_edited {
            // The status is keyed by extension: read it again for the new one.
            self.request_status_refresh();
        }
    }
}

/// The custom right-click verbs of a rule: add, edit and remove.
fn verb_rows(ui: &mut egui::Ui, tr: crate::i18n::Tr, rule: &mut ExtensionConfig) {
    let p = kit::current_palette(ui.ctx());
    ui.add_space(space::S);
    let add_label = tr.t("assoc.verb.add");
    let add = kit::SettingRow {
        icon: None,
        title: &tr.t("assoc.verbs"),
        description: None,
        hover: None,
    }
    .show(ui, |ui| ui.add(secondary_button(&add_label)).clicked());
    if add {
        rule.extra_verbs.push(CustomContextCommand {
            verb_key: "open_terminal".to_string(),
            menu_label: "Open in Windows Terminal".to_string(),
            command: "wt.exe -d \"%V\"".to_string(),
        });
    }
    if rule.extra_verbs.is_empty() {
        secondary_line(ui, p, &tr.t("assoc.verbs.none"));
    }

    let remove_label = tr.t("assoc.verb.remove");
    let mut remove = None;
    for (i, verb) in rule.extra_verbs.iter_mut().enumerate() {
        ui.add_space(space::S);
        kit::card(ui, |ui| {
            text_row(ui, &tr.t("assoc.verb.key"), &mut verb.verb_key, None);
            text_row(ui, &tr.t("assoc.verb.label"), &mut verb.menu_label, None);
            text_row(ui, &tr.t("assoc.verb.command"), &mut verb.command, None);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(secondary_button(&remove_label)).clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        rule.extra_verbs.remove(i);
    }
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

    fn has_two_px_ring(out: &egui::FullOutput) -> bool {
        ring_and_fill_order(out).0.is_some()
    }

    /// Paint-order indexes of the first 2 px stroke rect (the focus ring)
    /// and of the first row-height filled, stroke-less rect (the row fill).
    fn ring_and_fill_order(out: &egui::FullOutput) -> (Option<usize>, Option<usize>) {
        fn flat<'a>(shape: &'a egui::Shape, acc: &mut Vec<&'a egui::Shape>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| flat(s, acc)),
                s => acc.push(s),
            }
        }
        let mut all = Vec::new();
        out.shapes.iter().for_each(|c| flat(&c.shape, &mut all));
        let ring = all
            .iter()
            .position(|s| matches!(s, egui::Shape::Rect(r) if r.stroke.width == 2.0));
        let fill = all.iter().position(|s| {
            matches!(s, egui::Shape::Rect(r)
                if r.stroke.width == 0.0
                    && r.fill != egui::Color32::TRANSPARENT
                    && r.rect.height() >= ROW_HEIGHT - 0.5
                    && r.rect.height() <= ROW_HEIGHT + 0.5)
        });
        (ring, fill)
    }

    #[test]
    fn a_focused_rule_row_paints_a_focus_ring() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.active_tab = ActiveTab::Associations;

        frame(&mut app, &ctx, page_input(vec![]));
        let before = frame(&mut app, &ctx, page_input(vec![]));
        assert!(!has_two_px_ring(&before), "no ring while unfocused");

        ctx.memory_mut(|m| m.request_focus(rule_row_id(1)));
        frame(&mut app, &ctx, page_input(vec![]));
        let after = frame(&mut app, &ctx, page_input(vec![]));
        assert!(has_two_px_ring(&after), "a focused row paints a ring");

        // The selected row has a fill; the ring must be painted over it.
        app.selected_ext_index = 1;
        frame(&mut app, &ctx, page_input(vec![]));
        let selected = frame(&mut app, &ctx, page_input(vec![]));
        let (ring, fill) = ring_and_fill_order(&selected);
        let (ring, fill) = (ring.expect("ring"), fill.expect("row fill"));
        assert!(
            ring > fill,
            "ring ({ring}) must paint after the fill ({fill})"
        );
    }

    #[test]
    fn a_rule_row_is_labeled_with_its_extension_and_state() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".x y"]);
        app.active_tab = ActiveTab::Associations;
        app.rule_statuses.clear();
        app.rule_statuses
            .insert(".a".into(), crate::associations::RuleStatus::Applied);

        frame(&mut app, &ctx, page_input(vec![]));
        let up_to_date = app.tr.t("assoc.state.up_to_date");
        let invalid = app.tr.t("assoc.state.invalid");
        // Clicking a row reports its widget info.
        let mut info = |ext: &str| {
            let out = frame(&mut app, &ctx, page_input(vec![]));
            let pos = text_center(&out, ext).expect("row drawn");
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(
                &mut app,
                &ctx,
                page_input(vec![egui::Event::PointerMoved(pos)]),
            );
            frame(&mut app, &ctx, page_input(vec![button(true)]));
            let out = frame(&mut app, &ctx, page_input(vec![button(false)]));
            out.platform_output
                .events
                .iter()
                .find_map(|e| match e {
                    egui::output::OutputEvent::Clicked(i) => i.label.clone(),
                    _ => None,
                })
                .unwrap_or_default()
        };
        assert_eq!(info(".a"), format!(".a, {up_to_date}"));
        assert_eq!(info(".x y"), format!(".x y, {invalid}"));
    }

    #[test]
    fn rule_problems_are_computed_once_per_frame() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let rules: Vec<String> = (0..30).map(|i| format!(".r{i}")).collect();
        let refs: Vec<&str> = rules.iter().map(String::as_str).collect();
        let mut app = fx.app_with_rules(&ctx, &refs);
        app.config.extensions[3].extension = ".x y".into();
        app.active_tab = ActiveTab::Associations;
        app.selected_ext_index = 3;
        frame(&mut app, &ctx, page_input(vec![]));

        RULE_PROBLEM_CALLS.with(|c| c.set(0));
        frame(&mut app, &ctx, page_input(vec![]));

        assert_eq!(
            RULE_PROBLEM_CALLS.with(|c| c.get()),
            1,
            "the page validated the rules more than once in one frame"
        );
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

    /// Draws one frame `seconds` after the previous one, so the next click
    /// is not taken as the second half of a double-click.
    fn wait(app: &mut WinTuneApp, ctx: &egui::Context, seconds: f64) {
        let mut input = page_input(vec![]);
        input.time = Some(ctx.input(|i| i.time) + seconds);
        frame(app, ctx, input);
    }

    #[test]
    fn apply_in_the_bar_applies_only_this_rule() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".bar"]);
        app.active_tab = ActiveTab::Associations;
        app.config.extensions[0].enabled = false;
        app.selected_ext_index = 0;

        let apply = app.tr.t("assoc.apply");
        click_text(&mut app, &ctx, &apply);
        run_until_idle(&mut app, &ctx);

        assert!(fx.key_exists(&fx.classes(".foo")), "{}", app.last_notice());
        assert!(!fx.key_exists(&fx.classes(".bar")));
    }

    #[test]
    fn another_rules_problem_does_not_block_this_rules_apply() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo", ".x y"]);
        app.active_tab = ActiveTab::Associations;
        app.selected_ext_index = 0;
        // Control: "Apply all enabled" is blocked by `.x y`.
        assert!(app.apply_disabled_reason().is_some());
        assert_eq!(
            app.apply_rule_disabled_reason(0, &app.rule_problems()),
            None
        );

        let apply = app.tr.t("assoc.apply");
        click_text(&mut app, &ctx, &apply);
        assert!(app.is_busy(), "the bar's Apply was not enabled");
        run_until_idle(&mut app, &ctx);
        assert!(fx.key_exists(&fx.classes(".foo")), "{}", app.last_notice());
        // The apply toast is still up: it must not cover the bar.
        assert!(!app.toasts.is_empty());

        app.selected_ext_index = 1;
        let reason = app.tr.t("assoc.blocked.this_rule");
        assert_eq!(
            app.apply_rule_disabled_reason(1, &app.rule_problems()),
            Some(reason.clone())
        );
        let out = frame(&mut app, &ctx, page_input(vec![]));
        let at = text_center(&out, &apply).expect("Apply drawn");
        assert!(
            hover_texts_with(&mut app, &ctx, page_input, at).contains(&reason),
            "the disabled Apply does not say why"
        );
        click_text(&mut app, &ctx, &apply);
        assert!(!app.is_busy(), "a disabled Apply started a job");
    }

    #[test]
    fn restore_original_asks_first_and_is_hidden_without_a_snapshot() {
        use super::super::confirm::ConfirmAction;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.active_tab = ActiveTab::Associations;
        let restore = app.tr.t("assoc.restore");

        let out = frame(&mut app, &ctx, page_input(vec![]));
        assert!(!drawn_texts(&out).contains(&restore));

        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);
        assert!(fx.key_exists(&fx.classes(".foo")), "{}", app.last_notice());
        // The apply toast is still up: it must not cover the bar.
        assert!(!app.toasts.is_empty());

        click_text(&mut app, &ctx, &restore);
        assert_eq!(
            app.pending_confirm.as_ref().map(|c| c.action.clone()),
            Some(ConfirmAction::Rollback(vec![".foo".to_string()]))
        );
        assert!(!app.is_busy(), "restore ran without confirmation");
        assert!(fx.key_exists(&fx.classes(".foo")));
    }

    #[test]
    fn renaming_the_extension_requests_a_status_refresh() {
        use crate::associations::RuleStatus;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.active_tab = ActiveTab::Associations;
        app.rule_statuses.insert(".foo".into(), RuleStatus::Applied);
        assert_eq!(app.rule_state(0), RuleState::UpToDate);

        app.config.extensions[0].extension = ".foo2".into();
        frame(&mut app, &ctx, tall_input(vec![]));
        assert_eq!(app.rule_state(0), RuleState::Checking);

        // Open Advanced options, then type into its extension box.
        app.config.extensions[0].extension = ".foo".into();
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let advanced = app.tr.t("assoc.advanced");
        let at = text_center(&out, &advanced).expect("Advanced options drawn");
        click_at_with(&mut app, &ctx, tall_input, at);
        wait(&mut app, &ctx, 1.0);
        frame(&mut app, &ctx, tall_input(vec![]));
        let rect = ctx
            .read_response(advanced_extension_id())
            .expect("extension box drawn")
            .rect;
        click_at_with(&mut app, &ctx, tall_input, rect.center());
        assert!(!app.status_refresh_needed, "focusing is not an edit");

        frame(
            &mut app,
            &ctx,
            tall_input(vec![egui::Event::Text("2".into())]),
        );

        assert_eq!(app.config.extensions[0].extension, ".foo2");
        assert!(app.status_refresh_needed);
    }

    #[test]
    fn removing_the_last_rule_shows_the_select_empty_state() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.active_tab = ActiveTab::Associations;
        let select = app.tr.t("assoc.select");

        // The last row: the selection moves to a rule that still exists.
        app.selected_ext_index = 1;
        app.request_remove_rule(1);
        app.resolve_confirm(&ctx, true);
        let out = frame(&mut app, &ctx, page_input(vec![]));
        assert_eq!(app.selected_ext_index, 0);
        assert!(drawn_texts(&out).iter().any(|t| t.starts_with(".a ·")));

        // The only rule: nothing left to show.
        app.request_remove_rule(0);
        app.resolve_confirm(&ctx, true);
        let out = frame(&mut app, &ctx, page_input(vec![]));
        let texts = drawn_texts(&out);
        assert!(texts.contains(&select), "{texts:?}");
        assert!(!texts.contains(&app.tr.t("assoc.apply")));
    }

    #[test]
    fn while_a_job_runs_the_page_bar_shows_progress_and_no_actions() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.active_tab = ActiveTab::Associations;
        // A snapshot exists, so Restore original would be drawn when idle.
        let rules = app.config.extensions.clone();
        app.start_apply(&ctx, rules);
        run_until_idle(&mut app, &ctx);
        let idle = drawn_texts(&frame(&mut app, &ctx, page_input(vec![])));
        for key in ["assoc.apply", "assoc.remove", "assoc.restore"] {
            assert!(idle.contains(&app.tr.t(key)), "{key} not drawn when idle");
        }

        let release = hold_job(&mut app);
        let texts = drawn_texts(&frame(&mut app, &ctx, page_input(vec![])));

        let title = app.tr.t("job.build_pak");
        assert_eq!(
            texts.iter().filter(|t| **t == title).count(),
            1,
            "one action bar, with the job's title: {texts:?}"
        );
        for key in ["assoc.apply", "assoc.remove", "assoc.restore"] {
            assert!(!texts.contains(&app.tr.t(key)), "{key} drawn during a job");
        }
        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn the_ignore_open_with_hint_shows_over_the_row_title() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.active_tab = ActiveTab::Associations;
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        let title = app.tr.t("assoc.ignore_open_with");
        let at = text_center(&out, &title).expect("row title drawn");

        let hint = app.tr.t("assoc.ignore_open_with_hint");
        assert!(
            hover_texts_with(&mut app, &ctx, tall_input, at).contains(&hint),
            "no hint over the row title"
        );
    }

    #[test]
    fn the_bars_remove_asks_to_remove_the_selected_rule() {
        use super::super::confirm::ConfirmAction;
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".a", ".b"]);
        app.active_tab = ActiveTab::Associations;
        app.selected_ext_index = 1;

        let remove = app.tr.t("assoc.remove");
        click_text(&mut app, &ctx, &remove);

        assert_eq!(
            app.pending_confirm.as_ref().map(|c| c.action.clone()),
            Some(ConfirmAction::RemoveRule {
                index: 1,
                extension: ".b".to_string()
            })
        );
        assert_eq!(app.config.extensions.len(), 2, "removed without asking");
    }

    #[test]
    fn with_no_rules_the_header_offers_check_all_and_does_nothing() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[]);
        app.active_tab = ActiveTab::Associations;

        let texts = drawn_texts(&frame(&mut app, &ctx, page_input(vec![])));

        assert!(texts.contains(&app.tr.t("assoc.check_all")), "{texts:?}");
        assert!(!texts.contains(&app.tr.t("assoc.uncheck_all")));
    }

    #[test]
    fn run_mode_without_an_editor_has_no_dangling_text() {
        let fx = TempIcons::new(&["foo"]);
        let ctx = egui::Context::default();
        let mut app = fx.app_with_rules(&ctx, &[".foo"]);
        app.active_tab = ActiveTab::Associations;
        app.config.code_executable.clear();
        app.config.extensions[0].mode = DoubleClickMode::ExecuteScript;

        let texts = drawn_texts(&frame(&mut app, &ctx, page_input(vec![])));

        assert!(
            texts.contains(&app.tr.t("assoc.runs_no_editor")),
            "{texts:?}"
        );
    }

    #[test]
    fn editor_display_name_maps_code_to_vs_code() {
        assert_eq!(editor_display_name(r"C:\x\Code.exe"), "VS Code");
        assert_eq!(
            editor_display_name(r"C:\x\Code - Insiders.exe"),
            "VS Code Insiders"
        );
        assert_eq!(editor_display_name(r"C:\x\notepad++.exe"), "notepad++");
        assert_eq!(editor_display_name(""), "");
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
