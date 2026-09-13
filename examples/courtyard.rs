use std::sync::Arc;

use terrarium::{
    Color3, InstanceId, Material, MaterialSlot, Part, PartShape, Renderer, RendererError, Texture,
    TextureColorSpace, Workspace,
    glam::Vec3,
    winit::{
        application::ApplicationHandler,
        dpi::PhysicalSize,
        event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        keyboard::{KeyCode, PhysicalKey},
        window::{CursorGrabMode, Window, WindowId},
    },
};

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    workspace: Workspace,
    moving_parts: MovingParts,
    animation_time: f32,
    mouse_captured: bool,
}

struct MovingParts {
    ground: InstanceId,
    stone_blocks: Vec<InstanceId>,
    grass_block: InstanceId,
    tower: InstanceId,
    orb: InstanceId,
}

impl App {
    fn new() -> Self {
        let (workspace, moving_parts) = create_workspace();
        Self {
            window: None,
            renderer: None,
            workspace,
            moving_parts,
            animation_time: 0.0,
            mouse_captured: false,
        }
    }

    fn animate_parts(&mut self, delta_seconds: f32) {
        self.animation_time =
            (self.animation_time + delta_seconds.min(0.1)).rem_euclid(std::f32::consts::TAU);
        let time = self.animation_time;

        if let Some((_, part)) = self.workspace.find_first_child("PalePlatform") {
            part.set_position(Vec3::new(
                -1.0 + time.sin() * 2.2,
                0.55 + (time * 2.0).sin() * 0.12,
                1.6,
            ));
            part.set_orientation(Vec3::new(0.0, time.to_degrees() * 18.0, 0.0));
        }

        if let Some(part) = self.workspace.get_mut::<Part>(self.moving_parts.tower) {
            part.set_position(Vec3::new(-3.4, 1.0 + (time * 1.5).sin() * 0.35, -1.8));
            part.set_orientation(Vec3::new(0.0, -33.0 + time.to_degrees() * 0.85, 0.0));
        }

        if let Some(part) = self.workspace.get_mut::<Part>(self.moving_parts.orb) {
            let orbit_angle = time * 0.8;
            part.set_position(Vec3::new(
                orbit_angle.cos() * 3.3,
                2.8 + (time * 1.7).sin() * 0.45,
                orbit_angle.sin() * 3.3,
            ));
            part.set_orientation(Vec3::new(0.0, orbit_angle.to_degrees(), 0.0));
        }
    }

    fn capture_mouse(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let grabbed = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            .is_ok();
        window.set_cursor_visible(!grabbed);
        self.mouse_captured = grabbed;
    }

    fn release_mouse(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
        self.mouse_captured = false;
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
        if let Err(error) =
            apply_courtyard_textures(&mut renderer, &mut self.workspace, &self.moving_parts)
        {
            eprintln!("courtyard texture setup failed: {error}");
            event_loop.exit();
            return;
        }

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.capture_mouse();
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
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape) =>
            {
                self.release_mouse();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !self.mouse_captured => self.capture_mouse(),
            WindowEvent::RedrawRequested => {
                let delta = self.renderer.as_mut().map(|renderer| renderer.delta_secs());
                if let Some(delta) = delta {
                    self.workspace.update_camera(delta);
                    self.animate_parts(delta);

                    if let Some(renderer) = &mut self.renderer {
                        if let Err(error) = renderer.render(&self.workspace) {
                            eprintln!("rendering stopped: {error}");
                            event_loop.exit();
                        } else {
                            window.set_title(&format!("Courtyard | FPS: {:.0}", renderer.fps()));
                        }
                    }
                }
                window.request_redraw();
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.mouse_captured {
            self.workspace.process_device_event(&event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn create_workspace() -> (Workspace, MovingParts) {
    let mut workspace = Workspace::new();
    let ground = workspace.add_child_with(Part::new("Ground"), |part| {
        part.shape = PartShape::Block;
        part.set_position(Vec3::new(0.0, -0.1, 0.0));
        part.size = Vec3::new(42.0, 0.2, 42.0);
        part.color = Color3::new(0.07, 0.12, 0.13);
    });
    let mut stone_blocks = Vec::new();

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        let stone_block = workspace.add_child_with(Part::new("StoneBlock"), |part| {
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

    let teal_block = workspace.add_child_with(Part::new("TealBlock"), |part| {
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

    (
        workspace,
        MovingParts {
            ground,
            stone_blocks,
            grass_block: teal_block,
            tower,
            orb,
        },
    )
}

fn apply_courtyard_textures(
    renderer: &mut Renderer,
    workspace: &mut Workspace,
    moving_parts: &MovingParts,
) -> Result<(), RendererError> {
    let dirt = renderer.add_texture(&Texture::from_bytes(
        include_bytes!("../assets/dirt.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let cobblestone = renderer.add_texture(&Texture::from_bytes(
        include_bytes!("../assets/cobblestone.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_top = renderer.add_texture(&Texture::from_bytes(
        include_bytes!("../assets/grass_top.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let grass_side = renderer.add_texture(&Texture::from_bytes(
        include_bytes!("../assets/grass_side.png"),
        TextureColorSpace::Srgb,
    )?)?;
    let festival_lantern = renderer.add_texture(&Texture::from_bytes(
        include_bytes!("../assets/festival_lantern.png"),
        TextureColorSpace::Srgb,
    )?)?;

    let set_material = |workspace: &mut Workspace, part_id: InstanceId, material: Material| {
        if let Some(part) = workspace.get_mut::<Part>(part_id) {
            part.color = Color3::WHITE;
            part.material = material;
            part.material.roughness = 0.82;
        }
    };
    set_material(workspace, moving_parts.ground, Material::textured(dirt));
    for &part_id in &moving_parts.stone_blocks {
        set_material(workspace, part_id, Material::textured(cobblestone));
    }

    if let Some(part) = workspace.get_mut::<Part>(moving_parts.grass_block) {
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

    if let Some((_, part)) = workspace.find_first_child("PalePlatform") {
        part.color = Color3::WHITE;
        part.material = Material::textured(festival_lantern);
        part.material.roughness = 0.82;
    }
    Ok(())
}

fn main() {
    env_logger::init();
    println!(
        "WASD move | mouse look | Space/Ctrl rise and descend | Shift sprint | Esc release mouse"
    );
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run event loop");
}
