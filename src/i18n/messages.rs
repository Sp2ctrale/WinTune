//! Translation of the engines' message codes. The engines return enums
//! (`RuleProblem`, `DriftReason`, `CustomReason`, `LoadError`); the UI turns
//! them into text in the chosen language here. Arguments follow the enum
//! fields.

use super::Tr;
use crate::associations::{DriftField, DriftReason, RuleProblem};
use crate::context_menu::{EntryProblem, EntryStatus, ExistingState, Locked, Place, RenameProblem};
use crate::environment::{EnvProblem, EnvWarning};
use crate::mouse_keyboard::{
    MkFailure, MkIssue, MkProblem, Section, SettingId, SettingValue, WHEEL_PAGESCROLL,
};
use crate::persist::LoadError;
use crate::startup::{Source, StartupLock, StartupState};
use crate::tweaks::{CustomReason, TweakGroup};
use crate::visual_effects::{EffectId, VeFailure, VeIssue, VeProblem, VeSection};

pub fn rule_problem(tr: Tr, p: &RuleProblem) -> String {
    match p {
        RuleProblem::InvalidExtension { ext } => tr.t_args("problem.invalid_extension", &[ext]),
        RuleProblem::ReservedExtension { ext } => tr.t_args("problem.reserved_extension", &[ext]),
        RuleProblem::InvalidProgId { prog_id } => tr.t_args("problem.invalid_prog_id", &[prog_id]),
        RuleProblem::ReservedProgId {
            prog_id,
            suggestion,
        } => tr.t_args("problem.reserved_prog_id", &[prog_id, suggestion]),
        RuleProblem::DuplicateExtension { ext } => tr.t_args("problem.duplicate_extension", &[ext]),
        RuleProblem::SharedProgId { prog_id } => tr.t_args("problem.shared_prog_id", &[prog_id]),
    }
}

/// Name of a place where an entry can appear.
pub fn place_name(tr: Tr, place: Place) -> String {
    match place {
        Place::Folder => tr.t("menu.place.folder"),
        Place::Background => tr.t("menu.place.background"),
        Place::Drive => tr.t("menu.place.drive"),
        Place::Files => tr.t("menu.place.files"),
    }
}

pub fn entry_problem(tr: Tr, p: &EntryProblem) -> String {
    match p {
        EntryProblem::EmptyLabel => tr.t("menu.problem.empty_label"),
        EntryProblem::LabelTooLong => tr.t("menu.problem.label_too_long"),
        EntryProblem::NoPlace => tr.t("menu.problem.no_place"),
        EntryProblem::ProgramNotFound => tr.t("menu.problem.program_not_found"),
        EntryProblem::NotAnExe => tr.t("menu.problem.not_an_exe"),
        EntryProblem::LeadingAt => tr.t("menu.problem.leading_at"),
        EntryProblem::IconNotFound => tr.t("menu.problem.icon_not_found"),
        EntryProblem::DuplicateLabel(place) => {
            tr.t_args("menu.problem.duplicate_label", &[&place_name(tr, *place)])
        }
        EntryProblem::ShellOnFiles => tr.t("menu.problem.shell_on_files"),
    }
}

/// Why a new name for an existing entry is refused (the same texts as an
/// added entry's label problems).
pub fn rename_problem(tr: Tr, p: &RenameProblem) -> String {
    let as_entry = match p {
        RenameProblem::Empty => EntryProblem::EmptyLabel,
        RenameProblem::TooLong => EntryProblem::LabelTooLong,
        RenameProblem::LeadingAt => EntryProblem::LeadingAt,
    };
    entry_problem(tr, &as_entry)
}

/// The state of an existing right-click entry.
pub fn existing_state(tr: Tr, s: ExistingState) -> String {
    match s {
        ExistingState::Shown => tr.t("menu.state.shown"),
        ExistingState::HiddenByWinTune => tr.t("menu.state.hidden"),
        ExistingState::HiddenElsewhere => tr.t("menu.state.hidden_elsewhere"),
    }
}

/// Why WinTune cannot change an existing entry.
pub fn locked(tr: Tr, l: &Locked) -> String {
    match l {
        Locked::MachineWide => tr.t("menu.locked.machine"),
        Locked::Unreadable(detail) => tr.t_args("menu.locked.unreadable", &[detail]),
    }
}

/// The state of a startup app.
pub fn startup_state(tr: Tr, s: StartupState) -> String {
    match s {
        StartupState::On => tr.t("startup.state.on"),
        StartupState::Off => tr.t("startup.state.off"),
        StartupState::OffElsewhere => tr.t("startup.state.off_elsewhere"),
    }
}

/// Why WinTune cannot change a startup app.
pub fn startup_lock(tr: Tr, l: &StartupLock) -> String {
    match l {
        StartupLock::Unreadable(detail) => tr.t_args("startup.locked.unreadable", &[detail]),
        StartupLock::UnknownShape => tr.t("startup.locked.unknown"),
        StartupLock::OffForEveryone => tr.t("startup.locked.machine"),
        StartupLock::AllUsersOnly => tr.t("startup.locked.all_users"),
    }
}

/// Where a startup app comes from.
pub fn startup_source(tr: Tr, s: Source) -> String {
    match s {
        Source::UserRun => tr.t("startup.source.user_run"),
        Source::MachineRun => tr.t("startup.source.machine_run"),
        Source::MachineRun32 => tr.t("startup.source.machine_run32"),
        Source::UserFolder => tr.t("startup.source.user_folder"),
        Source::CommonFolder => tr.t("startup.source.common_folder"),
    }
}

/// Why an environment variable edit is refused.
pub fn env_problem(tr: Tr, p: &EnvProblem) -> String {
    match p {
        EnvProblem::EmptyName => tr.t("env.problem.empty_name"),
        EnvProblem::NameHasEquals => tr.t("env.problem.name_equals"),
        EnvProblem::NameHasNul => tr.t("env.problem.name_nul"),
        EnvProblem::NameTooLong => tr.t("env.problem.name_too_long"),
        EnvProblem::NameTaken(name) => tr.t_args("env.problem.name_taken", &[name]),
        EnvProblem::PathName => tr.t("env.problem.path_name"),
        EnvProblem::EmptyValue => tr.t("env.problem.empty_value"),
        EnvProblem::ValueTooLong => tr.t("env.problem.value_too_long"),
        EnvProblem::ValueHasNul => tr.t("env.problem.value_nul"),
        EnvProblem::EmptyEntry => tr.t("env.problem.empty_entry"),
        EnvProblem::EntryHasSemicolon => tr.t("env.problem.entry_semicolon"),
        EnvProblem::Gone(name) => tr.t_args("env.problem.gone", &[name]),
        EnvProblem::NotText(name) => tr.t_args("env.problem.not_text", &[name]),
        EnvProblem::Twice(name) => tr.t_args("env.problem.twice", &[name]),
        EnvProblem::ChangedSinceScan(name) => tr.t_args("env.problem.changed_since_scan", &[name]),
    }
}

/// What an environment Apply asks the user to confirm first.
pub fn env_warning(tr: Tr, w: &EnvWarning) -> String {
    match w {
        EnvWarning::Removes(name) => tr.t_args("env.warning.removes", &[name]),
        EnvWarning::PathTooLong(len) => tr.t_args("env.warning.path_too_long", &[len]),
        EnvWarning::PathChangedElsewhere => tr.t("env.warning.path_changed"),
    }
}

/// The title of a mouse and keyboard setting.
pub fn mk_title(tr: Tr, id: SettingId) -> String {
    match id {
        SettingId::PointerSpeed => tr.t("mk.pointer_speed.title"),
        SettingId::PointerPrecision => tr.t("mk.pointer_precision.title"),
        SettingId::PointerTrails => tr.t("mk.pointer_trails.title"),
        SettingId::PointerSonar => tr.t("mk.pointer_sonar.title"),
        SettingId::PointerVanish => tr.t("mk.pointer_vanish.title"),
        SettingId::SnapToDefault => tr.t("mk.snap_to_default.title"),
        SettingId::SwapButtons => tr.t("mk.swap_buttons.title"),
        SettingId::DoubleClickTime => tr.t("mk.double_click_time.title"),
        SettingId::ClickLock => tr.t("mk.click_lock.title"),
        SettingId::ClickLockTime => tr.t("mk.click_lock_time.title"),
        SettingId::WheelLines => tr.t("mk.wheel_lines.title"),
        SettingId::WheelChars => tr.t("mk.wheel_chars.title"),
        SettingId::ScrollInactive => tr.t("mk.scroll_inactive.title"),
        SettingId::KeyRepeatDelay => tr.t("mk.key_repeat_delay.title"),
        SettingId::KeyRepeatRate => tr.t("mk.key_repeat_rate.title"),
        SettingId::HoverTime => tr.t("mk.hover_time.title"),
        SettingId::MenuShowDelay => tr.t("mk.menu_show_delay.title"),
        SettingId::DragFullWindows => tr.t("mk.drag_full_windows.title"),
    }
}

/// The line under a setting's title, for the settings that have one.
pub fn mk_description(tr: Tr, id: SettingId) -> Option<String> {
    let text = match id {
        SettingId::PointerPrecision => tr.t("mk.pointer_precision.description"),
        SettingId::ClickLock => tr.t("mk.click_lock.description"),
        SettingId::ClickLockTime => tr.t("mk.click_lock_time.description"),
        SettingId::KeyRepeatDelay => tr.t("mk.key_repeat_delay.description"),
        SettingId::KeyRepeatRate => tr.t("mk.key_repeat_rate.description"),
        SettingId::HoverTime => tr.t("mk.hover_time.description"),
        SettingId::MenuShowDelay => tr.t("mk.menu_show_delay.description"),
        SettingId::PointerSpeed
        | SettingId::PointerTrails
        | SettingId::PointerSonar
        | SettingId::PointerVanish
        | SettingId::SnapToDefault
        | SettingId::SwapButtons
        | SettingId::DoubleClickTime
        | SettingId::WheelLines
        | SettingId::WheelChars
        | SettingId::ScrollInactive
        | SettingId::DragFullWindows => return None,
    };
    Some(text)
}

/// The title of a section of the Mouse & keyboard page.
pub fn mk_section(tr: Tr, s: Section) -> String {
    match s {
        Section::Pointer => tr.t("mk.section.pointer"),
        Section::Buttons => tr.t("mk.section.buttons"),
        Section::Wheel => tr.t("mk.section.wheel"),
        Section::Keyboard => tr.t("mk.section.keyboard"),
        Section::WindowsMenus => tr.t("mk.section.windows"),
    }
}

/// A setting's value as the page shows it ("On", "500 ms", "One screen at a
/// time", "{6, 10, 1}"). Without an id (a key this version does not know),
/// a number is shown as it is.
pub fn mk_value(tr: Tr, id: Option<SettingId>, v: &SettingValue) -> String {
    match *v {
        SettingValue::Flag(true) => tr.t("mk.value.on"),
        SettingValue::Flag(false) => tr.t("mk.value.off"),
        SettingValue::Mouse([t1, t2, accel]) => format!("{{{t1}, {t2}, {accel}}}"),
        SettingValue::Number(n) => {
            use SettingId::*;
            match (id, n) {
                (Some(WheelLines), WHEEL_PAGESCROLL) => tr.t("mk.value.one_screen"),
                (Some(WheelLines), 0) => tr.t("mk.value.no_scroll"),
                (Some(PointerTrails), 0 | 1) | (Some(ScrollInactive), 0) => tr.t("mk.value.off"),
                (Some(ScrollInactive), 1 | 2) => tr.t("mk.value.on"),
                (Some(DoubleClickTime | ClickLockTime | HoverTime | MenuShowDelay), _) => {
                    tr.t_args("mk.value.ms", &[&n])
                }
                _ => n.to_string(),
            }
        }
    }
}

/// A setting named by its stored key: its title, else the raw key.
fn mk_key_name(tr: Tr, key: &str) -> String {
    match SettingId::from_key(key) {
        Some(id) => mk_title(tr, id),
        None => key.to_string(),
    }
}

/// Why a mouse and keyboard Apply or Restore was refused (nothing written).
pub fn mk_problem(tr: Tr, p: &MkProblem) -> String {
    match p {
        MkProblem::OutOfRange(id) => tr.t_args("mk.problem.out_of_range", &[&mk_title(tr, *id)]),
        MkProblem::UnknownSetting(key) => tr.t_args("mk.problem.unknown", &[&mk_key_name(tr, key)]),
        MkProblem::Unreadable { id, code } => {
            tr.t_args("mk.problem.unreadable", &[&mk_title(tr, *id), code])
        }
        MkProblem::WrongAccount { process, session } => {
            tr.t_args("mk.problem.wrong_account", &[process, session])
        }
        MkProblem::AccountUnknown(why) => tr.t_args("mk.problem.account_unknown", &[why]),
        MkProblem::NotSaved(why) => tr.t_args("mk.problem.not_saved", &[why]),
        MkProblem::NoOriginal(key) => tr.t_args("mk.problem.no_original", &[&mk_key_name(tr, key)]),
    }
}

/// Why one setting of a mouse and keyboard job failed.
pub fn mk_failure(tr: Tr, f: &MkFailure) -> String {
    match f {
        MkFailure::SetFailed { id, code } => {
            tr.t_args("mk.failure.set", &[&mk_title(tr, *id), code])
        }
        MkFailure::Mismatch { id, wanted, read } => tr.t_args(
            "mk.failure.mismatch",
            &[
                &mk_title(tr, *id),
                &mk_value(tr, Some(*id), read),
                &mk_value(tr, Some(*id), wanted),
            ],
        ),
        MkFailure::Unverified { id, code } => {
            tr.t_args("mk.failure.unverified", &[&mk_title(tr, *id), code])
        }
        MkFailure::StoreNotSaved(why) => tr.t_args("mk.failure.store", &[why]),
    }
}

pub fn mk_issue(tr: Tr, issue: &MkIssue) -> String {
    match issue {
        MkIssue::Problem(p) => mk_problem(tr, p),
        MkIssue::Failure(f) => mk_failure(tr, f),
    }
}

/// The two Visual effects failures that say the effects gated by UI effects
/// were not written (review T3-C6); `title` is the effect's translated
/// title. `None` for every other failure, translated with the rest of the
/// module's messages.
pub fn ve_not_written(tr: Tr, f: &VeFailure, title: &str) -> Option<String> {
    match f {
        VeFailure::RereadFailed { code, .. } => {
            Some(tr.t_args("ve.failure.reread", &[&title, code]))
        }
        VeFailure::GatedNotSaved(why) => Some(tr.t_args("ve.failure.gated_not_saved", &[why])),
        _ => None,
    }
}

/// The title of a visual effect.
pub fn ve_title(tr: Tr, id: EffectId) -> String {
    tr.t(match id {
        EffectId::UiEffects => "ve.ui_effects.title",
        EffectId::WindowAnimation => "ve.window_animation.title",
        EffectId::ClientAreaAnimation => "ve.client_area_animation.title",
        EffectId::ComboboxAnimation => "ve.combobox_animation.title",
        EffectId::ListboxSmoothScroll => "ve.listbox_smooth_scroll.title",
        EffectId::MenuAnimation => "ve.menu_animation.title",
        EffectId::MenuFade => "ve.menu_fade.title",
        EffectId::SelectionFade => "ve.selection_fade.title",
        EffectId::TooltipAnimation => "ve.tooltip_animation.title",
        EffectId::TooltipFade => "ve.tooltip_fade.title",
        EffectId::CursorShadow => "ve.cursor_shadow.title",
        EffectId::DropShadow => "ve.drop_shadow.title",
        EffectId::FontSmoothing => "ve.font_smoothing.title",
        EffectId::FontSmoothingType => "ve.font_smoothing_type.title",
        EffectId::CleartypeContrast => "ve.cleartype_contrast.title",
        EffectId::CaretWidth => "ve.caret_width.title",
        EffectId::FocusBorderWidth => "ve.focus_border_width.title",
        EffectId::FocusBorderHeight => "ve.focus_border_height.title",
    })
}

/// The line under an effect's title, for the effects that have one.
pub fn ve_description(tr: Tr, id: EffectId) -> Option<String> {
    (id == EffectId::UiEffects).then(|| tr.t("ve.ui_effects.description"))
}

/// The title of a section of the Visual effects page.
pub fn ve_section(tr: Tr, s: VeSection) -> String {
    tr.t(match s {
        VeSection::UiEffects => "ve.section.ui_effects",
        VeSection::Animations => "ve.section.animations",
        VeSection::MenusTooltips => "ve.section.menus",
        VeSection::Shadows => "ve.section.shadows",
        VeSection::Text => "ve.section.text",
        VeSection::CaretFocus => "ve.section.caret",
    })
}

/// `n` with its digits grouped by threes, `sep` between the groups.
fn grouped(n: u32, sep: &str) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

/// An effect's value as the page shows it ("On", "Slide", "ClearType",
/// "1,200", "2 px"). Without an id (a key this version does not know), a
/// flag is On / Off and a number is shown as it is.
pub fn ve_value(tr: Tr, id: Option<EffectId>, v: &SettingValue) -> String {
    use crate::visual_effects::{FE_CLEARTYPE, FE_STANDARD};
    use EffectId::*;
    match *v {
        SettingValue::Flag(on) => match (id, on) {
            (Some(MenuFade | TooltipFade), true) => tr.t("ve.value.fade"),
            (Some(MenuFade | TooltipFade), false) => tr.t("ve.value.slide"),
            (_, true) => tr.t("ve.value.on"),
            (_, false) => tr.t("ve.value.off"),
        },
        SettingValue::Number(n) => match id {
            Some(FontSmoothingType) if n == FE_STANDARD => tr.t("ve.value.standard"),
            Some(FontSmoothingType) if n == FE_CLEARTYPE => tr.t("ve.value.cleartype"),
            Some(FontSmoothingType) => tr.t_args("ve.value.unknown", &[&n]),
            Some(CleartypeContrast) => grouped(n, &tr.t("ve.value.thousands")),
            Some(CaretWidth | FocusBorderWidth | FocusBorderHeight) => {
                tr.t_args("ve.value.px", &[&n])
            }
            _ => n.to_string(),
        },
        SettingValue::Mouse([a, b, c]) => format!("{{{a}, {b}, {c}}}"),
    }
}

/// An effect named by its stored key: its title, else the raw key.
fn ve_key_name(tr: Tr, key: &str) -> String {
    match EffectId::from_key(key) {
        Some(id) => ve_title(tr, id),
        None => key.to_string(),
    }
}

/// Why a visual effects Apply or Restore was refused (nothing written).
pub fn ve_problem(tr: Tr, p: &VeProblem) -> String {
    match p {
        VeProblem::OutOfRange(id) => tr.t_args("ve.problem.out_of_range", &[&ve_title(tr, *id)]),
        VeProblem::UnknownSetting(key) => tr.t_args("ve.problem.unknown", &[&ve_key_name(tr, key)]),
        VeProblem::Unreadable { id, code } => {
            tr.t_args("ve.problem.unreadable", &[&ve_title(tr, *id), code])
        }
        VeProblem::WrongAccount { process, session } => {
            tr.t_args("ve.problem.wrong_account", &[process, session])
        }
        VeProblem::AccountUnknown(why) => tr.t_args("ve.problem.account_unknown", &[why]),
        VeProblem::NotSaved(why) => tr.t_args("ve.problem.not_saved", &[why]),
        VeProblem::NoOriginal(key) => tr.t_args("ve.problem.no_original", &[&ve_key_name(tr, key)]),
    }
}

/// Why one effect of a visual effects job failed. The two failures that
/// say gated rows were not written read as [`ve_not_written`] gives them.
pub fn ve_failure(tr: Tr, f: &VeFailure) -> String {
    match f {
        VeFailure::SetFailed { id, code } => {
            tr.t_args("ve.failure.set", &[&ve_title(tr, *id), code])
        }
        VeFailure::Mismatch { id, wanted, read } => tr.t_args(
            "ve.failure.mismatch",
            &[
                &ve_title(tr, *id),
                &ve_value(tr, Some(*id), read),
                &ve_value(tr, Some(*id), wanted),
            ],
        ),
        VeFailure::Unverified { id, code } => {
            tr.t_args("ve.failure.unverified", &[&ve_title(tr, *id), code])
        }
        VeFailure::HeldByMaster { id, .. } => tr.t_args("ve.failure.held", &[&ve_title(tr, *id)]),
        VeFailure::StoreNotSaved(why) => tr.t_args("ve.failure.store", &[why]),
        VeFailure::RereadFailed { id, .. } => {
            ve_not_written(tr, f, &ve_title(tr, *id)).unwrap_or_default()
        }
        VeFailure::GatedNotSaved(_) => ve_not_written(tr, f, "").unwrap_or_default(),
    }
}

pub fn ve_issue(tr: Tr, issue: &VeIssue) -> String {
    match issue {
        VeIssue::Problem(p) => ve_problem(tr, p),
        VeIssue::Failure(f) => ve_failure(tr, f),
    }
}

/// The restore question's line for the master's companions: "Also
/// restores {n} effects: {titles}".
pub fn ve_companions(tr: Tr, n: usize, titles: &[String]) -> String {
    let key = if n == 1 {
        "ve.also_restores_one"
    } else {
        "ve.also_restores"
    };
    tr.t_args(key, &[&n, &titles.join(", ")])
}

/// The pill of an entry: its state in the registry.
pub fn entry_status(tr: Tr, s: &EntryStatus) -> String {
    match s {
        EntryStatus::Applied => tr.t("menu.status.applied"),
        EntryStatus::NotApplied => tr.t("menu.status.not_applied"),
        EntryStatus::Changed(_) => tr.t("menu.status.changed"),
    }
}

pub fn drift_reason(tr: Tr, r: &DriftReason) -> String {
    match r {
        DriftReason::DefaultChanged {
            ext,
            found,
            expected,
        } => tr.t_args("drift.default_changed", &[ext, found, expected]),
        DriftReason::DefaultMissing { ext, expected } => {
            tr.t_args("drift.default_missing", &[ext, expected])
        }
        DriftReason::ProgIdMissing { prog_id } => tr.t_args("drift.prog_id_missing", &[prog_id]),
        DriftReason::ValueDiffers {
            field,
            expected,
            found,
        } => {
            let label = tr.t(match field {
                DriftField::Description => "drift.field.description",
                DriftField::Icon => "drift.field.icon",
                DriftField::OpenCommand => "drift.field.open_command",
            });
            tr.t_args("drift.value_differs", &[&label, expected, found])
        }
        DriftReason::UserChoice {
            choice,
            user,
            ext,
            prog_id,
        } => tr.t_args("drift.user_choice", &[choice, user, ext, prog_id]),
        DriftReason::UserOverride { user, ext, prog_id } => {
            tr.t_args("drift.user_override", &[user, ext, prog_id])
        }
    }
}

pub fn custom_reason(tr: Tr, c: &CustomReason) -> String {
    match c {
        CustomReason::UnexpectedValue { name, value } => {
            tr.t_args("custom.unexpected_value", &[name, value])
        }
        CustomReason::UnexpectedType { name } => tr.t_args("custom.unexpected_type", &[name]),
        CustomReason::Unreadable(e) => tr.t_args("custom.unreadable", &[e]),
    }
}

/// A store or config file that could not be loaded: the path and the raw
/// serde / OS detail inside a translated sentence.
pub fn load_error(tr: Tr, e: &LoadError) -> String {
    match e {
        LoadError::Corrupt { path, detail } => {
            tr.t_args("load.corrupt", &[&path.display(), detail])
        }
        LoadError::Unreadable { path, detail } => {
            tr.t_args("load.unreadable", &[&path.display(), detail])
        }
    }
}

pub fn tweak_title(tr: Tr, id: &str) -> String {
    tr.t(&format!("tweak.{id}.title"))
}

pub fn tweak_description(tr: Tr, id: &str) -> String {
    tr.t(&format!("tweak.{id}.description"))
}

pub fn group_title(tr: Tr, g: TweakGroup) -> String {
    tr.t(&format!("group.{}", g.id()))
}

/// A module label as stored in `history.jsonl` (always English), shown in
/// the current language. Unknown labels are kept as stored.
pub fn history_module(tr: Tr, stored: &str) -> String {
    match stored {
        "File associations" => tr.t("history.module.associations"),
        "Explorer & Taskbar" => tr.t("history.module.explorer"),
        "Start & Privacy" => tr.t("history.module.start"),
        "Context menu" => tr.t("history.module.context_menu"),
        "Startup apps" => tr.t("history.module.startup"),
        "Environment variables" => tr.t("history.module.environment"),
        "Mouse & keyboard" => tr.t("history.module.mouse_keyboard"),
        "Visual effects" => tr.t("history.module.visual_effects"),
        other => other.to_string(),
    }
}

/// An action label as stored in `history.jsonl`, shown in the current
/// language. Unknown labels are kept as stored.
pub fn history_action(tr: Tr, stored: &str) -> String {
    match stored {
        "Apply" => tr.t("history.action.apply"),
        "Rollback" => tr.t("history.action.rollback"),
        "Restore" => tr.t("history.action.restore"),
        other => other.to_string(),
    }
}
