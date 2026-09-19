use terrarium::{
    HasBasePart, HasMaterials, HasPVInstance, HasPart, Instance, Material, Part, PartShape,
    RendererError, Terrarium, Texture, TextureColorSpace, glam::Vec3,
};

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
            Ok::<(), RendererError>(())
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
