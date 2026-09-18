use terrarium::{AppConfig, Engine, GltfError, Instance, MeshPart, eframe, glam::Vec3};

fn initialize(
    _creation_context: &eframe::CreationContext<'_>,
    engine: &mut Engine,
) -> Result<(), GltfError> {
    engine.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    let mut duck = MeshPart::from_gltf(
        "Duck",
        include_bytes!("../assets/DamagedHelmet.glb"),
        &mut engine.workspace,
    )?;
    duck.set_position(Vec3::new(0.0, 0.75, 0.0));
    engine.add_child(duck);
    Ok(())
}

fn main() {
    AppConfig::new()
        .with_canvas_id("the_canvas_id")
        .run(initialize)
        .unwrap();
}

#[test]
fn imported_gltf_is_rendered() -> Result<(), terrarium::AppCreationError> {
    let engine = AppConfig::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(initialize)?
        .unwrap();
    let rendered_pixel_count = engine
        .renderer()
        .read_pixels()?
        .into_iter()
        .filter(|pixel| *pixel != [0, 0, 0, 255])
        .count();

    assert!(rendered_pixel_count > 0);
    Ok(())
}
