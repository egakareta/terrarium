use std::ops::{Deref, DerefMut};

use crate::{glam::*, *};

instance! {
    #[instance(default = false, inspect = false)]
    /// A perspective camera capability with a world-space pivot and subject framing.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct Camera {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[compose]
        pub(crate) pv_instance: PVInstance,
        #[cfg_attr(feature = "devtools", inspect(readonly))]
        #[property]
        subject: Option<InstanceId>,
        /// Desired distance from the camera to its subject in orbit mode.
        #[property]
        subject_distance: f32,
        /// Subject-local point that the camera orbits around.
        #[property]
        subject_offset: Vec3,
        #[cfg_attr(feature = "devtools", inspect(readonly))]
        /// Viewport width divided by viewport height. Values set through this
        /// API are clamped to at least `0.001`.
        #[property(sanitize = |value: f32| value.max(0.001))]
        aspect: f32,
        /// Vertical field of view in radians.
        #[property]
        fovy: f32,
        /// Near clipping plane distance.
        #[property]
        znear: f32,
        /// Far clipping plane distance.
        #[property]
        zfar: f32,
    }
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
            pv_instance: PVInstance::from_world_transform(Mat4::from_rotation_translation(
                Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch),
                position,
            )),
            subject: None,
            subject_distance: 5.0,
            subject_offset: Vec3::ZERO,
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
        self.projection_matrix() * self.pose().inverse()
    }

    /// Returns the right-handed DirectX perspective projection matrix.
    pub fn projection_matrix(&self) -> Mat4 {
        glam::camera::rh::proj::directx::perspective(self.fovy, self.aspect, self.znear, self.zfar)
    }
}

/// Physical keyboard keys assigned to camera movement and orbit actions.
pub type CameraKeyBindings = ControllerKeyBindings;

/// Free-flight movement and third-person orbit for a [`Camera`].
///
/// Device state is stored in [`Self::input`]. Free flight follows the camera's
/// look direction. With a subject, look and horizontal movement orbit, vertical
/// movement changes pitch, and forward/backward movement adjusts the camera's
/// subject distance and offset configuration.
/// [`Self::step_camera_with_actions`] also accepts actions directly.
#[derive(Clone, Debug)]
pub struct CameraController {
    /// Keyboard, mouse, and touch state used by this controller.
    pub input: ControllerInput,
    /// Movement and zoom speed in world units per second.
    pub speed: f32,
    /// Look sensitivity in radians per raw mouse unit or physical touch pixel.
    pub sensitivity: f32,
    /// Minimum camera distance from the subject's focus point.
    pub min_subject_distance: f32,
    /// Maximum camera distance from the subject's focus point.
    pub max_subject_distance: f32,
    /// Orbit speed in radians per second for keyboard and joystick controls.
    pub orbit_speed: f32,
    /// Minimum vertical orbit angle in radians.
    pub min_subject_pitch: f32,
    /// Maximum vertical orbit angle in radians.
    pub max_subject_pitch: f32,
    orbit_subject: Option<InstanceId>,
    orbit_yaw: f32,
    orbit_pitch: f32,
    queued_look: Vec2,
    queued_subject_zoom: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self::new(6.0, 0.0025)
    }
}

impl CameraController {
    /// Creates a controller with a movement speed and look sensitivity.
    ///
    /// Default bindings are WASD, Space/Ctrl, and Shift. All three devices start enabled.
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self::new_with_key_bindings(speed, sensitivity, CameraKeyBindings::default())
    }

    /// Creates a controller with custom physical movement-key bindings.
    pub fn new_with_key_bindings(
        speed: f32,
        sensitivity: f32,
        key_bindings: CameraKeyBindings,
    ) -> Self {
        let mut input = ControllerInput::default();
        input.key_bindings = key_bindings;
        Self {
            input,
            speed,
            sensitivity,
            min_subject_distance: 0.5,
            max_subject_distance: 100.0,
            orbit_speed: 1.5,
            min_subject_pitch: -89.0_f32.to_radians(),
            max_subject_pitch: 89.0_f32.to_radians(),
            orbit_subject: None,
            orbit_yaw: 0.0,
            orbit_pitch: 0.0,
            queued_look: Vec2::ZERO,
            queued_subject_zoom: 0.0,
        }
    }

    /// Sets the bounds used to clamp orbit distance during zoom.
    pub fn with_subject_distance_limits(mut self, min: f32, max: f32) -> Self {
        self.min_subject_distance = min;
        self.max_subject_distance = max;
        self
    }

    /// Sets the pitch bounds for third-person orbit, in radians.
    pub fn with_subject_pitch_limits(mut self, min: f32, max: f32) -> Self {
        self.min_subject_pitch = min;
        self.max_subject_pitch = max;
        self
    }

    /// Sets the orbit rate for left/right and up/down movement, in radians per second.
    pub fn with_orbit_speed(mut self, speed: f32) -> Self {
        self.orbit_speed = speed;
        self
    }

    /// Enables or disables keyboard, mouse, and touch input together.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.input.set_enabled(enabled);
        self
    }

    /// Enables or disables keyboard actions.
    pub fn with_keyboard_enabled(mut self, enabled: bool) -> Self {
        self.input.set_keyboard_enabled(enabled);
        self
    }

    /// Enables or disables mouse look.
    pub fn with_mouse_enabled(mut self, enabled: bool) -> Self {
        self.input.set_mouse_enabled(enabled);
        self
    }

    /// Enables or disables touch movement and look.
    pub fn with_touch_enabled(mut self, enabled: bool) -> Self {
        self.input.set_touch_enabled(enabled);
        self
    }

    /// Applies pending device input and queued subject zoom to `camera`.
    ///
    /// In free flight, forward follows the camera's look direction, strafing stays
    /// horizontal, and elevation follows world Y. With a subject, look and
    /// left/right orbit, forward/backward zoom, and up/down changes orbit pitch.
    /// Sprint multiplies movement by `2.5`; frame time is capped at 100 ms.
    pub fn step_camera(
        &mut self,
        camera: &mut Camera,
        subject: Option<(InstanceId, Mat4)>,
        delta_seconds: f32,
    ) {
        let actions = self.input.take_actions();
        self.step_camera_with_actions(camera, subject, actions, delta_seconds);
    }

    /// Adds a look rotation in radians to the next camera update.
    ///
    /// Use this when another controller owns look input but the camera owns the
    /// orbit. The queued rotation is consumed on the next update even if camera
    /// input devices are disabled. Non-finite values are ignored.
    pub fn queue_look(&mut self, radians: Vec2) {
        let queued = self.queued_look + radians;
        if radians.is_finite() && queued.is_finite() {
            self.queued_look = queued;
        }
    }

    /// Queues a subject zoom distance change for the next camera update.
    ///
    /// Positive values move the camera closer; non-finite values are ignored.
    pub fn queue_subject_zoom(&mut self, amount: f32) {
        let queued = self.queued_subject_zoom + amount;
        if amount.is_finite() && queued.is_finite() {
            self.queued_subject_zoom = queued;
        }
    }

    pub(crate) fn queued_look(&self) -> Vec2 {
        self.queued_look
    }

    /// Applies actions, queued look rotation, and queued subject zoom to `camera`.
    ///
    /// Does not read device state. Look in `actions` is scaled by `sensitivity`;
    /// look queued with [`Self::queue_look`] is already in radians. Queued look
    /// and zoom are consumed by this call.
    pub fn step_camera_with_actions(
        &mut self,
        camera: &mut Camera,
        subject: Option<(InstanceId, Mat4)>,
        actions: ControllerActions,
        delta_seconds: f32,
    ) {
        let delta_seconds = crate::controller::frame_delta(delta_seconds);
        let queued_subject_zoom = std::mem::take(&mut self.queued_subject_zoom);
        let look = actions.look * self.sensitivity + std::mem::take(&mut self.queued_look);
        let look = if look.is_finite() { look } else { Vec2::ZERO };
        let Some((subject_id, subject_pose)) = subject else {
            self.orbit_subject = None;
            let rotation = ControllerActions { look, ..actions }.look_rotation(camera.pose(), 1.0);
            let position =
                camera.position() + actions.translation(rotation, self.speed, delta_seconds);
            camera.with_pose(Mat4::from_rotation_translation(rotation, position));
            return;
        };
        let offset = if camera.subject_offset.is_finite() {
            camera.subject_offset
        } else {
            Vec3::ZERO
        };
        let focus = subject_pose.transform_point3(offset);
        if self.orbit_subject != Some(subject_id) {
            let camera_offset = camera.position() - focus;
            let direction = if camera_offset.length_squared() > 1e-10 {
                camera_offset.normalize()
            } else {
                -camera.forward()
            };
            self.orbit_yaw = direction.x.atan2(direction.z);
            self.orbit_pitch = direction.y.clamp(-1.0, 1.0).asin();
            self.orbit_subject = Some(subject_id);
        }
        self.orbit_yaw -= look.x;
        let (min_pitch, max_pitch) = self.subject_pitch_limits();
        self.orbit_pitch = (self.orbit_pitch + look.y).clamp(min_pitch, max_pitch);
        let axes = actions.scaled_movement();
        let movement = self.speed * delta_seconds;
        camera.subject_distance -= queued_subject_zoom + axes.z * movement;
        let orbit_step = self.orbit_speed * delta_seconds;
        self.orbit_yaw -= axes.x * orbit_step;
        self.orbit_pitch += axes.y * orbit_step;
        self.orbit_pitch = self.orbit_pitch.clamp(min_pitch, max_pitch);
        let min_distance = if self.min_subject_distance.is_finite() {
            self.min_subject_distance.max(0.01)
        } else {
            0.5
        };
        let max_distance = if self.max_subject_distance.is_finite() {
            self.max_subject_distance.max(min_distance)
        } else {
            min_distance.max(100.0)
        };
        camera.subject_distance = if camera.subject_distance.is_finite() {
            camera.subject_distance
        } else {
            min_distance
        }
        .clamp(min_distance, max_distance);
        let horizontal_distance = self.orbit_pitch.cos() * camera.subject_distance;
        let camera_position = focus
            + Vec3::new(
                self.orbit_yaw.sin() * horizontal_distance,
                self.orbit_pitch.sin() * camera.subject_distance,
                self.orbit_yaw.cos() * horizontal_distance,
            );
        let pose = glam::camera::rh::view::look_at_mat4(camera_position, focus, Vec3::Y).inverse();
        camera.with_pose(pose);
    }

    fn subject_pitch_limits(&self) -> (f32, f32) {
        let limit = 89.0_f32.to_radians();
        let min = if self.min_subject_pitch.is_finite() {
            self.min_subject_pitch.clamp(-limit, limit)
        } else {
            -limit
        };
        let max = if self.max_subject_pitch.is_finite() {
            self.max_subject_pitch.clamp(-limit, limit)
        } else {
            limit
        };
        if min <= max { (min, max) } else { (max, min) }
    }
}

impl Deref for CameraController {
    type Target = ControllerInput;

    fn deref(&self) -> &Self::Target {
        &self.input
    }
}

impl DerefMut for CameraController {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.input
    }
}

impl Controller for CameraController {
    fn input(&self) -> Option<&ControllerInput> {
        Some(&self.input)
    }

    fn input_mut(&mut self) -> Option<&mut ControllerInput> {
        Some(&mut self.input)
    }

    fn step(&mut self, _workspace: &mut Workspace, _delta_seconds: f32) {}

    fn post_physics(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
        let subject = workspace.subject().and_then(|id| {
            workspace
                .view::<dyn Instance>(id)
                .and_then(|instance| instance.spatial().map(|spatial| (id, spatial.rigid_pose())))
        });
        self.step_camera(workspace.camera_mut(), subject, delta_seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winit::{event::DeviceEvent, keyboard::KeyCode};

    #[test]
    fn camera_projection_and_framing_properties_use_getters_and_builders() {
        let camera = Camera::default()
            .with_aspect(2.0)
            .with_fovy(0.75)
            .with_znear(0.2)
            .with_zfar(400.0)
            .with_subject_distance(8.0)
            .with_subject_offset(Vec3::Y);
        assert_eq!(camera.aspect(), 2.0);
        assert_eq!(camera.fovy(), 0.75);
        assert_eq!(camera.znear(), 0.2);
        assert_eq!(camera.zfar(), 400.0);
        assert_eq!(camera.subject_distance(), 8.0);
        assert_eq!(camera.subject_offset(), Vec3::Y);
    }

    #[test]
    fn camera_mouse_look_rotates_in_place_with_the_expected_horizontal_sign() {
        let mut camera = Camera::default();
        let original_position = camera.pivot().w_axis.truncate();
        let mut controller = CameraController::new(6.0, 0.1);
        controller.mouse_look_delta = (1.0, 0.0);
        controller.step_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.pivot().w_axis.truncate(), original_position);
        assert!(camera.forward().x > 0.0);
    }

    #[test]
    fn egui_keyboard_capture_clears_camera_movement() {
        let mut controller = CameraController::default();
        controller.process_key(KeyCode::KeyW, true);
        controller.process_key(KeyCode::ShiftLeft, true);
        let mut input = crate::egui::InputState::default();
        input.focused = true;
        controller
            .input
            .process_eframe_input_inner(&input, false, false, true, |_| false);
        assert!(!controller.forward);
        assert!(!controller.sprint);
    }

    #[test]
    fn egui_pointer_capture_clears_camera_look_delta() {
        let mut controller = CameraController::default();
        controller.process_mouse_motion((4.0, -2.0));
        let mut input = crate::egui::InputState::default();
        input.focused = true;
        controller
            .input
            .process_eframe_input_inner(&input, true, true, false, |_| false);
        assert_eq!(controller.mouse_look_delta, (0.0, 0.0));
    }

    #[test]
    fn keyboard_disabled_suppresses_movement_and_keys() {
        let mut controller = CameraController::default().with_keyboard_enabled(false);
        assert!(!controller.keyboard_enabled);
        let mut input = crate::egui::InputState::default();
        input.focused = true;
        input.keys_down.insert(crate::egui::Key::W);
        controller
            .input
            .process_eframe_input_inner(&input, false, false, false, |_| false);
        assert!(!controller.forward);
        assert!(!controller.process_key(KeyCode::KeyW, true));
        assert!(!controller.forward);
        let mut camera = Camera::default();
        let initial_position = camera.pivot().w_axis.truncate();
        controller.forward = true;
        controller.step_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.pivot().w_axis.truncate(), initial_position);
        assert!(!controller.forward);
    }

    #[test]
    fn mouse_disabled_suppresses_look_and_motion() {
        let mut controller = CameraController::default().with_mouse_enabled(false);
        assert!(!controller.mouse_enabled);
        controller.process_mouse_motion((10.0, -5.0));
        assert_eq!(controller.mouse_look_delta, (0.0, 0.0));
        controller.process_device_event(&DeviceEvent::MouseMotion {
            delta: (10.0, -5.0),
        });
        assert_eq!(controller.mouse_look_delta, (0.0, 0.0));
        let mut camera = Camera::default();
        let initial_forward = camera.forward();
        controller.mouse_look_delta = (5.0, 5.0);
        controller.step_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.forward(), initial_forward);
        assert_eq!(controller.mouse_look_delta, (0.0, 0.0));
    }

    #[test]
    fn disabling_input_clears_active_state_and_builder_methods_work() {
        let mut controller = CameraController::default();
        controller.process_key(KeyCode::KeyW, true);
        controller.process_mouse_motion((3.0, 4.0));
        assert!(controller.is_enabled());
        controller.set_enabled(false);
        assert!(!controller.keyboard_enabled);
        assert!(!controller.mouse_enabled);
        assert!(!controller.is_enabled());
        assert!(!controller.forward);
        assert_eq!(controller.mouse_look_delta, (0.0, 0.0));
        let controller = controller
            .with_enabled(true)
            .with_keyboard_enabled(false)
            .with_mouse_enabled(true);
        assert!(!controller.keyboard_enabled);
        assert!(controller.mouse_enabled);
        assert!(controller.is_enabled());
    }
}
