use crate::{
    HasPVInstance, InstanceData, InstanceId, PVInstance, glam,
    glam::{Mat4, Quat, Vec3},
    winit::{
        event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
        keyboard::{KeyCode, PhysicalKey},
    },
};

mod touch;

/// A perspective camera with a world-space pivot.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct Camera {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    pub(crate) instance: InstanceData,
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    pub(crate) pv_instance: PVInstance,
    #[cfg_attr(feature = "devtools", inspect(readonly))]
    subject: Option<InstanceId>,
    #[cfg_attr(feature = "devtools", inspect(readonly))]
    aspect: f32,
    fovy: f32,
    znear: f32,
    zfar: f32,
}

/// Access to a camera's projection properties.
pub trait HasCamera {
    /// Returns shared access to the underlying [`Camera`].
    fn camera(&self) -> &Camera;

    /// Returns mutable access to the underlying [`Camera`].
    fn camera_mut(&mut self) -> &mut Camera;

    /// Viewport width divided by viewport height.
    fn aspect(&self) -> f32 {
        self.camera().aspect
    }

    /// Vertical field of view in radians.
    fn fovy(&self) -> f32 {
        self.camera().fovy
    }

    /// Near clipping plane distance.
    fn znear(&self) -> f32 {
        self.camera().znear
    }

    /// Far clipping plane distance.
    fn zfar(&self) -> f32 {
        self.camera().zfar
    }

    /// Sets the viewport width divided by viewport height.
    fn with_aspect(mut self, aspect: f32) -> Self
    where
        Self: Sized,
    {
        self.camera_mut().aspect = aspect.max(0.001);
        self
    }

    /// Sets the vertical field of view in radians.
    fn with_fovy(mut self, fovy: f32) -> Self
    where
        Self: Sized,
    {
        self.camera_mut().fovy = fovy;
        self
    }

    /// Sets the near clipping plane distance.
    fn with_znear(mut self, znear: f32) -> Self
    where
        Self: Sized,
    {
        self.camera_mut().znear = znear;
        self
    }

    /// Sets the far clipping plane distance.
    fn with_zfar(mut self, zfar: f32) -> Self
    where
        Self: Sized,
    {
        self.camera_mut().zfar = zfar;
        self
    }
}

impl<T: HasCamera + ?Sized> HasCamera for &mut T {
    fn camera(&self) -> &Camera {
        (**self).camera()
    }

    fn camera_mut(&mut self) -> &mut Camera {
        (**self).camera_mut()
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
            instance: InstanceData::new("Camera"),
            pv_instance: PVInstance::from_world_transform(Mat4::from_rotation_translation(
                Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(pitch),
                position,
            )),
            subject: None,
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

    /// Returns the instance this camera follows in third-person mode.
    ///
    /// A missing subject leaves the camera in free-moving mode.
    pub fn subject(&self) -> Option<InstanceId> {
        self.subject
    }

    /// Sets the instance this camera follows in third-person mode.
    ///
    /// The ID must belong to a spatial instance in the active workspace. Set
    /// this to `None` to return to free-moving mode.
    pub fn set_subject(&mut self, subject: Option<InstanceId>) {
        self.subject = subject;
    }

    /// Sets the instance this camera follows, returning `self` for chaining.
    pub fn with_subject(mut self, subject: Option<InstanceId>) -> Self {
        self.set_subject(subject);
        self
    }
}

crate::impl_instance!(
    Camera,
    class_name = "Camera",
    data = instance,
    inspect = Camera,
);

/// Physical keyboard keys assigned to camera movement and orbit actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CameraKeyBindings {
    /// Keys that move forward freely or zoom toward the subject.
    pub forward: Vec<KeyCode>,
    /// Keys that move backward freely or zoom away from the subject.
    pub backward: Vec<KeyCode>,
    /// Keys that strafe left freely or orbit left around the subject.
    pub left: Vec<KeyCode>,
    /// Keys that strafe right freely or orbit right around the subject.
    pub right: Vec<KeyCode>,
    /// Keys that move up freely or raise the subject orbit.
    pub up: Vec<KeyCode>,
    /// Keys that move down freely or lower the subject orbit.
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

/// Keyboard, mouse, and touch input for free-moving and subject-following [`Camera`]s.
#[derive(Clone, Debug)]
pub struct CameraController {
    /// Movement and zoom speed in world units per second.
    pub speed: f32,
    /// Look sensitivity in radians per raw mouse unit or physical touch pixel.
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
    /// Whether mouse-look input is processed.
    pub mouse_enabled: bool,
    /// Whether keyboard movement input is processed.
    pub keyboard_enabled: bool,
    /// Whether touchscreen movement and drag-to-look input are processed.
    ///
    /// Enabled by default; the joystick appears once a touchscreen is detected.
    pub touch_enabled: bool,
    /// Distance from the subject's focus point in third-person mode.
    pub subject_distance: f32,
    /// Minimum camera distance from the subject's focus point.
    pub min_subject_distance: f32,
    /// Maximum camera distance from the subject's focus point.
    pub max_subject_distance: f32,
    /// Local-space offset from the subject pivot to the camera's focus point.
    pub subject_offset: Vec3,
    /// Orbit speed in radians per second for keyboard and joystick controls.
    pub orbit_speed: f32,
    /// Minimum vertical orbit angle in radians.
    pub min_subject_pitch: f32,
    /// Maximum vertical orbit angle in radians.
    pub max_subject_pitch: f32,
    mouse_dragging: bool,
    last_cursor_position: Option<(f64, f64)>,
    orbit_subject: Option<InstanceId>,
    orbit_yaw: f32,
    orbit_pitch: f32,
    touch: touch::TouchControls,
}

impl Default for CameraController {
    fn default() -> Self {
        CameraController::new(6.0, 0.0025)
    }
}

impl CameraController {
    /// Creates a camera controller with the given movement speed and sensitivity.
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self::new_with_key_bindings(speed, sensitivity, CameraKeyBindings::default())
    }

    /// Creates a camera controller with custom movement key bindings.
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
            mouse_enabled: true,
            keyboard_enabled: true,
            touch_enabled: true,
            subject_distance: 5.0,
            min_subject_distance: 0.5,
            max_subject_distance: 100.0,
            subject_offset: Vec3::ZERO,
            orbit_speed: 1.5,
            min_subject_pitch: -89.0_f32.to_radians(),
            max_subject_pitch: 89.0_f32.to_radians(),
            mouse_dragging: false,
            last_cursor_position: None,
            orbit_subject: None,
            orbit_yaw: 0.0,
            orbit_pitch: 0.0,
            touch: touch::TouchControls::default(),
        }
    }

    /// Sets the third-person distance from the subject's focus point.
    pub fn with_subject_distance(mut self, distance: f32) -> Self {
        self.subject_distance = distance;
        self
    }

    /// Sets the local-space offset from the subject pivot to the focus point.
    pub fn with_subject_offset(mut self, offset: Vec3) -> Self {
        self.subject_offset = offset;
        self
    }

    /// Sets the minimum and maximum third-person camera distance.
    pub fn with_subject_distance_limits(mut self, min: f32, max: f32) -> Self {
        self.min_subject_distance = min;
        self.max_subject_distance = max;
        self
    }

    /// Sets the minimum and maximum vertical orbit angles in radians.
    pub fn with_subject_pitch_limits(mut self, min: f32, max: f32) -> Self {
        self.min_subject_pitch = min;
        self.max_subject_pitch = max;
        self
    }

    /// Sets the keyboard and joystick orbit speed in radians per second.
    pub fn with_orbit_speed(mut self, speed: f32) -> Self {
        self.orbit_speed = speed;
        self
    }

    /// Enables or disables all camera input (keyboard, mouse, and touch).
    ///
    /// When set to `false`, any currently held movement keys, accumulated
    /// look deltas, and active mouse or touch gestures are cleared immediately.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.set_keyboard_enabled(enabled);
        self.set_mouse_enabled(enabled);
        self.set_touch_enabled(enabled);
    }

    /// Enables or disables keyboard movement input.
    ///
    /// When set to `false`, any currently held movement keys are cleared immediately.
    pub fn set_keyboard_enabled(&mut self, enabled: bool) {
        self.keyboard_enabled = enabled;
        if !enabled {
            self.clear_keyboard_input();
        }
    }

    /// Enables or disables mouse-look input.
    ///
    /// When set to `false`, accumulated mouse deltas and active mouse dragging
    /// states are cleared immediately.
    pub fn set_mouse_enabled(&mut self, enabled: bool) {
        self.mouse_enabled = enabled;
        if !enabled {
            self.mouse_delta = (0.0, 0.0);
            self.stop_mouse_drag();
        }
    }

    /// Enables or disables touchscreen movement and look input.
    ///
    /// Disabling touch also hides the joystick and clears active gestures.
    pub fn set_touch_enabled(&mut self, enabled: bool) {
        self.touch_enabled = enabled;
        if !enabled {
            self.touch.clear();
        }
    }

    /// Records touchscreen capability before the first touch event, if known.
    ///
    /// Otherwise normalized input detects it automatically. Passing `false`
    /// clears active touch gestures; normalized input may detect it again.
    pub fn set_touchscreen_detected(&mut self, detected: bool) {
        self.touch.detected = detected;
        if !detected {
            self.touch.clear();
        }
    }

    /// Returns `true` if any camera input (keyboard, mouse, or touch) is enabled.
    pub fn is_enabled(&self) -> bool {
        self.keyboard_enabled || self.mouse_enabled || self.touch_enabled
    }

    /// Sets whether all camera input is enabled, returning `self` for chaining.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.set_enabled(enabled);
        self
    }

    /// Sets whether keyboard movement input is enabled, returning `self` for chaining.
    pub fn with_keyboard_enabled(mut self, enabled: bool) -> Self {
        self.set_keyboard_enabled(enabled);
        self
    }

    /// Sets whether mouse-look input is enabled, returning `self` for chaining.
    pub fn with_mouse_enabled(mut self, enabled: bool) -> Self {
        self.set_mouse_enabled(enabled);
        self
    }

    /// Sets whether touchscreen input is enabled, returning `self` for chaining.
    pub fn with_touch_enabled(mut self, enabled: bool) -> Self {
        self.set_touch_enabled(enabled);
        self
    }

    /// Feeds a winit window event into the controller. Returns true when it was used.
    /// Applies keyboard movement and left-button mouse-drag look input.
    ///
    /// The controller recognizes the physical keys in [`CameraKeyBindings`].
    /// Losing window focus clears all held keys and ends an active mouse drag.
    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if !self.keyboard_enabled {
                    return false;
                }
                let PhysicalKey::Code(key) = event.physical_key else {
                    return false;
                };
                let pressed = event.state == ElementState::Pressed;
                self.process_key(key, pressed)
            }
            WindowEvent::MouseInput { state, button, .. } if *button == self.mouse_drag_button => {
                if !self.mouse_enabled {
                    return false;
                }
                self.mouse_dragging = *state == ElementState::Pressed;
                self.last_cursor_position = None;
                true
            }
            WindowEvent::CursorMoved { position, .. } if self.mouse_dragging => {
                if !self.mouse_enabled {
                    self.stop_mouse_drag();
                    return false;
                }
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

    /// Returns whether an enabled movement binding is held in a focused eframe window.
    ///
    /// Ignores UI keyboard capture so integrations can route focus to the scene.
    /// Sprint bindings alone do not request movement.
    pub fn has_eframe_movement_input(&self, input: &crate::egui::InputState) -> bool {
        self.keyboard_enabled
            && input.focused
            && [
                &self.key_bindings.forward,
                &self.key_bindings.backward,
                &self.key_bindings.left,
                &self.key_bindings.right,
                &self.key_bindings.up,
                &self.key_bindings.down,
            ]
            .into_iter()
            .any(|binding| eframe_key_down(input, binding))
    }

    /// Feeds eframe's normalized input state into the controller.
    pub fn process_eframe_input(&mut self, input: &crate::egui::InputState) {
        self.process_eframe_input_with_capture(input, false, false);
    }

    /// Feeds eframe's normalized input state into the controller while
    /// respecting egui's pointer and keyboard capture state.
    ///
    /// `egui_wants_pointer_input` and `egui_wants_keyboard_input` should come
    /// from the matching [`crate::egui::Context`] methods. Camera input is
    /// suppressed independently for each device while egui owns it.
    pub fn process_eframe_input_with_capture(
        &mut self,
        input: &crate::egui::InputState,
        egui_wants_pointer_input: bool,
        egui_wants_keyboard_input: bool,
    ) {
        self.process_eframe_input_inner(
            input,
            egui_wants_pointer_input,
            egui_wants_keyboard_input,
            |_| egui_wants_pointer_input,
        );
    }

    /// Processes eframe input with UI capture and draws the touchscreen joystick.
    ///
    /// Call once per frame before [`Self::update_camera`]. Touch gestures are
    /// assigned at touch-down, allowing simultaneous joystick movement and look.
    /// Touches starting on UI areas are left to egui.
    pub fn process_eframe_context(&mut self, context: &crate::egui::Context) {
        let pointer_capture = context.egui_wants_pointer_input();
        let keyboard_capture = context.egui_wants_keyboard_input();
        let mut captured_touches = context.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    crate::egui::Event::Touch {
                        phase: crate::egui::TouchPhase::Start,
                        pos,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        let interactive_rects = if captured_touches.is_empty() {
            Vec::new()
        } else {
            context.interactive_rects_last_pass()
        };
        captured_touches.retain(|position| {
            interactive_rects
                .iter()
                .any(|rect| rect.contains(*position))
                || context
                    .layer_id_at(*position)
                    .is_some_and(|layer| layer.order != crate::egui::Order::Background)
        });
        context.input(|input| {
            self.process_eframe_input_inner(input, pointer_capture, keyboard_capture, |position| {
                captured_touches.contains(&position)
            });
        });
        self.paint_touch_controls(context);
    }

    /// Draws the joystick for callers that process normalized input themselves.
    pub fn paint_touch_controls(&self, context: &crate::egui::Context) {
        if self.touch_enabled && self.touch.detected {
            self.touch.paint(context);
        }
    }

    fn process_eframe_input_inner(
        &mut self,
        input: &crate::egui::InputState,
        egui_wants_pointer_input: bool,
        egui_wants_keyboard_input: bool,
        touch_captured: impl Fn(crate::egui::Pos2) -> bool,
    ) {
        self.touch.observe_input(input);
        if !input.focused {
            self.clear_keys();
            return;
        }

        if self.touch_enabled {
            self.touch.process(input, touch_captured);
        } else {
            self.touch.clear();
        }

        if !self.keyboard_enabled || egui_wants_keyboard_input {
            self.clear_keyboard_input();
        } else {
            let key_down = |binding: &[KeyCode]| eframe_key_down(input, binding);
            self.forward = key_down(&self.key_bindings.forward);
            self.backward = key_down(&self.key_bindings.backward);
            self.left = key_down(&self.key_bindings.left);
            self.right = key_down(&self.key_bindings.right);
            self.up = key_down(&self.key_bindings.up);
            self.down = key_down(&self.key_bindings.down);
            self.sprint = key_down(&self.key_bindings.sprint);
        }

        // Integrations emit pointer events alongside touch events. Processing
        // both would rotate twice or rotate when dragging the joystick.
        let touch_pointer = input.any_touches()
            || input
                .events
                .iter()
                .any(|event| matches!(event, crate::egui::Event::Touch { .. }));
        if !self.mouse_enabled || egui_wants_pointer_input || touch_pointer {
            self.mouse_delta = (0.0, 0.0);
            self.stop_mouse_drag();
        } else if let Some(button) = eframe_button(self.mouse_drag_button)
            && input.pointer.button_down(button)
        {
            let delta = input.pointer.delta() * input.pixels_per_point;
            self.process_mouse_motion((delta.x, delta.y));
        }
    }

    fn process_key(&mut self, key: KeyCode, pressed: bool) -> bool {
        if !self.keyboard_enabled {
            return false;
        }
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
        if !self.mouse_enabled {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            self.process_mouse_motion((delta.0 as f32, delta.1 as f32));
        }
    }

    /// Accumulates a mouse-motion delta for the next camera update.
    pub fn process_mouse_motion(&mut self, delta: (f32, f32)) {
        if !self.mouse_enabled {
            return;
        }
        self.mouse_delta.0 += delta.0;
        self.mouse_delta.1 += delta.1;
    }

    fn movement_axes(&self) -> Vec3 {
        let axis =
            |positive: bool, negative: bool| (i8::from(positive) - i8::from(negative)) as f32;
        Vec3::new(
            (axis(self.right, self.left) + self.touch.movement.x).clamp(-1.0, 1.0),
            axis(self.up, self.down),
            (axis(self.forward, self.backward) - self.touch.movement.y).clamp(-1.0, 1.0),
        )
    }

    fn speed_multiplier(&self) -> f32 {
        if self.sprint { 2.5 } else { 1.0 }
    }

    fn update_free_camera(
        &mut self,
        camera: &mut Camera,
        delta_seconds: f32,
        axes: Vec3,
        look: crate::egui::Vec2,
    ) {
        self.orbit_subject = None;

        // Rebuild a roll-free orientation from yaw/pitch every frame.
        // Incremental matrix multiplies (`pivot * rotation`) accumulate
        // floating-point error as roll/scale drift over many frames.
        let pose = camera.pose();
        let position = pose.w_axis.truncate();
        let forward0 = camera.forward();
        let right0 = pose.transform_vector3(Vec3::X);
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

        let yaw_delta = -look.x * self.sensitivity;
        let new_yaw = yaw + yaw_delta;
        let new_pitch = (current_pitch - look.y * self.sensitivity)
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        let new_rotation = Quat::from_rotation_y(new_yaw) * Quat::from_rotation_x(new_pitch);

        let forward = new_rotation * Vec3::NEG_Z;
        let right = Quat::from_rotation_y(new_yaw) * Vec3::X;
        let movement = forward * axes.z + right * axes.x + Vec3::Y * axes.y;

        let mut new_position = position;
        if movement.length_squared() > 0.0 {
            let speed = self.speed * self.speed_multiplier();
            new_position += movement.normalize() * axes.length().min(1.0) * speed * delta_seconds;
        }

        camera.with_pose(Mat4::from_rotation_translation(new_rotation, new_position));
    }

    /// Moves and rotates `camera` using accumulated input, then clears look deltas.
    ///
    /// Without a subject, movement is frame-rate independent. The delta is
    /// capped at 100 ms, sprinting multiplies movement speed by `2.5`, and
    /// pitch is clamped to 89 degrees from the horizon.
    ///
    /// In free-moving mode, `forward`/`backward` fly along the camera's look
    /// direction, so looking down and pressing forward descends. Strafing stays
    /// horizontal, and the orientation is rebuilt from yaw/pitch every update
    /// so no roll can accumulate.
    ///
    /// With a subject, the camera follows it in third person: mouse movement
    /// and left/right keys orbit, forward/backward keys zoom, and up/down keys
    /// adjust orbit elevation. Passing `None` keeps the camera free-moving.
    /// The joystick uses the same forward/backward and left/right actions,
    /// with speed proportional to its displacement. Screen drags apply look input.
    pub fn update_camera(
        &mut self,
        camera: &mut Camera,
        subject: Option<(InstanceId, Mat4)>,
        delta_seconds: f32,
    ) {
        if !self.mouse_enabled {
            self.mouse_delta = (0.0, 0.0);
        }
        if !self.keyboard_enabled {
            self.clear_keyboard_input();
        }
        if !self.touch_enabled {
            self.touch.clear();
        }

        let axes = self.movement_axes();
        let look =
            crate::egui::vec2(self.mouse_delta.0, self.mouse_delta.1) + self.touch.look_delta;
        self.mouse_delta = (0.0, 0.0);
        self.touch.look_delta = crate::egui::Vec2::ZERO;

        let delta_seconds = if delta_seconds.is_finite() {
            delta_seconds.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let Some((subject_id, subject_pose)) = subject else {
            self.update_free_camera(camera, delta_seconds, axes, look);
            return;
        };
        let offset = if self.subject_offset.is_finite() {
            self.subject_offset
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

        self.orbit_yaw -= look.x * self.sensitivity;
        let (min_pitch, max_pitch) = self.subject_pitch_limits();
        self.orbit_pitch =
            (self.orbit_pitch + look.y * self.sensitivity).clamp(min_pitch, max_pitch);

        let speed_multiplier = self.speed_multiplier();
        let movement = self.speed * speed_multiplier * delta_seconds;
        self.subject_distance -= axes.z * movement;
        let orbit_step = self.orbit_speed * speed_multiplier * delta_seconds;
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
        self.subject_distance = if self.subject_distance.is_finite() {
            self.subject_distance
        } else {
            min_distance
        }
        .clamp(min_distance, max_distance);

        let horizontal_distance = self.orbit_pitch.cos() * self.subject_distance;
        let camera_position = focus
            + Vec3::new(
                self.orbit_yaw.sin() * horizontal_distance,
                self.orbit_pitch.sin() * self.subject_distance,
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

    fn clear_keys(&mut self) {
        self.clear_keyboard_input();
        self.mouse_delta = (0.0, 0.0);
        self.stop_mouse_drag();
        self.touch.clear();
    }

    fn clear_keyboard_input(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.sprint = false;
    }

    fn stop_mouse_drag(&mut self) {
        self.mouse_dragging = false;
        self.last_cursor_position = None;
    }
}

impl HasPVInstance for Camera {
    fn pv(&self) -> &PVInstance {
        &self.pv_instance
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        &mut self.pv_instance
    }
}

impl HasCamera for Camera {
    fn camera(&self) -> &Camera {
        self
    }

    fn camera_mut(&mut self) -> &mut Camera {
        self
    }
}

fn set_key(key: &mut bool, pressed: bool) -> bool {
    *key = pressed;
    true
}

fn eframe_key_down(input: &crate::egui::InputState, binding: &[KeyCode]) -> bool {
    binding
        .iter()
        .copied()
        .any(|key| eframe_key(key).is_some_and(|key| input.key_down(key)))
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
    fn camera_projection_properties_use_getters_and_mutable_builders() {
        let camera = Camera::default()
            .with_aspect(2.0)
            .with_fovy(0.75)
            .with_znear(0.2)
            .with_zfar(400.0);

        assert_eq!(camera.aspect(), 2.0);
        assert_eq!(camera.fovy(), 0.75);
        assert_eq!(camera.znear(), 0.2);
        assert_eq!(camera.zfar(), 400.0);
    }

    #[test]
    fn camera_mouse_look_rotates_in_place_with_the_expected_horizontal_sign() {
        let mut camera = Camera::default();
        let original_position = camera.pivot().w_axis.truncate();
        let mut controller = CameraController::new(6.0, 0.1);
        controller.mouse_delta = (1.0, 0.0);

        controller.update_camera(&mut camera, None, 1.0 / 60.0);

        assert_eq!(camera.pivot().w_axis.truncate(), original_position);
        assert!(camera.forward().x > 0.0);
    }

    #[test]
    fn egui_keyboard_capture_clears_camera_movement() {
        let mut controller = CameraController {
            forward: true,
            sprint: true,
            ..Default::default()
        };
        let mut input = crate::egui::InputState::default();
        input.focused = true;

        controller.process_eframe_input_with_capture(&input, false, true);

        assert!(!controller.forward);
        assert!(!controller.sprint);
    }

    #[test]
    fn egui_pointer_capture_clears_camera_look_delta() {
        let mut controller = CameraController {
            mouse_delta: (4.0, -2.0),
            ..Default::default()
        };
        let mut input = crate::egui::InputState::default();
        input.focused = true;

        controller.process_eframe_input_with_capture(&input, true, false);

        assert_eq!(controller.mouse_delta, (0.0, 0.0));
    }

    #[test]
    fn keyboard_disabled_suppresses_movement_and_keys() {
        let mut controller = CameraController::default().with_keyboard_enabled(false);
        assert!(!controller.keyboard_enabled);

        let mut input = crate::egui::InputState::default();
        input.focused = true;
        input.keys_down.insert(crate::egui::Key::W);

        controller.process_eframe_input_with_capture(&input, false, false);
        assert!(!controller.forward);

        assert!(!controller.process_key(KeyCode::KeyW, true));
        assert!(!controller.forward);

        let mut camera = Camera::default();
        let initial_position = camera.pivot().w_axis.truncate();
        controller.forward = true; // Even if force-set, update_camera clears disabled keyboard input
        controller.update_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.pivot().w_axis.truncate(), initial_position);
        assert!(!controller.forward);
    }

    #[test]
    fn mouse_disabled_suppresses_look_and_motion() {
        let mut controller = CameraController::default().with_mouse_enabled(false);
        assert!(!controller.mouse_enabled);

        controller.process_mouse_motion((10.0, -5.0));
        assert_eq!(controller.mouse_delta, (0.0, 0.0));

        controller.process_device_event(&DeviceEvent::MouseMotion {
            delta: (10.0, -5.0),
        });
        assert_eq!(controller.mouse_delta, (0.0, 0.0));

        let mut camera = Camera::default();
        let initial_forward = camera.forward();
        controller.mouse_delta = (5.0, 5.0); // Force-set delta should be ignored if disabled
        controller.update_camera(&mut camera, None, 1.0 / 60.0);
        assert_eq!(camera.forward(), initial_forward);
        assert_eq!(controller.mouse_delta, (0.0, 0.0));
    }

    #[test]
    fn disabling_input_clears_active_state_and_builder_methods_work() {
        let mut controller = CameraController {
            forward: true,
            mouse_delta: (3.0, 4.0),
            ..Default::default()
        };
        assert!(controller.is_enabled());

        controller.set_enabled(false);
        assert!(!controller.keyboard_enabled);
        assert!(!controller.mouse_enabled);
        assert!(!controller.is_enabled());
        assert!(!controller.forward);
        assert_eq!(controller.mouse_delta, (0.0, 0.0));

        let controller = controller
            .with_enabled(true)
            .with_keyboard_enabled(false)
            .with_mouse_enabled(true);
        assert!(!controller.keyboard_enabled);
        assert!(controller.mouse_enabled);
        assert!(controller.is_enabled());
    }
}
