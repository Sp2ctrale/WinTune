#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use wintune::ui::{WinTuneApp, APP_VERSION};

fn main() -> eframe::Result<()> {
    let icon_bytes = include_bytes!("../assets/WinTune.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load WinTune.png icon")
        .into_rgba8();

    let (width, height) = image.dimensions();
    let icon = egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("WinTune v{}", APP_VERSION))
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([920.0, 580.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "WinTune",
        options,
        Box::new(|cc| Ok(Box::new(WinTuneApp::new(cc)))),
    )
}
