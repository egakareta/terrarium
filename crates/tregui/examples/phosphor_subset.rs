#[path = "support.rs"]
mod support;

tregui::egui_phosphor::subset! {
    pub mod demo_icons {
        use regular::{ARROW_CLOCKWISE, GEAR, HOUSE};
    }
}

fn main() -> eframe::Result<()> {
    support::run(
        "Phosphor: subset",
        demo_icons::regular::add_to_fonts,
        |ui| {
            ui.vertical(|ui| {
                ui.label(format!("{} Home", demo_icons::regular::HOUSE));
                ui.label(format!("{} Settings", demo_icons::regular::GEAR));
                ui.label(format!("{} Refresh", demo_icons::regular::ARROW_CLOCKWISE));
            });
        },
    )
}
