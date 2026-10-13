use crate::{
    AnimationError, AnimationPlayer, HasPVInstance, Instance, InstanceData, PVInstance, glam::Mat4,
    instance_pivot, pivot_hierarchy_to, pivot_instance_to,
};

/// A spatial assembly whose descendants move together when its pivot changes.
///
/// Children retain world-space pivots, as elsewhere in the scene tree. All
/// [`HasPVInstance`] setters on a rig carry its spatial descendants with it.
/// With physics enabled, the outermost rig owns a single body with colliders
/// from its descendant parts. The rig's anchored state controls that body;
/// descendant parts retain their individual collision flags and shapes.
#[derive(Clone, Debug)]
pub struct Rig {
    instance: InstanceData,
    pv_instance: PVInstance,
    anchored: bool,
    animation: Option<AnimationPlayer>,
}

impl Rig {
    /// Creates an anchored rig at the identity transform.
    pub fn new() -> Self {
        Self {
            instance: InstanceData::new("Rig"),
            pv_instance: PVInstance::new(),
            anchored: true,
            animation: None,
        }
    }

    /// Returns whether the rig's physics body is immovable.
    pub fn anchored(&self) -> bool {
        self.anchored
    }

    /// Sets whether the rig's physics body is immovable.
    pub fn with_anchored(mut self, anchored: bool) -> Self {
        self.set_anchored(anchored);
        self
    }

    /// Sets whether the rig's physics body is immovable in place.
    pub fn set_anchored(&mut self, anchored: bool) {
        self.anchored = anchored;
    }

    /// Adds a spatial child, interpreting its pivot relative to this rig.
    ///
    /// [`Instance::add_child`] instead retains the child's existing world pivot.
    pub fn add_child_local<T: Instance + HasPVInstance>(
        &mut self,
        mut child: T,
    ) -> crate::InstanceId {
        let pivot = self.pivot() * child.pivot();
        child.set_pivot(pivot);
        self.add_child(child)
    }

    /// Starts an animation and immediately applies its current pose.
    ///
    /// Every target path must resolve unambiguously to a built-in spatial
    /// descendant. A failed validation leaves the current animation and pose
    /// untouched. Removed targets and subtrees with singular pivots are skipped
    /// during playback.
    pub fn play_animation(&mut self, player: AnimationPlayer) -> Result<(), AnimationError> {
        for track in player.clip().tracks() {
            let mut instance: &dyn Instance = self;
            for name in track.target() {
                if instance_pivot(instance).is_some_and(|pivot| !pivot.inverse().is_finite()) {
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
            let Some(pivot) = instance_pivot(instance) else {
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

fn apply_animation(rig: &mut Rig, player: &AnimationPlayer) {
    let pivot = rig.pivot();
    let mut path = Vec::new();
    for child in rig.children_mut() {
        apply_tracks(child, pivot, &mut path, player);
    }
}

fn apply_tracks(
    instance: &mut dyn Instance,
    parent_pivot: Mat4,
    path: &mut Vec<String>,
    player: &AnimationPlayer,
) {
    if instance_pivot(instance).is_some_and(|pivot| !pivot.inverse().is_finite()) {
        return;
    }
    path.push(instance.name().to_owned());
    if let Some(track) = player
        .clip()
        .tracks()
        .iter()
        .find(|track| track.target() == path)
    {
        pivot_instance_to(instance, parent_pivot * track.sample(player.time()));
    }
    let pivot = instance_pivot(instance).unwrap_or(parent_pivot);
    for child in instance.children_mut() {
        apply_tracks(child, pivot, path, player);
    }
    path.pop();
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_instance!(Rig, class_name = "Rig", data = instance, inspect = Rig);

impl HasPVInstance for Rig {
    fn pv(&self) -> &PVInstance {
        &self.pv_instance
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        &mut self.pv_instance
    }

    fn set_pivot(&mut self, pivot: Mat4) {
        self.pivot_to(pivot);
    }

    fn pivot_to(&mut self, pivot: Mat4) {
        assert!(
            pivot.inverse().is_finite(),
            "rig pivots must be finite and invertible"
        );
        pivot_hierarchy_to(self, pivot);
    }
}

#[cfg(feature = "devtools")]
impl crate::devtools::Inspect for Rig {
    fn properties(&self) -> Vec<crate::devtools::Property> {
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
        value: &crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        if matches!(path, "position" | "orientation" | "size") {
            let mut pivot = self.pivot();
            pivot.set_property(path, value.clone())?;
            if !pivot.inverse().is_finite() {
                return Err(crate::devtools::PropertyError::InvalidValue);
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
        value: crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        <Self as crate::devtools::Inspect>::validate_property(self, path, &value)?;
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
