use terrarium::{
    App, Color3, Easing, Engine, Instance, Material, MaterialSlot, Part, PartShape, RendererError,
    Repeat, RunConfig, Texture, TextureColorSpace, TextureFilter, Tween, Workspace, eframe, egui,
    glam::Vec3,
};

struct FpsOverlay;

impl App for FpsOverlay {
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

fn create_workspace(workspace: &mut Workspace) -> Result<(), RendererError> {
    let dirt = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../assets/dirt.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let cobblestone = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../assets/cobblestone.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_top = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../assets/grass_top.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_side = workspace.add_texture(
        Texture::from_bytes(
            include_bytes!("../assets/grass_side.png"),
            TextureColorSpace::Srgb,
        )?
        .rotate180(),
    )?;
    let festival_lantern = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../assets/festival_lantern.png"),
        TextureColorSpace::Srgb,
    )?)?;

    let ground = workspace.add_child_with_ref(Part::new("Ground"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(0.0, -0.1, 0.0));
        part.size = Vec3::new(42.0, 0.2, 42.0);
        part.material = Material::textured(dirt);
        part.material.roughness = 0.9;
    });

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        ground.add_child_with(Part::unnamed(), |part| {
            part.shape = PartShape::Block;
            part.set_position(Vec3::new(x, 0.22, -4.0));
            part.size = Vec3::new(0.72, 0.45, 0.72);
            part.material = Material::textured(cobblestone);
            part.material.roughness = 0.82;
        });
    }

    let tower = workspace.add_child_with(Part::unnamed(), |part| {
        part.shape = PartShape::Cylinder;
        part.set_position(Vec3::new(-3.4, 1.0, -1.8));
        part.size = Vec3::new(1.2, 2.0, 1.2);
        part.color = Color3::new(0.76, 0.30, 0.14);
        part.material = Material {
            metallic: 0.82,
            roughness: 0.24,
            ..Material::default()
        };
        part.set_orientation(Vec3::new(0.0, -33.0, 0.0));
        part.set_material_slot(MaterialSlot::Top, Material::textured(grass_top));
        part.set_material_slot(MaterialSlot::Right, Material::textured(grass_side));
        part.set_material_slot(MaterialSlot::Bottom, Material::textured(dirt));
    });

    workspace.add_child_with(Part::new("GrassBlock"), |part| {
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
        part.set_material_slot(MaterialSlot::Base, grass_side);
        part.set_material_slot(MaterialSlot::Top, grass_top);
        part.set_material_slot(MaterialSlot::Bottom, dirt);
        part.set_material_slot(MaterialSlot::Front, grass_side);
        part.set_material_slot(MaterialSlot::Back, grass_side);
        part.set_material_slot(MaterialSlot::Left, grass_side);
        part.set_material_slot(MaterialSlot::Right, grass_side);
    });

    let platform = workspace.add_child_with(Part::unnamed(), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(-1.0, 0.55, 1.6));
        part.size = Vec3::new(2.0, 1.1, 2.0);
        part.color = Color3::new(0.60, 0.68, 0.50);
        part.material = Material::textured(festival_lantern);
        part.material.roughness = 0.82;
    });

    let orb = workspace.add_child_with(Part::unnamed(), |part| {
        part.shape = PartShape::Ball;
        part.set_position(Vec3::new(3.3, 2.8, 0.0));
        part.size = Vec3::splat(0.8);
        part.color = Color3::new(0.95, 0.72, 0.22);
        part.material.roughness = 0.18;
        part.can_collide = false;
    });

    workspace.add_child_with(Part::unnamed(), |part| {
        part.shape = PartShape::Wedge;
        part.set_position(Vec3::new(2.7, 1.5, 2.2));
        part.size = Vec3::new(1.1, 3.0, 1.1);
        part.color = Color3::new(0.76, 0.30, 0.14);
        part.set_orientation(Vec3::new(0.0, 26.0, 0.0));
    });

    workspace.add_child_with(Part::unnamed(), |part| {
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
    workspace
        .tweens_mut()
        .add_position(platform, platform_position);
    workspace.tweens_mut().add_orientation(
        platform,
        Tween::new(Vec3::ZERO, Vec3::new(0.0, 6480.0, 0.0), 6.0).repeat_forever(),
    );

    workspace.tweens_mut().add_position(
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
    workspace.tweens_mut().add_orientation(
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
    workspace.tweens_mut().add_position(
        orb,
        Tween::path(orbit_points, std::f32::consts::TAU / 0.8)
            .easing(Easing::EaseInOut)
            .repeat_forever(),
    );
    workspace.tweens_mut().add_orientation(
        orb,
        Tween::new(
            Vec3::ZERO,
            Vec3::new(0.0, 360.0, 0.0),
            std::f32::consts::TAU / 0.8,
        )
        .repeat_forever(),
    );

    Ok(())
}

fn main() {
    RunConfig::new()
        .with_title("App")
        .with_canvas_id("the_canvas_id")
        .run(|_creation_context, engine| {
            engine.set_clear_color([0.012, 0.019, 0.050, 1.0]);
            create_workspace(&mut engine.workspace).map(|()| FpsOverlay)
        })
        .unwrap();
}
