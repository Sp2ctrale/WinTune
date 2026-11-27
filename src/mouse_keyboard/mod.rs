//! Mouse & keyboard: pointer, button, wheel, keyboard and window-timing
//! settings, read and written through `SystemParametersInfo`.
//!
//! Hard rule: WinTune never writes under `HKCU\Control Panel`. The values
//! live there, but only the system call changes them (it also notifies
//! running programs); the registry is read for the `.reg` backup only.

mod backend;
mod catalog;
mod engine;
mod store;

pub use backend::*;
pub use catalog::*;
pub use engine::*;
pub use store::*;
