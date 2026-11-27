//! Translation of the engines' message codes. The engines return enums
//! (`RuleProblem`, `DriftReason`, `CustomReason`, `LoadError`); the UI turns
//! them into text in the chosen language here. Arguments follow the enum
//! fields.

use super::Tr;
use crate::associations::{DriftField, DriftReason, RuleProblem};
use crate::context_menu::{EntryProblem, EntryStatus, ExistingState, Locked, Place, RenameProblem};
use crate::environment::{EnvProblem, EnvWarning};
use crate::persist::LoadError;
use crate::startup::{Source, StartupLock, StartupState};
use crate::tweaks::{CustomReason, TweakGroup};

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
