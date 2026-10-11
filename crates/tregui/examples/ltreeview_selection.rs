#[path = "support.rs"]
mod support;

use tregui::{
    egui,
    egui_ltreeview::{Action, TreeView},
};

fn main() -> eframe::Result<()> {
    let mut selected = Vec::new();
    support::run(
        "Tree view: selection",
        |_| {},
        move |ui| {
            ui.vertical(|ui| {
                ui.label("Click to select; Shift-click for a range; Ctrl/Command-click to toggle.");
                let (_, actions) = TreeView::new(ui.make_persistent_id("selection-tree"))
                    .allow_multi_selection(true)
                    .range_selection_modifier(egui::Modifiers::SHIFT)
                    .set_selection_modifier(egui::Modifiers::COMMAND)
                    .show(ui, |tree| {
                        tree.dir(0, "Workspace");
                        tree.leaf(1, "Camera");
                        tree.leaf(2, "Light");
                        tree.leaf(3, "Ground");
                        tree.leaf(4, "Props");
                        tree.close_dir();
                    });
                for action in actions {
                    if let Action::SetSelected(ids) = action {
                        selected = ids;
                    }
                }
                ui.separator();
                ui.label(format!("Selected node IDs: {selected:?}"));
            });
        },
    )
}
