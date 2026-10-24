use crate::{glam::*, *};

instance! {
    #[instance(inspect = false)]
    /// A spatial assembly whose descendants move together when its pivot changes.
    ///
    /// Children retain world-space pivots, as elsewhere in the scene tree. All
    /// [`PVInstanceExt`] setters on a rig carry its spatial descendants with it.
    /// With physics enabled, the outermost rig owns a single body with colliders
    /// from its descendant parts. The rig's anchored state controls that body;
    /// descendant parts retain their individual collision flags and shapes.
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

        /// Starts an animation and immediately applies its current pose.
        ///
        /// Every target path must resolve unambiguously to a built-in spatial
        /// descendant. A failed validation leaves the current animation and pose
        /// untouched. Removed targets and subtrees with singular pivots are skipped
        /// during playback.
        pub fn play_animation(&mut self, player: AnimationPlayer) -> Result<(), AnimationError> {
            for track in player.clip().tracks() {
                let mut instance: &dyn Instance = self.as_instance();
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
                    instance = child.as_ref();
                }
                let Some(pivot) = instance.spatial().map(|spatial| spatial.world_pivot()) else {
                    return Err(AnimationError::TargetNotFound(track.target().to_vec()));
                };
                if !pivot.inverse().is_finite() {
                    return Err(AnimationError::InvalidTransform);
                }
            }
            apply_animation(self, &player);
            self.animation = Some(player);
            Ok(())
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
            self.animation.take()
        }

        /// Advances this rig's animation and applies poses from ancestors to descendants.
        ///
        /// [`crate::Workspace::update_animations`] advances every rig in a workspace.
        pub fn update_animation(&mut self, delta_seconds: f32) {
            let Some(mut player) = self.animation.take() else {
                return;
            };
            if !player.is_finished() {
                player.update(delta_seconds);
                apply_animation(self, &player);
            }
            self.animation = Some(player);
        }
    }
}

fn apply_animation<T: RigExt + ?Sized>(rig: &mut T, player: &AnimationPlayer) {
    let pivot = rig.pivot();
    let mut path = Vec::new();
    for child in rig.as_instance_mut().children_mut() {
        apply_tracks(child, pivot, &mut path, player);
    }
}

fn apply_tracks(
    instance: &mut dyn Instance,
    parent_pivot: Mat4,
    path: &mut Vec<String>,
    player: &AnimationPlayer,
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
        spatial.move_to_world_pivot(parent_pivot * track.sample(player.time()));
    }
    let pivot = instance
        .spatial()
        .map(|spatial| spatial.world_pivot())
        .unwrap_or(parent_pivot);
    for child in instance.children_mut() {
        apply_tracks(child, pivot, path, player);
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
