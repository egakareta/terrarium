use std::ops::{Deref, DerefMut};

use crate::{
    InstanceData, PVInstance,
    glam::{Mat4, Quat, Vec3},
    winit::{
        event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
        keyboard::{KeyCode, PhysicalKey},
    },
};

/// A perspective camera with a world-space pivot.
#[derive(Clone, Debug)]
pub struct Camera {
    pub(crate) instance: InstanceData,
    pub(crate) pv_instance: PVInstance,
    /// Viewport width divided by viewport height.
    pub aspect: f32,
    /// Vertical field of view in radians.
    pub fovy: f32,
    /// Near clipping plane distance.
    pub znear: f32,
    /// Far clipping plane distance.
    pub zfar: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera::new(Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.0, 1.0, 0.0), 1.0)
    }
}

impl Camera {
    /// Creates a camera at `position` looking toward `target`.
    ///
    /// `aspect` is clamped to a small positive value. The default projection
    /// uses a 60-degree vertical field of view, a near plane at `0.1`, and a
    /// far plane at `200.0`.
    pub fn new(position: Vec3, target: Vec3, aspect: f32) -> Self {
        let direction = (target - position).normalize_or_zero();
        let yaw = direction.x.atan2(-direction.z);
        let pitch = direction.y.asin();
        Self {
            instance: InstanceData::new("Camera"),
            pv_instance: PVInstance::from_world_transform(Mat4::from_rotation_translation(
                Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch),
                position,
            )),
            aspect: aspect.max(0.001),
            fovy: 60.0_f32.to_radians(),
            znear: 0.1,
            zfar: 200.0,
        }
    }

    /// Updates the aspect ratio from a physical window size.
    ///
    /// A height of zero is ignored, which makes this safe to call for minimized
    /// windows.
    pub fn resize(&mut self, width: u32, height: u32) {
        if height != 0 {
            self.aspect = width as f32 / height as f32;
        }
    }

    /// Returns the right-handed DirectX view-projection matrix for this camera.
    pub fn view_projection_matrix(&self) -> Mat4 {
        let view = self.pivot().inverse();
        let projection = glam::camera::rh::proj::directx::perspective(
            self.fovy,
            self.aspect,
            self.znear,
            self.zfar,
        );
        projection * view
    }
}

crate::impl_instance!(Camera, class_name = "Camera", data = instance,);

/// Physical keyboard keys assigned to camera movement actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraKeyBindings {
    /// Keys that move the camera forward.
    pub forward: Vec<KeyCode>,
    /// Keys that move the camera backward.
    pub backward: Vec<KeyCode>,
    /// Keys that move the camera left.
    pub left: Vec<KeyCode>,
    /// Keys that move the camera right.
    pub right: Vec<KeyCode>,
    /// Keys that move the camera up.
    pub up: Vec<KeyCode>,
    /// Keys that move the camera down.
    pub down: Vec<KeyCode>,
    /// Keys that activate sprinting.
    pub sprint: Vec<KeyCode>,
}

impl Default for CameraKeyBindings {
    fn default() -> Self {
        Self {
            forward: vec![KeyCode::KeyW],
            backward: vec![KeyCode::KeyS],
            left: vec![KeyCode::KeyA],
            right: vec![KeyCode::KeyD],
            up: vec![KeyCode::Space],
            down: vec![KeyCode::ControlLeft, KeyCode::ControlRight],
            sprint: vec![KeyCode::ShiftLeft, KeyCode::ShiftRight],
        }
    }
}

/// First-person keyboard and mouse input for a [`Camera`].
#[derive(Clone, Debug)]
pub struct CameraController {
    /// Movement speed in world units per second.
    pub speed: f32,
    /// Mouse-look sensitivity in radians per raw mouse unit.
    pub sensitivity: f32,
    /// Physical keys assigned to each movement action.
    pub key_bindings: CameraKeyBindings,
    /// Whether a forward key is currently held.
    pub forward: bool,
    /// Whether a backward key is currently held.
    pub backward: bool,
    /// Whether a left key is currently held.
    pub left: bool,
    /// Whether a right key is currently held.
    pub right: bool,
    /// Whether an up key is currently held.
    pub up: bool,
    /// Whether a down key is currently held.
    pub down: bool,
    /// Whether a sprint key is currently held.
    pub sprint: bool,
    /// Accumulated mouse delta as `(x, y)` until the next update.
    pub mouse_delta: (f32, f32),
    /// Mouse button that activates drag-to-look.
    pub mouse_drag_button: MouseButton,
    mouse_dragging: bool,
    last_cursor_position: Option<(f64, f64)>,
}

impl Default for CameraController {
    fn default() -> Self {
        CameraController::new(6.0, 0.0025)
    }
}

impl CameraController {
    /// Creates a first-person controller with the given speed and sensitivity.
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self::new_with_key_bindings(speed, sensitivity, CameraKeyBindings::default())
    }

    /// Creates a first-person controller with custom movement key bindings.
    pub fn new_with_key_bindings(
        speed: f32,
        sensitivity: f32,
        key_bindings: CameraKeyBindings,
    ) -> Self {
        Self {
            speed,
            sensitivity,
            key_bindings,
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
            sprint: false,
            mouse_delta: (0.0, 0.0),
            mouse_drag_button: MouseButton::Left,
            mouse_dragging: false,
            last_cursor_position: None,
        }
    }

    /// Feeds a winit window event into the controller. Returns true when it was used.
    /// Applies keyboard movement and left-button mouse-drag look input.
    ///
    /// The controller recognizes the physical keys in [`CameraKeyBindings`].
    /// Losing window focus clears all held keys and ends an active mouse drag.
    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(key) = event.physical_key else {
                    return false;
                };
                let pressed = event.state == ElementState::Pressed;
                self.process_key(key, pressed)
            }
            WindowEvent::MouseInput { state, button, .. } if *button == self.mouse_drag_button => {
                self.mouse_dragging = *state == ElementState::Pressed;
                self.last_cursor_position = None;
                true
            }
            WindowEvent::CursorMoved { position, .. } if self.mouse_dragging => {
                if let Some((last_x, last_y)) = self.last_cursor_position {
                    self.process_mouse_motion((
                        (position.x - last_x) as f32,
                        (position.y - last_y) as f32,
                    ));
                }
                self.last_cursor_position = Some((position.x, position.y));
                true
            }
            WindowEvent::CursorLeft { .. } => {
                let was_dragging = self.mouse_dragging;
                self.stop_mouse_drag();
                was_dragging
            }
            WindowEvent::Focused(false) => {
                self.clear_keys();
                false
            }
            _ => false,
        }
    }

    /// Feeds eframe's normalized input state into the controller.
    pub fn process_eframe_input(&mut self, input: &crate::egui::InputState) {
        if !input.focused {
            self.clear_keys();
            return;
        }

        let key_down = |binding: &[KeyCode]| {
            binding
                .iter()
                .copied()
                .any(|key| eframe_key(key).is_some_and(|key| input.key_down(key)))
        };
        self.forward = key_down(&self.key_bindings.forward);
        self.backward = key_down(&self.key_bindings.backward);
        self.left = key_down(&self.key_bindings.left);
        self.right = key_down(&self.key_bindings.right);
        self.up = key_down(&self.key_bindings.up);
        self.down = key_down(&self.key_bindings.down);
        self.sprint = key_down(&self.key_bindings.sprint);

        if let Some(button) = eframe_button(self.mouse_drag_button)
            && input.pointer.button_down(button)
        {
            let delta = input.pointer.delta() * input.pixels_per_point;
            self.process_mouse_motion((delta.x, delta.y));
        }
    }

    fn process_key(&mut self, key: KeyCode, pressed: bool) -> bool {
        if self.key_bindings.forward.contains(&key) {
            set_key(&mut self.forward, pressed)
        } else if self.key_bindings.backward.contains(&key) {
            set_key(&mut self.backward, pressed)
        } else if self.key_bindings.left.contains(&key) {
            set_key(&mut self.left, pressed)
        } else if self.key_bindings.right.contains(&key) {
            set_key(&mut self.right, pressed)
        } else if self.key_bindings.up.contains(&key) {
            set_key(&mut self.up, pressed)
        } else if self.key_bindings.down.contains(&key) {
            set_key(&mut self.down, pressed)
        } else if self.key_bindings.sprint.contains(&key) {
            set_key(&mut self.sprint, pressed)
        } else {
            false
        }
    }

    /// Feeds raw device events into the controller for mouse-look.
    /// Accumulates a raw mouse-motion event for the next camera update.
    pub fn process_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.process_mouse_motion((delta.0 as f32, delta.1 as f32));
        }
    }

    /// Accumulates a mouse-motion delta for the next camera update.
    pub fn process_mouse_motion(&mut self, delta: (f32, f32)) {
        self.mouse_delta.0 += delta.0;
        self.mouse_delta.1 += delta.1;
    }

    /// Moves and rotates `camera` using accumulated input, then clears the mouse delta.
    ///
    /// Movement is frame-rate independent. The delta is capped at 100 ms,
    /// sprinting multiplies movement speed by `2.5`, and pitch is clamped to
    /// 89 degrees from the horizon.
    ///
    /// `forward`/`backward` fly along the camera's look direction, so looking
    /// down and pressing forward descends. Strafing stays horizontal and the
    /// orientation is rebuilt from yaw/pitch every update so no roll can
    /// accumulate.
    pub fn update_camera(&mut self, camera: &mut Camera, delta_seconds: f32) {
        let delta_seconds = delta_seconds.min(0.1);

        // Rebuild a roll-free orientation from yaw/pitch every frame.
        // Incremental matrix multiplies (`pivot * rotation`) accumulate
        // floating-point error as roll/scale drift over many frames.
        let pivot = camera.pivot();
        let position = pivot.w_axis.truncate();
        let forward0 = camera.forward();
        let right0 = pivot.transform_vector3(Vec3::X);
        // Yaw from the camera's right axis stays well-defined when looking
        // straight up/down, where the forward vector's horizontal projection
        // degenerates to zero.
        let horizontal_right_sq = right0.x * right0.x + right0.z * right0.z;
        let yaw = if horizontal_right_sq > 1e-10 {
            (-right0.z).atan2(right0.x)
        } else {
            (-forward0.x).atan2(-forward0.z)
        };
        let current_pitch = forward0.y.clamp(-1.0, 1.0).asin();

        let yaw_delta = -self.mouse_delta.0 * self.sensitivity;
        let new_yaw = yaw + yaw_delta;
        let new_pitch = (current_pitch - self.mouse_delta.1 * self.sensitivity)
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        let new_rotation = Quat::from_rotation_y(new_yaw) * Quat::from_rotation_x(new_pitch);

        let forward = new_rotation * Vec3::NEG_Z;
        let right = Quat::from_rotation_y(new_yaw) * Vec3::X;
        let mut movement = Vec3::ZERO;
        if self.forward {
            movement += forward;
        }
        if self.backward {
            movement -= forward;
        }
        if self.right {
            movement += right;
        }
        if self.left {
            movement -= right;
        }
        if self.up {
            movement += Vec3::Y;
        }
        if self.down {
            movement -= Vec3::Y;
        }

        let mut new_position = position;
        if movement.length_squared() > 0.0 {
            let speed = self.speed * if self.sprint { 2.5 } else { 1.0 };
            new_position += movement.normalize() * speed * delta_seconds;
        }

        camera.pivot_to(Mat4::from_rotation_translation(new_rotation, new_position));
        self.mouse_delta = (0.0, 0.0);
    }

    fn clear_keys(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.sprint = false;
        self.mouse_delta = (0.0, 0.0);
        self.stop_mouse_drag();
    }

    fn stop_mouse_drag(&mut self) {
        self.mouse_dragging = false;
        self.last_cursor_position = None;
    }
}

impl Deref for Camera {
    type Target = PVInstance;
    fn deref(&self) -> &Self::Target {
        &self.pv_instance
    }
}
impl DerefMut for Camera {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pv_instance
    }
}

fn set_key(key: &mut bool, pressed: bool) -> bool {
    *key = pressed;
    true
}

fn eframe_key(key: KeyCode) -> Option<crate::egui::Key> {
    use crate::egui::Key;

    Some(match key {
        KeyCode::KeyA => Key::A,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyW => Key::W,
        KeyCode::Space => Key::Space,
        KeyCode::ControlLeft => Key::ControlLeft,
        KeyCode::ControlRight => Key::ControlRight,
        KeyCode::ShiftLeft => Key::ShiftLeft,
        KeyCode::ShiftRight => Key::ShiftRight,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,
        _ => return None,
    })
}

fn eframe_button(button: MouseButton) -> Option<crate::egui::PointerButton> {
    use crate::egui::PointerButton;

    Some(match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Extra1,
        MouseButton::Forward => PointerButton::Extra2,
        MouseButton::Other(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_mouse_look_rotates_in_place_with_the_expected_horizontal_sign() {
        let mut camera = Camera::default();
        let original_position = camera.pivot().w_axis.truncate();
        let mut controller = CameraController::new(6.0, 0.1);
        controller.mouse_delta = (1.0, 0.0);

        controller.update_camera(&mut camera, 1.0 / 60.0);

        assert_eq!(camera.pivot().w_axis.truncate(), original_position);
        assert!(camera.forward().x > 0.0);
    }
}
