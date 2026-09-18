use terrarium::{
    Instance, Material, MaterialSlot, Part, PartShape, RunConfig, Texture, TextureColorSpace,
    TextureError, glam::Vec3,
};

fn main() {
    RunConfig::new()
        .with_canvas_id("the_canvas_id")
        .run(|_creation_context, engine| -> Result<(), TextureError> {
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
        })
        .unwrap();
}
