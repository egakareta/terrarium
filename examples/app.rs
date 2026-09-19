use terrarium::{
    App, Color3, Easing, Engine, Face, HasBasePart, HasLight, HasMaterials, HasPVInstance, HasPart,
    HasPointLight, HasSpotLight, Instance, Material, Part, PartShape, PointLight, RendererError,
    Repeat, SpotLight, Terrarium, Texture, TextureColorSpace, TextureFilter, Tween, eframe, egui,
    glam::Vec3,
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
    engine.with_clock_time(18.0);

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

    let ground = engine.add_child_ref(
        Part::new()
            .with_name("Ground")
            .with_shape(PartShape::Block)
            .with_position(Vec3::new(0.0, -0.1, 0.0))
            .with_size(Vec3::new(42.0, 0.2, 42.0))
            .with_material(Material::textured(dirt).with_roughness(0.9)),
    );

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        ground.add_child(
            Part::new()
                .with_shape(PartShape::Block)
                .with_position(Vec3::new(x, 0.22, -4.0))
                .with_size(Vec3::new(0.72, 0.45, 0.72))
                .with_material(Material::textured(cobblestone).with_roughness(0.82)),
        );
    }

    let tower = engine.add_child_with(
        Part::new()
            .with_shape(PartShape::Cylinder)
            .with_position(Vec3::new(-3.4, 1.0, -1.8))
            .with_size(Vec3::new(1.2, 2.0, 1.2))
            .with_color(Color3::new(0.76, 0.30, 0.14))
            .with_material(Material::default().with_metallic(0.82).with_roughness(0.24))
            .with_orientation(Vec3::new(0.0, -33.0, 0.0))
            .with_material_slot(Face::Top, Material::textured(grass_top))
            .with_material_slot(Face::Right, Material::textured(grass_side))
            .with_material_slot(Face::Bottom, Material::textured(dirt)),
        |part| {
            part.add_child(
                SpotLight::new()
                    .with_name("TowerSpotLight")
                    .with_color(Color3::new(0.18, 0.45, 1.0))
                    .with_brightness(4.0)
                    .with_face(Face::Back)
                    .with_angle(55.0),
            );
        },
    );

    engine.add_child(
        Part::new()
            .with_name("GrassBlock")
            .with_shape(PartShape::Block)
            .with_position(Vec3::new(3.2, 0.8, -2.3))
            .with_size(Vec3::new(1.5, 1.6, 1.5))
            .with_orientation(Vec3::new(0.0, 31.0, 0.0))
            .with_material(
                Material::textured(grass_side)
                    .with_filter(TextureFilter::Nearest)
                    .with_roughness(0.82),
            )
            .with_material_slot(
                Face::Top,
                Material::textured(grass_top)
                    .with_filter(TextureFilter::Nearest)
                    .with_roughness(0.82),
            )
            .with_material_slot(
                Face::Bottom,
                Material::textured(dirt)
                    .with_filter(TextureFilter::Nearest)
                    .with_roughness(0.9),
            ),
    );

    let platform = engine.add_child(
        Part::new()
            .with_shape(PartShape::Block)
            .with_position(Vec3::new(-1.0, 0.55, 1.6))
            .with_size(Vec3::new(2.0, 1.1, 2.0))
            .with_color(Color3::new(0.60, 0.68, 0.50))
            .with_material(Material::textured(festival_lantern).with_roughness(0.82)),
    );

    let orb = engine.add_child_with(
        Part::new()
            .with_shape(PartShape::Ball)
            .with_position(Vec3::new(3.3, 2.8, 0.0))
            .with_size(Vec3::splat(0.8))
            .with_color(Color3::new(0.95, 0.72, 0.22))
            .with_material(Material::default().with_roughness(0.18))
            .with_can_collide(false),
        |part| {
            part.add_child(
                PointLight::new()
                    .with_color(Color3::new(1.0, 0.35, 0.08))
                    .with_brightness(3.5)
                    .with_shadows(true)
                    .with_range(8.0),
            );
        },
    );

    engine.add_child(
        Part::new()
            .with_shape(PartShape::Wedge)
            .with_position(Vec3::new(2.7, 1.5, 2.2))
            .with_size(Vec3::new(1.1, 3.0, 1.1))
            .with_color(Color3::new(0.76, 0.30, 0.14))
            .with_orientation(Vec3::new(0.0, 26.0, 0.0)),
    );

    engine.add_child(
        Part::new()
            .with_shape(PartShape::CornerWedge)
            .with_position(Vec3::new(-4.7, 0.6, 3.1))
            .with_size(Vec3::new(1.8, 1.2, 1.8))
            .with_color(Color3::new(0.10, 0.48, 0.47))
            .with_orientation(Vec3::new(0.0, -46.0, 0.0)),
    );

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
