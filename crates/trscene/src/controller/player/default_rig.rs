use crate::{glam::*, *};

const DEFAULT_PLAYER_RIG: &[u8] = include_bytes!("../../../assets/UAL1_Lite.glb");

impl PlayerController {
    /// Spawns the built-in player rig and returns its controller.
    ///
    /// The rig starts at the origin, facing `-Z`, with an invisible collider and
    /// idle, walk, sprint, and airborne animations. It is dynamic when physics is
    /// enabled. The camera is left unchanged.
    pub fn spawn(workspace: &mut Workspace) -> Self {
        let mut rig = Rig::from_gltf(DEFAULT_PLAYER_RIG, workspace)
            .unwrap()
            .with_name("Player")
            .with_anchored(false);
        rig.find_first_child::<Rig>("Armature")
            .expect("embedded player armature")
            .1
            .with_orientation(Vec3::new(0.0, 180.0, 0.0));
        rig.add_child_local(
            Part::new()
                .with_name("Collider")
                .with_size(Vec3::new(0.8, 1.8, 0.8))
                .with_position(Vec3::Y * 0.9)
                .with_transparency(1.0),
        );

        let mut animations = PlayerAnimations::default();
        for (state, name) in [
            (PlayerAnimationState::Idle, "Idle_Loop"),
            (PlayerAnimationState::Walk, "Walk_Loop"),
            (PlayerAnimationState::Sprint, "Sprint_Loop"),
            (PlayerAnimationState::Jump, "Jump_Loop"),
            (PlayerAnimationState::Fall, "Jump_Loop"),
        ] {
            animations = animations.with_state(
                state,
                AnimationPlayer::new(rig.animation_clips()[name].clone()).with_looping(true),
            );
        }
        rig.play_animation(animations.get(PlayerAnimationState::Idle).unwrap().clone())
            .unwrap();
        let rig = workspace.add_child(rig);
        let mut controller = Self::new(rig).with_animations(animations);
        controller.animated_state = Some((rig, PlayerAnimationState::Idle));
        controller
    }
}
