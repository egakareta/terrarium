use terrarium::{
    Engine, Instance, Material, MaterialSlot, Part, PartShape, RendererError, Terrarium, Texture,
    TextureColorSpace, glam::Vec3,
};

fn initialize(engine: &mut Engine) -> Result<(), RendererError> {
    engine.clear_skybox();
    engine.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    let lantern = engine.add_texture(Texture::from_bytes(
        include_bytes!("../assets/festival_lantern.png"),
        TextureColorSpace::Srgb,
    )?)?;

    engine.add_child_with(Part::new("Lantern"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(0.0, 1.0, 0.0));
        part.size = Vec3::new(2.0, 2.0, 2.0);
        part.set_material_slot(MaterialSlot::Base, Material::textured(lantern));
    });
    Ok(())
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}

#[test]
fn screen_pixels_are_readable() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(initialize)
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
