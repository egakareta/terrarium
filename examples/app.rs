use terrarium::{
    App, Color3, Easing, Engine, Face, Instance, Material, Part, PartShape, PointLight,
    RendererError, Repeat, SpotLight, Terrarium, Texture, TextureColorSpace, TextureFilter, Tween,
    eframe, egui, glam::Vec3,
};

struct SceneApp;

impl App for SceneApp {
    fn after_update(
        &mut self,
        engine: &mut Engine,
        context: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        let delta_seconds = context.input(|input| input.stable_dt.min(0.1));
        engine.advance_clock_time(delta_seconds, 0.25);
    }

    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let fps = engine.renderer().fps();
        egui::Area::new("fps_counter".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(180))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(format!("FPS: {fps:.0}"));
                    });
            });
    }
}

fn initialize(engine: &mut Engine) -> Result<SceneApp, RendererError> {
    engine.lighting.set_clock_time(18.0);

    let dirt = engine.add_texture(Texture::from_bytes(
        include_bytes!("../assets/dirt.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let cobblestone = engine.add_texture(Texture::from_bytes(
        include_bytes!("../assets/cobblestone.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_top = engine.add_texture(Texture::from_bytes(
        include_bytes!("../assets/grass_top.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_side = engine.add_texture(
        Texture::from_bytes(
            include_bytes!("../assets/grass_side.png"),
            TextureColorSpace::Srgb,
        )?
        .rotate180(),
    )?;
    let festival_lantern = engine.add_texture(Texture::from_bytes(
        include_bytes!("../assets/festival_lantern.png"),
        TextureColorSpace::Srgb,
    )?)?;

    let ground = engine.add_child_with_ref(Part::new().named("Ground"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(0.0, -0.1, 0.0));
        part.size = Vec3::new(42.0, 0.2, 42.0);
        let mut material = Material::textured(dirt);
        material.roughness = 0.9;
        part.set_material(material);
    });

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        ground.add_child_with(Part::new(), |part| {
            part.shape = PartShape::Block;
            part.set_position(Vec3::new(x, 0.22, -4.0));
            part.size = Vec3::new(0.72, 0.45, 0.72);
            let mut material = Material::textured(cobblestone);
            material.roughness = 0.82;
            part.set_material(material);
        });
    }

    let tower = engine.add_child_with(Part::new(), |part| {
        part.shape = PartShape::Cylinder;
        part.set_position(Vec3::new(-3.4, 1.0, -1.8));
        part.size = Vec3::new(1.2, 2.0, 1.2);
        part.color = Color3::new(0.76, 0.30, 0.14);
        part.set_material(Material {
            metallic: 0.82,
            roughness: 0.24,
            ..Material::default()
        });
        part.set_orientation(Vec3::new(0.0, -33.0, 0.0));
        part.set_material_slot(Face::Top, Material::textured(grass_top));
        part.set_material_slot(Face::Right, Material::textured(grass_side));
        part.set_material_slot(Face::Bottom, Material::textured(dirt));
        part.add_child_with(SpotLight::new().named("TowerSpotLight"), |light| {
            light.color = Color3::new(0.18, 0.45, 1.0);
            light.brightness = 4.0;
            light.face = Face::Back;
            light.angle = 55.0;
        });
    });

    engine.add_child_with(Part::new().named("GrassBlock"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(3.2, 0.8, -2.3));
        part.size = Vec3::new(1.5, 1.6, 1.5);
        part.set_orientation(Vec3::new(0.0, 31.0, 0.0));
        // Each face carries its own full PBR material: the top face is grass,
        // the bottom is dirt, and the sides blend grass over dirt.
        let mut grass_side = Material::textured(grass_side).with_filter(TextureFilter::Nearest);
        grass_side.roughness = 0.82;
        let mut grass_top = Material::textured(grass_top).with_filter(TextureFilter::Nearest);
        grass_top.roughness = 0.82;
        let mut dirt = Material::textured(dirt).with_filter(TextureFilter::Nearest);
        dirt.roughness = 0.9;
        part.set_material(grass_side);
        part.set_material_slot(Face::Top, grass_top);
        part.set_material_slot(Face::Bottom, dirt);
        part.set_material_slot(Face::Front, grass_side);
        part.set_material_slot(Face::Back, grass_side);
        part.set_material_slot(Face::Left, grass_side);
        part.set_material_slot(Face::Right, grass_side);
    });

    let platform = engine.add_child_with(Part::new(), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(-1.0, 0.55, 1.6));
        part.size = Vec3::new(2.0, 1.1, 2.0);
        part.color = Color3::new(0.60, 0.68, 0.50);
        let mut material = Material::textured(festival_lantern);
        material.roughness = 0.82;
        part.set_material(material);
    });

    let orb = engine.add_child_with(Part::new(), |part| {
        part.shape = PartShape::Ball;
        part.set_position(Vec3::new(3.3, 2.8, 0.0));
        part.size = Vec3::splat(0.8);
        part.color = Color3::new(0.95, 0.72, 0.22);
        part.set_material(Material {
            roughness: 0.18,
            ..Default::default()
        });
        part.can_collide = false;
        part.add_child_with(PointLight::new(), |light| {
            light.color = Color3::new(1.0, 0.35, 0.08);
            light.brightness = 3.5;
            light.range = 8.0;
            light.shadows = true;
        });
    });

    engine.add_child_with(Part::new(), |part| {
        part.shape = PartShape::Wedge;
        part.set_position(Vec3::new(2.7, 1.5, 2.2));
        part.size = Vec3::new(1.1, 3.0, 1.1);
        part.color = Color3::new(0.76, 0.30, 0.14);
        part.set_orientation(Vec3::new(0.0, 26.0, 0.0));
    });

    engine.add_child_with(Part::new(), |part| {
        part.shape = PartShape::CornerWedge;
        part.set_position(Vec3::new(-4.7, 0.6, 3.1));
        part.size = Vec3::new(1.8, 1.2, 1.8);
        part.color = Color3::new(0.10, 0.48, 0.47);
        part.set_orientation(Vec3::new(0.0, -46.0, 0.0));
    });

    let platform_position = Tween::path(
        [
            Vec3::new(-1.0, 0.55, 1.6),
            Vec3::new(0.1, 0.67, 1.6),
            Vec3::new(1.2, 0.55, 1.6),
            Vec3::new(0.1, 0.43, 1.6),
            Vec3::new(-1.0, 0.55, 1.6),
        ],
        6.0,
    )
    .easing(Easing::EaseInOut)
    .repeat(Repeat::Forever);
    engine
        .tweens_mut()
        .add_position(platform, platform_position);
    engine.tweens_mut().add_orientation(
        platform,
        Tween::new(Vec3::ZERO, Vec3::new(0.0, 6480.0, 0.0), 6.0).repeat_forever(),
    );

    engine.tweens_mut().add_position(
        tower,
        Tween::path(
            [
                Vec3::new(-3.4, 1.0, -1.8),
                Vec3::new(-3.4, 1.35, -1.8),
                Vec3::new(-3.4, 1.0, -1.8),
            ],
            4.2,
        )
        .easing(Easing::EaseInOut)
        .repeat_forever(),
    );
    engine.tweens_mut().add_orientation(
        tower,
        Tween::new(Vec3::new(0.0, -33.0, 0.0), Vec3::new(0.0, 273.0, 0.0), 8.4)
            .easing(Easing::Linear)
            .repeat_forever()
            .yoyo(),
    );

    let orbit_points = (0..=8).map(|step| {
        let angle = step as f32 * std::f32::consts::TAU / 8.0;
        Vec3::new(
            angle.cos() * 3.3,
            2.8 + (angle * 1.7 / 0.8).sin() * 0.45,
            angle.sin() * 3.3,
        )
    });
    engine.tweens_mut().add_position(
        orb,
        Tween::path(orbit_points, std::f32::consts::TAU / 0.8)
            .easing(Easing::EaseInOut)
            .repeat_forever(),
    );
    engine.tweens_mut().add_orientation(
        orb,
        Tween::new(
            Vec3::ZERO,
            Vec3::new(0.0, 360.0, 0.0),
            std::f32::consts::TAU / 0.8,
        )
        .repeat_forever(),
    );
    Ok(SceneApp)
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}

#[test]
fn app_loads() -> Result<(), terrarium::AppCreationError> {
    Terrarium::new()
        .with_size([32, 32])
        .with_headless(Some(1))
        .run(initialize)?;
    Ok(())
}
