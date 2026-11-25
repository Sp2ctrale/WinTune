//! The "Context menu" page: entries the user adds to the Explorer right-click
//! menu, per account. The page body is built on top of the app wiring in
//! `mod.rs` and `jobs.rs`.

use super::{kit, ActiveTab, WinTuneApp};
use crate::context_menu::{
    clean_program_path, entry_problems, new_entry_id, EntryProblem, EntryStatus, MenuEntry, Place,
    ProgramChoice,
};
use crate::i18n::messages;
use eframe::egui::{self, RichText};
use kit::{primary_button, secondary_button, space, text_size, PillKind};
use std::collections::BTreeSet;

/// Height of a row of the entry list that has no problem line.
const ROW_HEIGHT: f32 = 48.0;
/// Extra height of a row that shows its first problem.
const PROBLEM_LINE: f32 = 18.0;
/// Width of the text boxes of the entry editor.
const FIELD_WIDTH: f32 = 260.0;

/// Id of the label box of the entry editor (tests focus it). Salted per
/// entry, so selecting another entry never keeps the typing focus.
fn menu_label_id(id: &str) -> egui::Id {
    egui::Id::new(("menu_entry_label", id))
}

/// Id of the program path box of the entry editor.
fn menu_path_id(id: &str) -> egui::Id {
    egui::Id::new(("menu_entry_path", id))
}

/// Id of a row of the entry list.
fn menu_row_id(id: &str) -> egui::Id {
    egui::Id::new(("menu_entry_row", id))
}

/// What the list shows for one entry.
struct EntryRow {
    id: String,
    label: String,
    places: String,
    status: EntryStatus,
    problem: Option<String>,
}

/// What the user did in the list this frame.
#[derive(Default)]
struct ListAction {
    select: Option<String>,
    remove: Option<String>,
    add: bool,
}

fn pill_kind(status: &EntryStatus) -> PillKind {
    match status {
        EntryStatus::Applied => PillKind::Success,
        EntryStatus::NotApplied => PillKind::Neutral,
        EntryStatus::Changed(_) => PillKind::Warning,
    }
}

/// Asks for a program (the "Other program…" browse button).
fn pick_program() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Executable", &["exe"])
        .pick_file()
        .map(|f| f.to_string_lossy().into_owned())
}

/// Asks for an icon file (the editor's "Change…" button).
fn pick_icon() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Icon", &["ico", "exe", "dll"])
        .pick_file()
        .map(|f| f.to_string_lossy().into_owned())
}

impl WinTuneApp {
    pub(super) fn show_context_menu_tab(&mut self, ctx: &egui::Context) {
        // Validated once per frame; the bar, the list and the editor read it.
        let problems = entry_problems(self.menu_entries(), &self.env.programs);
        // Before the panel, so the bar spans the whole page width and the
        // progress of a running job shows.
        self.menu_action_bar(ctx, &problems);
        let tr = self.tr;
        let name = tr.t("nav.context_menu");
        let subtitle = tr.t_args("menu.subtitle", &[&self.env.user_label]);
        let mut use_classic = false;
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |_| {});
            if self.env.is_windows_11 {
                let text = tr.t("menu.win11_banner");
                let action = tr.t("menu.use_classic");
                use_classic = kit::banner(ui, kit::BannerKind::Info, &text, Some(&action));
                ui.add_space(space::S);
            }
            if let Some(e) = &self.menu_store_error {
                let detail = messages::load_error(tr, e);
                let text = tr.t_args("tweaks.store_unreadable", &[&detail]);
                kit::banner(ui, kit::BannerKind::Error, &text, None);
                ui.add_space(space::S);
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.menu_entry_list(ui, &problems);
                    if let Some(id) = self.selected_menu_entry.clone() {
                        self.menu_entry_editor(ui, &id);
                    }
                    self.show_menu_windows_section(ui);
                    self.show_menu_others_section(ui);
                });
        });
        if use_classic {
            self.open_page(ActiveTab::Tweaks);
        }
    }

    /// Why Apply is disabled: a running job, an unreadable store or config,
    /// or the first entry problem.
    fn menu_apply_disabled_reason(&self, problems: &[(String, EntryProblem)]) -> Option<String> {
        self.busy_or_blocked(&self.menu_store_error)
            .or_else(|| self.menu_blocked_reason(false))
            .or_else(|| {
                problems
                    .first()
                    .map(|(_, problem)| messages::entry_problem(self.tr, problem))
            })
            .or_else(|| {
                (self.menu_pending_count() == 0).then(|| self.tr.t("menu.nothing_to_apply"))
            })
    }

    /// Entries not applied yet, keys of removed entries still to delete, and
    /// pending hides and renames of existing entries.
    fn menu_pending_count(&self) -> usize {
        let unapplied = self
            .menu_entries()
            .iter()
            .filter(|e| self.menu_statuses.get(&e.id) != Some(&EntryStatus::Applied))
            .count();
        unapplied + self.menu_orphans.len() + self.menu_existing_pending_count()
    }

    /// The page's bar: how much is not applied, Discard (pending hides and
    /// renames) and Apply; or the job's progress while one runs.
    fn menu_action_bar(&mut self, ctx: &egui::Context, problems: &[(String, EntryProblem)]) {
        let tr = self.tr;
        let pending = self.menu_pending_count();
        let status = if pending > 0 {
            tr.t_args("menu.pending", &[&pending])
        } else {
            tr.t("menu.up_to_date")
        };
        let disabled_reason = self.menu_apply_disabled_reason(problems);
        let p = kit::current_palette(ctx);
        let apply_label = tr.t("tweaks.apply");
        let discard_label = tr.t("tweaks.discard");
        let can_discard = !self.menu_pending.is_empty();

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
            self.start_menu_apply(ctx);
        }
        if discard {
            self.discard_menu_changes();
        }
    }

    /// Adds a new entry (VS Code when found, else a custom program), selects
    /// it and saves.
    fn add_menu_entry(&mut self) {
        let program = if self.env.programs.resolve(&ProgramChoice::VsCode).is_some() {
            ProgramChoice::VsCode
        } else {
            ProgramChoice::Custom(String::new())
        };
        let label = self.tr.t("menu.new_label");
        let entries = self.menu_entries_mut();
        let id = new_entry_id(entries);
        entries.push(MenuEntry {
            id: id.clone(),
            label,
            program,
            places: BTreeSet::from([Place::Folder]),
            icon: None,
        });
        self.selected_menu_entry = Some(id);
        self.refresh_menu_statuses_if_idle();
    }

    fn menu_rows(&self, problems: &[(String, EntryProblem)]) -> Vec<EntryRow> {
        let tr = self.tr;
        self.menu_entries()
            .iter()
            .map(|e| EntryRow {
                id: e.id.clone(),
                label: if e.label.trim().is_empty() {
                    e.id.clone()
                } else {
                    e.label.trim().to_string()
                },
                places: e
                    .places
                    .iter()
                    .map(|p| messages::place_name(tr, *p))
                    .collect::<Vec<_>>()
                    .join(", "),
                status: self
                    .menu_statuses
                    .get(&e.id)
                    .cloned()
                    .unwrap_or(EntryStatus::NotApplied),
                problem: problems
                    .iter()
                    .find(|(id, _)| id == &e.id)
                    .map(|(_, problem)| messages::entry_problem(tr, problem)),
            })
            .collect()
    }

    /// The "My entries" section: the rows, or an empty state.
    fn menu_entry_list(&mut self, ui: &mut egui::Ui, problems: &[(String, EntryProblem)]) {
        let tr = self.tr;
        let rows = self.menu_rows(problems);
        let mut act = ListAction::default();
        let mut add_clicked = false;
        let add_label = tr.t("menu.add");
        kit::section_with_action(
            ui,
            &tr.t("menu.my_entries"),
            |ui| {
                add_clicked = ui.add(secondary_button(&add_label)).clicked();
            },
            |ui| {
                if rows.is_empty() {
                    if kit::empty_state(
                        ui,
                        kit::Icon::ContextMenu,
                        &tr.t("menu.empty"),
                        Some(&add_label),
                    ) {
                        act.add = true;
                    }
                    return;
                }
                ui.spacing_mut().item_spacing.y = 2.0;
                for row in &rows {
                    let selected = self.selected_menu_entry.as_deref() == Some(row.id.as_str());
                    self.menu_entry_row(ui, row, selected, &mut act);
                }
            },
        );
        if act.add || add_clicked {
            self.add_menu_entry();
        }
        if let Some(id) = act.select {
            self.selected_menu_entry = Some(id);
        }
        if let Some(id) = act.remove {
            self.request_remove_menu_entry(&id);
        }
    }

    fn menu_entry_row(
        &self,
        ui: &mut egui::Ui,
        row: &EntryRow,
        selected: bool,
        act: &mut ListAction,
    ) {
        let tr = self.tr;
        let p = kit::current_palette(ui.ctx());
        let status_label = messages::entry_status(tr, &row.status);
        let height = if row.problem.is_some() {
            ROW_HEIGHT + PROBLEM_LINE
        } else {
            ROW_HEIGHT
        };
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        let resp = ui.interact(rect, menu_row_id(&row.id), egui::Sense::click());
        let accessible = format!("{}, {}", row.label, status_label);
        resp.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                true,
                selected,
                accessible.clone(),
            )
        });
        let fill = if selected {
            Some(p.accent.gamma_multiply(0.14))
        } else if resp.hovered() {
            Some(p.surface_hover)
        } else {
            None
        };
        if let Some(fill) = fill {
            ui.painter()
                .rect_filled(rect, egui::Rounding::same(kit::radius::CONTROL), fill);
        }
        // The ring goes over the fill, so it stays visible.
        if resp.has_focus() {
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
        content.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add(
                egui::Label::new(
                    RichText::new(&row.label)
                        .size(text_size::BODY)
                        .color(p.text),
                )
                .selectable(false),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(&row.places)
                        .size(text_size::SECONDARY)
                        .color(p.text_secondary),
                )
                .selectable(false),
            );
            if let Some(problem) = &row.problem {
                ui.add(
                    egui::Label::new(
                        RichText::new(problem)
                            .size(text_size::SECONDARY)
                            .color(p.danger),
                    )
                    .selectable(false),
                );
            }
        });
        content.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let more = tr.t("tweaks.more");
            let more_for = tr.t_args("tweaks.more_for", &[&row.label]);
            let remove = tr.t("menu.remove");
            let items = [kit::MenuItem {
                label: &remove,
                enabled: true,
                disabled_hint: None,
            }];
            let salt = format!("menu_row_{}", row.id);
            if kit::overflow_menu(ui, &salt, &more, &more_for, &items) == Some(0) {
                act.remove = Some(row.id.clone());
            }
            let pill = kit::pill(ui, &status_label, pill_kind(&row.status));
            if matches!(row.status, EntryStatus::Changed(_)) {
                pill.on_hover_text(tr.t("menu.changed_hint"));
            }
        });
        if resp.clicked() {
            act.select = Some(row.id.clone());
        }
    }

    /// The editor of the selected entry. It works on a copy and writes it
    /// back only when something changed, so merely drawing never edits the
    /// list (nor marks the config dirty).
    fn menu_entry_editor(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(original) = self.menu_entries().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let tr = self.tr;
        let programs = self.env.programs.clone();
        let mut edited = original.clone();
        let (mut browse, mut change_icon, mut reset_icon) = (false, false, false);

        kit::section(ui, &tr.t("menu.details"), |ui| {
            kit::SettingRow {
                icon: None,
                title: &tr.t("menu.label"),
                description: None,
                hover: None,
            }
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut edited.label)
                        .id(menu_label_id(id))
                        .desired_width(FIELD_WIDTH),
                );
            });

            kit::SettingRow {
                icon: None,
                title: &tr.t("menu.program"),
                description: None,
                hover: None,
            }
            .show(ui, |ui| {
                let presets = [
                    (ProgramChoice::VsCode, "menu.program.vscode"),
                    (ProgramChoice::Terminal, "menu.program.terminal"),
                    (ProgramChoice::PowerShell, "menu.program.powershell"),
                    (ProgramChoice::Cmd, "menu.program.cmd"),
                ];
                let current = presets
                    .iter()
                    .find(|(choice, _)| *choice == edited.program)
                    .map_or("menu.program.other", |(_, key)| key);
                egui::ComboBox::from_id_salt(("menu_program", id))
                    .selected_text(tr.t(current))
                    .show_ui(ui, |ui| {
                        for (choice, key) in &presets {
                            let found = programs.resolve(choice).is_some();
                            let label =
                                egui::SelectableLabel::new(edited.program == *choice, tr.t(key));
                            let resp = ui.add_enabled(found, label);
                            let resp = if found {
                                resp
                            } else {
                                resp.on_disabled_hover_text(tr.t("menu.program_missing"))
                            };
                            if resp.clicked() {
                                edited.program = choice.clone();
                            }
                        }
                        let custom = matches!(edited.program, ProgramChoice::Custom(_));
                        let other = egui::SelectableLabel::new(custom, tr.t("menu.program.other"));
                        if ui.add(other).clicked() && !custom {
                            edited.program = ProgramChoice::Custom(String::new());
                        }
                    });
            });

            if let ProgramChoice::Custom(path) = &mut edited.program {
                kit::SettingRow {
                    icon: None,
                    title: &tr.t("menu.path"),
                    description: None,
                    hover: None,
                }
                .show(ui, |ui| {
                    browse = ui.add(secondary_button(&tr.t("menu.browse"))).clicked();
                    let resp = ui.add(
                        egui::TextEdit::singleline(path)
                            .id(menu_path_id(id))
                            .desired_width(FIELD_WIDTH),
                    );
                    // Only once the box is left: trimming while typing would
                    // eat the space of "Program Files" as it is typed.
                    if resp.lost_focus() {
                        *path = clean_program_path(path);
                    }
                });
            }

            kit::SettingRow {
                icon: None,
                title: &tr.t("menu.appears_on"),
                description: None,
                hover: None,
            }
            .show(ui, |ui| {
                // Right to left: reversed, so they read in `Place::ALL` order.
                for place in Place::ALL.into_iter().rev() {
                    let mut on = edited.places.contains(&place);
                    if ui
                        .checkbox(&mut on, messages::place_name(tr, place))
                        .changed()
                    {
                        if on {
                            edited.places.insert(place);
                        } else {
                            edited.places.remove(&place);
                        }
                    }
                }
            });

            let shown_icon = edited
                .icon
                .clone()
                .unwrap_or_else(|| tr.t("menu.icon_default"));
            kit::SettingRow {
                icon: None,
                title: &tr.t("menu.icon"),
                description: Some(&shown_icon),
                hover: None,
            }
            .show(ui, |ui| {
                change_icon = ui.add(secondary_button(&tr.t("menu.change"))).clicked();
                reset_icon = ui
                    .add_enabled(edited.icon.is_some(), secondary_button(&tr.t("menu.reset")))
                    .clicked();
            });
        });

        if browse {
            if let Some(path) = pick_program() {
                edited.program = ProgramChoice::Custom(path);
            }
        }
        if change_icon {
            if let Some(path) = pick_icon() {
                edited.icon = Some(path);
            }
        }
        if reset_icon {
            edited.icon = None;
        }
        if edited != original {
            if let Some(slot) = self.menu_entries_mut().iter_mut().find(|e| e.id == id) {
                *slot = edited;
            }
            self.refresh_menu_statuses_if_idle();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::confirm::ConfirmAction;
    use super::super::test_support::*;
    use super::super::{ActiveTab, WinTuneApp};
    use super::*;
    use crate::context_menu::{entry_key, Place};
    use crate::i18n::messages;
    use egui::{Event, Key, Modifiers};

    fn page_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        app.active_tab = ActiveTab::ContextMenu;
        app
    }

    /// Where `text` is drawn on the page, after the layout has settled.
    fn find(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        text_center(&out, text)
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)))
    }

    fn find_key(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) -> egui::Pos2 {
        let text = app.tr.t(key);
        find(app, ctx, &text)
    }

    fn click_key(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) {
        let pos = find_key(app, ctx, key);
        click_at(app, ctx, pos);
    }

    fn texts(app: &mut WinTuneApp, ctx: &egui::Context) -> Vec<String> {
        frame(app, ctx, tall_input(vec![]));
        drawn_texts(&frame(app, ctx, tall_input(vec![])))
    }

    /// A frame `seconds` later than the last one, so the autosave fires.
    fn later(app: &mut WinTuneApp, ctx: &egui::Context, seconds: f64) {
        let mut input = tall_input(vec![]);
        input.time = Some(ctx.input(|i| i.time) + seconds);
        frame(app, ctx, input);
    }

    #[test]
    fn adding_an_entry_selects_it_and_autosaves() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        assert!(app.menu_entries().is_empty());

        click_key(&mut app, &ctx, "menu.add");

        let new_label = app.tr.t("menu.new_label");
        let entries = app.menu_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, new_label);
        assert_eq!(
            app.selected_menu_entry.as_deref(),
            Some(entries[0].id.as_str())
        );
        later(&mut app, &ctx, 2.0);
        let saved = std::fs::read_to_string(fx.dir.join("config.json")).unwrap_or_default();
        assert!(saved.contains(&new_label), "not saved: {saved}");
    }

    #[test]
    fn apply_from_the_page_writes_the_entry() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        click_key(&mut app, &ctx, "menu.add");
        frame(&mut app, &ctx, tall_input(vec![]));

        ctx.memory_mut(|m| m.request_focus(menu_label_id("e1")));
        let key = |pressed| Event::Key {
            key: Key::A,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(
            &mut app,
            &ctx,
            tall_input(vec![
                key(true),
                key(false),
                Event::Text("Open with App".into()),
            ]),
        );
        assert_eq!(app.menu_entries()[0].label, "Open with App");

        click_key(&mut app, &ctx, "menu.place.background");
        assert!(app.menu_entries()[0].places.contains(&Place::Background));
        click_key(&mut app, &ctx, "tweaks.apply");
        run_until_idle(&mut app, &ctx);

        for place in [Place::Folder, Place::Background] {
            let key = fx.hkcu(&entry_key(place, "e1"));
            assert!(fx.key_exists(&key), "{key}: {}", app.last_notice());
        }
        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t("menu.status.applied")),
            "{shown:?}"
        );
    }

    #[test]
    fn apply_is_disabled_with_the_problem_as_reason() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        click_key(&mut app, &ctx, "menu.add");
        app.menu_entries_mut()[0].label = String::new();
        app.refresh_menu_statuses();

        let apply = find_key(&mut app, &ctx, "tweaks.apply");
        click_at(&mut app, &ctx, apply);
        assert!(!app.is_busy(), "apply started with an empty label");
        let hover = hover_texts(&mut app, &ctx, apply);

        let reason =
            messages::entry_problem(app.tr, &crate::context_menu::EntryProblem::EmptyLabel);
        assert!(hover.contains(&reason), "{hover:?}");
    }

    #[test]
    fn apply_is_disabled_when_nothing_is_pending() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        assert_eq!(app.menu_pending_count(), 0);

        let apply = find_key(&mut app, &ctx, "tweaks.apply");
        click_at(&mut app, &ctx, apply);
        assert!(!app.is_busy(), "apply started with nothing to apply");
        let hover = hover_texts(&mut app, &ctx, apply);

        assert!(
            hover.contains(&app.tr.t("menu.nothing_to_apply")),
            "{hover:?}"
        );
    }

    #[test]
    fn a_pasted_program_path_is_cleaned_when_the_box_is_left() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        click_key(&mut app, &ctx, "menu.add");
        let exe = fx.dir.join("pasted app.exe");
        std::fs::write(&exe, b"x").unwrap();
        let pasted = format!("  \"{}\" ", exe.display());
        app.menu_entries_mut()[0].program = ProgramChoice::Custom(pasted);
        frame(&mut app, &ctx, tall_input(vec![]));

        ctx.memory_mut(|m| m.request_focus(menu_path_id("e1")));
        frame(&mut app, &ctx, tall_input(vec![]));
        // Tab leaves the box.
        let tab = |pressed| Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&mut app, &ctx, tall_input(vec![tab(true), tab(false)]));
        frame(&mut app, &ctx, tall_input(vec![]));

        let clean = exe.display().to_string();
        assert_eq!(app.menu_entries()[0].program, ProgramChoice::Custom(clean));
    }

    #[test]
    fn editing_while_a_job_runs_does_not_reread_the_registry() {
        use super::super::jobs::RunningJob;
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        click_key(&mut app, &ctx, "menu.add");
        assert!(app.menu_statuses.contains_key("e1"));

        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        app.job = Some(RunningJob::BuildPak(crate::tasks::BackgroundJob::spawn(
            "Building gallery.pak",
            || {},
            move |_| {
                release_rx.recv().unwrap();
                Ok(crate::gallery::PakStats {
                    count: 0,
                    bytes_before: 0,
                    bytes_after: 0,
                })
            },
        )));
        app.add_menu_entry();
        assert_eq!(app.menu_entries().len(), 2);
        assert!(
            !app.menu_statuses.contains_key("e2"),
            "re-read during a job"
        );

        app.request_remove_menu_entry("e1");
        app.resolve_confirm(&ctx, true);
        assert_eq!(app.menu_entries().len(), 1);
        assert!(app.menu_statuses.contains_key("e1"), "re-read during a job");

        release_tx.send(()).unwrap();
        run_until_idle(&mut app, &ctx);
    }

    #[test]
    fn remove_asks_first_and_apply_then_deletes_the_keys() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        click_key(&mut app, &ctx, "menu.add");
        app.start_menu_apply(&ctx);
        run_until_idle(&mut app, &ctx);
        let key = fx.hkcu(&entry_key(Place::Folder, "e1"));
        assert!(fx.key_exists(&key), "{}", app.last_notice());

        frame(&mut app, &ctx, tall_input(vec![]));
        let more = overflow_button(&mut app, &ctx);
        click_at(&mut app, &ctx, more);
        click_key(&mut app, &ctx, "menu.remove");

        assert!(matches!(
            app.pending_confirm.as_ref().map(|c| &c.action),
            Some(ConfirmAction::RemoveMenuEntry { id }) if id == "e1"
        ));
        assert_eq!(app.menu_entries().len(), 1, "removed before confirming");
        app.resolve_confirm(&ctx, true);
        assert!(app.menu_entries().is_empty());
        assert!(fx.key_exists(&key), "keys must stay until Apply");

        click_key(&mut app, &ctx, "tweaks.apply");
        run_until_idle(&mut app, &ctx);
        assert!(!fx.key_exists(&key), "{}", app.last_notice());
    }

    /// Center of the rightmost `…` button, which is the page's.
    fn overflow_button(app: &mut WinTuneApp, ctx: &egui::Context) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        ctx.viewport_for(egui::ViewportId::ROOT, |v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, widgets)| widgets.iter())
                .filter(|w| w.rect.size() == kit::OVERFLOW_BUTTON)
                .map(|w| w.rect.center())
                // The sidebar has a button of the same size: the page's is
                // the one further right.
                .max_by(|a, b| a.x.total_cmp(&b.x))
        })
        .expect("a ... button")
    }

    #[test]
    fn the_windows_11_banner_opens_the_explorer_page() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        let banner = app.tr.t("menu.win11_banner");
        assert!(texts(&mut app, &ctx).contains(&banner));

        click_key(&mut app, &ctx, "menu.use_classic");
        assert_eq!(app.active_tab, ActiveTab::Tweaks);

        let mut other = page_app(&fx, &ctx);
        other.env.is_windows_11 = false;
        assert!(!texts(&mut other, &ctx).contains(&banner));
    }

    #[test]
    fn drawing_the_page_never_edits_the_config() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let mut app = page_app(&fx, &ctx);
        let before = app.config.clone();

        texts(&mut app, &ctx);

        assert_eq!(app.config, before);
    }
}
