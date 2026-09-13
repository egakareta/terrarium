use std::ops::{Deref, DerefMut};

use glam::{Mat4, Quat, Vec3};
use winit::{
    event::{DeviceEvent, ElementState, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

use crate::{InstanceData, PVInstance};

/// A perspective camera with a world-space pivot.
#[derive(Clone, Debug)]
pub struct Camera {
    pub(crate) instance: InstanceData,
    pub(crate) pv_instance: PVInstance,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera::new(Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.0, 1.0, 0.0), 1.0)
    }
}

impl Camera {
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

    pub fn resize(&mut self, width: u32, height: u32) {
        if height != 0 {
            self.aspect = width as f32 / height as f32;
        }
    }

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

/// First-person keyboard and raw mouse input for a [`Camera`].
#[derive(Clone, Debug)]
pub struct CameraController {
    pub speed: f32,
    pub sensitivity: f32,
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub sprint: bool,
    pub mouse_delta: (f32, f32),
}

impl Default for CameraController {
    fn default() -> Self {
        CameraController::new(6.0, 0.0025)
    }
}

impl CameraController {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            speed,
            sensitivity,
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
            sprint: false,
            mouse_delta: (0.0, 0.0),
        }
    }

    /// Feeds a winit window event into the controller. Returns true when it was used.
    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput { event, .. } = event else {
            if matches!(event, WindowEvent::Focused(false)) {
                self.clear_keys();
            }
            return false;
        };

        let PhysicalKey::Code(key) = event.physical_key else {
            return false;
        };
        let pressed = event.state == ElementState::Pressed;
        match key {
            KeyCode::KeyW => set_key(&mut self.forward, pressed),
            KeyCode::KeyS => set_key(&mut self.backward, pressed),
            KeyCode::KeyA => set_key(&mut self.left, pressed),
            KeyCode::KeyD => set_key(&mut self.right, pressed),
            KeyCode::Space => set_key(&mut self.up, pressed),
            KeyCode::ControlLeft | KeyCode::ControlRight => set_key(&mut self.down, pressed),
            KeyCode::ShiftLeft | KeyCode::ShiftRight => set_key(&mut self.sprint, pressed),
            _ => false,
        }
    }

    /// Feeds raw device events into the controller for mouse-look.
    pub fn process_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.mouse_delta.0 += delta.0 as f32;
            self.mouse_delta.1 += delta.1 as f32;
        }
    }

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
