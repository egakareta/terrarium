use std::ops::{Deref, DerefMut};

use crate::{glam::*, *};

/// Moves a [`Rig`] with keyboard or touch input and turns it toward travel.
///
/// The rig turns toward horizontal travel using [`Self::turn_factor`], stopping
/// mid-turn when movement stops. Mouse and touch look orbit the active camera
/// when it follows this rig; the mouse wheel changes its orbit distance. Orbit
/// input never turns an idle rig.
///
/// With the `physics` feature, unanchored rigs move through their physics bodies,
/// preserving gravity and collisions; the up action jumps grounded dynamic rigs.
/// Anchored rigs, and builds without physics, move their poses directly. Descendants
/// and scale are preserved in both cases.
/// Movement uses the active camera's horizontal heading. Player look input orbits
/// the camera when its subject is this rig.
#[derive(Clone, Debug)]
pub struct PlayerController {
    /// ID of the rig to move. Missing or removed IDs leave the scene unchanged.
    pub rig: Option<InstanceId>,
    /// Device state and bindings used to generate controller actions.
    pub input: ControllerInput,
    /// Camera behavior used when the workspace camera follows this rig.
    pub camera_controller: CameraController,
    /// Movement speed in world units per second.
    pub speed: f32,
    /// Upward jump velocity in world units per second.
    pub jump_speed: f32,
    /// Camera orbit sensitivity in radians per physical pixel.
    pub sensitivity: f32,
    /// Controls how quickly the rig turns toward its movement direction.
    ///
    /// Each moving frame uses `(turn_factor * delta_seconds).clamp(0.0, 1.0)`
    /// along the shortest yaw arc. Zero disables turning; idle frames retain yaw.
    pub turn_factor: f32,
}

impl PlayerController {
    /// Creates a controller for `rig` with WASD/Space/Shift movement and drag-look bindings.
    ///
    /// Space uses the up action to jump a grounded dynamic rig. The down action is
    /// unbound. Mouse and touch look orbit the active camera when it follows the rig.
    pub fn new(rig: InstanceId) -> Self {
        let mut input = ControllerInput::default();
        input.key_bindings.down.clear();
        Self {
            rig: Some(rig),
            input,
            camera_controller: CameraController::default(),
            speed: 6.0,
            jump_speed: 7.0,
            sensitivity: 0.0025,
            turn_factor: 10.0,
        }
    }

    /// Replaces the input state, device settings, and key bindings.
    pub fn with_input(mut self, input: ControllerInput) -> Self {
        self.input = input;
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

    /// Enables or disables mouse look and wheel zoom input.
    pub fn with_mouse_enabled(mut self, enabled: bool) -> Self {
        self.input.set_mouse_enabled(enabled);
        self
    }

    /// Enables or disables touch movement and look input.
    pub fn with_touch_enabled(mut self, enabled: bool) -> Self {
        self.input.set_touch_enabled(enabled);
        self
    }

    /// Sets the yaw turn rate used to face the movement direction.
    ///
    /// The per-frame interpolation fraction is `clamp(factor * delta, 0, 1)`.
    /// Non-positive or non-finite values leave the current heading unchanged.
    pub fn with_turn_factor(mut self, factor: f32) -> Self {
        self.turn_factor = factor;
        self
    }

    /// Sets the upward jump velocity in world units per second. Non-positive or
    /// non-finite values disable jumping.
    pub fn with_jump_speed(mut self, speed: f32) -> Self {
        self.jump_speed = speed;
        self
    }

    /// Applies accumulated input directly to a rig without a workspace.
    ///
    /// Horizontal movement is world-aligned, look and jumping are discarded, and
    /// no physics step is performed. Use this controller as the engine's custom
    /// controller for camera-relative movement, camera orbit, and physics integration.
    pub fn step_rig(&mut self, rig: &mut Rig, delta_seconds: f32) {
        let actions = self.input.take_actions();
        self.step_rig_with_actions(rig, actions, delta_seconds);
    }

    /// Applies scripted actions as world-aligned, non-physics pose updates.
    ///
    /// Look and the up action are ignored; pass movement on `x` and `z`. The rig
    /// turns only while moving. This method neither reads device input nor advances physics.
    pub fn step_rig_with_actions(
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
            self.camera_controller
                .queue_look(actions.look * self.sensitivity);
            reference.look = self.camera_controller.queued_look();
        }
        let movement_rotation = reference.yaw_rotation(workspace.camera().pose(), 1.0);
        let delta_seconds = super::frame_delta(delta_seconds);
        let (movement, rotation) = self.motion(actions, movement_rotation, pose, delta_seconds);
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
            return;
        }
        if let Some(rig) = workspace.get_mut::<Rig>(id) {
            Self::apply_pose(rig, movement, rotation);
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

impl Deref for PlayerController {
    type Target = ControllerInput;

    fn deref(&self) -> &Self::Target {
        &self.input
    }
}

impl DerefMut for PlayerController {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.input
    }
}

impl Controller for PlayerController {
    fn input(&self) -> Option<&ControllerInput> {
        Some(&self.input)
    }

    fn input_mut(&mut self) -> Option<&mut ControllerInput> {
        Some(&mut self.input)
    }

    fn step(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
        let (actions, scroll_delta) = self.input.take_actions_and_scroll();
        self.step_workspace_with_actions(workspace, actions, delta_seconds);
        if scroll_delta != 0.0 {
            let follows_rig = self
                .rig
                .is_some_and(|id| workspace.camera().subject() == Some(id));
            if follows_rig {
                self.camera_controller.queue_subject_zoom(scroll_delta);
            }
        }
    }

    fn post_physics(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
        let subject = workspace.subject().and_then(|id| {
            workspace
                .view::<dyn Instance>(id)
                .and_then(|instance| instance.spatial().map(|spatial| (id, spatial.rigid_pose())))
        });
        self.camera_controller.step_camera_with_actions(
            workspace.camera_mut(),
            subject,
            ControllerActions::default(),
            delta_seconds,
        );
    }
}
