use crate::{glam::*, *};
mod rig;
pub use rig::*;

instance!({
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
});

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

instance!({
    #[instance(default = false)]
    /// Free-flight movement and third-person orbit for a [`Camera`].
    ///
    /// It inherits device state from [`Controller`] and delegates camera motion to
    /// [`CameraRig`]. Free flight follows the camera's look direction. With a subject,
    /// look and horizontal movement orbit, vertical movement changes pitch, and
    /// forward/backward movement adjusts subject distance.
    #[derive(Clone, Debug)]
    pub struct CameraController {
        #[extends]
        controller: Controller,
        #[compose]
        camera_rig: CameraRig,
        #[property]
        /// Look sensitivity in radians per raw mouse unit or physical touch pixel.
        sensitivity: f32,
    }

    impl CameraController {
        #[r#override(Controller::post_physics)]
        fn post_physics(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
            let camera_controller = self.camera_controller_mut();
            let subject = workspace.subject().and_then(|id| {
                workspace.view::<dyn Instance>(id).and_then(|instance| {
                    instance.spatial().map(|spatial| (id, spatial.rigid_pose()))
                })
            });
            camera_controller.step_camera(workspace.camera_mut(), subject, delta_seconds);
        }
    }
});

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
        Self::new_with_key_bindings(speed, sensitivity, ControllerKeyBindings::default())
    }

    /// Creates a controller with custom physical movement-key bindings.
    pub fn new_with_key_bindings(
        speed: f32,
        sensitivity: f32,
        key_bindings: ControllerKeyBindings,
    ) -> Self {
        let mut controller = Controller::default().with_name("CameraController");
        controller.set_key_bindings(key_bindings);
        Self {
            controller,
            camera_rig: CameraRig::default().with_speed(speed),
            sensitivity,
        }
    }

    /// Applies pending device input and queued subject zoom to `camera`.
    ///
    /// In free flight, forward follows the camera's look direction, strafing stays
    /// horizontal, and elevation follows world Y. With a subject, look and
    /// left/right orbit, forward/backward zoom, and up/down changes orbit pitch.
    /// Sprint uses [`Self::sprint_scale`]; frame time is capped at 100 ms.
    pub fn step_camera(
        &mut self,
        camera: &mut Camera,
        subject: Option<(InstanceId, Mat4)>,
        delta_seconds: f32,
    ) {
        let actions = self.controller_input_mut().take_actions_inner().0;
        self.step_camera_with_actions(camera, subject, actions, delta_seconds);
    }

    /// Applies actions, queued look rotation, and queued subject zoom to `camera`.
    ///
    /// Does not read device state. Look in `actions` is scaled by `sensitivity`;
    /// look queued with [`CameraRig::queue_look`] is already in radians. Queued
    /// look and zoom are consumed by this call.
    pub fn step_camera_with_actions(
        &mut self,
        camera: &mut Camera,
        subject: Option<(InstanceId, Mat4)>,
        actions: ControllerActions,
        delta_seconds: f32,
    ) {
        self.camera_rig
            .update_camera(camera, subject, actions, self.sensitivity(), delta_seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winit::event::DeviceEvent;

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
        controller.set_mouse_look_delta((1.0, 0.0));
        controller.step_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.pivot().w_axis.truncate(), original_position);
        assert!(camera.forward().x > 0.0);
    }

    #[test]
    fn egui_keyboard_capture_clears_camera_movement() {
        let context = crate::egui::Context::default();
        let mut text = String::new();
        let mut controller = CameraController::default();
        controller.set_forward(true);
        controller.set_sprint(true);
        let screen_rect = crate::egui::Rect::from_min_size(
            crate::egui::Pos2::ZERO,
            crate::egui::vec2(800.0, 600.0),
        );
        let mut output = context.run_ui(
            crate::egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ui| {
                ui.text_edit_singleline(&mut text).request_focus();
            },
        );
        output.textures_delta.clear();
        let mut output = context.run_ui(
            crate::egui::RawInput {
                screen_rect: Some(screen_rect),
                events: vec![crate::egui::Event::Key {
                    key: crate::egui::Key::W,
                    physical_key: Some(crate::egui::Key::W),
                    pressed: true,
                    repeat: false,
                    modifiers: crate::egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                ui.text_edit_singleline(&mut text);
                controller.process_input(ui.ctx());
            },
        );
        output.textures_delta.clear();
        assert!(!controller.forward());
        assert!(!controller.sprint());
    }

    #[test]
    fn egui_pointer_capture_clears_camera_look_delta() {
        let context = crate::egui::Context::default();
        let mut controller = CameraController::default();
        controller.set_mouse_look_delta((4.0, -2.0));
        let screen_rect = crate::egui::Rect::from_min_size(
            crate::egui::Pos2::ZERO,
            crate::egui::vec2(800.0, 600.0),
        );
        let mut pointer_position = crate::egui::Pos2::ZERO;
        let mut output = context.run_ui(
            crate::egui::RawInput {
                screen_rect: Some(screen_rect),
                ..Default::default()
            },
            |ui| pointer_position = ui.button("capture").rect.center(),
        );
        output.textures_delta.clear();
        let mut output = context.run_ui(
            crate::egui::RawInput {
                screen_rect: Some(screen_rect),
                events: vec![
                    crate::egui::Event::PointerMoved(pointer_position),
                    crate::egui::Event::PointerButton {
                        pos: pointer_position,
                        button: crate::egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: crate::egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                let _ = ui.button("capture");
                controller.process_input(ui.ctx());
            },
        );
        output.textures_delta.clear();
        assert_eq!(controller.mouse_look_delta(), (0.0, 0.0));
    }

    #[test]
    fn keyboard_disabled_suppresses_movement_and_keys() {
        let mut controller = CameraController::default();
        controller.set_keyboard_enabled(false);
        assert!(!controller.keyboard_enabled());
        controller.set_forward(true);
        let mut camera = Camera::default();
        let initial_position = camera.pivot().w_axis.truncate();
        controller.step_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.pivot().w_axis.truncate(), initial_position);
        assert!(!controller.forward());
    }

    #[test]
    fn mouse_disabled_suppresses_look_and_motion() {
        let mut controller = CameraController::default();
        controller.set_mouse_enabled(false);
        assert!(!controller.mouse_enabled());
        controller.process_device_event(&DeviceEvent::MouseMotion {
            delta: (10.0, -5.0),
        });
        assert_eq!(controller.mouse_look_delta(), (0.0, 0.0));
        let mut camera = Camera::default();
        let initial_forward = camera.forward();
        controller.set_mouse_look_delta((5.0, 5.0));
        controller.step_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.forward(), initial_forward);
        assert_eq!(controller.mouse_look_delta(), (0.0, 0.0));
    }

    #[test]
    fn disabling_input_clears_active_state_and_device_setters_work() {
        let mut controller = CameraController::default();
        controller.set_forward(true);
        controller.set_mouse_look_delta((3.0, 4.0));
        controller.set_keyboard_enabled(false);
        controller.set_mouse_enabled(false);
        controller.set_touch_enabled(false);
        assert!(!controller.keyboard_enabled());
        assert!(!controller.mouse_enabled());
        assert!(!controller.touch_enabled());
        assert!(!controller.forward());
        assert_eq!(controller.mouse_look_delta(), (0.0, 0.0));
        controller.set_keyboard_enabled(true);
        controller.set_mouse_enabled(true);
        controller.set_touch_enabled(true);
        controller.set_keyboard_enabled(false);
        assert!(!controller.keyboard_enabled());
        assert!(controller.mouse_enabled());
        assert!(controller.touch_enabled());
    }
}
