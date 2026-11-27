//! The "Environment variables" page: the user's `Path` as a list of folders,
//! their other variables, and the machine ones (read only). Edits stay
//! pending until Apply. The page is built on top of the app wiring in
//! `environment_state.rs` and `jobs.rs`.

use super::environment_state::{EnvDialog, EnvPending, VarKey};
use super::{kit, polish, ActiveTab, WinTuneApp};
use crate::environment::{
    entry_problem, env_slot, is_path_name, kind_to_write, name_problem, names_after, value_problem,
    var_change, EntryInfo, EntryMark, EnvChange, EnvProblem, EnvScan, EnvVar, VarEdit, PATH_NAME,
    REG_EXPAND_SZ_KIND,
};
use crate::i18n::messages;
use eframe::egui::{self, RichText};
use kit::{primary_button, secondary_button, space, text_size, PillKind};

const DIALOG_WIDTH: f32 = 520.0;
/// Characters of a value shown on its row; the whole value is on hover.
const VALUE_CHARS: usize = 80;
/// Room the add line keeps for its two buttons.
const ADD_BUTTONS_WIDTH: f32 = 300.0;

/// What the user did on a `Path` row this frame.
enum PathAction {
    Up(usize),
    Down(usize),
    Edit(usize),
    Remove(usize),
}

/// What the user did on a variable row this frame.
enum VarAction {
    Edit,
    Delete,
    Restore,
}

/// How a variable row differs from the registry.
#[derive(Clone, PartialEq)]
enum VarPending {
    New,
    Change,
    /// With the wanted name.
    Rename(String),
    Delete,
}

#[derive(Clone)]
struct VarRow {
    key: VarKey,
    /// The current name, or the wanted name of a new variable.
    name: String,
    /// The name the edit dialog starts with: the wanted one.
    edit_name: String,
    /// The wanted value, else the current one (empty when not text).
    value: String,
    /// `None` for a variable that does not exist yet.
    scanned: Option<EnvVar>,
    pending: Option<VarPending>,
}

fn short(value: &str) -> String {
    if value.chars().count() > VALUE_CHARS {
        let head: String = value.chars().take(VALUE_CHARS).collect();
        format!("{head}\u{2026}")
    } else {
        value.to_string()
    }
}

/// `query` is trimmed and lower case; empty matches everything.
fn matches(query: &str, texts: &[&str]) -> bool {
    query.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(query))
}

fn note_line(ui: &mut egui::Ui, text: &str) {
    let p = kit::current_palette(ui.ctx());
    ui.label(
        RichText::new(text)
            .size(text_size::BODY)
            .color(p.text_secondary),
    );
}

fn field_id(name: &str) -> egui::Id {
    egui::Id::new(("env_dialog_field", name))
}

impl WinTuneApp {
    pub(super) fn show_environment_tab(&mut self, ctx: &egui::Context) {
        // Before the panel, so the bar spans the whole page width and the
        // progress of a running job shows.
        self.env_action_bar(ctx);
        let tr = self.tr;
        let name = tr.t("nav.environment");
        let subtitle = tr.t("env.subtitle");
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |_| {});
            let p = kit::current_palette(ui.ctx());
            ui.label(
                RichText::new(tr.t("env.note"))
                    .size(text_size::SECONDARY)
                    .color(p.text_secondary),
            );
            ui.add_space(space::S);
            self.env_search_box(ui);
            ui.add_space(space::S);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.env_sections(ui));
        });
    }

    /// Why Apply is disabled: an unreadable snapshot store, a running job, or
    /// no change to write.
    fn env_apply_disabled_reason(&self) -> Option<String> {
        self.env_blocked_reason()
            .or_else(|| self.busy_or_blocked(&None))
            .or_else(|| (self.env_pending_count() == 0).then(|| self.tr.t("env.nothing_to_apply")))
    }

    /// The page's bar: how much is not applied, Discard and Apply; or the
    /// job's progress while one runs.
    fn env_action_bar(&mut self, ctx: &egui::Context) {
        let tr = self.tr;
        let status = match self.env_pending_count() {
            0 => tr.t("env.up_to_date"),
            1 => tr.t("env.pending_one"),
            pending => tr.t_args("env.pending", &[&pending]),
        };
        let disabled_reason = self.env_apply_disabled_reason();
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("tweaks.apply");
        let discard_label = tr.t("tweaks.discard");
        let can_discard = self.env_pending != EnvPending::default();

        let (mut apply, mut discard) = (false, false);
        // Right to left: Apply sits at the far right.
        self.page_action_bar(ctx, Some(&status), |ui| {
            let resp = ui.add_enabled(disabled_reason.is_none(), primary_button(p, &apply_label));
            let resp = match &disabled_reason {
                Some(reason) => resp.on_disabled_hover_text(reason),
                None => resp,
            };
            apply = resp.clicked();
            if can_discard {
                discard = ui.add(secondary_button(&discard_label)).clicked();
            }
        });
        if apply {
            self.start_env_apply(ctx);
        }
        if discard {
            self.discard_env_changes();
        }
    }

    fn env_search_box(&mut self, ui: &mut egui::Ui) {
        let id = polish::search_box_id(ActiveTab::Environment).expect("search box");
        let mut search = std::mem::take(&mut self.env_search);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut search)
                .id(id)
                .hint_text(self.tr.t("env.search"))
                .desired_width(kit::SEARCH_WIDTH),
        );
        self.env_search = search;
        self.focus_search_if_requested(&resp);
    }

    /// The three sections, or the loading line, or the error banner.
    fn env_sections(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let Some(scan) = self.env_scan.clone() else {
            kit::card(ui, |ui| note_line(ui, &tr.t("env.scanning")));
            return;
        };
        let query = self.env_search.trim().to_lowercase();
        let mut shown = 0;
        if let Some(error) = &scan.user_error {
            let text = tr.t_args("env.unreadable", &[error]);
            kit::banner(ui, kit::BannerKind::Error, &text, None);
        } else {
            shown += self.env_path_section(ui, &scan, &query);
            shown += self.env_vars_section(ui, &scan, &query);
        }
        shown += self.env_system_section(ui, &scan, &query);
        if scan.user_error.is_none() && !query.is_empty() && shown == 0 {
            note_line(ui, &tr.t("env.no_match"));
        }
    }

    /// Your Path. Returns how many rows the search left.
    fn env_path_section(&mut self, ui: &mut egui::Ui, scan: &EnvScan, query: &str) -> usize {
        let tr = self.tr;
        let infos = self.env_path_infos();
        let edit = self.env_pending.path.clone();
        let removed = edit.as_ref().map(|e| e.removed()).unwrap_or_default();
        let path_var = scan.user_path();
        let not_text = path_var.is_some_and(|v| !v.is_text());
        let has_snapshot = self
            .env_store
            .snapshots
            .find_id(&self.env.user_label, &env_slot(PATH_NAME).id())
            .is_some();
        let busy = self.busy_or_blocked(&None);
        let flagged = infos.iter().any(|i| i.missing || i.duplicate);
        let (clean_label, restore_label) =
            (tr.t("env.path.clean"), tr.t("tweaks.restore_original"));

        let (mut clean, mut restore) = (false, false);
        let mut actions = Vec::new();
        let mut added = None;
        let shown = kit::section_with_action(
            ui,
            &tr.t("env.path.title"),
            |ui| {
                // Right to left: Remove missing and duplicates at the far right.
                if !not_text {
                    let reason = busy
                        .clone()
                        .or_else(|| (!flagged).then(|| tr.t("env.path.clean_none")));
                    let resp = ui.add_enabled(reason.is_none(), secondary_button(&clean_label));
                    clean = match &reason {
                        Some(reason) => resp.on_disabled_hover_text(reason).clicked(),
                        None => resp.clicked(),
                    };
                }
                if has_snapshot {
                    let resp = ui.add_enabled(busy.is_none(), secondary_button(&restore_label));
                    restore = match &busy {
                        Some(reason) => resp.on_disabled_hover_text(reason).clicked(),
                        None => resp.clicked(),
                    };
                }
            },
            |ui| {
                if not_text {
                    kit::SettingRow {
                        icon: None,
                        title: PATH_NAME,
                        description: None,
                        hover: Some(&tr.t("env.not_text")),
                    }
                    .show(ui, |ui| {
                        kit::pill(ui, &tr.t("env.pill.read_only"), PillKind::Neutral);
                    });
                    return 1;
                }
                let mut shown = 0;
                if path_var.is_none() && edit.is_none() {
                    kit::empty_state(ui, kit::Icon::Environment, &tr.t("env.path.empty"), None);
                }
                let last = infos.len().saturating_sub(1);
                for (i, info) in infos.iter().enumerate() {
                    if !matches(query, &[&info.text, &info.expanded]) {
                        continue;
                    }
                    shown += 1;
                    let mark = edit.as_ref().map_or(EntryMark::Same, |e| e.mark(i));
                    if let Some(action) =
                        self.env_path_row(ui, i, last, info, mark, busy.as_deref())
                    {
                        actions.push(action);
                    }
                }
                for text in removed.iter().filter(|t| matches(query, &[t])) {
                    shown += 1;
                    kit::SettingRow {
                        icon: None,
                        title: text,
                        description: None,
                        hover: None,
                    }
                    .show(ui, |ui| {
                        kit::pill(ui, &tr.t("env.pill.will_remove"), PillKind::Accent);
                    });
                }
                ui.add_space(space::S);
                added = self.env_add_line(ui, busy.as_deref());
                shown
            },
        );

        if clean {
            // From the last entry, so the indexes still point at the flagged
            // ones; the entries keep their origin and show as removed.
            if let Some(edit) = self.env_path_edit_mut() {
                for (i, info) in infos.iter().enumerate().rev() {
                    if info.missing || info.duplicate {
                        edit.remove(i);
                    }
                }
            }
        }
        if restore {
            self.request_restore_environment(vec![env_slot(PATH_NAME).id()]);
        }
        if let Some(entry) = added {
            if let Some(edit) = self.env_path_edit_mut() {
                edit.add(entry);
            }
        }
        for action in actions {
            match action {
                PathAction::Edit(i) => {
                    self.env_dialog = Some(EnvDialog::Entry {
                        index: Some(i),
                        text: infos[i].text.clone(),
                    });
                }
                PathAction::Up(i) => {
                    if let Some(edit) = self.env_path_edit_mut() {
                        edit.move_up(i);
                    }
                }
                PathAction::Down(i) => {
                    if let Some(edit) = self.env_path_edit_mut() {
                        edit.move_down(i);
                    }
                }
                PathAction::Remove(i) => {
                    if let Some(edit) = self.env_path_edit_mut() {
                        edit.remove(i);
                    }
                }
            }
        }
        shown
    }

    /// One wanted `Path` entry: the folder, its pills and its `…` menu.
    fn env_path_row(
        &self,
        ui: &mut egui::Ui,
        i: usize,
        last: usize,
        info: &EntryInfo,
        mark: EntryMark,
        busy: Option<&str>,
    ) -> Option<PathAction> {
        let tr = self.tr;
        let (up, down, edit, remove) = (
            tr.t("env.move_up"),
            tr.t("env.move_down"),
            tr.t("env.edit"),
            tr.t("env.remove"),
        );
        let no_move = tr.t("env.no_move");
        let hint = |at_end: bool| busy.or(at_end.then_some(no_move.as_str()));
        let items = [
            kit::MenuItem {
                label: &up,
                enabled: busy.is_none() && i > 0,
                disabled_hint: hint(i == 0),
            },
            kit::MenuItem {
                label: &down,
                enabled: busy.is_none() && i < last,
                disabled_hint: hint(i >= last),
            },
            kit::MenuItem {
                label: &edit,
                enabled: busy.is_none(),
                disabled_hint: busy,
            },
            kit::MenuItem {
                label: &remove,
                enabled: busy.is_none(),
                disabled_hint: busy,
            },
        ];
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&info.text]);

        let mut action = None;
        kit::SettingRow {
            icon: None,
            title: &info.text,
            description: None,
            hover: (info.expanded != info.text).then_some(info.expanded.as_str()),
        }
        .show(ui, |ui| {
            let salt = format!("env_path_{i}");
            match kit::overflow_menu(ui, &salt, &more, &more_for, &items) {
                Some(0) => action = Some(PathAction::Up(i)),
                Some(1) => action = Some(PathAction::Down(i)),
                Some(2) => action = Some(PathAction::Edit(i)),
                Some(3) => action = Some(PathAction::Remove(i)),
                _ => {}
            }
            if info.missing {
                kit::pill(ui, &tr.t("env.pill.not_found"), PillKind::Warning);
            }
            if info.duplicate {
                kit::pill(ui, &tr.t("env.pill.duplicate"), PillKind::Warning);
            }
            match mark {
                EntryMark::Added => {
                    kit::pill(ui, &tr.t("env.pill.will_add"), PillKind::Accent);
                }
                EntryMark::Moved => {
                    kit::pill(ui, &tr.t("env.pill.moved"), PillKind::Accent);
                }
                EntryMark::Same => {}
            }
        });
        action
    }

    /// The field, Browse and Add under the list. Returns the folder to add
    /// when the user chose Add. While a job runs (`busy` is why) nothing is
    /// added: the job's end would drop the edit.
    fn env_add_line(&mut self, ui: &mut egui::Ui, busy: Option<&str>) -> Option<String> {
        let tr = self.tr;
        let p = kit::current_palette(ui.ctx());
        let mut text = std::mem::take(&mut self.env_new_entry);
        let problem = busy
            .map(str::to_string)
            .or_else(|| entry_problem(&text).map(|p| messages::env_problem(tr, &p)));
        let (browse_label, add_label) = (tr.t("env.browse"), tr.t("env.path.add"));
        let (mut browse, mut add) = (false, false);
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text(tr.t("env.path.add_hint"))
                    .desired_width((ui.available_width() - ADD_BUTTONS_WIDTH).max(160.0)),
            );
            browse = ui.add(secondary_button(&browse_label)).clicked();
            let resp = ui.add_enabled(problem.is_none(), primary_button(p, &add_label));
            add = match &problem {
                Some(reason) => resp.on_disabled_hover_text(reason).clicked(),
                None => resp.clicked(),
            };
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            add |= problem.is_none() && enter;
        });
        if browse {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                text = folder.display().to_string();
            }
        }
        if add {
            return Some(text.trim().to_string());
        }
        self.env_new_entry = text;
        None
    }

    /// The scanned user variables except `Path`, and the added ones, with
    /// what is pending for each, sorted by lower case name.
    fn env_var_rows(&self, scan: &EnvScan) -> Vec<VarRow> {
        let mut rows = Vec::new();
        for var in scan.user.iter().filter(|v| !v.is_path()) {
            let key = VarKey::Existing(var.name.to_ascii_lowercase());
            let edit = self.env_pending.vars.get(&key);
            let change = edit.and_then(|e| var_change(e, Some(var)));
            let pending = change.as_ref().map(|c| match c {
                EnvChange::Rename { to, .. } => VarPending::Rename(to.clone()),
                EnvChange::Delete { .. } => VarPending::Delete,
                _ => VarPending::Change,
            });
            let wanted = change
                .is_some()
                .then(|| edit.and_then(|e| e.wanted.clone()))
                .flatten();
            let (edit_name, value) = match wanted {
                Some((name, value)) => (name, value),
                None => (var.name.clone(), var.value.clone().unwrap_or_default()),
            };
            rows.push(VarRow {
                key,
                name: var.name.clone(),
                edit_name,
                value,
                scanned: Some(var.clone()),
                pending,
            });
        }
        for (key, edit) in &self.env_pending.vars {
            if let (VarKey::New(_), Some(EnvChange::Add { name, value })) =
                (key, var_change(edit, None))
            {
                rows.push(VarRow {
                    key: key.clone(),
                    name: name.clone(),
                    edit_name: name,
                    value,
                    scanned: None,
                    pending: Some(VarPending::New),
                });
            }
        }
        rows.sort_by_key(|r| r.name.to_lowercase());
        rows
    }

    /// Your variables. Returns how many rows the search left.
    fn env_vars_section(&mut self, ui: &mut egui::Ui, scan: &EnvScan, query: &str) -> usize {
        let tr = self.tr;
        let all = self.env_var_rows(scan);
        let none_at_all = all.is_empty();
        let rows: Vec<VarRow> = all
            .into_iter()
            .filter(|r| matches(query, &[&r.name, &r.edit_name, &r.value]))
            .collect();
        if !query.is_empty() && rows.is_empty() {
            return 0;
        }
        let busy = self.busy_or_blocked(&None);
        let add_label = tr.t("env.vars.add");
        let mut add = false;
        let mut actions = Vec::new();
        kit::section_with_action(
            ui,
            &tr.t("env.vars.title"),
            |ui| {
                let resp = ui.add_enabled(busy.is_none(), secondary_button(&add_label));
                add = match &busy {
                    Some(reason) => resp.on_disabled_hover_text(reason).clicked(),
                    None => resp.clicked(),
                };
            },
            |ui| {
                if none_at_all {
                    note_line(ui, &tr.t("env.vars.empty"));
                }
                for row in &rows {
                    if let Some(action) = self.env_var_row(ui, row, busy.as_deref()) {
                        actions.push((row.clone(), action));
                    }
                }
            },
        );

        if add {
            self.env_dialog = Some(EnvDialog::Variable {
                key: None,
                name: String::new(),
                value: String::new(),
            });
        }
        for (row, action) in actions {
            match action {
                VarAction::Edit => {
                    self.env_dialog = Some(EnvDialog::Variable {
                        key: Some(row.key),
                        name: row.edit_name,
                        value: row.value,
                    });
                }
                VarAction::Delete => match (&row.key, &row.scanned) {
                    (VarKey::Existing(_), Some(var)) => {
                        self.env_pending.vars.insert(
                            row.key.clone(),
                            VarEdit {
                                original: Some(var.name.clone()),
                                wanted: None,
                            },
                        );
                    }
                    _ => {
                        self.env_pending.vars.remove(&row.key);
                    }
                },
                VarAction::Restore => {
                    if let Some(var) = &row.scanned {
                        self.request_restore_environment(vec![var.slot().id()]);
                    }
                }
            }
        }
        rows.len()
    }

    /// One user variable: name, cut value, pills and `…` menu.
    fn env_var_row(
        &self,
        ui: &mut egui::Ui,
        row: &VarRow,
        busy: Option<&str>,
    ) -> Option<VarAction> {
        let tr = self.tr;
        let text = row.scanned.as_ref().is_none_or(EnvVar::is_text);
        let not_text = tr.t("env.not_text");
        let hint = busy.or((!text).then_some(not_text.as_str()));
        let usable = busy.is_none() && text;
        let (edit, delete, restore) = (
            tr.t("env.edit"),
            tr.t("env.delete"),
            tr.t("tweaks.restore_original"),
        );
        let mut items = vec![
            kit::MenuItem {
                label: &edit,
                enabled: usable,
                disabled_hint: hint,
            },
            kit::MenuItem {
                label: &delete,
                enabled: usable,
                disabled_hint: hint,
            },
        ];
        let mut actions = vec![VarAction::Edit, VarAction::Delete];
        if row.scanned.as_ref().is_some_and(|v| v.has_snapshot) {
            items.push(kit::MenuItem {
                label: &restore,
                enabled: busy.is_none(),
                disabled_hint: busy,
            });
            actions.push(VarAction::Restore);
        }
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&row.name]);
        let cut = short(&row.value);
        // A value nobody edits is what the registry holds; an edited or new
        // one gets the type the write will give it.
        let uses_refs = match (&row.scanned, &row.pending) {
            (Some(var), None | Some(VarPending::Delete)) => var.uses_references(),
            _ => {
                text && kind_to_write(
                    row.scanned.as_ref().map(|v| v.kind),
                    &row.edit_name,
                    &row.value,
                ) == REG_EXPAND_SZ_KIND
            }
        };

        let mut chosen = None;
        kit::SettingRow {
            icon: None,
            title: &row.name,
            description: (!cut.is_empty()).then_some(cut.as_str()),
            hover: (!row.value.is_empty()).then_some(row.value.as_str()),
        }
        .show(ui, |ui| {
            let salt = format!("env_var_{}", row.name.to_lowercase());
            if let Some(i) = kit::overflow_menu(ui, &salt, &more, &more_for, &items) {
                chosen = actions.into_iter().nth(i);
            }
            if uses_refs {
                kit::pill(ui, &tr.t("env.pill.uses_refs"), PillKind::Neutral);
            }
            if !text {
                kit::pill(ui, &tr.t("env.pill.read_only"), PillKind::Neutral);
            }
            match &row.pending {
                Some(VarPending::New) => {
                    kit::pill(ui, &tr.t("env.pill.will_add"), PillKind::Accent);
                }
                Some(VarPending::Change) => {
                    kit::pill(ui, &tr.t("env.pill.will_change"), PillKind::Accent);
                }
                Some(VarPending::Rename(to)) => {
                    let label = tr.t_args("env.pill.will_rename", &[to]);
                    kit::pill(ui, &label, PillKind::Accent);
                }
                Some(VarPending::Delete) => {
                    kit::pill(ui, &tr.t("env.pill.will_delete"), PillKind::Accent);
                }
                None => {}
            }
        });
        chosen
    }

    /// For every account: a collapsed, read only block. Returns how many rows
    /// the search left (whether or not the block is open).
    fn env_system_section(&mut self, ui: &mut egui::Ui, scan: &EnvScan, query: &str) -> usize {
        let tr = self.tr;
        let resolver = self.env_resolver();
        let vars: Vec<&EnvVar> = scan
            .system
            .iter()
            .filter(|v| !v.is_path())
            .filter(|v| matches(query, &[&v.name, v.value.as_deref().unwrap_or("")]))
            .collect();
        let entries: Vec<(String, String)> = scan
            .system_path_entries()
            .into_iter()
            .map(|text| {
                let expanded = resolver.expand(&text).text;
                (text, expanded)
            })
            .filter(|(text, expanded)| matches(query, &[text, expanded]))
            .collect();
        let found = vars.len() + entries.len();
        if !query.is_empty() && found == 0 {
            return 0;
        }

        kit::disclosure(ui, "env_system", &tr.t("env.system.title"), false, |ui| {
            ui.add_space(space::S);
            kit::card(ui, |ui| {
                if let Some(error) = &scan.system_error {
                    let text = tr.t_args("env.system.unreadable", &[error]);
                    kit::banner(ui, kit::BannerKind::Error, &text, None);
                }
                for var in &vars {
                    let value = var.value.clone().unwrap_or_default();
                    let cut = short(&value);
                    kit::SettingRow {
                        icon: None,
                        title: &var.name,
                        description: (!cut.is_empty()).then_some(cut.as_str()),
                        hover: (!value.is_empty()).then_some(value.as_str()),
                    }
                    .show(ui, |_| {});
                }
                if !entries.is_empty() {
                    kit::SettingRow {
                        icon: None,
                        title: &tr.t("env.system.path"),
                        description: None,
                        hover: None,
                    }
                    .show(ui, |_| {});
                }
                for (text, expanded) in &entries {
                    kit::SettingRow {
                        icon: None,
                        title: text,
                        description: None,
                        hover: (expanded != text).then_some(expanded.as_str()),
                    }
                    .show(ui, |_| {});
                }
            });
        });
        ui.add_space(space::L);
        found
    }

    /// Why the variable dialog's fields cannot be used, if they cannot: the
    /// first of a bad name, `Path`, a name another variable has or will have,
    /// and a bad value. The variable being edited does not count as another
    /// one, so its own name (in any case) is free for it.
    fn env_variable_problem(
        &self,
        key: Option<&VarKey>,
        name: &str,
        value: &str,
    ) -> Option<EnvProblem> {
        if let Some(problem) = name_problem(name) {
            return Some(problem);
        }
        let name = name.trim();
        if is_path_name(name) {
            return Some(EnvProblem::PathName);
        }
        if let Some(scan) = &self.env_scan {
            let own = match key {
                Some(VarKey::Existing(lower)) => scan.user_var(lower).map(|v| v.name.as_str()),
                _ => None,
            };
            let others: Vec<EnvVar> = scan
                .user
                .iter()
                .filter(|v| !own.is_some_and(|o| v.name.eq_ignore_ascii_case(o)))
                .cloned()
                .collect();
            // A variable that only a pending delete or rename frees stays
            // taken: the user edits that one instead.
            let taken = others.iter().any(|v| v.name.eq_ignore_ascii_case(name))
                || names_after(&others, &self.env_changes_except(key))
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(name));
            if taken {
                return Some(EnvProblem::NameTaken(name.to_string()));
            }
        }
        value_problem(value)
    }

    /// The variable or folder dialog. Escape cancels; Enter in the last field
    /// accepts when allowed.
    pub(super) fn show_env_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.env_dialog.take() else {
            return;
        };
        let tr = self.tr;
        let title = match &dialog {
            EnvDialog::Variable { key: None, .. } => tr.t("env.var_title_add"),
            EnvDialog::Variable { .. } => tr.t("env.var_title_edit"),
            EnvDialog::Entry { .. } => tr.t("env.entry_title"),
        };
        let (ok_label, cancel_label) = (tr.t("env.dialog_ok"), tr.t("dialog.cancel"));
        let (mut ok, mut cancel) = (false, false);
        let this = &*self;
        kit::dialog(ctx, "env_dialog", &title, DIALOG_WIDTH, |ui| {
            let p = kit::current_palette(ui.ctx());
            let label = |ui: &mut egui::Ui, key: &str| {
                ui.label(
                    RichText::new(tr.t(key))
                        .size(text_size::SECONDARY)
                        .color(p.text_secondary),
                );
            };
            let width = ui.available_width();
            let (problem, enter) = match &mut dialog {
                EnvDialog::Variable { key, name, value } => {
                    label(ui, "env.name");
                    let first = ui.add(
                        egui::TextEdit::singleline(name)
                            .id(field_id("name"))
                            .desired_width(width),
                    );
                    // Typing goes to the name until another dialog widget is
                    // focused.
                    if ui.memory(|m| m.focused().is_none()) {
                        first.request_focus();
                    }
                    ui.add_space(space::S);
                    label(ui, "env.value");
                    let last = ui.add(
                        egui::TextEdit::singleline(value)
                            .id(field_id("value"))
                            .desired_width(width),
                    );
                    let problem = this.env_variable_problem(key.as_ref(), name, value);
                    (problem, last.lost_focus())
                }
                EnvDialog::Entry { text, .. } => {
                    let field = ui.add(
                        egui::TextEdit::singleline(text)
                            .id(field_id("entry"))
                            .desired_width(width),
                    );
                    if ui.memory(|m| m.focused().is_none()) {
                        field.request_focus();
                    }
                    (entry_problem(text), field.lost_focus())
                }
            };
            // A running job would drop the edit when it ends.
            let problem = this
                .busy_or_blocked(&None)
                .or_else(|| problem.map(|p| messages::env_problem(tr, &p)));
            if let Some(problem) = &problem {
                ui.label(
                    RichText::new(problem)
                        .size(text_size::SECONDARY)
                        .color(p.danger),
                );
            }
            ui.add_space(space::L);
            ui.horizontal(|ui| {
                let allowed = problem.is_none();
                ok = ui
                    .add_enabled(allowed, primary_button(p, &ok_label))
                    .clicked();
                cancel = ui.add(secondary_button(&cancel_label)).clicked();
                ok |= allowed && enter && ui.input(|i| i.key_pressed(egui::Key::Enter));
            });
        });
        cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if ok {
            self.finish_env_dialog(dialog);
        } else if !cancel {
            self.env_dialog = Some(dialog);
        }
    }

    /// Stores what the accepted dialog chose as a pending edit.
    fn finish_env_dialog(&mut self, dialog: EnvDialog) {
        match dialog {
            EnvDialog::Variable { key, name, value } => {
                let name = name.trim().to_string();
                let new_key = VarKey::New(name.to_ascii_lowercase());
                let wanted = Some((name, value));
                match key {
                    Some(key @ VarKey::Existing(_)) => {
                        let VarKey::Existing(lower) = &key else {
                            return;
                        };
                        let Some(current) = self
                            .env_scan
                            .as_ref()
                            .and_then(|s| s.user_var(lower))
                            .cloned()
                        else {
                            return;
                        };
                        let edit = VarEdit {
                            original: Some(current.name.clone()),
                            wanted,
                        };
                        // Back to what the registry has is no edit at all.
                        if var_change(&edit, Some(&current)).is_none() {
                            self.env_pending.vars.remove(&key);
                        } else {
                            self.env_pending.vars.insert(key, edit);
                        }
                    }
                    other => {
                        if let Some(old) = other {
                            self.env_pending.vars.remove(&old);
                        }
                        let edit = VarEdit {
                            original: None,
                            wanted,
                        };
                        self.env_pending.vars.insert(new_key, edit);
                    }
                }
            }
            EnvDialog::Entry { index, text } => {
                let current = match (&self.env_pending.path, &self.env_scan) {
                    (Some(edit), _) => edit.texts(),
                    (None, Some(scan)) => scan.user_path_entries(),
                    (None, None) => Vec::new(),
                };
                // OK on a folder nobody changed is no edit, and keeps its
                // spaces; a changed one is trimmed.
                if index.is_some_and(|i| current.get(i) == Some(&text)) {
                    return;
                }
                let text = text.trim().to_string();
                if let Some(edit) = self.env_path_edit_mut() {
                    match index {
                        Some(i) => edit.edit(i, text),
                        None => edit.add(text),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::confirm::ConfirmAction;
    use super::super::environment_state::{EnvDialog, VarKey};
    use super::super::test_support::*;
    use super::super::{kit, ActiveTab, WinTuneApp};
    use crate::environment::{env_slot, EnvScan, VarEdit};
    use crate::i18n::messages;
    use egui::{Event, Key, Modifiers};

    /// The Environment variables page, its first scan finished.
    fn page_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.request_env_scan();
        run_until_env_scanned(&mut app, ctx);
        app.active_tab = ActiveTab::Environment;
        app
    }

    /// Two frames, so the layout has settled; the texts of the second.
    fn texts(app: &mut WinTuneApp, ctx: &egui::Context) -> Vec<String> {
        frame(app, ctx, tall_input(vec![]));
        drawn_texts(&frame(app, ctx, tall_input(vec![])))
    }

    fn count(shown: &[String], text: &str) -> usize {
        shown.iter().filter(|t| *t == text).count()
    }

    fn pos_of(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        text_center(&out, text)
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)))
    }

    fn pos_of_key(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) -> egui::Pos2 {
        let text = app.tr.t(key);
        pos_of(app, ctx, &text)
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        let pos = pos_of(app, ctx, text);
        click_at(app, ctx, pos);
    }

    fn click_key(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) {
        let text = app.tr.t(key);
        click_text(app, ctx, &text);
    }

    /// Center of the widget of exactly `size` on the row titled `title`, if
    /// the row has one.
    fn find_row_widget(
        app: &mut WinTuneApp,
        ctx: &egui::Context,
        title: &str,
        size: egui::Vec2,
    ) -> Option<egui::Pos2> {
        let at = pos_of(app, ctx, title);
        ctx.viewport_for(egui::ViewportId::ROOT, |v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, widgets)| widgets.iter())
                .filter(|w| w.rect.size() == size && w.rect.center().x > at.x)
                .map(|w| w.rect.center())
                .filter(|c| c.y > at.y - 8.0 && c.y - at.y < 48.0)
                .min_by(|a, b| (a.y - at.y).total_cmp(&(b.y - at.y)))
        })
    }

    fn row_widget(
        app: &mut WinTuneApp,
        ctx: &egui::Context,
        title: &str,
        size: egui::Vec2,
    ) -> egui::Pos2 {
        find_row_widget(app, ctx, title, size)
            .unwrap_or_else(|| panic!("no widget of {size:?} on row {title:?}"))
    }

    /// Opens the `...` menu of the row titled `title`, any other closed first.
    fn open_menu(app: &mut WinTuneApp, ctx: &egui::Context, title: &str) {
        ctx.memory_mut(|m| m.close_popup());
        let more = row_widget(app, ctx, title, kit::OVERFLOW_BUTTON);
        click_at(app, ctx, more);
    }

    /// Where the open menu draws the item `key`: the last such text, since a
    /// popup paints over the page.
    fn menu_item(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) -> egui::Pos2 {
        let text = app.tr.t(key);
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        text_centers(&out, &text)
            .pop()
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)))
    }

    fn click_menu_item(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) {
        let at = menu_item(app, ctx, key);
        click_at(app, ctx, at);
    }

    fn dir(fx: &TempIcons) -> String {
        fx.dir.display().to_string()
    }

    /// The folder `<dir>\tools`, as `TOOLS` and as the first of the entries
    /// `tools;%TOOLS%;C:\Nowhere\x;%UNKNOWN%\y`.
    fn seed_pills_path(fx: &TempIcons) -> String {
        let tools = format!(r"{}\tools", dir(fx));
        std::fs::create_dir_all(&tools).unwrap();
        fx.seed_env(false, "TOOLS", 1, &tools);
        fx.seed_env(
            false,
            "Path",
            2,
            &format!(r"{tools};%TOOLS%;C:\Nowhere\x;%UNKNOWN%\y"),
        );
        tools
    }

    fn set_variable_dialog(app: &mut WinTuneApp, new_name: &str, new_value: &str) {
        match &mut app.env_dialog {
            Some(EnvDialog::Variable { name, value, .. }) => {
                *name = new_name.into();
                *value = new_value.into();
            }
            other => panic!("no variable dialog: {other:?}"),
        }
    }

    fn open_add_dialog(app: &mut WinTuneApp) {
        app.env_dialog = Some(EnvDialog::Variable {
            key: None,
            name: String::new(),
            value: String::new(),
        });
    }

    fn apply_and_accept(app: &mut WinTuneApp, ctx: &egui::Context) {
        click_key(app, ctx, "tweaks.apply");
        if app.pending_confirm.is_some() {
            app.resolve_confirm(ctx, true);
        }
        run_until_idle(app, ctx);
    }

    #[test]
    fn the_path_section_lists_entries_with_their_pills() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let tools = seed_pills_path(&fx);
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);

        for want in [
            tools.clone(),
            "%TOOLS%".to_string(),
            r"C:\Nowhere\x".to_string(),
            r"%UNKNOWN%\y".to_string(),
            app.tr.t("env.path.title"),
            app.tr.t("env.vars.title"),
            app.tr.t("env.up_to_date"),
        ] {
            assert!(shown.contains(&want), "{want:?} not drawn: {shown:?}");
        }
        assert_eq!(count(&shown, &app.tr.t("env.pill.duplicate")), 1);
        assert_eq!(count(&shown, &app.tr.t("env.pill.not_found")), 1);
        // The folder's row, and the value of `TOOLS`.
        assert_eq!(count(&shown, &tools), 2);

        let at = pos_of(&mut app, &ctx, "%TOOLS%");
        let hover = hover_texts(&mut app, &ctx, at);
        assert_eq!(count(&hover, &tools), 3, "no expanded form on hover");
    }

    #[test]
    fn a_duplicate_of_the_system_path_is_flagged() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let tools = format!(r"{}\tools", dir(&fx));
        std::fs::create_dir_all(&tools).unwrap();
        fx.seed_env(true, "Path", 2, &tools);
        fx.seed_env(false, "Path", 2, &format!(r"{}\TOOLS\", dir(&fx)));
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);

        assert_eq!(
            count(&shown, &app.tr.t("env.pill.duplicate")),
            1,
            "{shown:?}"
        );
    }

    #[test]
    fn moving_an_entry_marks_it_and_apply_writes_the_new_order() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "TOOLS", 1, &d);
        fx.seed_env(false, "Path", 2, &format!(r"{d}\a;%TOOLS%\b"));
        let mut app = page_app(&fx, &ctx);

        open_menu(&mut app, &ctx, r"%TOOLS%\b");
        click_menu_item(&mut app, &ctx, "env.move_up");

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.pill.moved")), "{shown:?}");
        assert!(shown.contains(&app.tr.t("env.pending_one")));
        apply_and_accept(&mut app, &ctx);

        assert_eq!(
            fx.user_env("Path"),
            Some((2, format!(r"%TOOLS%\b;{d}\a"))),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn the_first_and_last_rows_cannot_move_further() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "Path", 2, &format!(r"{d}\a;{d}\b"));
        let mut app = page_app(&fx, &ctx);

        open_menu(&mut app, &ctx, &format!(r"{d}\a"));
        click_menu_item(&mut app, &ctx, "env.move_up");
        assert!(app.env_changes().is_empty(), "the first row moved up");
        assert_eq!(app.env_pending.path, None);
        open_menu(&mut app, &ctx, &format!(r"{d}\a"));
        let at = menu_item(&mut app, &ctx, "env.move_up");
        let hover = hover_texts(&mut app, &ctx, at);
        assert!(hover.contains(&app.tr.t("env.no_move")), "{hover:?}");

        open_menu(&mut app, &ctx, &format!(r"{d}\b"));
        click_menu_item(&mut app, &ctx, "env.move_down");
        assert_eq!(app.env_pending.path, None, "the last row moved down");
        // The middle moves are enabled.
        open_menu(&mut app, &ctx, &format!(r"{d}\b"));
        click_menu_item(&mut app, &ctx, "env.move_up");
        assert_eq!(app.env_pending_count(), 1);
    }

    #[test]
    fn remove_missing_and_duplicates_makes_pending_removals() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let tools = seed_pills_path(&fx);
        let mut app = page_app(&fx, &ctx);

        click_key(&mut app, &ctx, "env.path.clean");

        let shown = texts(&mut app, &ctx);
        assert_eq!(
            count(&shown, &app.tr.t("env.pill.will_remove")),
            2,
            "{shown:?}"
        );
        assert!(shown.contains(&app.tr.t("env.pending_one")));
        click_key(&mut app, &ctx, "tweaks.apply");
        assert!(app.pending_confirm.is_some(), "removals were not confirmed");
        app.resolve_confirm(&ctx, true);
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            fx.user_env("Path"),
            Some((2, format!(r"{tools};%UNKNOWN%\y"))),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn the_clean_button_is_disabled_when_nothing_is_flagged() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let tools = format!(r"{}\tools", dir(&fx));
        std::fs::create_dir_all(&tools).unwrap();
        fx.seed_env(false, "Path", 2, &tools);
        let mut app = page_app(&fx, &ctx);

        let at = pos_of_key(&mut app, &ctx, "env.path.clean");
        click_at(&mut app, &ctx, at);
        assert_eq!(app.env_pending_count(), 0);
        let hover = hover_texts(&mut app, &ctx, at);

        assert!(
            hover.contains(&app.tr.t("env.path.clean_none")),
            "{hover:?}"
        );
    }

    #[test]
    fn adding_a_folder_from_the_field() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "Path", 2, &format!(r"{d}\a"));
        let mut app = page_app(&fx, &ctx);

        app.env_new_entry = r"C:\x;y".into();
        let add = pos_of_key(&mut app, &ctx, "env.path.add");
        click_at(&mut app, &ctx, add);
        assert_eq!(app.env_pending_count(), 0, "a bad folder was added");
        let hover = hover_texts(&mut app, &ctx, add);
        let reason =
            messages::env_problem(app.tr, &crate::environment::EnvProblem::EntryHasSemicolon);
        assert!(hover.contains(&reason), "{hover:?}");

        app.env_new_entry = format!(r"{d}\new");
        click_key(&mut app, &ctx, "env.path.add");

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.pill.will_add")), "{shown:?}");
        assert!(app.env_new_entry.is_empty());
        assert!(shown.contains(&format!(r"{d}\new")));
    }

    #[test]
    fn no_user_path_offers_the_first_folder() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.path.empty")), "{shown:?}");
        assert!(shown.contains(&app.tr.t("env.vars.empty")));

        let first = format!(r"{}\first", dir(&fx));
        app.env_new_entry = first.clone();
        click_key(&mut app, &ctx, "env.path.add");
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&app.tr.t("env.path.empty")), "{shown:?}");
        apply_and_accept(&mut app, &ctx);

        assert_eq!(
            fx.user_env("Path"),
            Some((2, first)),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn browsing_is_offered_next_to_the_field() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);

        assert!(shown.contains(&app.tr.t("env.browse")), "{shown:?}");
    }

    #[test]
    fn the_variable_dialog_adds_and_refuses_bad_names() {
        use crate::environment::EnvProblem;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "FOO", 1, "x");
        let mut app = page_app(&fx, &ctx);

        click_key(&mut app, &ctx, "env.vars.add");
        assert!(matches!(
            app.env_dialog,
            Some(EnvDialog::Variable { key: None, .. })
        ));

        for (name, problem) in [
            ("A=B", EnvProblem::NameHasEquals),
            ("FOO", EnvProblem::NameTaken("FOO".into())),
            ("path", EnvProblem::PathName),
        ] {
            set_variable_dialog(&mut app, name, "v");
            let shown = texts(&mut app, &ctx);
            let line = messages::env_problem(app.tr, &problem);
            assert!(shown.contains(&line), "{line:?} not drawn: {shown:?}");
            click_key(&mut app, &ctx, "env.dialog_ok");
            assert!(app.env_dialog.is_some(), "{name:?} was accepted");
            assert!(app.env_pending.vars.is_empty());
        }

        set_variable_dialog(&mut app, " NEW ", r"%FOO%\x");
        click_key(&mut app, &ctx, "env.dialog_ok");

        assert_eq!(app.env_dialog, None);
        let edit = app.env_pending.vars.get(&VarKey::New("new".into()));
        assert_eq!(
            edit.and_then(|e| e.wanted.clone()),
            Some(("NEW".to_string(), r"%FOO%\x".to_string()))
        );
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.pill.will_add")), "{shown:?}");
        apply_and_accept(&mut app, &ctx);
        assert_eq!(
            fx.user_env("NEW"),
            Some((2, r"%FOO%\x".to_string())),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn a_name_freed_only_by_a_pending_change_is_refused() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "x", 1, "1");
        fx.seed_env(false, "y", 1, "1");
        let mut app = page_app(&fx, &ctx);
        app.env_pending.vars.insert(
            VarKey::Existing("x".into()),
            VarEdit {
                original: Some("x".into()),
                wanted: None,
            },
        );
        app.env_pending.vars.insert(
            VarKey::Existing("y".into()),
            VarEdit {
                original: Some("y".into()),
                wanted: Some(("z".into(), "1".into())),
            },
        );

        for taken in ["X", "Y", "z"] {
            open_add_dialog(&mut app);
            set_variable_dialog(&mut app, taken, "v");
            let shown = texts(&mut app, &ctx);
            let line = messages::env_problem(
                app.tr,
                &crate::environment::EnvProblem::NameTaken(taken.into()),
            );
            assert!(
                shown.contains(&line),
                "{taken}: {line:?} not drawn: {shown:?}"
            );
            click_key(&mut app, &ctx, "env.dialog_ok");
            assert!(app.env_dialog.is_some(), "{taken} was accepted");
        }
    }

    #[test]
    fn editing_keeps_the_own_name_and_moves_a_new_variable_key() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Foo", 1, "x");
        let mut app = page_app(&fx, &ctx);

        // The variable's own name is free for itself, in any case.
        app.env_dialog = Some(EnvDialog::Variable {
            key: Some(VarKey::Existing("foo".into())),
            name: "FOO".into(),
            value: "x".into(),
        });
        click_key(&mut app, &ctx, "env.dialog_ok");
        assert_eq!(app.env_dialog, None);
        assert_eq!(
            app.env_pending.vars.get(&VarKey::Existing("foo".into())),
            Some(&VarEdit {
                original: Some("Foo".into()),
                wanted: Some(("FOO".into(), "x".into())),
            })
        );

        // An added variable renamed in the dialog moves its key.
        app.env_pending.vars.insert(
            VarKey::New("bar".into()),
            VarEdit {
                original: None,
                wanted: Some(("Bar".into(), "1".into())),
            },
        );
        app.env_dialog = Some(EnvDialog::Variable {
            key: Some(VarKey::New("bar".into())),
            name: "Baz".into(),
            value: "2".into(),
        });
        click_key(&mut app, &ctx, "env.dialog_ok");
        assert!(!app
            .env_pending
            .vars
            .contains_key(&VarKey::New("bar".into())));
        assert_eq!(
            app.env_pending
                .vars
                .get(&VarKey::New("baz".into()))
                .and_then(|e| e.wanted.clone()),
            Some(("Baz".to_string(), "2".to_string()))
        );
    }

    #[test]
    fn a_case_only_rename_is_stored_with_the_new_spelling() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "Foo", 1, "x");
        let mut app = page_app(&fx, &ctx);
        app.env_dialog = Some(EnvDialog::Variable {
            key: Some(VarKey::Existing("foo".into())),
            name: "FOO".into(),
            value: "x".into(),
        });
        click_key(&mut app, &ctx, "env.dialog_ok");

        apply_and_accept(&mut app, &ctx);

        assert_eq!(fx.user_env_names(), ["FOO"], "{}", app.last_notice());
    }

    #[test]
    fn nothing_can_be_added_or_cleaned_while_a_job_runs() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "Path", 2, &format!(r"{d}\a;{d}\a"));
        let mut app = page_app(&fx, &ctx);
        let release = hold_job(&mut app);
        let reason = app.busy_or_blocked(&None).expect("busy");

        app.env_new_entry = format!(r"{d}\new");
        let add = pos_of_key(&mut app, &ctx, "env.path.add");
        click_at(&mut app, &ctx, add);
        assert_eq!(app.env_pending.path, None, "a folder was added");
        assert!(hover_texts(&mut app, &ctx, add).contains(&reason));

        click_key(&mut app, &ctx, "env.path.clean");
        assert_eq!(app.env_pending.path, None, "the list was cleaned");

        click_key(&mut app, &ctx, "env.vars.add");
        assert_eq!(app.env_dialog, None, "the variable dialog opened");

        app.env_dialog = Some(EnvDialog::Variable {
            key: None,
            name: "NEW".into(),
            value: "v".into(),
        });
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&reason), "{shown:?}");
        click_key(&mut app, &ctx, "env.dialog_ok");
        assert!(app.env_dialog.is_some(), "OK was accepted during a job");
        assert!(app.env_pending.vars.is_empty());

        release.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn uses_refs_follows_the_stored_type_until_the_value_is_edited() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "PLAIN", 1, "%FOO%");
        let mut app = page_app(&fx, &ctx);
        let pill = app.tr.t("env.pill.uses_refs");

        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&pill), "REG_SZ shown as expandable");

        // An edit gets the type the write will give it.
        app.env_pending.vars.insert(
            VarKey::Existing("plain".into()),
            VarEdit {
                original: Some("PLAIN".into()),
                wanted: Some(("PLAIN".into(), "%BAR%".into())),
            },
        );
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&pill), "{shown:?}");
    }

    #[test]
    fn ok_on_an_unchanged_folder_is_no_edit_and_keeps_its_spaces() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        let padded = format!(r"  {d}\a ");
        fx.seed_env(false, "Path", 2, &format!(r"{d}\b;{padded}"));
        let mut app = page_app(&fx, &ctx);

        for (i, text) in [(0, format!(r"{d}\b")), (1, padded.clone())] {
            app.env_dialog = Some(EnvDialog::Entry {
                index: Some(i),
                text,
            });
            click_key(&mut app, &ctx, "env.dialog_ok");
            assert_eq!(app.env_dialog, None);
            assert_eq!(app.env_pending.path, None, "entry {i} became an edit");
        }
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&app.tr.t("env.pill.will_add")), "{shown:?}");
        assert!(!shown.contains(&app.tr.t("env.pill.will_remove")));
        assert!(!shown.contains(&app.tr.t("tweaks.discard")));

        // A changed entry is trimmed, and the folder it replaces is removed.
        app.env_dialog = Some(EnvDialog::Entry {
            index: Some(1),
            text: format!(r" {d}\c "),
        });
        click_key(&mut app, &ctx, "env.dialog_ok");
        assert_eq!(
            app.env_pending.path.as_ref().map(|p| p.texts()),
            Some(vec![format!(r"{d}\b"), format!(r"{d}\c")])
        );
    }

    #[test]
    fn escape_cancels_a_dialog() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        open_add_dialog(&mut app);
        frame(&mut app, &ctx, tall_input(vec![]));

        frame(
            &mut app,
            &ctx,
            tall_input(vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]),
        );

        assert_eq!(app.env_dialog, None);
        assert!(app.env_pending.vars.is_empty());
    }

    #[test]
    fn the_entry_dialog_edits_a_folder() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "Path", 2, &format!(r"{d}\a"));
        let mut app = page_app(&fx, &ctx);

        open_menu(&mut app, &ctx, &format!(r"{d}\a"));
        click_menu_item(&mut app, &ctx, "env.edit");
        assert_eq!(
            app.env_dialog,
            Some(EnvDialog::Entry {
                index: Some(0),
                text: format!(r"{d}\a")
            })
        );

        if let Some(EnvDialog::Entry { text, .. }) = &mut app.env_dialog {
            *text = "a;b".into();
        }
        let shown = texts(&mut app, &ctx);
        let line =
            messages::env_problem(app.tr, &crate::environment::EnvProblem::EntryHasSemicolon);
        assert!(shown.contains(&line), "{shown:?}");
        click_key(&mut app, &ctx, "env.dialog_ok");
        assert!(app.env_dialog.is_some(), "a bad folder was accepted");

        if let Some(EnvDialog::Entry { text, .. }) = &mut app.env_dialog {
            *text = format!(r"{d}\b");
        }
        click_key(&mut app, &ctx, "env.dialog_ok");

        assert_eq!(app.env_dialog, None);
        assert_eq!(
            app.env_pending.path.as_ref().map(|p| p.texts()),
            Some(vec![format!(r"{d}\b")])
        );
    }

    #[test]
    fn variable_rows_show_their_pending_state_and_delete_from_the_menu() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "BAR", 1, "b");
        fx.seed_env(false, "BAZ", 1, "c");
        fx.seed_env(false, "FOO", 2, "a");
        let mut app = page_app(&fx, &ctx);
        let edit = |original: &str, wanted: Option<(&str, &str)>| VarEdit {
            original: Some(original.into()),
            wanted: wanted.map(|(n, v)| (n.into(), v.into())),
        };
        app.env_pending.vars.insert(
            VarKey::Existing("foo".into()),
            edit("FOO", Some(("FOO", "z"))),
        );
        app.env_pending.vars.insert(
            VarKey::Existing("bar".into()),
            edit("BAR", Some(("BAR2", "b"))),
        );
        app.env_pending
            .vars
            .insert(VarKey::Existing("baz".into()), edit("BAZ", None));
        app.env_pending.vars.insert(
            VarKey::New("new".into()),
            VarEdit {
                original: None,
                wanted: Some(("NEW".into(), "n".into())),
            },
        );

        let shown = texts(&mut app, &ctx);

        for want in [
            app.tr.t("env.pill.will_change"),
            app.tr.t_args("env.pill.will_rename", &[&"BAR2"]),
            app.tr.t("env.pill.will_delete"),
            app.tr.t("env.pill.will_add"),
            app.tr.t("env.pill.uses_refs"),
            app.tr.t_args("env.pending", &[&4]),
            app.tr.t("tweaks.discard"),
            "NEW".to_string(),
        ] {
            assert!(shown.contains(&want), "{want:?} not drawn: {shown:?}");
        }
        // The row's title is its current name, even when a rename is pending.
        assert!(shown.contains(&"BAR".to_string()));
        // A wanted value replaces the current one on the row.
        assert!(shown.contains(&"z".to_string()));

        // Delete drops an added variable, and marks a scanned one.
        open_menu(&mut app, &ctx, "NEW");
        click_menu_item(&mut app, &ctx, "env.delete");
        assert!(!app
            .env_pending
            .vars
            .contains_key(&VarKey::New("new".into())));
        open_menu(&mut app, &ctx, "FOO");
        click_menu_item(&mut app, &ctx, "env.delete");
        assert_eq!(
            app.env_pending
                .vars
                .get(&VarKey::Existing("foo".into()))
                .map(|e| e.wanted.clone()),
            Some(None)
        );

        click_key(&mut app, &ctx, "tweaks.discard");
        assert_eq!(app.env_pending, Default::default());
    }

    #[test]
    fn a_name_with_non_ascii_letters_can_be_edited_and_deleted() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "RÉPERTOIRE", 1, "1");
        let mut app = page_app(&fx, &ctx);

        open_menu(&mut app, &ctx, "RÉPERTOIRE");
        click_menu_item(&mut app, &ctx, "env.edit");
        set_variable_dialog(&mut app, "RÉPERTOIRE", "2");
        click_key(&mut app, &ctx, "env.dialog_ok");
        assert_eq!(app.env_dialog, None, "its own name was refused");
        assert_eq!(app.env_pending_count(), 1);
        apply_and_accept(&mut app, &ctx);
        assert_eq!(
            fx.user_env("RÉPERTOIRE"),
            Some((1, "2".to_string())),
            "{}",
            app.last_notice()
        );
        run_until_env_scanned(&mut app, &ctx);

        open_menu(&mut app, &ctx, "RÉPERTOIRE");
        click_menu_item(&mut app, &ctx, "env.delete");
        assert_eq!(app.env_pending_count(), 1);
        apply_and_accept(&mut app, &ctx);
        assert_eq!(fx.user_env("RÉPERTOIRE"), None, "{}", app.last_notice());
    }

    #[test]
    fn the_pending_count_reads_singular_for_one_change() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "A", 1, "1");
        fx.seed_env(false, "B", 1, "1");
        let mut app = page_app(&fx, &ctx);
        let edit = |name: &str| VarEdit {
            original: Some(name.into()),
            wanted: Some((name.into(), "2".into())),
        };
        app.env_pending
            .vars
            .insert(VarKey::Existing("a".into()), edit("A"));
        let shown = texts(&mut app, &ctx);
        let one = app.tr.t("env.pending_one");
        assert!(shown.contains(&one), "{one:?} not drawn: {shown:?}");

        app.env_pending
            .vars
            .insert(VarKey::Existing("b".into()), edit("B"));
        let shown = texts(&mut app, &ctx);
        let two = app.tr.t_args("env.pending", &[&2]);
        assert!(shown.contains(&two), "{two:?} not drawn: {shown:?}");
    }

    #[test]
    fn the_variable_menu_edits_with_the_wanted_name_and_value() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "FOO", 1, "a");
        let mut app = page_app(&fx, &ctx);
        app.env_pending.vars.insert(
            VarKey::Existing("foo".into()),
            VarEdit {
                original: Some("FOO".into()),
                wanted: Some(("FOO2".into(), "b".into())),
            },
        );

        open_menu(&mut app, &ctx, "FOO");
        click_menu_item(&mut app, &ctx, "env.edit");

        assert_eq!(
            app.env_dialog,
            Some(EnvDialog::Variable {
                key: Some(VarKey::Existing("foo".into())),
                name: "FOO2".into(),
                value: "b".into(),
            })
        );
    }

    #[test]
    fn a_long_value_is_cut_on_the_row_and_whole_on_hover() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let long = "x".repeat(100);
        fx.seed_env(false, "FOO", 1, &long);
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);

        let cut = format!("{}\u{2026}", "x".repeat(80));
        assert!(shown.contains(&cut), "{shown:?}");
        assert!(!shown.contains(&long));
        let at = pos_of(&mut app, &ctx, "FOO");
        assert!(hover_texts(&mut app, &ctx, at).contains(&long));
    }

    #[test]
    fn a_non_text_variable_is_read_only() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env_raw(
            "DWORDVAR",
            winreg::RegValue {
                bytes: vec![5, 0, 0, 0],
                vtype: winreg::enums::RegType::REG_DWORD,
            },
        );
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.pill.read_only")), "{shown:?}");

        for item in ["env.edit", "env.delete"] {
            open_menu(&mut app, &ctx, "DWORDVAR");
            click_menu_item(&mut app, &ctx, item);
            assert_eq!(app.env_dialog, None, "{item} opened a dialog");
            assert!(app.env_pending.vars.is_empty(), "{item} changed the value");

            open_menu(&mut app, &ctx, "DWORDVAR");
            let at = menu_item(&mut app, &ctx, item);
            let hover = hover_texts(&mut app, &ctx, at);
            assert!(
                hover.contains(&app.tr.t("env.not_text")),
                "{item}: {hover:?}"
            );
        }
    }

    #[test]
    fn the_system_section_is_collapsed_and_read_only() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(true, "JAVA_HOME", 1, r"C:\Java");
        fx.seed_env(true, "Path", 2, &format!(r"{d}\sys;%SystemRoot%\x"));
        let mut app = page_app(&fx, &ctx);

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.system.title")), "{shown:?}");
        assert!(!shown.contains(&"JAVA_HOME".to_string()));

        click_key(&mut app, &ctx, "env.system.title");

        let shown = texts(&mut app, &ctx);
        for want in [
            "JAVA_HOME".to_string(),
            r"C:\Java".to_string(),
            app.tr.t("env.system.path"),
            format!(r"{d}\sys"),
        ] {
            assert!(shown.contains(&want), "{want:?} not drawn: {shown:?}");
        }
        for title in ["JAVA_HOME", &format!(r"{d}\sys"), r"%SystemRoot%\x"] {
            let menu = find_row_widget(&mut app, &ctx, title, kit::OVERFLOW_BUTTON);
            assert_eq!(menu, None, "the system row {title:?} has a menu");
        }
        // A user row does, so the check can fail.
        fx.seed_env(false, "FOO", 1, "x");
        app.request_env_scan();
        run_until_env_scanned(&mut app, &ctx);
        assert!(find_row_widget(&mut app, &ctx, "FOO", kit::OVERFLOW_BUTTON).is_some());
    }

    #[test]
    fn an_unreadable_system_key_says_so_inside_its_section() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        app.env_scan = Some(EnvScan {
            system_error: Some("denied".into()),
            ..Default::default()
        });

        click_key(&mut app, &ctx, "env.system.title");

        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t_args("env.system.unreadable", &[&"denied"])),
            "{shown:?}"
        );
    }

    #[test]
    fn an_unreadable_user_key_replaces_the_user_sections() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        app.env_scan = Some(EnvScan {
            user_error: Some("boom".into()),
            ..Default::default()
        });

        let shown = texts(&mut app, &ctx);

        assert!(
            shown.contains(&app.tr.t_args("env.unreadable", &[&"boom"])),
            "{shown:?}"
        );
        assert!(!shown.contains(&app.tr.t("env.path.title")));
        assert!(!shown.contains(&app.tr.t("env.vars.title")));
    }

    #[test]
    fn the_page_never_shows_the_volatile_environment() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "FOO", 1, "x");
        fx.seed_volatile("VOLATILE_ONLY", "secret-volatile");
        let mut app = page_app(&fx, &ctx);
        click_key(&mut app, &ctx, "env.system.title");

        let shown = texts(&mut app, &ctx);

        assert!(shown.contains(&"FOO".to_string()), "{shown:?}");
        assert!(
            !shown
                .iter()
                .any(|t| t.contains("VOLATILE_ONLY") || t.contains("secret-volatile")),
            "{shown:?}"
        );
    }

    #[test]
    fn restore_original_asks_for_the_row_and_the_path() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "FOO", 1, "x");
        fx.seed_env(false, "Path", 2, &format!(r"{d}\a"));
        let mut app = page_app(&fx, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(
            !shown.contains(&app.tr.t("tweaks.restore_original")),
            "restore offered before any change"
        );
        app.env_pending.vars.insert(
            VarKey::Existing("foo".into()),
            VarEdit {
                original: Some("FOO".into()),
                wanted: Some(("FOO".into(), "y".into())),
            },
        );
        app.env_path_edit_mut().unwrap().add(format!(r"{d}\b"));
        app.start_env_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        run_until_env_scanned(&mut app, &ctx);
        assert_eq!(
            fx.user_env("FOO"),
            Some((1, "y".into())),
            "{}",
            app.last_notice()
        );

        click_key(&mut app, &ctx, "tweaks.restore_original");
        assert_eq!(
            app.pending_confirm.as_ref().map(|c| c.action.clone()),
            Some(ConfirmAction::RestoreEnvironment(vec![
                env_slot("Path").id()
            ]))
        );
        app.resolve_confirm(&ctx, false);

        open_menu(&mut app, &ctx, "FOO");
        click_menu_item(&mut app, &ctx, "tweaks.restore_original");
        assert_eq!(
            app.pending_confirm.as_ref().map(|c| c.action.clone()),
            Some(ConfirmAction::RestoreEnvironment(
                vec![env_slot("FOO").id()]
            ))
        );
    }

    #[test]
    fn ctrl_f_focuses_the_environment_search() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        frame(&mut app, &ctx, tall_input(vec![]));
        let mut input = tall_input(vec![Event::Key {
            key: Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        input.modifiers = Modifiers::COMMAND;

        frame(&mut app, &ctx, input);
        frame(&mut app, &ctx, tall_input(vec![]));

        let id = super::super::polish::search_box_id(ActiveTab::Environment);
        assert_eq!(id, Some(egui::Id::new("search_environment")));
        assert!(ctx.memory(|m| m.has_focus(id.unwrap())));
    }

    #[test]
    fn search_filters_path_entries_and_variables() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let d = dir(&fx);
        fx.seed_env(false, "TOOLS", 1, &format!(r"{d}\qzq"));
        fx.seed_env(false, "Path", 2, &format!(r"{d}\alphaz;%TOOLS%\zed"));
        fx.seed_env(false, "FOO", 1, "needle");
        fx.seed_env(false, "BAR", 1, "hay");
        fx.seed_env(true, "SYSVAR", 1, "sysvalue");
        let mut app = page_app(&fx, &ctx);
        let shown = texts(&mut app, &ctx);
        for want in [r"%TOOLS%\zed", "FOO", "BAR"] {
            assert!(shown.contains(&want.to_string()), "{want}: {shown:?}");
        }

        // Case-insensitive, on the entry text.
        app.env_search = "ALPHAZ".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&format!(r"{d}\alphaz")), "{shown:?}");
        assert!(!shown.contains(&r"%TOOLS%\zed".to_string()));
        assert!(!shown.contains(&"FOO".to_string()));

        // On the expanded form of an entry.
        app.env_search = r"qzq\zed".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&r"%TOOLS%\zed".to_string()), "{shown:?}");
        assert!(!shown.contains(&format!(r"{d}\alphaz")));

        // On a variable's value, and not on its name only.
        app.env_search = "needle".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"FOO".to_string()), "{shown:?}");
        assert!(!shown.contains(&"BAR".to_string()));
        app.env_search = "bar".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"BAR".to_string()), "{shown:?}");

        // A system row is a match too, though its section is collapsed.
        app.env_search = "sysvalue".into();
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&app.tr.t("env.no_match")), "{shown:?}");

        app.env_search = "nothing like it".into();
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("env.no_match")), "{shown:?}");
        assert!(!shown.contains(&"FOO".to_string()));
    }

    #[test]
    fn the_page_shows_the_loading_line_until_the_first_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_env(false, "FOO", 1, "x");
        let mut app = fx.app(&ctx);
        app.env_scan_needed = false;
        app.active_tab = ActiveTab::Environment;
        let scanning = app.tr.t("env.scanning");

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&scanning), "{shown:?}");
        assert!(!shown.contains(&"FOO".to_string()));

        app.request_env_scan();
        run_until_env_scanned(&mut app, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&scanning), "{shown:?}");
        assert!(shown.contains(&"FOO".to_string()), "{shown:?}");
    }

    #[test]
    fn opening_the_page_requests_a_scan() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = fx.app(&ctx);
        run_until_env_scanned(&mut app, &ctx);
        app.active_tab = ActiveTab::Home;
        assert!(!app.env_scan_needed);

        app.open_page(ActiveTab::Environment);

        assert_eq!(app.active_tab, ActiveTab::Environment);
        assert!(app.env_scan_needed);
    }

    #[test]
    fn apply_is_disabled_when_nothing_is_pending() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);

        let apply = pos_of_key(&mut app, &ctx, "tweaks.apply");
        click_at(&mut app, &ctx, apply);
        assert!(!app.is_busy());
        let hover = hover_texts(&mut app, &ctx, apply);

        assert!(
            hover.contains(&app.tr.t("env.nothing_to_apply")),
            "{hover:?}"
        );
    }
}
