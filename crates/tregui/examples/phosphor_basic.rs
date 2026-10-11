#[path = "support.rs"]
mod support;

use tregui::egui_phosphor::{self, regular};

fn main() -> eframe::Result<()> {
    support::run(
        "Phosphor: basic",
        |fonts| egui_phosphor::add_to_fonts(fonts, egui_phosphor::Variant::Regular),
        |ui| {
            ui.vertical(|ui| {
                ui.heading("Mix icons and text");
                ui.label(format!("{} Home", regular::HOUSE));
                ui.label(format!("{} Settings", regular::GEAR));
                ui.label(format!("{} Search", regular::MAGNIFYING_GLASS));
            });
        },
    )
}
