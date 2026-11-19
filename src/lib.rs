//! WinTune core library: everything that touches the registry or the file
//! system lives here so it can be unit-tested without the GUI.

pub mod associations;
pub mod clock;
pub mod detector;
pub mod gallery;
pub mod models;
pub mod regsnap;
pub mod theme;
