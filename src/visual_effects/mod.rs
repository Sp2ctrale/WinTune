//! Visual effects: the animations, fades, shadows, font smoothing, caret and
//! focus-border settings Windows exposes through `SystemParametersInfoW`.
//!
//! Hard rule: WinTune never writes under `HKCU\Control Panel`. The values
//! live there (most effects are bits in `UserPreferencesMask`), but only the
//! system call changes them, and it also notifies running programs; the
//! registry is read for the `.reg` backup only.

mod backend;
mod catalog;

pub use backend::*;
pub use catalog::*;
