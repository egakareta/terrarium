use crate::{glam::*, *};

instance! {
    #[instance(default = false)]
    /// Stateful camera motion shared by camera and player controllers.
    ///
    /// The rig owns free-flight and subject-orbit behavior, queued look and zoom,
    /// and orbit settings. Controllers provide input and call [`Self::update_camera`].
    #[derive(Clone, Debug)]
    pub struct CameraRig {
        #[property]
        /// Camera movement and subject-zoom speed in world units per second.
        speed: f32,
        #[property]
        /// Minimum camera distance from the subject's focus point.
        min_subject_distance: f32,
        #[property]
        /// Maximum camera distance from the subject's focus point.
        max_subject_distance: f32,
        #[property]
        /// Orbit speed in radians per second for keyboard and joystick controls.
        orbit_speed: f32,
        #[property]
        /// Minimum vertical orbit angle in radians.
        min_subject_pitch: f32,
        #[property]
        /// Maximum vertical orbit angle in radians.
        max_subject_pitch: f32,
        orbit_subject: Option<InstanceId>,
        orbit_yaw: f32,
        orbit_pitch: f32,
        queued_look: Vec2,
        queued_subject_zoom: f32,
    }

    impl CameraRig {
        /// Adds a look rotation in radians to the next camera update.
        ///
        /// Queued look is consumed by the next update, even if the controller's input
        /// devices are disabled. Non-finite values are ignored.
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
    }
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            speed: 6.0,
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
}

impl CameraRig {
    /// Sets the bounds used to clamp orbit distance during zoom.
    pub fn with_subject_distance_limits(mut self, min: f32, max: f32) -> Self {
        self.set_min_subject_distance(min);
        self.set_max_subject_distance(max);
        self
    }

    /// Sets the pitch bounds for third-person orbit, in radians.
    pub fn with_subject_pitch_limits(mut self, min: f32, max: f32) -> Self {
        self.set_min_subject_pitch(min);
        self.set_max_subject_pitch(max);
        self
    }

    pub(crate) fn queued_look(&self) -> Vec2 {
        self.queued_look
    }

    pub(crate) fn update_camera(
        &mut self,
        camera: &mut Camera,
        subject: Option<(InstanceId, Mat4)>,
        actions: ControllerActions,
        sensitivity: f32,
        delta_seconds: f32,
    ) {
        let delta_seconds = crate::controller::frame_delta(delta_seconds);
        let queued_subject_zoom = std::mem::take(&mut self.queued_subject_zoom);
        let look = actions.look * sensitivity + std::mem::take(&mut self.queued_look);
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
