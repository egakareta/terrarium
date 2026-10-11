#[path = "support.rs"]
mod support;

use taffy::prelude::length;
use tregui::{
    egui,
    egui_taffy::{TuiBuilderLogic, taffy, tui},
};

fn main() -> eframe::Result<()> {
    support::run(
        "Taffy: egui widgets",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                tui(ui, ui.id().with("egui-widgets"))
                    .reserve_available_space()
                    .style(taffy::Style {
                        display: taffy::Display::Flex,
                        flex_direction: taffy::FlexDirection::Column,
                        align_items: Some(taffy::AlignItems::Stretch),
                        gap: length(10.0),
                        ..Default::default()
                    })
                    .show(|tui| {
                        tui.ui(|ui| {
                            let mut enabled = true;
                            ui.label("A normal egui Ui inside a Taffy node:");
                            ui.checkbox(&mut enabled, "Enabled");
                        });
                        tui.ui_add(egui::Button::new("An egui widget leaf"));
                        tui.add_with_border(|tui| {
                            tui.label("A nested Taffy container");
                        });
                    });
            });
        },
    )
}
