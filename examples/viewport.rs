//! Render an arbitrary mesh into a GPU texture and display it in egui.

use terrarium::{
    App, AppCreationError, Color3, Engine, GltfError, HasBasePart, HasPVInstance, Instance, Mesh,
    MeshPart, Part, Terrarium, Viewport, ViewportTexture, eframe, egui, glam::Vec3, push_triangle,
};

struct ViewportApp {
    texture1: ViewportTexture,
    texture2: ViewportTexture,
}

impl App for ViewportApp {
    fn ui(&mut self, _engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Window::new("Viewport").show(ui, |ui| {
            ui.label("This is an image :)");
            let [width, height] = self.texture1.size();
            ui.image((self.texture1.id(), egui::vec2(width as f32, height as f32)));

            let [width, height] = self.texture2.size();
            ui.image((self.texture2.id(), egui::vec2(width as f32, height as f32)));

            // TODO get this working on wasm
            // let pixels = engine.renderer().read_pixels().unwrap();
            // let red_pixels = pixels.iter().filter(|pixel| pixel[0] > 180).count();
            // ui.label(format!("Red pixels: {}", red_pixels));
        });
    }
}

fn create_viewport1() -> Result<Viewport, GltfError> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    push_triangle(
        &mut vertices,
        &mut indices,
        [[-0.75, -0.75, 0.0], [0.75, -0.75, 0.0], [0.0, 0.75, 0.0]],
        [0.95, 0.25, 0.1, 1.0],
    );
    let mut viewport = Viewport::new([256, 256]);

    // triangle
    let handle = viewport
        .workspace_mut()
        .add_mesh(Mesh::new(vertices, indices))?;
    viewport.workspace_mut().add_child(MeshPart::new(handle));

    // background
    viewport.workspace_mut().add_child(
        Part::new()
            .with_color(Color3::BLACK)
            .with_position(Vec3::new(0.0, 0.0, -20.0))
            .with_size(Vec3::new(100.0, 100.0, 10.0)),
    );

    // cube
    viewport
        .workspace_mut()
        .add_child(Part::new().with_position(Vec3::new(0.0, 2.0, 0.0)));
    Ok(viewport)
}

fn create_viewport2() -> Result<Viewport, GltfError> {
    // background
    let mut viewport = Viewport::new([256, 256]);

    viewport.workspace_mut().add_child(
        Part::new()
            .with_color(Color3::WHITE)
            .with_position(Vec3::new(0.0, 0.0, -20.0))
            .with_size(Vec3::new(100.0, 100.0, 10.0)),
    );
    Ok(viewport)
}

fn initialize(engine: &mut Engine) -> Result<ViewportApp, AppCreationError> {
    Ok(ViewportApp {
        texture1: engine.render_viewport(&create_viewport1()?)?,
        texture2: engine.render_viewport(&create_viewport2()?)?,
    })
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}
