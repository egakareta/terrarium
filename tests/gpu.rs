use terrarium::{glam::*, *};

#[test]
fn imported_gltf_is_rendered() -> Result<(), terrarium::AppCreationError> {
    use terrarium::HasMaterials as _;
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|engine| {
            let mesh = engine.add_mesh(include_bytes!("../assets/DamagedHelmet.glb"))?;
            engine.add_child(MeshPart::new(mesh).with_position(Vec3::new(0.0, 0.75, 0.0)));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    let meshpart = engine.get_all::<MeshPart>().next().unwrap();
    assert!(
        terrarium::Face::ALL.into_iter().any(|slot| meshpart
            .material_slot(slot)
            .textures()
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
fn screen_pixels_are_readable() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|engine| {
            engine.lighting.clear_skybox();
            engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
            let lantern = engine.add_texture(Texture::from_bytes(
                include_bytes!("../assets/festival_lantern.png"),
                TextureColorSpace::Srgb,
            )?)?;

            engine.add_child(
                Part::new()
                    .with_name("Lantern")
                    .with_shape(PartShape::Block)
                    .with_position(Vec3::new(0.0, 1.0, 0.0))
                    .with_size(Vec3::new(2.0, 2.0, 2.0))
                    .with_material(Material::textured(lantern)),
            );
            Ok::<(), AppCreationError>(())
        })
        .unwrap()
        .unwrap();
    let renderer = engine.renderer();
    let pixels = renderer.read_pixels()?;
    let top_left = pixels[0];
    let center = pixels[64 * 128 + 64];
    let rendered_pixel_count = pixels
        .iter()
        .filter(|pixel| **pixel != [0, 0, 0, 255])
        .count();
    println!("screen pixels: top_left={top_left:?}, center={center:?}");
    assert_eq!(top_left, [0, 0, 0, 255]);
    assert!(rendered_pixel_count > 0);
    Ok(())
}
