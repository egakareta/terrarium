#[path = "support.rs"]
mod support;

use tregui::{egui, egui_text_outline::OutlinedLabel};

fn main() -> eframe::Result<()> {
    support::run(
        "Text outline: basic",
        |_| {},
        |ui| {
            ui.heading("Outlined text widget");
            ui.add(
                OutlinedLabel::new("Hello from tregui")
                    .text_color(egui::Color32::WHITE)
                    .outline(egui::Stroke::new(
                        2.0,
                        egui::Color32::from_rgb(42, 160, 150),
                    )),
            );
        },
    )
}
