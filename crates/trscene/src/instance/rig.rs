use std::collections::BTreeMap;

use crate::{glam::*, *};

#[cfg(feature = "meshpart")]
mod gltf;
#[cfg(feature = "physics")]
pub(crate) mod physics;
#[cfg(feature = "physics")]
pub use physics::{RigJoint, RigJointMotion, RigPhysicsError};

#[derive(Clone, Debug)]
struct AnimationTransition {
    poses: std::sync::Arc<BTreeMap<Vec<String>, Mat4>>,
    duration: f32,
    elapsed: f32,
}

instance!({
    #[instance(inspect = false)]
    /// A spatial assembly whose descendants move together when its pivot changes.
    ///
    /// Children retain world-space pivots, as elsewhere in the scene tree. All
    /// [`PVInstanceExt`] setters on a rig carry its spatial descendants with it.
    /// With physics enabled, the outermost rig owns its descendants' colliders.
    /// By default they share one body. `Rig::set_physics_joints` splits selected
    /// subtrees into ragdoll or spring-driven bodies. The rig's anchored state
    /// controls the root body, not its joints.
    #[derive(Clone, Debug)]
    pub struct Rig {
        #[data]
        instance: InstanceData,
        #[compose]
        pv_instance: PVInstance,
        /// Whether the rig's physics body is immovable.
        #[property(default = true)]
        anchored: bool,
        animation: Option<AnimationPlayer>,
        transition: Option<AnimationTransition>,
        clips: BTreeMap<String, AnimationClip>,
        #[cfg(feature = "physics")]
        pub(crate) joint_physics: physics::RigPhysics,
        #[cfg(feature = "meshpart")]
        skins: std::sync::Arc<[gltf::SkinBinding]>,
    }
    impl Rig {
        #[r#override(PVInstance::set_pivot)]
        fn set_pivot(&mut self, pivot: crate::glam::Mat4) {
            self.pivot_to(pivot);
        }

        #[r#override(PVInstance::accepts_pivot)]
        fn accepts_pivot(&self, pivot: crate::glam::Mat4) -> bool {
            pivot.inverse().is_finite()
        }

        #[r#override(PVInstance::pivot_to)]
        fn pivot_to(&mut self, pivot: crate::glam::Mat4) {
            assert!(
                pivot.inverse().is_finite(),
                "rig pivots must be finite and invertible"
            );
            pivot_hierarchy_to(self, pivot);
        }

        /// Creates an anchored rig at the identity transform.
        pub fn new() -> Self {
            Self::default()
        }

        /// Replaces the outermost rig's simulated joints, binding their current rest pose.
        ///
        /// Each target owns its subtree up to the next configured joint, and connects
        /// to its nearest configured ancestor or the rig root. Targets must be unique,
        /// unambiguous spatial paths. Invalid configuration leaves the rig unchanged.
        /// An empty collection restores single-body physics on the next physics step.
        ///
        /// Jiggle follows the rest pose or animation; ragdoll ignores animation forces.
        ///
        /// Collision shapes come from descendant parts and their collision flags.
        /// Colliderless joint bodies use unit mass and principal inertia.
        ///
        /// Root pivot edits teleport the assembly; use body velocities or forces for inertia.
        /// Rebind joints after changing the rig's scale.
        #[cfg(feature = "physics")]
        pub fn set_physics_joints(&mut self, joints: Vec<RigJoint>) -> Result<(), RigPhysicsError> {
            physics::configure(self, joints)
        }

        /// Returns this rig's joint configuration.
        #[cfg(feature = "physics")]
        pub fn physics_joints(&self) -> &[RigJoint] {
            &self.joint_physics.joints
        }

        /// Starts an animation and immediately applies its current pose.
        ///
        /// Every target path must resolve unambiguously to a built-in spatial
        /// descendant. A failed validation leaves the current animation and pose
        /// untouched. Removed targets and subtrees with singular pivots are skipped
        /// during playback.
        pub fn play_animation(&mut self, player: AnimationPlayer) -> Result<(), AnimationError> {
            self.transition_animation(player, 0.0)
        }

        /// Crossfades from the displayed pose to a new player over `duration_seconds`.
        ///
        /// Interrupted fades start from the current blended pose. Zero switches
        /// immediately; negative or nonfinite durations leave playback unchanged.
        pub fn transition_animation(
            &mut self,
            player: AnimationPlayer,
            duration_seconds: f32,
        ) -> Result<(), AnimationError> {
            if !duration_seconds.is_finite() || duration_seconds < 0.0 {
                return Err(AnimationError::InvalidTime);
            }
            let mut poses = BTreeMap::new();
            for track in player.clip().tracks() {
                let mut instance: &dyn Instance = self.as_instance();
                let mut parent_pivot = self.pivot();
                for name in track.target() {
                    if instance
                        .spatial()
                        .is_some_and(|spatial| !spatial.world_pivot().inverse().is_finite())
                    {
                        return Err(AnimationError::InvalidTransform);
                    }
                    let mut matches = instance
                        .children()
                        .iter()
                        .filter(|child| child.name() == name);
                    let Some(child) = matches.next() else {
                        return Err(AnimationError::TargetNotFound(track.target().to_vec()));
                    };
                    if matches.next().is_some() {
                        return Err(AnimationError::AmbiguousTarget(track.target().to_vec()));
                    }
                    parent_pivot = instance
                        .spatial()
                        .map_or(parent_pivot, |spatial| spatial.world_pivot());
                    instance = child.as_ref();
                }
                let Some(pivot) = instance.spatial().map(|spatial| spatial.world_pivot()) else {
                    return Err(AnimationError::TargetNotFound(track.target().to_vec()));
                };
                if !pivot.inverse().is_finite() {
                    return Err(AnimationError::InvalidTransform);
                }
                let local = parent_pivot.inverse() * pivot;
                if duration_seconds > 0.0 && !crate::interpolation::valid_transform(local) {
                    return Err(AnimationError::InvalidTransform);
                }
                poses.insert(track.target().to_vec(), local);
            }
            self.transition = (duration_seconds > 0.0).then_some(AnimationTransition {
                poses: std::sync::Arc::new(poses),
                duration: duration_seconds,
                elapsed: 0.0,
            });
            #[cfg(feature = "physics")]
            physics::restore_targets(self);
            apply_animation(self, &player);
            self.animation = Some(player);
            Ok(())
        }

        /// Returns named clips imported with the rig.
        pub fn animation_clips(&self) -> &BTreeMap<String, AnimationClip> {
            &self.clips
        }

        /// Returns the current animation's playback state, including completed clips.
        pub fn animation(&self) -> Option<&AnimationPlayer> {
            self.animation.as_ref()
        }

        /// Returns mutable playback state for pausing, seeking, and restarting.
        pub fn animation_mut(&mut self) -> Option<&mut AnimationPlayer> {
            self.animation.as_mut()
        }

        /// Stops playback, retaining the current pose.
        pub fn stop_animation(&mut self) -> Option<AnimationPlayer> {
            self.transition = None;
            self.animation.take()
        }

        /// Advances this rig's animation and applies poses from ancestors to descendants.
        ///
        /// [`Workspace::step_animations`] advances every rig in a workspace.
        pub fn step_animation(&mut self, delta_seconds: f32) {
            #[cfg(feature = "physics")]
            physics::restore_targets(self);
            let Some(mut player) = self.animation.take() else {
                return;
            };
            if !player.is_finished() || self.transition.is_some() {
                player.update(delta_seconds);
                if let Some(transition) = &mut self.transition {
                    if !player.is_paused() && delta_seconds.is_finite() && delta_seconds > 0.0 {
                        transition.elapsed =
                            (transition.elapsed + delta_seconds).min(transition.duration);
                    }
                    if transition.elapsed >= transition.duration {
                        self.transition = None;
                    }
                }
                apply_animation(self, &player);
            }
            self.animation = Some(player);
        }
    }
});

fn apply_animation<T: RigExt + ?Sized>(rig: &mut T, player: &AnimationPlayer) {
    let pivot = rig.pivot();
    let transition = rig.rig().transition.clone();
    let mut path = Vec::new();
    for child in rig.as_instance_mut().children_mut() {
        apply_tracks(child, pivot, &mut path, player, transition.as_ref());
    }
    #[cfg(feature = "physics")]
    physics::capture_targets(rig);
    #[cfg(feature = "meshpart")]
    gltf::update_skins(rig);
}

#[cfg(all(feature = "physics", feature = "meshpart"))]
pub(crate) fn update_physics_skins(instance: &mut dyn Instance, mut articulated: bool) {
    if let Some(rig) = instance.view_mut::<Rig>(instance.id()) {
        articulated |= !rig.physics_joints().is_empty();
        if articulated {
            gltf::update_skins(rig);
        }
    }
    for child in instance.children_mut() {
        update_physics_skins(child, articulated);
    }
}

fn apply_tracks(
    instance: &mut dyn Instance,
    parent_pivot: Mat4,
    path: &mut Vec<String>,
    player: &AnimationPlayer,
    transition: Option<&AnimationTransition>,
) {
    if instance
        .spatial()
        .is_some_and(|spatial| !spatial.world_pivot().inverse().is_finite())
    {
        return;
    }
    path.push(instance.name().to_owned());
    if let Some(track) = player
        .clip()
        .tracks()
        .iter()
        .find(|track| track.target() == path)
        && let Some(spatial) = instance.spatial_mut()
    {
        let target = track.sample(player.time());
        let local = transition
            .and_then(|fade| {
                fade.poses.get(path).map(|from| {
                    crate::interpolation::interpolate_transform(
                        *from,
                        target,
                        fade.elapsed / fade.duration,
                    )
                })
            })
            .unwrap_or(target);
        let pivot = parent_pivot * local;
        if pivot.inverse().is_finite() {
            spatial.move_to_world_pivot(pivot);
        }
    }
    let pivot = instance
        .spatial()
        .map(|spatial| spatial.world_pivot())
        .unwrap_or(parent_pivot);
    for child in instance.children_mut() {
        apply_tracks(child, pivot, path, player, transition);
    }
    path.pop();
}

#[cfg(feature = "devtools")]
impl devtools::Inspect for Rig {
    fn properties(&self) -> Vec<devtools::Property> {
        let mut properties = self.instance.properties();
        properties.extend(self.pivot().properties());
        properties.extend(
            self.anchored
                .properties()
                .into_iter()
                .map(|property| property.prefixed("anchored")),
        );
        properties
    }

    fn validate_property(
        &self,
        path: &str,
        value: &devtools::PropertyValue,
    ) -> Result<(), devtools::PropertyError> {
        if matches!(path, "position" | "orientation" | "size") {
            let mut pivot = self.pivot();
            pivot.set_property(path, value.clone())?;
            if !pivot.inverse().is_finite() {
                return Err(devtools::PropertyError::InvalidValue);
            }
            return Ok(());
        }
        if path == "anchored" {
            return self.anchored.validate_property("", value);
        }
        self.instance.validate_property(path, value)
    }

    fn set_property(
        &mut self,
        path: &str,
        value: devtools::PropertyValue,
    ) -> Result<(), devtools::PropertyError> {
        <Self as devtools::Inspect>::validate_property(self, path, &value)?;
        if matches!(path, "position" | "orientation" | "size") {
            let mut pivot = self.pivot();
            pivot.set_property(path, value)?;
            self.set_pivot(pivot);
            return Ok(());
        }
        if path == "anchored" {
            return self.anchored.set_property("", value);
        }
        self.instance.set_property(path, value)
    }
}
