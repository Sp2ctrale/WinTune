//! Translation of the engines' message codes. The engines return enums
//! (`RuleProblem`, `DriftReason`, `CustomReason`); the UI turns them into
//! text in the chosen language here. Arguments follow the enum fields.

use super::Tr;
use crate::associations::{DriftField, DriftReason, RuleProblem};
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

pub fn tweak_title(tr: Tr, id: &str) -> String {
    tr.t(&format!("tweak.{id}.title"))
}

pub fn tweak_description(tr: Tr, id: &str) -> String {
    tr.t(&format!("tweak.{id}.description"))
}

pub fn group_title(tr: Tr, g: TweakGroup) -> String {
    tr.t(&format!("group.{}", g.id()))
}
