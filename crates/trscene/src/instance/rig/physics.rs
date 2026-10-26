use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{glam::*, *};

/// How a simulated rig joint responds to its rest or animated pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RigJointMotion {
    /// Freely rotates under gravity, contacts, and impulses, subject to angular limits.
    Ragdoll,
    /// Uses angular springs to follow the rest pose or current animation.
    Jiggle {
        /// Angular spring stiffness; must be finite and nonnegative.
        stiffness: f32,
        /// Angular spring damping; must be finite and nonnegative.
        damping: f32,
    },
}

/// A spherical physics joint targeting a named spatial subtree within a rig.
#[derive(Clone, Debug, PartialEq)]
pub struct RigJoint {
    target: Vec<String>,
    /// The joint's response to animation and external forces.
    pub motion: RigJointMotion,
    /// Attachment point in the target's local space, including its scale.
    pub anchor: Vec3,
    /// Optional symmetric XYZ angular limits in radians, each in `0..=PI`.
    ///
    /// Limits are relative to the bound rest pose, or the animated pose for jiggle.
    pub angular_limits: Option<Vec3>,
}

impl RigJoint {
    /// Creates a ragdoll joint at the target's origin without angular limits.
    pub fn new(target: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            target: target.into_iter().map(Into::into).collect(),
            motion: RigJointMotion::Ragdoll,
            anchor: Vec3::ZERO,
            angular_limits: None,
        }
    }

    /// Sets the joint's response to animation and external forces.
    pub fn with_motion(mut self, motion: RigJointMotion) -> Self {
        self.motion = motion;
        self
    }

    /// Sets the attachment point in the target's local space.
    pub fn with_anchor(mut self, anchor: Vec3) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets symmetric XYZ angular limits in radians.
    pub fn with_angular_limits(mut self, limits: Vec3) -> Self {
        self.angular_limits = Some(limits);
        self
    }

    /// Returns the target path relative to the owning rig.
    pub fn target(&self) -> &[String] {
        &self.target
    }
}

/// A rejected rig physics configuration.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum RigPhysicsError {
    /// A path is empty, missing, ambiguous, or does not resolve to a spatial node.
    #[error("invalid rig joint target: {0:?}")]
    InvalidTarget(Vec<String>),
    /// Two joints target the same node.
    #[error("duplicate rig joint target: {0:?}")]
    DuplicateTarget(Vec<String>),
    /// A target or ancestor has a nonfinite, singular, or sheared transform.
    #[error("invalid transform for rig joint: {0:?}")]
    InvalidTransform(Vec<String>),
    /// An anchor, angular limit, or spring parameter is invalid.
    #[error("invalid parameters for rig joint: {0:?}")]
    InvalidParameters(Vec<String>),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RigPhysics {
    pub(crate) joints: Vec<RigJoint>,
    pub(crate) bindings: BTreeMap<Vec<String>, JointBinding>,
}

#[derive(Clone, Debug)]
pub(crate) struct JointBinding {
    pub(crate) parent: Vec<String>,
    pub(crate) frame1: Mat4,
    pub(crate) frame2: Mat4,
    pub(crate) joint: RigJoint,
    // Animation targets must not absorb the previous frame's simulated displacement.
    local_target: Mat4,
}

pub(super) fn configure<T: RigExt + ?Sized>(
    rig: &mut T,
    joints: Vec<RigJoint>,
) -> Result<(), RigPhysicsError> {
    let mut paths = BTreeSet::new();
    for joint in &joints {
        if !paths.insert(joint.target.clone()) {
            return Err(RigPhysicsError::DuplicateTarget(joint.target.clone()));
        }
        if !joint.anchor.is_finite()
            || joint.angular_limits.is_some_and(|limits| {
                !limits.is_finite()
                    || limits.min_element() < 0.0
                    || limits.max_element() > std::f32::consts::PI
            })
            || matches!(joint.motion, RigJointMotion::Jiggle { stiffness, damping }
                if !stiffness.is_finite() || stiffness < 0.0 || !damping.is_finite() || damping < 0.0)
        {
            return Err(RigPhysicsError::InvalidParameters(joint.target.clone()));
        }
    }
    let root = rig.as_instance();
    let mut bindings = BTreeMap::new();
    for joint in &joints {
        if joint.target.is_empty() {
            return Err(RigPhysicsError::InvalidTarget(joint.target.clone()));
        }
        let (target, parent_pivot) = resolve(root, &joint.target)
            .ok_or_else(|| RigPhysicsError::InvalidTarget(joint.target.clone()))?;
        let pivot = target
            .spatial()
            .ok_or_else(|| RigPhysicsError::InvalidTarget(joint.target.clone()))?
            .world_pivot();
        let parent = (1..joint.target.len())
            .rev()
            .find_map(|len| {
                paths
                    .contains(&joint.target[..len])
                    .then(|| joint.target[..len].to_vec())
            })
            .unwrap_or_default();
        let parent_body = resolve(root, &parent)
            .and_then(|(node, _)| node.spatial())
            .ok_or_else(|| RigPhysicsError::InvalidTarget(joint.target.clone()))?;
        let local_target = parent_pivot.inverse() * pivot;
        if !crate::interpolation::valid_transform(pivot)
            || !crate::interpolation::valid_transform(parent_pivot)
            || !crate::interpolation::valid_transform(parent_body.world_pivot())
            || !crate::interpolation::valid_transform(local_target)
        {
            return Err(RigPhysicsError::InvalidTransform(joint.target.clone()));
        }
        let anchor_frame = Mat4::from_rotation_translation(
            pivot.to_scale_rotation_translation().1,
            pivot.transform_point3(joint.anchor),
        );
        let frame1 = parent_body.rigid_pose().inverse() * anchor_frame;
        let frame2 = PVInstance::pose_from_transform(pivot).inverse() * anchor_frame;
        if !frame1.inverse().is_finite() || !frame2.inverse().is_finite() {
            return Err(RigPhysicsError::InvalidTransform(joint.target.clone()));
        }
        bindings.insert(
            joint.target.clone(),
            JointBinding {
                parent,
                frame1,
                frame2,
                joint: joint.clone(),
                local_target,
            },
        );
    }
    rig.rig_mut().joint_physics = RigPhysics { joints, bindings };
    Ok(())
}

pub(crate) fn resolve<'a>(
    root: &'a dyn Instance,
    path: &[String],
) -> Option<(&'a dyn Instance, Mat4)> {
    let mut node = root;
    let mut parent_pivot = root.spatial()?.world_pivot();
    for name in path {
        if let Some(spatial) = node.spatial() {
            parent_pivot = spatial.world_pivot();
            if !parent_pivot.inverse().is_finite() {
                return None;
            }
        }
        let mut matches = node.children().iter().filter(|child| child.name() == name);
        node = matches.next()?.as_ref();
        if matches.next().is_some() {
            return None;
        }
    }
    Some((node, parent_pivot))
}

pub(super) fn restore_targets<T: RigExt + ?Sized>(rig: &mut T) {
    fn restore(node: &mut dyn Instance, parent: Mat4, targets: &HashMap<InstanceId, Mat4>) {
        if node
            .spatial()
            .is_some_and(|spatial| !spatial.world_pivot().inverse().is_finite())
        {
            return;
        }
        if let Some(local) = targets.get(&node.id())
            && let Some(spatial) = node.spatial_mut()
        {
            let pivot = parent * *local;
            if pivot.inverse().is_finite() {
                spatial.move_to_world_pivot(pivot);
            }
        }
        let parent = node
            .spatial()
            .map_or(parent, |spatial| spatial.world_pivot());
        for child in node.children_mut() {
            restore(child, parent, targets);
        }
    }
    if rig.rig().joint_physics.bindings.is_empty() {
        return;
    }
    let targets = rig
        .rig()
        .joint_physics
        .bindings
        .iter()
        .filter_map(|(path, binding)| {
            let (target, _) = resolve(rig.as_instance(), path)?;
            Some((target.id(), binding.local_target))
        })
        .collect();
    let pivot = rig.pivot();
    restore(rig.as_instance_mut(), pivot, &targets);
}

pub(super) fn capture_targets<T: RigExt + ?Sized>(rig: &mut T) {
    let mut physics = std::mem::take(&mut rig.rig_mut().joint_physics);
    for (path, binding) in &mut physics.bindings {
        let Some((target, parent_pivot)) = resolve(rig.as_instance(), path) else {
            continue;
        };
        let Some(spatial) = target.spatial() else {
            continue;
        };
        let pivot = spatial.world_pivot();
        let local = parent_pivot.inverse() * pivot;
        if !crate::interpolation::valid_transform(local) {
            continue;
        }
        binding.local_target = local;
        if matches!(binding.joint.motion, RigJointMotion::Jiggle { .. })
            && let Some(parent) =
                resolve(rig.as_instance(), &binding.parent).and_then(|(node, _)| node.spatial())
        {
            binding.frame1 = parent.rigid_pose().inverse() * spatial.rigid_pose() * binding.frame2;
        }
    }
    rig.rig_mut().joint_physics = physics;
}
