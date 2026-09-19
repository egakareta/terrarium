use terrarium::{Engine, GltfError, Instance, MeshPart, Terrarium, glam::Vec3};

fn initialize(engine: &mut Engine) -> Result<(), GltfError> {
    engine.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    let helmet_mesh = engine.add_mesh(include_bytes!("../assets/DamagedHelmet.glb"))?;
    let mut helmet = MeshPart::new(helmet_mesh).named("DamagedHelmet");
    helmet.set_position(Vec3::new(0.0, 0.75, 0.0));
    engine.add_child(helmet);
    Ok(())
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}

#[test]
fn imported_gltf_is_rendered() -> Result<(), terrarium::AppCreationError> {
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(initialize)?
        .unwrap();
    let meshpart = engine.get_all::<MeshPart>().next().unwrap();
    assert!(
        terrarium::Face::ALL.into_iter().any(|slot| meshpart
            .material_slot(slot)
            .textures
            .emissive
            .is_some())
    );
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

#[test]
fn baseline() -> Result<(), terrarium::AppCreationError> {
    Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|_| Ok::<(), terrarium::AppCreationError>(()))?;
    Ok(())
}
