#[path = "support.rs"]
mod support;

use taffy::prelude::length;
use tregui::egui_taffy::{TuiBuilderLogic, taffy, tui};

fn main() -> Result<(), trengine::AppCreationError> {
    support::run(
        "Taffy: flex column",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                tui(ui, ui.id().with("flex-column"))
                    .reserve_available_space()
                    .style(taffy::Style {
                        display: taffy::Display::Flex,
                        flex_direction: taffy::FlexDirection::Column,
                        align_items: Some(taffy::AlignItems::Stretch),
                        gap: length(8.0),
                        ..Default::default()
                    })
                    .show(|tui| {
                        tui.heading("Scene hierarchy");
                        tui.style(taffy::Style {
                            flex_grow: 1.0,
                            ..Default::default()
                        })
                        .add_with_border(|tui| {
                            for item in ["Camera", "Key light", "Ground", "Props"] {
                                tui.label(item);
                            }
                        });
                        tui.separator();
                        tui.label("Status: ready");
                    });
            });
        },
    )
}
