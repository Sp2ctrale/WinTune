//! Translation of the engines' message codes. The engines return enums
//! (`RuleProblem`, `DriftReason`, `CustomReason`, `LoadError`); the UI turns
//! them into text in the chosen language here. Arguments follow the enum
//! fields.

use super::Tr;
use crate::associations::{DriftField, DriftReason, RuleProblem};
use crate::context_menu::{EntryProblem, EntryStatus, Place};
use crate::persist::LoadError;
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
