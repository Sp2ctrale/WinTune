//! WinTune core library: everything that touches the registry or the file
//! system lives here so it can be unit-tested without the GUI.

pub mod associations;
pub mod backups;
pub mod clock;
pub mod context_menu;
pub mod detector;
pub mod gallery;
pub mod history;
pub mod i18n;
pub mod models;
pub mod persist;
pub mod regsnap;
pub mod tasks;
pub mod tweaks;
pub mod ui;
