use terrarium::{
    App, AppCreationError, Color3, Engine, Instance, InstanceId, MeshPart, Outline, OutlineMode,
    Terrarium, eframe, egui,
};

struct OutlinesApp {
    mesh_part: InstanceId,
    outline: Option<InstanceId>,
    mode: OutlineMode,
}

impl OutlinesApp {
    fn set_mode(&mut self, engine: &mut Engine, mode: OutlineMode) {
        if let Some(outline) = self.outline {
            let Some(outline) = engine.get_mut::<Outline>(outline) else {
                return;
            };
            if !outline.destroy() {
                return;
            };
        };

        let Some(mesh_part) = engine.get_mut::<MeshPart>(self.mesh_part) else {
            return;
        };
        self.outline =
            Some(mesh_part.add_child(Outline::new(mode).with_color(Color3::WHITE).with_width(2.0)));
        self.mode = mode;
    }
}

impl App for OutlinesApp {
    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut selected_mode = self.mode;
        egui::Panel::top("outline_modes")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Outline:");
                    ui.selectable_value(&mut selected_mode, OutlineMode::Toon, "Toon");
                    ui.selectable_value(&mut selected_mode, OutlineMode::Silhouette, "Silhouette");
                    ui.selectable_value(&mut selected_mode, OutlineMode::Stencil, "Stencil");
                });
            });

        if selected_mode != self.mode {
            self.set_mode(engine, selected_mode);
        }
    }
}

fn main() {
    Terrarium::new()
        .run(|engine| {
            let mesh = engine.add_mesh(include_bytes!("../assets/Duck.glb"))?;

            let mesh_part = engine.add_child(MeshPart::new(mesh));

            let mut app = OutlinesApp {
                mesh_part,
                outline: None,
                mode: OutlineMode::Stencil,
            };
            app.set_mode(engine, app.mode);
            Ok::<OutlinesApp, AppCreationError>(app)
        })
        .unwrap();
}
