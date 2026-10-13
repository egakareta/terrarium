use terrarium::{
    App, Color3, Engine, HasBasePart, HasCamera, HasLight, HasMaterials, HasPVInstance, HasPart,
    HasPointLight, Instance, InstanceId, Material, Part, PartShape, PointLight, RendererError,
    Terrarium,
    glam::{Mat4, Vec3},
};

struct PhysicsApp {
    dynamic_parts: Vec<InstanceId>,
    initial_transforms: Vec<(InstanceId, Mat4)>,
}

impl PhysicsApp {
    fn reset(&self, engine: &mut Engine) {
        for &(id, transform) in &self.initial_transforms {
            if let Some(part) = engine.get_mut::<Part>(id) {
                part.with_pose(transform);
            }
        }
    }

    fn kick(&self, engine: &mut Engine) {
        for (index, &id) in self.dynamic_parts.iter().enumerate() {
            if let Some(body) = engine.physics_mut().body_mut(id) {
                let direction = if index % 2 == 0 { 1.0 } else { -1.0 };
                body.apply_impulse(
                    terrarium::rapier3d::math::Vector::new(2.5 * direction, 3.0, 0.0),
                    true,
                );
            }
        }
    }
}

impl App for PhysicsApp {
    fn state(&self, engine: &Engine) -> terrarium::serde_json::Value {
        let gravity = engine.physics().gravity();
        terrarium::serde_json::json!({ "bodies": self.dynamic_parts.len(), "gravity": [gravity.x, gravity.y, gravity.z] })
    }
    fn command(
        &mut self,
        engine: &mut Engine,
        command: &str,
        _payload: terrarium::serde_json::Value,
    ) -> Result<terrarium::serde_json::Value, terrarium::AppCreationError> {
        match command {
            "reset" => self.reset(engine),
            "kick" => self.kick(engine),
            _ => return Err("unknown command".into()),
        }
        Ok(terrarium::serde_json::Value::Null)
    }
}

fn initialize(engine: &mut Engine) -> Result<PhysicsApp, RendererError> {
    engine.lighting.with_clock_time(16.5);
    engine.current_camera = terrarium::Camera::new(
        Vec3::new(10.0, 7.5, 13.0),
        Vec3::new(0.0, 2.2, 0.0),
        16.0 / 9.0,
    )
    .with_zfar(100.0);

    let floor_material = Material::from_color(Color3::new(0.12, 0.16, 0.22)).with_roughness(0.88);
    engine.add_child(
        Part::new()
            .with_name("Floor")
            .with_size(Vec3::new(18.0, 0.5, 18.0))
            .with_position(Vec3::new(0.0, -0.25, 0.0))
            .with_material(floor_material),
    );

    for (position, size) in [
        (Vec3::new(-4.0, 0.8, -1.5), Vec3::new(3.0, 1.6, 3.0)),
        (Vec3::new(4.0, 1.2, 1.0), Vec3::new(3.0, 2.4, 3.0)),
    ] {
        engine.add_child(
            Part::new()
                .with_size(size)
                .with_position(position)
                .with_color(Color3::new(0.18, 0.25, 0.34)),
        );
    }

    let light_anchor = engine.add_child_ref(
        Part::new()
            .with_can_collide(false)
            .with_size(Vec3::splat(0.1))
            .with_position(Vec3::new(0.0, 5.0, 2.0))
            .with_name("LightAnchor"),
    );
    light_anchor.add_child(
        PointLight::new()
            .with_color(Color3::new(1.0, 0.78, 0.55))
            .with_brightness(8.0)
            .with_shadows(true)
            .with_range(18.0),
    );

    let mut dynamic_parts = Vec::new();
    let mut initial_transforms = Vec::new();
    for layer in 0..5 {
        for column in 0..4 {
            let index = layer * 4 + column;
            let x = (column as f32 - 1.5) * 1.15 + if layer % 2 == 0 { 0.0 } else { 0.18 };
            let z = (layer as f32 % 2.0 - 0.5) * 0.9;
            let position = Vec3::new(x, 2.0 + layer as f32 * 1.05, z);
            let shape = match index % 3 {
                0 => PartShape::Block,
                1 => PartShape::Ball,
                _ => PartShape::Cylinder,
            };
            let transform = Mat4::from_translation(position);
            let id = engine.add_child(
                Part::new()
                    .with_shape(shape)
                    .with_anchored(false)
                    .with_size(if shape == PartShape::Ball {
                        Vec3::splat(0.9)
                    } else {
                        Vec3::new(0.95, 0.95, 0.95)
                    })
                    .with_color(Color3::new(
                        0.24 + (index % 3) as f32 * 0.18,
                        0.42 + (index % 2) as f32 * 0.18,
                        0.72 - (index % 4) as f32 * 0.08,
                    ))
                    .with_position(position)
                    .with_material(Material::default().with_metallic(0.12).with_roughness(0.3)),
            );
            dynamic_parts.push(id);
            initial_transforms.push((id, transform));
        }
    }

    Ok(PhysicsApp {
        dynamic_parts,
        initial_transforms,
    })
}

fn main() {
    Terrarium::new().with_ui("physics").run(initialize).unwrap();
}

#[test]
fn physics_example_loads_headlessly() -> Result<(), terrarium::AppCreationError> {
    let engine = Terrarium::new()
        .with_ui("physics")
        .with_size([64, 64])
        .with_headless(Some(2))
        .run(initialize)?
        .expect("native headless runs return the engine");
    assert_eq!(engine.get_all::<Part>().count(), 24);
    Ok(())
}
