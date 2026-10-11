#[path = "support.rs"]
mod support;

use tregui::egui_ltreeview::TreeView;

fn main() -> eframe::Result<()> {
    support::run(
        "Tree view: basic",
        |_| {},
        |ui| {
            ui.vertical(|ui| {
                let (_, actions) =
                    TreeView::new(ui.make_persistent_id("basic-tree")).show(ui, |tree| {
                        tree.dir(0, "Workspace");
                        tree.dir(1, "Scenes");
                        tree.leaf(2, "Main scene");
                        tree.leaf(3, "Preview scene");
                        tree.close_dir();
                        tree.dir(4, "Assets");
                        tree.leaf(5, "Materials");
                        tree.leaf(6, "Textures");
                        tree.close_dir();
                        tree.close_dir();
                    });
                ui.label(format!("{} tree actions this frame", actions.len()));
            });
        },
    )
}
