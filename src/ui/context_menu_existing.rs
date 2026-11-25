//! The "Windows" and "Other programs" sections of the Context menu page:
//! entries already in the right-click menu, which the user hides, shows or
//! renames for this account. Changes stay pending until Apply.

use super::menu_existing_state::{existing_change, PendingExisting};
use super::{kit, polish, ActiveTab, WinTuneApp};
use crate::context_menu::{find_catalog, rename_problem, ExistingEntry, ExistingState, Mechanism};
use crate::i18n::{messages, Tr};
use eframe::egui::{self, RichText};
use kit::{space, text_size, PillKind};

/// Width of the rename dialog (the confirmation dialog's).
const RENAME_DIALOG_WIDTH: f32 = 440.0;

/// Id of the text box of the rename dialog.
fn rename_field_id() -> egui::Id {
    egui::Id::new("menu_rename_field")
}

/// Which of the two sections a row is drawn in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Windows,
    Others,
}

/// What the user did on a row this frame.
#[derive(Clone, Copy)]
enum RowAction {
    /// The toggle: the wanted hidden state.
    Hidden(bool),
    Rename,
    ResetName,
    Restore,
}

fn hidden_now(entry: &ExistingEntry) -> bool {
    entry.state != ExistingState::Shown
}

/// The row's toggle: shown (on) or hidden (off), the pending wish over the
/// registry's state.
fn wanted_shown(entry: &ExistingEntry, pending: &PendingExisting) -> bool {
    !pending.hidden.unwrap_or_else(|| hidden_now(entry))
}

/// The accent pill of a row whose pending wish differs from the registry:
/// a hide or show first, else a rename.
fn pending_pill(tr: Tr, entry: &ExistingEntry, pending: &PendingExisting) -> Option<String> {
    let change = existing_change(entry, pending)?;
    Some(match (change.hidden != hidden_now(entry), change.hidden) {
        (true, true) => tr.t("menu.pill.will_hide"),
        (true, false) => tr.t("menu.pill.will_show"),
        (false, _) => tr.t("menu.pill.will_rename"),
    })
}

/// A `…` menu item, disabled with `reason` when there is one.
fn menu_item<'a>(label: &'a str, reason: Option<&'a str>) -> kit::MenuItem<'a> {
    kit::MenuItem {
        label,
        enabled: reason.is_none(),
        disabled_hint: reason,
    }
}

/// The row's places, in `Place` order.
fn places_line(tr: Tr, entry: &ExistingEntry) -> String {
    entry
        .places
        .iter()
        .map(|p| messages::place_name(tr, *p))
        .collect::<Vec<_>>()
        .join(", ")
}

impl WinTuneApp {
    /// No scan result yet, or a scan running: the rows may be stale (pending
    /// changes are cleared when an Apply ends, before its rescan does).
    fn menu_scanning(&self) -> bool {
        self.menu_scan.is_none() || self.menu_scan_job.is_some()
    }

    /// Pending hides and renames that differ from the registry (the action
    /// bar's count).
    pub(super) fn menu_existing_pending_count(&self) -> usize {
        self.menu_existing_changes().len()
    }

    fn menu_scanning_line(&self, ui: &mut egui::Ui) {
        let p = kit::current_palette(ui.ctx());
        ui.label(
            RichText::new(self.tr.t("menu.scanning"))
                .size(text_size::BODY)
                .color(p.text_secondary),
        );
    }

    /// The "Windows" section: catalog entries present on this PC. Not drawn
    /// once a scan found none.
    pub(super) fn show_menu_windows_section(&mut self, ui: &mut egui::Ui) {
        let rows = match &self.menu_scan {
            Some(scan) if !self.menu_scanning() => {
                if scan.windows.is_empty() {
                    return;
                }
                Some(scan.windows.clone())
            }
            _ => None,
        };
        let mut actions = Vec::new();
        kit::section(ui, &self.tr.t("menu.windows"), |ui| match &rows {
            None => self.menu_scanning_line(ui),
            Some(rows) => {
                for entry in rows {
                    if let Some(action) = self.existing_row(ui, entry, Section::Windows) {
                        actions.push((entry.clone(), action));
                    }
                }
            }
        });
        for (entry, action) in actions {
            self.apply_row_action(&entry, action);
        }
    }

    /// Rows of Other programs whose label or origin contains the search
    /// text (case-insensitive).
    fn visible_others(&self) -> Vec<ExistingEntry> {
        let query = self.menu_search.trim().to_lowercase();
        let Some(scan) = &self.menu_scan else {
            return Vec::new();
        };
        scan.others
            .iter()
            .filter(|e| {
                query.is_empty()
                    || e.label.to_lowercase().contains(&query)
                    || e.origin
                        .as_deref()
                        .is_some_and(|o| o.to_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }

    /// The "Other programs" section: third-party entries, with a search box
    /// (Ctrl+F) on its title line.
    pub(super) fn show_menu_others_section(&mut self, ui: &mut egui::Ui) {
        let tr = self.tr;
        let scanning = self.menu_scanning();
        let none_at_all = self.menu_scan.as_ref().is_none_or(|s| s.others.is_empty());
        let rows = self.visible_others();
        let query = self.menu_search.trim().to_string();
        let mut search = std::mem::take(&mut self.menu_search);
        let mut search_box = None;
        let mut actions = Vec::new();
        kit::section_with_action(
            ui,
            &tr.t("menu.others"),
            |ui| {
                let id = polish::search_box_id(ActiveTab::ContextMenu).expect("search box");
                search_box = Some(
                    ui.add(
                        egui::TextEdit::singleline(&mut search)
                            .id(id)
                            .hint_text(tr.t("menu.search"))
                            .desired_width(kit::SEARCH_WIDTH),
                    ),
                );
            },
            |ui| {
                if scanning {
                    self.menu_scanning_line(ui);
                } else if none_at_all {
                    kit::empty_state(ui, kit::Icon::ContextMenu, &tr.t("menu.others_empty"), None);
                } else if rows.is_empty() {
                    let p = kit::current_palette(ui.ctx());
                    let text = tr.t_args("menu.no_match", &[&query]);
                    ui.label(
                        RichText::new(text)
                            .size(text_size::BODY)
                            .color(p.text_secondary),
                    );
                } else {
                    for entry in &rows {
                        if let Some(action) = self.existing_row(ui, entry, Section::Others) {
                            actions.push((entry.clone(), action));
                        }
                    }
                }
            },
        );
        self.menu_search = search;
        if let Some(search_box) = search_box {
            self.focus_search_if_requested(&search_box);
        }
        for (entry, action) in actions {
            self.apply_row_action(&entry, action);
        }
    }

    /// One existing entry: name and details on the left; then, right to
    /// left, the toggle, the `…` menu and the pills.
    fn existing_row(
        &self,
        ui: &mut egui::Ui,
        entry: &ExistingEntry,
        section: Section,
    ) -> Option<RowAction> {
        let tr = self.tr;
        let pending = self
            .menu_pending
            .get(&entry.id)
            .cloned()
            .unwrap_or_default();
        let mut shown = wanted_shown(entry, &pending);
        let clsid = matches!(entry.mechanism, Mechanism::Clsid(_));
        let busy = self.busy_or_blocked(&self.menu_store_error);
        // Locked first: waiting for a job would not unlock it.
        let change_reason = entry
            .locked
            .as_ref()
            .map(|l| messages::locked(tr, l))
            .or_else(|| busy.clone());

        let title = self.existing_name(entry);
        let mut lines = vec![places_line(tr, entry)];
        match section {
            Section::Windows => {
                if let Some(item) = entry.catalog.and_then(find_catalog) {
                    lines.push(tr.t(item.description_key));
                    lines.extend(item.also_affects_key.map(|key| tr.t(key)));
                }
            }
            Section::Others => {
                if let Some(origin) = &entry.origin {
                    lines.push(tr.t_args("menu.added_by", &[origin]));
                }
            }
        }
        lines.retain(|line| !line.is_empty());
        let description = lines.join("\n");

        let (rename, reset_name, restore) = (
            tr.t("menu.rename"),
            tr.t("menu.reset_name"),
            tr.t("tweaks.restore_original"),
        );
        let mut items = Vec::new();
        if section == Section::Others && entry.renamable() {
            let reason = change_reason.as_deref();
            items.push((menu_item(&rename, reason), RowAction::Rename));
            let renamed = entry
                .original_label
                .as_deref()
                .is_some_and(|original| original != entry.label);
            if renamed || matches!(pending.label, Some(Some(_))) {
                items.push((menu_item(&reset_name, reason), RowAction::ResetName));
            }
        }
        if entry.has_snapshot {
            items.push((menu_item(&restore, busy.as_deref()), RowAction::Restore));
        }
        let more = tr.t("tweaks.more");
        let more_for = tr.t_args("tweaks.more_for", &[&title]);

        let mut action = None;
        let trailing = |ui: &mut egui::Ui| {
            if kit::toggle_enabled(ui, &mut shown, change_reason.as_deref()).changed() {
                action = Some(RowAction::Hidden(!shown));
            }
            if !items.is_empty() {
                let (menu, kinds): (Vec<_>, Vec<_>) = items.into_iter().unzip();
                let salt = format!("menu_existing_{}", entry.id);
                if let Some(i) = kit::overflow_menu(ui, &salt, &more, &more_for, &menu) {
                    action = Some(kinds[i]);
                }
            }
            if clsid {
                kit::pill(ui, &tr.t("tweaks.pill.unofficial"), PillKind::Warning)
                    .on_hover_text(tr.t("tweaks.unofficial_hint"));
            }
            if entry.state == ExistingState::HiddenElsewhere {
                kit::pill(
                    ui,
                    &messages::existing_state(tr, entry.state),
                    PillKind::Neutral,
                );
            }
            // After the toggle, so the pill follows this frame's click.
            let mut now = pending.clone();
            if let Some(RowAction::Hidden(hidden)) = action {
                now.hidden = Some(hidden);
            }
            if let Some(text) = pending_pill(tr, entry, &now) {
                kit::pill(ui, &text, PillKind::Accent);
            }
        };
        let row = kit::SettingRow {
            icon: None,
            title: &title,
            description: (!description.is_empty()).then_some(description.as_str()),
            hover: None,
        };
        // A shell extension hidden through `Blocked` shows once Explorer
        // reloads its handlers.
        if clsid {
            let hint = tr.t("menu.restart_hint");
            row.show_with_note(ui, &hint, &tr.t("refresh.explorer_restart_hint"), trailing);
        } else {
            row.show(ui, trailing);
        }
        action
    }

    fn apply_row_action(&mut self, entry: &ExistingEntry, action: RowAction) {
        match action {
            RowAction::Hidden(hidden) => {
                let pending = self.menu_pending.entry(entry.id.clone()).or_default();
                pending.hidden = (hidden != hidden_now(entry)).then_some(hidden);
            }
            RowAction::Rename => {
                let text = match self.menu_pending.get(&entry.id).map(|p| &p.label) {
                    Some(Some(Some(text))) => text.clone(),
                    _ => entry.label.clone(),
                };
                self.menu_rename = Some((entry.id.clone(), text));
            }
            RowAction::ResetName => {
                let at_original = entry
                    .original_label
                    .as_deref()
                    .is_none_or(|original| original == entry.label);
                let pending = self.menu_pending.entry(entry.id.clone()).or_default();
                pending.label = (!at_original).then_some(None);
            }
            RowAction::Restore => {
                let slots = entry.mechanism.slots().iter().map(|s| s.id()).collect();
                self.request_restore_menu_keys(slots);
            }
        }
        self.menu_pending
            .retain(|_, pending| *pending != PendingExisting::default());
    }

    /// Stores the dialog's new name as pending; the current name is no
    /// rename at all.
    fn set_menu_rename(&mut self, id: &str, text: &str) {
        let text = text.trim();
        let current = self.menu_scan.as_ref().and_then(|s| s.find(id));
        let same = current.is_some_and(|entry| entry.label == text);
        let pending = self.menu_pending.entry(id.to_string()).or_default();
        pending.label = (!same).then(|| Some(text.to_string()));
        self.menu_pending
            .retain(|_, pending| *pending != PendingExisting::default());
    }

    /// The rename dialog of an Other programs row: the new name, why it
    /// cannot be used, and Rename / Cancel. Escape cancels, Enter renames.
    pub(super) fn show_menu_rename_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, mut text)) = self.menu_rename.take() else {
            return;
        };
        let tr = self.tr;
        let (ok_label, cancel_label) = (tr.t("menu.rename_ok"), tr.t("dialog.cancel"));
        let (mut ok, mut cancel) = (false, false);
        kit::dialog(
            ctx,
            "menu_rename",
            &tr.t("menu.rename_title"),
            RENAME_DIALOG_WIDTH,
            |ui| {
                let p = kit::current_palette(ui.ctx());
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .id(rename_field_id())
                        .desired_width(ui.available_width()),
                );
                // Typing goes to the name until another dialog widget is focused.
                if ui.memory(|m| m.focused().is_none()) {
                    field.request_focus();
                }
                let problem = rename_problem(&text).map(|p| messages::rename_problem(tr, &p));
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
                        .add_enabled(allowed, kit::primary_button(p, &ok_label))
                        .clicked();
                    cancel = ui.add(kit::secondary_button(&cancel_label)).clicked();
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ok |= allowed && enter;
                });
            },
        );
        cancel |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if ok {
            self.set_menu_rename(&id, &text);
        } else if !cancel {
            self.menu_rename = Some((id, text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::{kit, ActiveTab, WinTuneApp};
    use crate::context_menu::{Locked, Scan, BLOCKED_KEY};
    use crate::i18n::messages;
    use crate::tasks::BackgroundJob;
    use eframe::egui;
    use egui::{Event, Key, Modifiers};

    /// Size of a toggle.
    const TOGGLE: egui::Vec2 = egui::vec2(40.0, 20.0);
    const GIVE_ACCESS: &str = "{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}";
    const SEVEN_ZIP: &str = "{23170F69-40C1-278A-1000-000100020000}";
    const VERB_COPY: &str = r"Software\Classes\Directory\shell\AppVerb";

    /// The Context menu page, its first scan finished.
    fn page_app(fx: &TempIcons, ctx: &egui::Context) -> WinTuneApp {
        let mut app = fx.app(ctx);
        scan_now(&mut app, ctx);
        app.active_tab = ActiveTab::ContextMenu;
        app
    }

    /// Two frames, so the layout has settled; the texts of the second.
    fn texts(app: &mut WinTuneApp, ctx: &egui::Context) -> Vec<String> {
        frame(app, ctx, tall_input(vec![]));
        drawn_texts(&frame(app, ctx, tall_input(vec![])))
    }

    fn click_text(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        let pos = text_center(&out, text)
            .unwrap_or_else(|| panic!("{text:?} not drawn: {:?}", drawn_texts(&out)));
        click_at(app, ctx, pos);
    }

    fn click_key(app: &mut WinTuneApp, ctx: &egui::Context, key: &str) {
        let text = app.tr.t(key);
        click_text(app, ctx, &text);
    }

    /// Center of the widget of exactly `size` on the row titled `title`: to
    /// its right, at most one row height below the title line.
    fn row_widget(
        app: &mut WinTuneApp,
        ctx: &egui::Context,
        title: &str,
        size: egui::Vec2,
    ) -> egui::Pos2 {
        frame(app, ctx, tall_input(vec![]));
        let out = frame(app, ctx, tall_input(vec![]));
        let at = text_center(&out, title)
            .unwrap_or_else(|| panic!("{title:?} not drawn: {:?}", drawn_texts(&out)));
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
        .unwrap_or_else(|| panic!("no widget of {size:?} on row {title:?}"))
    }

    fn select_all_and_type(app: &mut WinTuneApp, ctx: &egui::Context, text: &str) {
        let key = |pressed| Event::Key {
            key: Key::A,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(
            app,
            ctx,
            tall_input(vec![key(true), key(false), Event::Text(text.into())]),
        );
        frame(app, ctx, tall_input(vec![]));
    }

    fn user_copy_value(fx: &TempIcons, name: &str) -> Option<String> {
        fx.sandbox
            .root
            .open_subkey(fx.hkcu(VERB_COPY))
            .ok()?
            .get_value(name)
            .ok()
    }

    /// Opens the rename dialog of the "Open with App" row.
    fn open_rename(app: &mut WinTuneApp, ctx: &egui::Context) {
        let more = row_widget(app, ctx, "Open with App", kit::OVERFLOW_BUTTON);
        click_at(app, ctx, more);
        click_key(app, ctx, "menu.rename");
        assert!(app.menu_rename.is_some(), "the rename dialog did not open");
        frame(app, ctx, tall_input(vec![]));
        frame(app, ctx, tall_input(vec![]));
    }

    #[test]
    fn toggling_a_windows_entry_marks_it_pending_and_discard_clears_it() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_clsid(GIVE_ACCESS);
        let mut app = page_app(&fx, &ctx);
        let name = app.tr.t("menu.win.give_access");
        let will_hide = app.tr.t("menu.pill.will_hide");
        let discard = app.tr.t("tweaks.discard");
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("menu.windows")), "{shown:?}");
        assert!(shown.contains(&app.tr.t("menu.restart_hint")), "{shown:?}");
        assert!(!shown.contains(&discard), "Discard with nothing pending");

        let toggle = row_widget(&mut app, &ctx, &name, TOGGLE);
        click_at(&mut app, &ctx, toggle);

        assert_eq!(
            app.menu_pending
                .get("win:give_access")
                .and_then(|p| p.hidden),
            Some(true)
        );
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&will_hide), "{shown:?}");
        assert!(
            shown.contains(&app.tr.t_args("menu.pending", &[&1])),
            "{shown:?}"
        );

        click_text(&mut app, &ctx, &discard);

        assert!(app.menu_pending.is_empty());
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&will_hide), "{shown:?}");
        assert!(!shown.contains(&discard), "{shown:?}");
    }

    #[test]
    fn apply_hides_a_third_party_verb_from_the_page() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = page_app(&fx, &ctx);

        let toggle = row_widget(&mut app, &ctx, "Open with App", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        click_key(&mut app, &ctx, "tweaks.apply");
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            user_copy_value(&fx, "ProgrammaticAccessOnly").as_deref(),
            Some(""),
            "{}",
            app.last_notice()
        );
        run_until_scanned(&mut app, &ctx);
        assert!(app.menu_pending.is_empty());

        // The toggle now reads hidden: clicking it asks to show the entry.
        let toggle = row_widget(&mut app, &ctx, "Open with App", TOGGLE);
        click_at(&mut app, &ctx, toggle);
        let id = app.menu_scan.as_ref().unwrap().others[0].id.clone();
        assert_eq!(
            app.menu_pending.get(&id).and_then(|p| p.hidden),
            Some(false)
        );
        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t("menu.pill.will_show")),
            "{shown:?}"
        );
    }

    #[test]
    fn rename_from_the_menu_applies_the_new_label() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = page_app(&fx, &ctx);

        open_rename(&mut app, &ctx);
        select_all_and_type(&mut app, &ctx, "Mine");
        click_key(&mut app, &ctx, "menu.rename_ok");

        assert!(app.menu_rename.is_none(), "the dialog stayed open");
        let id = app.menu_scan.as_ref().unwrap().others[0].id.clone();
        assert_eq!(
            app.menu_pending.get(&id).and_then(|p| p.label.clone()),
            Some(Some("Mine".to_string()))
        );
        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t("menu.pill.will_rename")),
            "{shown:?}"
        );

        click_key(&mut app, &ctx, "tweaks.apply");
        run_until_idle(&mut app, &ctx);

        assert_eq!(
            user_copy_value(&fx, "MUIVerb").as_deref(),
            Some("Mine"),
            "{}",
            app.last_notice()
        );
    }

    #[test]
    fn rename_ok_is_disabled_for_a_leading_at() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = page_app(&fx, &ctx);

        open_rename(&mut app, &ctx);
        select_all_and_type(&mut app, &ctx, "@x");
        let problem = app.tr.t("menu.problem.leading_at");
        assert!(texts(&mut app, &ctx).contains(&problem));
        click_key(&mut app, &ctx, "menu.rename_ok");

        assert!(app.menu_rename.is_some(), "OK closed the dialog");
        assert!(app.menu_pending.is_empty(), "{:?}", app.menu_pending);

        // Escape cancels.
        let escape = Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&mut app, &ctx, tall_input(vec![escape]));
        assert!(app.menu_rename.is_none());
        assert!(app.menu_pending.is_empty());
    }

    #[test]
    fn a_machine_wide_block_shows_a_disabled_toggle_with_the_reason() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        let machine = fx.machine_key();
        let handler = r"Software\Classes\*\shellex\ContextMenuHandlers\7-Zip";
        let (k, _) = machine.create_subkey(handler).unwrap();
        k.set_value("", &SEVEN_ZIP).unwrap();
        let server = format!(r"Software\Classes\CLSID\{SEVEN_ZIP}\InprocServer32");
        let (k, _) = machine.create_subkey(server).unwrap();
        k.set_value("", &r"C:\Apps\7-zip.dll").unwrap();
        let (k, _) = machine.create_subkey(BLOCKED_KEY).unwrap();
        k.set_value(SEVEN_ZIP, &"").unwrap();
        let mut app = page_app(&fx, &ctx);
        let row = app.menu_scan.as_ref().unwrap().others[0].clone();
        assert_eq!(row.locked, Some(Locked::MachineWide));
        let title = app.existing_name(&row);

        let toggle = row_widget(&mut app, &ctx, &title, TOGGLE);
        click_at(&mut app, &ctx, toggle);
        assert!(app.menu_pending.is_empty(), "a locked toggle changed");
        let hover = hover_texts(&mut app, &ctx, toggle);

        let reason = messages::locked(app.tr, &Locked::MachineWide);
        assert!(hover.contains(&reason), "{hover:?}");
        assert!(hover.contains(&app.tr.t("tweaks.pill.unofficial")));
    }

    #[test]
    fn ctrl_f_focuses_the_other_programs_search() {
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

        let id = super::super::polish::search_box_id(ActiveTab::ContextMenu);
        assert_eq!(id, Some(egui::Id::new("search_menu_others")));
        assert!(ctx.memory(|m| m.has_focus(id.unwrap())));
    }

    #[test]
    fn search_filters_by_origin() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        fx.seed_machine_verb("Directory", "OtherVerb", "Other thing");
        let mut app = page_app(&fx, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"Other thing".to_string()), "{shown:?}");

        let id = super::super::polish::search_box_id(ActiveTab::ContextMenu).unwrap();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, &ctx, tall_input(vec![]));
        // "AppVerb" is the origin (the program's file name), not the label.
        frame(
            &mut app,
            &ctx,
            tall_input(vec![Event::Text("APPVERB".into())]),
        );

        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&"Open with App".to_string()), "{shown:?}");
        assert!(!shown.contains(&"Other thing".to_string()), "{shown:?}");

        app.menu_search = "nothing like it".into();
        let shown = texts(&mut app, &ctx);
        assert!(
            shown.contains(&app.tr.t_args("menu.no_match", &[&"nothing like it"])),
            "{shown:?}"
        );
    }

    #[test]
    fn sections_show_the_loading_line_until_the_scan_ends() {
        let fx = TempIcons::new(&[]);
        let ctx = egui::Context::default();
        fx.seed_machine_verb("Directory", "AppVerb", "Open with App");
        let mut app = fx.app(&ctx);
        app.active_tab = ActiveTab::ContextMenu;
        let scanning = app.tr.t("menu.scanning");
        let label = "Open with App".to_string();

        // No scan result yet.
        frame(&mut app, &ctx, tall_input(vec![]));
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        assert_eq!(text_centers(&out, &scanning).len(), 2);
        assert!(!drawn_texts(&out).contains(&label));

        scan_now(&mut app, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(!shown.contains(&scanning), "{shown:?}");
        assert!(shown.contains(&label), "{shown:?}");

        // A rescan running: the last result is kept but not drawn.
        let (release, wait) = std::sync::mpsc::channel::<()>();
        app.menu_scan_job = Some(BackgroundJob::spawn(
            "Reading the right-click menu",
            || {},
            move |_| {
                let _ = wait.recv();
                Scan::default()
            },
        ));
        frame(&mut app, &ctx, tall_input(vec![]));
        let out = frame(&mut app, &ctx, tall_input(vec![]));
        assert_eq!(text_centers(&out, &scanning).len(), 2);
        assert!(!drawn_texts(&out).contains(&label));

        release.send(()).unwrap();
        run_until_scanned(&mut app, &ctx);
        let shown = texts(&mut app, &ctx);
        assert!(shown.contains(&app.tr.t("menu.others_empty")), "{shown:?}");
    }
}
