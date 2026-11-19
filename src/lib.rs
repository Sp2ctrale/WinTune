//! WinTune core library: everything that touches the registry or the file
//! system lives here so it can be unit-tested without the GUI.

pub mod associations;
pub mod clock;
pub mod detector;
pub mod gallery;
pub mod history;
pub mod models;
pub mod persist;
pub mod regsnap;
pub mod tasks;
pub mod theme;
pub mod tweaks;
pub mod ui;
