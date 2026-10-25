use crate::{glam::*, *};

#[cfg(feature = "default-player-rig")]
mod default_rig;

instance! {
    #[instance(default = false)]
    /// Moves a [`Rig`] with keyboard or touch input and turns it toward travel.
    ///
    /// The rig turns toward horizontal travel using [`Self::turn_factor`], stopping
    /// mid-turn when movement stops. Mouse and touch look orbit the active camera
    /// when it follows this rig; the mouse wheel changes its orbit distance through
    /// the controller's [`CameraRig`]. Orbit input never turns an idle rig.
    ///
    /// With the `physics` feature, unanchored rigs move through their physics bodies,
    /// preserving gravity and collisions; the up action jumps grounded dynamic rigs.
    /// Anchored rigs, and builds without physics, move their poses directly. Descendants
    /// and scale are preserved in both cases.
    /// Movement uses the active camera's horizontal heading. The composed camera rig
    /// handles look and orbit when the camera's subject is this rig.
    #[derive(Clone, Debug)]
    pub struct PlayerController {
        #[extends]
        controller: Controller,
        #[property]
        /// ID of the rig to move. Missing or removed IDs leave the scene unchanged.
        rig: Option<InstanceId>,
        /// Camera motion used when the workspace camera follows this rig.
        #[property(borrow)]
        pub camera_rig: CameraRig,
        #[property]
        /// Movement speed in world units per second.
        speed: f32,
        #[property]
        /// Upward jump velocity in world units per second. Non-positive or non-finite
        /// values disable jumping.
        jump_speed: f32,
        #[property]
        /// Camera orbit sensitivity in radians per physical pixel.
        sensitivity: f32,
        #[property]
        /// Controls how quickly the rig turns toward its movement direction.
        ///
        /// Each moving frame uses `(turn_factor * delta_seconds).clamp(0.0, 1.0)`
        /// along the shortest yaw arc. Zero disables turning; idle frames retain yaw.
        turn_factor: f32,
        /// Animation players selected automatically from movement and physics state.
        pub animations: PlayerAnimations,
        locomotion: PlayerAnimationState,
        #[property]
        /// The most recently selected locomotion animation state.
        animation_state: PlayerAnimationState,
        animated_state: Option<(InstanceId, PlayerAnimationState)>,
    }

    impl PlayerController {
        #[property]
        /// Animation players selected automatically from movement and physics state.
        pub fn animations(&self) -> &PlayerAnimations {
            &self.animations
        }

        /// Replaces animation bindings and resets the selected animation state.
        pub fn set_animations(&mut self, animations: PlayerAnimations) {
            self.animations = animations;
            self.animated_state = None;
        }

        #[r#override(Controller::step)]
        fn step(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
            let (actions, scroll_delta) = self
                .controller_input_mut()
                .take_actions_inner();
            self.player_controller_mut().step_workspace_with_actions(workspace, actions, delta_seconds);
            if scroll_delta != 0.0 {
                let follows_rig = self
                    .rig()
                    .is_some_and(|id| workspace.camera().subject() == Some(id));
                if follows_rig {
                    self.camera_rig.queue_subject_zoom(scroll_delta);
                }
            }
        }

        #[r#override(Controller::post_physics)]
        fn post_physics(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
            let player = self.player_controller_mut();
            player.update_animation(workspace);
            let subject = workspace.subject().and_then(|id| {
                workspace
                    .view::<dyn Instance>(id)
                    .and_then(|instance| instance.spatial().map(|spatial| (id, spatial.rigid_pose())))
            });
            player.camera_rig.update_camera(
                workspace.camera_mut(),
                subject,
                ControllerActions::default(),
                1.0,
                delta_seconds,
            );
        }
    }
}

impl PlayerController {
    /// Creates a controller for a custom rig with WASD/Space/Shift and drag-look bindings.
    ///
    /// Space uses the up action to jump a grounded dynamic rig. The down action is
    /// unbound. Shift scales movement by `3.0` by default. Mouse and touch look orbit
    /// the active camera when it follows the rig.
    pub fn new(rig: InstanceId) -> Self {
        let controller = Controller::default().with_name("PlayerController");
        let mut key_bindings = controller.key_bindings().clone();
        key_bindings.down.clear();
        Self {
            controller: controller
                .with_key_bindings(key_bindings)
                .with_sprint_scale(3.0),
            rig: Some(rig),
            camera_rig: CameraRig::default(),
            speed: 1.3,
            jump_speed: 3.0,
            sensitivity: 0.0025,
            turn_factor: 10.0,
            animations: PlayerAnimations::default(),
            locomotion: PlayerAnimationState::Idle,
            animation_state: PlayerAnimationState::Idle,
            animated_state: None,
        }
    }

    /// Applies accumulated input directly to a rig without a workspace.
    ///
    /// Horizontal movement is world-aligned, look and jumping are discarded, and
    /// no physics step is performed. Use [`Self::step_workspace_with_actions`] for
    /// camera-relative movement and physics integration.
    pub fn step_rig(&mut self, rig: &mut Rig, delta_seconds: f32) {
        let actions = self.controller_input_mut().take_actions_inner().0;
        self.step_rig_with_actions(rig, actions, delta_seconds);
    }

    pub(crate) fn step_rig_with_actions(
        &self,
        rig: &mut Rig,
        actions: ControllerActions,
        delta_seconds: f32,
    ) {
        let (movement, rotation) = self.motion(actions, Quat::IDENTITY, rig.pose(), delta_seconds);
        Self::apply_pose(rig, movement, rotation);
    }

    /// Applies actions through the workspace's camera and physics state.
    ///
    /// Movement follows the active camera's horizontal heading. Look is queued
    /// for the camera only when it follows this rig. Dynamic rig movement is
    /// queued for the next physics step; anchored rigs move immediately. This
    /// method does not read or consume device input.
    pub fn step_workspace_with_actions(
        &mut self,
        workspace: &mut Workspace,
        actions: ControllerActions,
        delta_seconds: f32,
    ) {
        let Some(id) = self.rig else {
            return;
        };
        let Some(rig) = workspace.get::<Rig>(id) else {
            return;
        };
        let pose = rig.pose();
        #[cfg(feature = "physics")]
        let anchored = rig.anchored();

        let mut reference = ControllerActions::default();
        if workspace.camera().subject() == Some(id) {
            self.camera_rig.queue_look(actions.look * self.sensitivity);
            reference.look = self.camera_rig.queued_look();
        }
        let movement_rotation = reference.yaw_rotation(workspace.camera().pose(), 1.0);
        let delta_seconds = super::frame_delta(delta_seconds);
        let (movement, rotation) = self.motion(actions, movement_rotation, pose, delta_seconds);
        let mut horizontal = actions;
        horizontal.movement.y = 0.0;
        let movement_magnitude = horizontal
            .movement
            .clamp(Vec3::NEG_ONE, Vec3::ONE)
            .length_squared();
        let sprint_magnitude = horizontal.scaled_movement().length_squared();
        self.locomotion = if horizontal
            .translation(movement_rotation, self.speed, 1.0)
            .length_squared()
            <= 1e-8
        {
            PlayerAnimationState::Idle
        } else if sprint_magnitude > movement_magnitude {
            PlayerAnimationState::Sprint
        } else {
            PlayerAnimationState::Walk
        };
        #[cfg(feature = "physics")]
        if !anchored {
            if delta_seconds > 0.0 {
                let jump_speed = (actions.movement.y > 0.0
                    && self.jump_speed.is_finite()
                    && self.jump_speed > 0.0)
                    .then_some(self.jump_speed);
                workspace.physics_mut().queue_player_motion(
                    id,
                    movement / delta_seconds,
                    rotation,
                    jump_speed,
                );
            }
            self.update_animation(workspace);
            return;
        }
        if let Some(rig) = workspace.get_mut::<Rig>(id) {
            Self::apply_pose(rig, movement, rotation);
        }
        self.update_animation(workspace);
    }

    fn update_animation(&mut self, workspace: &mut Workspace) {
        let Some(id) = self.rig else { return };
        let Some(rig) = workspace.get::<Rig>(id) else {
            return;
        };
        let state = self.locomotion;
        #[cfg(feature = "physics")]
        let state = if !rig.anchored()
            && let Some(body) = workspace.physics().body(id)
            && !workspace.physics().is_grounded(id)
        {
            if body.linvel().y > 0.05 {
                PlayerAnimationState::Jump
            } else {
                PlayerAnimationState::Fall
            }
        } else {
            state
        };
        self.animation_state = state;
        let Some(player) = self.animations.get(state) else {
            return;
        };
        if self.animated_state == Some((id, state))
            && rig
                .animation()
                .is_some_and(|current| current.clip().same_clip(player.clip()))
        {
            return;
        }
        let duration = self.animations.transition_duration;
        let duration = if duration.is_finite() {
            duration.max(0.0)
        } else {
            0.0
        };
        if let Some(rig) = workspace.get_mut::<Rig>(id) {
            match rig.transition_animation(player.clone(), duration) {
                Ok(()) => self.animated_state = Some((id, state)),
                Err(error) => log::error!("could not play {state:?} animation: {error}"),
            }
        }
    }

    fn motion(
        &self,
        mut actions: ControllerActions,
        reference: Quat,
        pose: Mat4,
        delta_seconds: f32,
    ) -> (Vec3, Option<Quat>) {
        let delta_seconds = super::frame_delta(delta_seconds);
        actions.movement.y = 0.0;
        let movement = actions.translation(reference, self.speed, delta_seconds);
        if movement.length_squared() <= 1e-8 {
            return (movement, None);
        }
        let facing = pose.transform_vector3(Vec3::NEG_Z);
        let yaw = (-facing.x).atan2(-facing.z);
        let target_yaw = (-movement.x).atan2(-movement.z);
        let difference = (target_yaw - yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let factor = if self.turn_factor.is_finite() {
            (self.turn_factor * delta_seconds).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if factor == 0.0 {
            return (movement, None);
        }
        (
            movement,
            Some(Quat::from_rotation_y(yaw + difference * factor)),
        )
    }

    fn apply_pose(rig: &mut Rig, movement: Vec3, rotation: Option<Quat>) {
        if let Some(rotation) = rotation {
            let position = rig.position() + movement;
            rig.with_pose(Mat4::from_rotation_translation(rotation, position));
        } else if movement != Vec3::ZERO {
            let position = rig.position() + movement;
            rig.with_position(position);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reusable_actions_drive_camera_and_player_without_enabled_devices() {
        let actions = ControllerActions {
            movement: Vec3::new(0.3, 0.0, 0.4),
            look: Vec2::ZERO,
            movement_scale: Vec3::splat(2.5),
        };
        let mut rig = Rig::new();
        let mut player = PlayerController::new(rig.id());
        player.set_keyboard_enabled(false);
        player.set_mouse_enabled(false);
        player.set_touch_enabled(false);
        let mut camera = Camera::default().with_pose(Mat4::IDENTITY);
        let mut camera_controller = CameraController::default().with_speed(player.speed);
        camera_controller.set_keyboard_enabled(false);
        camera_controller.set_mouse_enabled(false);
        camera_controller.set_touch_enabled(false);
        player.step_rig_with_actions(&mut rig, actions, 0.1);
        camera_controller.step_camera_with_actions(&mut camera, None, actions, 0.1);
        assert!(rig.position().abs_diff_eq(camera.position(), 1e-5));
        assert!((rig.position().length() - player.speed * 2.5 * 0.5 * 0.1).abs() < 1e-5);
        assert!(rig.forward().abs_diff_eq(Vec3::new(0.6, 0.0, -0.8), 1e-5));
    }
}
