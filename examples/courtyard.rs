use std::{sync::Arc, time::Instant};

use glam::Vec3;
use terrarium::{Camera, CameraController, Color3, PartShape, Renderer, Workspace};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    workspace: Option<Workspace>,
    camera: Camera,
    controller: CameraController,
    last_frame: Instant,
    mouse_captured: bool,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            workspace: None,
            camera: Camera::new(
                Vec3::new(7.0, 3.8, 10.0),
                Vec3::new(0.0, 1.0, 0.0),
                16.0 / 9.0,
            ),
            controller: CameraController::new(6.0, 0.0025),
            last_frame: Instant::now(),
            mouse_captured: false,
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
            .with_title("Terrarium | Courtyard")
            .with_inner_size(PhysicalSize::new(1280, 720));
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        let size = window.inner_size();
        self.camera.resize(size.width, size.height);

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
        let world = create_world();

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.workspace = Some(world);
        self.capture_mouse();
        self.last_frame = Instant::now();
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

        self.controller.process_window_event(&event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.camera.resize(size.width, size.height);
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
                let now = Instant::now();
                let delta = now.duration_since(self.last_frame).as_secs_f32();
                self.last_frame = now;
                self.controller.update_camera(&mut self.camera, delta);

                if let (Some(renderer), Some(workspace)) = (&mut self.renderer, &self.workspace)
                    && let Err(error) = renderer.render(workspace, &self.camera)
                {
                    eprintln!("rendering stopped: {error}");
                    event_loop.exit();
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
            self.controller.process_device_event(&event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn create_world() -> Workspace {
    let mut workspace = Workspace::new();
    add_part(
        &mut workspace,
        "Ground",
        PartShape::Block,
        Vec3::new(0.0, -0.1, 0.0),
        Vec3::new(42.0, 0.2, 42.0),
        Color3::new(0.07, 0.12, 0.13),
    );

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        add_part(
            &mut workspace,
            "StoneBlock",
            PartShape::Block,
            Vec3::new(x, 0.22, -4.0),
            Vec3::new(0.72, 0.45, 0.72),
            Color3::new(0.29, 0.34, 0.39),
        );
    }

    for (name, shape, position, size, color, orientation) in [
        (
            "CopperTower",
            PartShape::Cylinder,
            Vec3::new(-3.4, 1.0, -1.8),
            Vec3::new(1.2, 2.0, 1.2),
            Color3::new(0.76, 0.30, 0.14),
            Vec3::new(0.0, -33.0, 0.0),
        ),
        (
            "TealBlock",
            PartShape::Ball,
            Vec3::new(3.2, 0.8, -2.3),
            Vec3::new(1.5, 1.6, 1.5),
            Color3::new(0.10, 0.48, 0.47),
            Vec3::new(0.0, 31.0, 0.0),
        ),
        (
            "PalePlatform",
            PartShape::Block,
            Vec3::new(-1.0, 0.55, 1.6),
            Vec3::new(2.0, 1.1, 2.0),
            Color3::new(0.60, 0.68, 0.50),
            Vec3::ZERO,
        ),
        (
            "CopperPillar",
            PartShape::Wedge,
            Vec3::new(2.7, 1.5, 2.2),
            Vec3::new(1.1, 3.0, 1.1),
            Color3::new(0.76, 0.30, 0.14),
            Vec3::new(0.0, 26.0, 0.0),
        ),
        (
            "TealMonolith",
            PartShape::CornerWedge,
            Vec3::new(-4.7, 0.6, 3.1),
            Vec3::new(1.8, 1.2, 1.8),
            Color3::new(0.10, 0.48, 0.47),
            Vec3::new(0.0, -46.0, 0.0),
        ),
    ] {
        let id = add_part(&mut workspace, name, shape, position, size, color);
        workspace.part_mut(id).expect("new part").orientation = orientation;
    }

    workspace
}

fn add_part(
    workspace: &mut Workspace,
    name: &str,
    shape: PartShape,
    position: Vec3,
    size: Vec3,
    color: Color3,
) -> terrarium::PartId {
    let id = workspace.create_part(name);
    let part = workspace.part_mut(id).expect("new part");
    part.shape = shape;
    part.position = position;
    part.size = size;
    part.color = color;
    id
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
