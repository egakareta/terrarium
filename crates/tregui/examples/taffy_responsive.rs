#[path = "support.rs"]
mod support;

use taffy::prelude::length;
use tregui::egui_taffy::{TuiBuilderLogic, taffy, tui};

fn main() -> Result<(), trengine::AppCreationError> {
    support::run(
        "Taffy: responsive",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                let is_wide = ui.available_width() >= 620.0;
                ui.label(if is_wide {
                    "Wide layout: row"
                } else {
                    "Narrow layout: column"
                });
                tui(ui, ui.id().with("responsive-layout"))
                    .reserve_available_space()
                    .style(taffy::Style {
                        display: taffy::Display::Flex,
                        flex_direction: if is_wide {
                            taffy::FlexDirection::Row
                        } else {
                            taffy::FlexDirection::Column
                        },
                        align_items: Some(taffy::AlignItems::Stretch),
                        gap: length(12.0),
                        ..Default::default()
                    })
                    .show(|tui| {
                        tui.style(taffy::Style {
                            flex_grow: if is_wide { 0.0 } else { 1.0 },
                            ..Default::default()
                        })
                        .add_with_border(|tui| {
                            tui.heading("Navigation");
                            tui.label("Scene tree");
                            tui.label("Assets");
                        });
                        tui.style(taffy::Style {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .add_with_border(|tui| {
                            tui.heading("Content");
                            tui.label("The layout direction follows available width.");
                        });
                    });
            });
        },
    )
}
