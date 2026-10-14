#[path = "support.rs"]
mod support;

use tregui::egui_phosphor::{self, regular};

fn main() -> Result<(), trengine::AppCreationError> {
    support::run(
        "Phosphor: toolbar",
        |fonts| egui_phosphor::add_to_fonts(fonts, egui_phosphor::Variant::Regular),
        |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let created = ui.button(format!("{} New", regular::PLUS)).clicked();
                    let saved = ui
                        .button(format!("{} Save", regular::FLOPPY_DISK))
                        .clicked();
                    let deleted = ui.button(format!("{} Delete", regular::TRASH)).clicked();
                    if created || saved || deleted {
                        ui.label("Toolbar action selected");
                    }
                });
                ui.separator();
                ui.label("Use Phosphor glyphs in ordinary egui controls.");
            });
        },
    )
}
