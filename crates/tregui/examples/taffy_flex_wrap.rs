#[path = "support.rs"]
mod support;

use taffy::prelude::{Size, length};
use tregui::egui_taffy::{TuiBuilderLogic, taffy, tui};

fn main() -> Result<(), trengine::AppCreationError> {
    support::run(
        "Taffy: flex wrap",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                tui(ui, ui.id().with("flex-wrap"))
                    .reserve_available_space()
                    .style(taffy::Style {
                        display: taffy::Display::Flex,
                        flex_direction: taffy::FlexDirection::Row,
                        flex_wrap: taffy::FlexWrap::Wrap,
                        gap: length(8.0),
                        ..Default::default()
                    })
                    .show(|tui| {
                        for index in 1..=18 {
                            tui.style(taffy::Style {
                                size: Size {
                                    width: length(120.0),
                                    height: length(56.0),
                                },
                                ..Default::default()
                            })
                            .add_with_border(|tui| {
                                tui.label(format!("Item {index}"));
                            });
                        }
                    });
            });
        },
    )
}
