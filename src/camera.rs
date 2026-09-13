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
    pub fn update_camera(&mut self, camera: &mut Camera, delta_seconds: f32) {
        let delta_seconds = delta_seconds.min(0.1);
        let forward = camera.forward();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let horizontal_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let mut movement = Vec3::ZERO;
        if self.forward {
            movement += horizontal_forward;
        }
        if self.backward {
            movement -= horizontal_forward;
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

        if movement.length_squared() > 0.0 {
            let speed = self.speed * if self.sprint { 2.5 } else { 1.0 };
            let pivot = camera.pivot();
            camera.pivot_to(
                Mat4::from_translation(movement.normalize() * speed * delta_seconds) * pivot,
            );
        }

        let yaw_delta = -self.mouse_delta.0 * self.sensitivity;
        let current_pitch = camera.forward().y.asin();
        let target_pitch = (current_pitch - self.mouse_delta.1 * self.sensitivity)
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        let mut pivot = camera.pivot();
        if yaw_delta != 0.0 {
            let (_, rotation, position) = pivot.to_scale_rotation_translation();
            pivot = Mat4::from_rotation_translation(
                Quat::from_rotation_y(yaw_delta) * rotation,
                position,
            );
        }
        if target_pitch != current_pitch {
            pivot *= Mat4::from_rotation_x(target_pitch - current_pitch);
        }
        camera.pivot_to(pivot);
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
