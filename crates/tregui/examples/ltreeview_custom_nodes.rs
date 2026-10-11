#[path = "support.rs"]
mod support;

use tregui::egui_ltreeview::{NodeBuilder, TreeView};

fn main() -> eframe::Result<()> {
    support::run(
        "Tree view: custom nodes",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                TreeView::new(ui.make_persistent_id("custom-node-tree")).show(ui, |tree| {
                    tree.node(
                        NodeBuilder::dir(0)
                            .default_open(true)
                            .icon(|ui| {
                                ui.label("▣");
                            })
                            .label("Workspace"),
                    );
                    tree.node(
                        NodeBuilder::leaf(1)
                            .icon(|ui| {
                                ui.label("◆");
                            })
                            .label("Custom leaf node"),
                    );
                    tree.close_dir();
                });
            });
        },
    )
}
