//! The "Context menu" page: entries the user adds to the Explorer right-click
//! menu, per account. The page body is built on top of the app wiring in
//! `mod.rs` and `jobs.rs`.

use super::{kit, WinTuneApp};
use eframe::egui;

impl WinTuneApp {
    pub(super) fn show_context_menu_tab(&mut self, ctx: &egui::Context) {
        // Before the panel, so the bar spans the whole page width and the
        // progress of a running job shows.
        self.page_action_bar(ctx, None, |_| {});
        let tr = self.tr;
        let name = tr.t("nav.context_menu");
        let subtitle = tr.t_args("menu.subtitle", &[&self.env.user_label]);
        Self::page_panel(ctx, |ui| {
            kit::page_header(ui, &["WinTune", &name], &name, Some(&subtitle), |_| {});
        });
    }
}
