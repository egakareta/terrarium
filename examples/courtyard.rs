use std::sync::Arc;

use terrarium::{
    Color3, Easing, InstanceId, Material, MaterialSlot, Part, PartShape, Renderer, RendererError,
    Repeat, Texture, TextureColorSpace, Tween, Workspace, egui,
    glam::Vec3,
    winit::{
        application::ApplicationHandler,
        dpi::PhysicalSize,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        window::{Window, WindowId},
    },
};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    workspace: Workspace,
}

impl App {
    fn new() -> Self {
        Self {
            workspace: create_workspace().unwrap(),
            ..Default::default()
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Courtyard")
            .with_inner_size(PhysicalSize::new(1280, 720));
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        let size = window.inner_size();
        self.workspace
            .current_camera
            .resize(size.width, size.height);

        let mut renderer = match pollster::block_on(Renderer::new(window.clone())) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("renderer initialization failed: {error}");
                event_loop.exit();
                return;
            }
        };
        renderer.set_clear_color(wgpu::Color {
            r: 0.012,
            g: 0.019,
            b: 0.050,
            a: 1.0,
        });

        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        if let Some(renderer) = &mut self.renderer {
            let _ = renderer.on_window_event(&event);
        }
        self.workspace.process_window_event(&event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.workspace
                    .current_camera
                    .resize(size.width, size.height);
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let delta = self.renderer.as_mut().map(|renderer| renderer.delta_secs());
                if let Some(delta) = delta {
                    self.workspace.update(delta);

                    let fps = self
                        .renderer
                        .as_ref()
                        .map(Renderer::fps)
                        .unwrap_or_default();
                    if let Some(renderer) = &mut self.renderer {
                        let render_result = renderer.render_egui(&self.workspace, |ui| {
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
                        });
                        if let Err(error) = render_result {
                            eprintln!("rendering stopped: {error}");
                            event_loop.exit();
                        }
                    }
                }
                window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn create_workspace() -> Result<Workspace, RendererError> {
    let mut workspace = Workspace::new();
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
    let grass_side = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../assets/grass_side.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let festival_lantern = workspace.add_texture(Texture::from_bytes(
        include_bytes!("../assets/festival_lantern.png"),
        TextureColorSpace::Srgb,
    )?)?;

    let ground = workspace.add_child_with(Part::new("Ground"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(0.0, -0.1, 0.0));
        part.size = Vec3::new(42.0, 0.2, 42.0);
        part.color = Color3::new(0.07, 0.12, 0.13);
    });
    let mut stone_blocks = Vec::new();

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        let stone_block = workspace.add_child_with(Part::unnamed(), |part| {
            part.shape = PartShape::Block;
            part.set_position(Vec3::new(x, 0.22, -4.0));
            part.size = Vec3::new(0.72, 0.45, 0.72);
            part.color = Color3::new(0.29, 0.34, 0.39);
        });
        stone_blocks.push(stone_block);
    }

    let tower = workspace.add_child_with(Part::new("CopperTower"), |part| {
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
    });

    let grass_block = workspace.add_child_with(Part::new("GrassBlock"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(3.2, 0.8, -2.3));
        part.size = Vec3::new(1.5, 1.6, 1.5);
        part.color = Color3::new(0.10, 0.48, 0.47);
        part.set_orientation(Vec3::new(0.0, 31.0, 0.0));
    });

    workspace.add_child_with(Part::new("PalePlatform"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(-1.0, 0.55, 1.6));
        part.size = Vec3::new(2.0, 1.1, 2.0);
        part.color = Color3::new(0.60, 0.68, 0.50);
    });

    let platform = workspace
        .find_first_child::<Part>("PalePlatform")
        .map(|(id, _)| id)
        .expect("PalePlatform was just created");

    let orb = workspace.add_child_with(Part::new("OrbitingOrb"), |part| {
        part.shape = PartShape::Ball;
        part.set_position(Vec3::new(3.3, 2.8, 0.0));
        part.size = Vec3::splat(0.8);
        part.color = Color3::new(0.95, 0.72, 0.22);
        part.material.roughness = 0.18;
        part.can_collide = false;
    });

    workspace.add_child_with(Part::new("CopperPillar"), |part| {
        part.shape = PartShape::Wedge;
        part.set_position(Vec3::new(2.7, 1.5, 2.2));
        part.size = Vec3::new(1.1, 3.0, 1.1);
        part.color = Color3::new(0.76, 0.30, 0.14);
        part.set_orientation(Vec3::new(0.0, 26.0, 0.0));
    });

    workspace.add_child_with(Part::new("TealMonolith"), |part| {
        part.shape = PartShape::CornerWedge;
        part.set_position(Vec3::new(-4.7, 0.6, 3.1));
        part.size = Vec3::new(1.8, 1.2, 1.8);
        part.color = Color3::new(0.10, 0.48, 0.47);
        part.set_orientation(Vec3::new(0.0, -46.0, 0.0));
    });

    let mut set_material = |part_id: InstanceId, material: Material| {
        if let Some(part) = workspace.get_mut::<Part>(part_id) {
            part.color = Color3::WHITE;
            part.material = material;
            part.material.roughness = 0.82;
        }
    };
    set_material(ground, Material::textured(dirt));
    for &part_id in &stone_blocks {
        set_material(part_id, Material::textured(cobblestone));
    }

    if let Some(part) = workspace.get_mut::<Part>(grass_block) {
        part.color = Color3::WHITE;
        part.set_material_slot(MaterialSlot::Base, Material::textured(grass_side));
        part.set_material_slot(MaterialSlot::Top, Material::textured(grass_top));
        part.set_material_slot(MaterialSlot::Bottom, Material::textured(dirt));
        part.set_material_slot(MaterialSlot::Front, Material::textured(grass_side));
        part.set_material_slot(MaterialSlot::Back, Material::textured(grass_side));
        part.set_material_slot(MaterialSlot::Left, Material::textured(grass_side));
        part.set_material_slot(MaterialSlot::Right, Material::textured(grass_side));
        part.material.roughness = 0.82;
    }

    if let Some((_, part)) = workspace.find_first_child::<Part>("PalePlatform") {
        part.color = Color3::WHITE;
        part.material = Material::textured(festival_lantern);
        part.material.roughness = 0.82;
    }

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

    Ok(workspace)
}

fn main() {
    env_logger::init();
    println!(
        "WASD move | drag with left mouse to look | Space/Ctrl rise and descend | Shift sprint"
    );
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run event loop");
}
