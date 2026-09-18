use terrarium::{AppConfig, Engine, GltfError, Instance, MeshPart, eframe, glam::Vec3};

fn initialize(
    _creation_context: &eframe::CreationContext<'_>,
    engine: &mut Engine,
) -> Result<(), GltfError> {
    engine.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    let mut helmet = MeshPart::from_gltf(
        "DamagedHelmet",
        include_bytes!("../assets/DamagedHelmet.glb"),
        &mut engine.workspace,
    )?;
    helmet.set_position(Vec3::new(0.0, 0.75, 0.0));
    engine.add_child(helmet);
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
    let meshpart = engine.get_all::<MeshPart>().next().unwrap();
    assert!(meshpart.material.textures.emissive.is_some());
    let pixels = engine.renderer().read_pixels()?;
    let rendered_pixels = pixels
        .iter()
        .filter(|pixel| **pixel != [0, 0, 0, 255])
        .collect::<Vec<_>>();
    let rendered_pixel_count = rendered_pixels.len();
    let max_channel_difference = rendered_pixels
        .iter()
        .map(|pixel| pixel[..3].iter().max().unwrap() - pixel[..3].iter().min().unwrap())
        .max()
        .unwrap_or(0);

    assert!(rendered_pixel_count > 0);
    assert!(max_channel_difference > 32);
    Ok(())
}
