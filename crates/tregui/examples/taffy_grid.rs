#[path = "support.rs"]
mod support;

use taffy::prelude::length;
use tregui::egui_taffy::{TuiBuilderLogic, taffy, tui};

fn main() -> eframe::Result<()> {
    support::run(
        "Taffy: grid",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                tui(ui, ui.id().with("grid"))
                    .reserve_available_space()
                    .style(taffy::Style {
                        display: taffy::Display::Grid,
                        grid_template_columns: vec![length(180.0), length(180.0), length(180.0)],
                        grid_template_rows: vec![length(72.0)],
                        gap: length(8.0),
                        ..Default::default()
                    })
                    .show(|tui| {
                        for cell in 0..12 {
                            tui.add_with_border(|tui| {
                                tui.heading(format!("Cell {cell}"));
                                tui.label("Grid item");
                            });
                        }
                    });
            });
        },
    )
}
