use terrarium::{AppCreationError, HasPVInstance as _, Instance, MeshPart, Terrarium, glam::Vec3};

fn main() {
    Terrarium::new()
        .run(|engine| {
            let helmet_mesh = engine.add_mesh(include_bytes!("../assets/DamagedHelmet.glb"))?;

            engine.add_child(
                MeshPart::new(helmet_mesh)
                    .with_name("DamagedHelmet")
                    .with_position(Vec3::new(0.0, 0.75, 0.0)),
            );

            Ok::<(), AppCreationError>(())
        })
        .unwrap();
}
