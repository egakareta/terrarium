use terrarium::{
    App, AppCreationError, Color3, Engine, Instance, InstanceId, MeshPart, Outline, OutlineMode,
    Part, Terrarium, eframe, egui,
};

struct OutlinesApp {
    mesh_part: InstanceId,
    outline: Option<InstanceId>,
    mode: OutlineMode,
    width: f32,
    focus_on_duck: bool,
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
        self.outline = Some(
            mesh_part.add_child(
                Outline::new(mode)
                    .with_color(Color3::WHITE)
                    .with_width(self.width),
            ),
        );
        self.mode = mode;
    }
}

impl App for OutlinesApp {
    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut selected_mode = self.mode;
        let mut focus_on_duck = self.focus_on_duck;
        let mut width_changed = false;
        egui::Panel::top("outline_modes")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Outline:");
                    ui.selectable_value(&mut selected_mode, OutlineMode::Toon, "Toon");
                    ui.selectable_value(&mut selected_mode, OutlineMode::Silhouette, "Silhouette");
                    ui.selectable_value(&mut selected_mode, OutlineMode::Stencil, "Stencil");

                    ui.separator();

                    width_changed = ui
                        .add(egui::Slider::new(&mut self.width, 0.5..=10.0).text("Width"))
                        .changed();

                    ui.separator();
                    ui.checkbox(&mut focus_on_duck, "Focus on duck");
                });
            });

        if selected_mode != self.mode || width_changed {
            self.set_mode(engine, selected_mode);
        }

        if focus_on_duck != self.focus_on_duck {
            self.focus_on_duck = focus_on_duck;
            engine
                .workspace
                .current_camera
                .set_subject(focus_on_duck.then_some(self.mesh_part));
        }
    }
}

fn main() {
    Terrarium::new()
        .run(|engine| {
            let mesh = engine.add_mesh(include_bytes!("../assets/Duck.glb"))?;

            let mesh_part = engine.add_child(MeshPart::new(mesh));
            engine.workspace.current_camera.set_subject(Some(mesh_part));

            let mut app = OutlinesApp {
                mesh_part,
                outline: None,
                mode: OutlineMode::Stencil,
                width: 2.0,
                focus_on_duck: true,
            };
            app.set_mode(engine, app.mode);
            Ok::<OutlinesApp, AppCreationError>(app)
        })
        .unwrap();
}
