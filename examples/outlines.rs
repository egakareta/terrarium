use terrarium::{
    App, AppCreationError, Color3, Engine, Instance, InstanceId, MeshPart, Outline, OutlineMode,
    Terrarium, glam::Vec3,
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
    fn state(&self, _engine: &Engine) -> terrarium::serde_json::Value {
        terrarium::serde_json::json!({ "mode": format!("{:?}", self.mode), "width": self.width, "focus": self.focus_on_duck })
    }
    fn command(
        &mut self,
        engine: &mut Engine,
        command: &str,
        payload: terrarium::serde_json::Value,
    ) -> Result<terrarium::serde_json::Value, AppCreationError> {
        match command {
            "mode" => self.set_mode(
                engine,
                match payload.as_str() {
                    Some("Toon") => OutlineMode::Toon,
                    Some("Silhouette") => OutlineMode::Silhouette,
                    Some("Stencil") => OutlineMode::Stencil,
                    _ => return Err("invalid outline mode".into()),
                },
            ),
            "width" => {
                self.width = payload
                    .as_f64()
                    .filter(|v| (0.5..=10.0).contains(v))
                    .ok_or("invalid outline width")? as f32;
                self.set_mode(engine, self.mode);
            }
            "focus" => {
                self.focus_on_duck = payload.as_bool().ok_or("focus must be a boolean")?;
                engine
                    .current_camera
                    .set_subject(self.focus_on_duck.then_some(self.mesh_part));
            }
            _ => return Err("unknown command".into()),
        }
        Ok(terrarium::serde_json::Value::Null)
    }
}

fn main() {
    Terrarium::new()
        .with_ui("outlines")
        .run(|engine| {
            let mesh = engine.add_mesh(include_bytes!("../crates/trscene/fixtures/Duck.glb"))?;

            // the mesh is bounded Y +0.86, lets apply an offset
            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for vertex in &mesh.mesh().vertices {
                let position = Vec3::from_array(vertex.position);
                min = min.min(position);
                max = max.max(position);
            }
            let focus_offset = if min.is_finite() && max.is_finite() {
                (min + max) * 0.5
            } else {
                Vec3::ZERO
            };
            let mesh_part = engine.add_child(MeshPart::new(mesh));
            engine.workspace.camera_controller_mut().subject_offset = focus_offset;

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
