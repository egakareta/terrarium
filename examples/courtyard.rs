use std::{sync::Arc, time::Instant};

use glam::{Mat4, Vec3};
use terrarium::{Camera, CameraController, Mesh, Renderer, Scene};
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
    scene: Option<Scene>,
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
            scene: None,
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
        let scene = create_scene(&mut renderer);

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.scene = Some(scene);
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

                if let (Some(renderer), Some(scene)) = (&mut self.renderer, &self.scene)
                    && let Err(error) = renderer.render(scene, &self.camera)
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

fn create_scene(renderer: &mut Renderer) -> Scene {
    let ground = renderer
        .add_mesh(&Mesh::plane(42.0, [0.07, 0.12, 0.13, 1.0]))
        .expect("ground mesh");
    let warm_cube = renderer
        .add_mesh(&Mesh::cube(1.0, [0.76, 0.30, 0.14, 1.0]))
        .expect("warm cube mesh");
    let teal_cube = renderer
        .add_mesh(&Mesh::cube(1.0, [0.10, 0.48, 0.47, 1.0]))
        .expect("teal cube mesh");
    let pale_cube = renderer
        .add_mesh(&Mesh::cube(1.0, [0.60, 0.68, 0.50, 1.0]))
        .expect("pale cube mesh");
    let stone_cube = renderer
        .add_mesh(&Mesh::cube(1.0, [0.29, 0.34, 0.39, 1.0]))
        .expect("stone cube mesh");

    let mut scene = Scene::new();
    scene.add(ground, Mat4::IDENTITY);

    for x in -4..=4 {
        let x = x as f32 * 2.1;
        scene.add(
            stone_cube,
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.72, 0.45, 0.72),
                glam::Quat::IDENTITY,
                Vec3::new(x, 0.22, -4.0),
            ),
        );
    }

    for (position, scale, mesh) in [
        (
            Vec3::new(-3.4, 1.0, -1.8),
            Vec3::new(1.2, 2.0, 1.2),
            warm_cube,
        ),
        (
            Vec3::new(3.2, 0.8, -2.3),
            Vec3::new(1.5, 1.6, 1.5),
            teal_cube,
        ),
        (
            Vec3::new(-1.0, 0.55, 1.6),
            Vec3::new(2.0, 1.1, 2.0),
            pale_cube,
        ),
        (
            Vec3::new(2.7, 1.5, 2.2),
            Vec3::new(1.1, 3.0, 1.1),
            warm_cube,
        ),
        (
            Vec3::new(-4.7, 0.6, 3.1),
            Vec3::new(1.8, 1.2, 1.8),
            teal_cube,
        ),
    ] {
        scene.add(
            mesh,
            Mat4::from_scale_rotation_translation(
                scale,
                glam::Quat::from_rotation_y(position.x * 0.17),
                position,
            ),
        );
    }

    scene
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
