#[path = "support.rs"]
mod support;

use taffy::prelude::length;
use tregui::egui_taffy::{TuiBuilderLogic, taffy, tui};

fn main() -> Result<(), trengine::AppCreationError> {
    support::run(
        "Taffy: flex row",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                tui(ui, ui.id().with("flex-row"))
                    .reserve_available_space()
                    .style(taffy::Style {
                        display: taffy::Display::Flex,
                        flex_direction: taffy::FlexDirection::Row,
                        align_items: Some(taffy::AlignItems::Stretch),
                        gap: length(12.0),
                        ..Default::default()
                    })
                    .show(|tui| {
                        for label in ["Explorer", "Viewport", "Inspector"] {
                            tui.style(taffy::Style {
                                flex_grow: 1.0,
                                ..Default::default()
                            })
                            .add_with_border(|tui| {
                                tui.heading(label);
                                tui.label("A flex item sized by its parent layout.");
                            });
                        }
                    });
            });
        },
    )
}
