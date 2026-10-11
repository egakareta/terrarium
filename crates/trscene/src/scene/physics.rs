use std::{
    collections::{BTreeSet, HashMap},
    fmt,
};

use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, IntegrationParameters, PhysicsWorld as RapierPhysicsWorld,
    QueryFilter, RigidBody, RigidBodyBuilder, RigidBodyHandle, RigidBodyType,
};

#[cfg(feature = "meshpart")]
use crate::MeshPart;
use crate::{
    BasePart, BoxOverlapQuery, HasBasePart, HasPVInstance, HasPart, Instance, InstanceId,
    PVInstance, Part, PartShape,
    glam::{Mat4, Quat, Vec3},
};

/// A Rapier-backed physics simulation synchronized with workspace parts.
///
/// [`Workspace`](crate::Workspace) creates one automatically when the `physics` feature is
/// enabled. Parts with [`HasBasePart::anchored`](crate::HasBasePart::anchored) set to `false`
/// are simulated as dynamic rigid bodies; anchored parts are fixed rigid bodies.
/// Use [`Self::rapier_mut`] for forces, impulses, joints, and other advanced Rapier operations.
pub struct PhysicsWorld {
    world: RapierPhysicsWorld,
    bodies: HashMap<InstanceId, BodyEntry>,
    collider_instances: HashMap<ColliderHandle, InstanceId>,
    last_transforms: HashMap<InstanceId, Mat4>,
    touching: BTreeSet<(InstanceId, InstanceId)>,
}

#[derive(Default)]
pub(crate) struct PhysicsStep {
    pub(crate) transforms: Vec<(InstanceId, Mat4)>,
    pub(crate) touch_started: Vec<(InstanceId, InstanceId)>,
    pub(crate) touch_ended: Vec<(InstanceId, InstanceId)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BodyDescriptor {
    anchored: bool,
    can_collide: bool,
    shape: PartShape,
}

#[derive(Clone, Copy, Debug)]
struct BodyEntry {
    handle: RigidBodyHandle,
    collider: Option<ColliderHandle>,
    descriptor: BodyDescriptor,
    transform: Mat4,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PhysicsInstance {
    id: InstanceId,
    transform: Mat4,
    descriptor: BodyDescriptor,
}

impl PhysicsWorld {
    /// Creates an empty physics world with Rapier's default gravity and integration parameters.
    pub fn new() -> Self {
        Self {
            world: RapierPhysicsWorld::new(),
            bodies: HashMap::new(),
            collider_instances: HashMap::new(),
            last_transforms: HashMap::new(),
            touching: BTreeSet::new(),
        }
    }

    /// Returns the gravity applied to dynamic bodies, in world units per second squared.
    pub fn gravity(&self) -> Vec3 {
        from_rapier_vector(self.world.gravity)
    }

    /// Sets the gravity applied to dynamic bodies, in world units per second squared.
    ///
    pub fn with_gravity(&mut self, gravity: Vec3) -> &mut Self {
        self.world.gravity = rapier_vector(gravity);
        self
    }

    /// Returns Rapier's integration parameters.
    pub fn integration_parameters(&self) -> &IntegrationParameters {
        &self.world.integration_parameters
    }

    /// Returns mutable access to Rapier's integration parameters.
    pub fn integration_parameters_mut(&mut self) -> &mut IntegrationParameters {
        &mut self.world.integration_parameters
    }

    /// Returns the Rapier world used by this physics integration.
    pub fn rapier(&self) -> &RapierPhysicsWorld {
        &self.world
    }

    /// Returns mutable access to the Rapier world used by this physics integration.
    pub fn rapier_mut(&mut self) -> &mut RapierPhysicsWorld {
        &mut self.world
    }

    /// Returns the Rapier body handle associated with a workspace instance.
    pub fn body_handle(&self, instance: InstanceId) -> Option<RigidBodyHandle> {
        self.bodies.get(&instance).map(|entry| entry.handle)
    }

    /// Returns the Rapier body associated with a workspace instance.
    pub fn body(&self, instance: InstanceId) -> Option<&RigidBody> {
        let handle = self.body_handle(instance)?;
        self.world.bodies.get(handle)
    }

    /// Returns mutable access to the Rapier body associated with a workspace instance.
    pub fn body_mut(&mut self, instance: InstanceId) -> Option<&mut RigidBody> {
        let handle = self.body_handle(instance)?;
        self.world.bodies.get_mut(handle)
    }

    pub(crate) fn get_part_bounds_in_box(&self, query: &BoxOverlapQuery) -> Vec<InstanceId> {
        if !query
            .transform
            .to_cols_array()
            .into_iter()
            .all(f32::is_finite)
        {
            return Vec::new();
        }

        let size = query.transform.to_scale_rotation_translation().0;
        if !size.is_finite() {
            return Vec::new();
        }
        let half_size = (size.abs() * 0.5).max(Vec3::splat(0.001));
        let query_shape = ColliderBuilder::cuboid(half_size.x, half_size.y, half_size.z).build();
        let query_pose = rapier_pose(query.transform);
        let mut results = Vec::new();
        for (collider, _) in
            self.world
                .intersect_shape(query_pose, query_shape.shape(), QueryFilter::default())
        {
            let Some(instance) = self.collider_instances.get(&collider).copied() else {
                continue;
            };
            if query.excluded_instances.contains(&instance) {
                continue;
            }
            results.push(instance);
            if query.max_parts != 0 && results.len() >= query.max_parts {
                break;
            }
        }
        results
    }

    pub(crate) fn step(
        &mut self,
        instances: &[PhysicsInstance],
        delta_seconds: f32,
    ) -> PhysicsStep {
        self.sync_instances(instances);

        let mut result = PhysicsStep::default();
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.world.integration_parameters.dt = delta_seconds.min(0.1);
            self.world.step();
            let touching: BTreeSet<_> = self
                .world
                .contact_pairs()
                .filter(|pair| pair.has_any_active_contact())
                .filter_map(|pair| {
                    let first = *self.collider_instances.get(&pair.collider1)?;
                    let second = *self.collider_instances.get(&pair.collider2)?;
                    Some((first.min(second), first.max(second)))
                })
                .collect();
            result
                .touch_ended
                .extend(self.touching.difference(&touching));
            result
                .touch_started
                .extend(touching.difference(&self.touching));
            self.touching = touching;
        }

        for instance in instances {
            let Some(entry) = self.bodies.get(&instance.id) else {
                continue;
            };
            if instance.descriptor.anchored {
                continue;
            }
            let Some(body) = self.world.bodies.get(entry.handle) else {
                continue;
            };
            let transform = body_transform(body);
            self.last_transforms.insert(instance.id, transform);
            result.transforms.push((instance.id, transform));
        }
        result
    }

    pub(crate) fn clone_configuration(&self) -> Self {
        let mut clone = Self::new();
        clone.world.gravity = self.world.gravity;
        clone.world.integration_parameters = self.world.integration_parameters;
        clone
    }

    fn sync_instances(&mut self, instances: &[PhysicsInstance]) {
        let active: HashMap<_, _> = instances.iter().map(|instance| (instance.id, ())).collect();
        let stale: Vec<_> = self
            .bodies
            .keys()
            .copied()
            .filter(|id| !active.contains_key(id))
            .collect();
        for id in stale {
            if let Some(entry) = self.bodies.remove(&id) {
                self.world.remove_body(entry.handle);
                if let Some(collider) = entry.collider {
                    self.collider_instances.remove(&collider);
                }
            }
            self.last_transforms.remove(&id);
        }

        for instance in instances {
            let existing = self.bodies.get(&instance.id).copied();
            if let Some(entry) =
                existing.filter(|entry| self.world.bodies.get(entry.handle).is_some())
            {
                self.update_instance(instance, entry);
            } else {
                if let Some(entry) = self.bodies.remove(&instance.id)
                    && let Some(collider) = entry.collider
                {
                    self.collider_instances.remove(&collider);
                }
                self.last_transforms.remove(&instance.id);
                self.insert_instance(instance);
            }
        }
    }

    fn insert_instance(&mut self, instance: &PhysicsInstance) {
        let builder = body_builder(instance);
        let handle = self.world.insert_body(builder);
        let collider = if instance.descriptor.can_collide {
            let collider = self.world.insert_collider(
                collider_builder(instance.descriptor, instance.transform),
                Some(handle),
            );
            self.collider_instances.insert(collider, instance.id);
            Some(collider)
        } else {
            None
        };
        self.bodies.insert(
            instance.id,
            BodyEntry {
                handle,
                collider,
                descriptor: instance.descriptor,
                transform: instance.transform,
            },
        );
        self.last_transforms.insert(
            instance.id,
            PVInstance::pose_from_transform(instance.transform),
        );
    }

    fn update_instance(&mut self, instance: &PhysicsInstance, mut entry: BodyEntry) {
        let Some(body) = self.world.bodies.get_mut(entry.handle) else {
            return;
        };
        let pose = PVInstance::pose_from_transform(instance.transform);

        if entry.descriptor.anchored != instance.descriptor.anchored {
            body.set_body_type(body_type(instance.descriptor.anchored), true);
        }

        if instance.descriptor.anchored {
            body.set_position(rapier_pose(pose), true);
            self.last_transforms.insert(instance.id, pose);
        } else if self
            .last_transforms
            .get(&instance.id)
            .is_none_or(|last| !matrices_approximately_equal(*last, pose))
        {
            body.set_position(rapier_pose(pose), true);
            body.set_linvel(rapier_vector(Vec3::ZERO), true);
            body.set_angvel(rapier_vector(Vec3::ZERO), true);
        }

        let current_size = instance.transform.to_scale_rotation_translation().0;
        let previous_size = entry.transform.to_scale_rotation_translation().0;
        let collider_changed = entry.descriptor.can_collide != instance.descriptor.can_collide
            || !vectors_approximately_equal(previous_size, current_size)
            || entry.descriptor.shape != instance.descriptor.shape;
        if collider_changed {
            if let Some(collider) = entry.collider.take() {
                self.world.remove_collider(collider);
                self.collider_instances.remove(&collider);
            }
            if instance.descriptor.can_collide {
                let collider = self.world.insert_collider(
                    collider_builder(instance.descriptor, instance.transform),
                    Some(entry.handle),
                );
                self.collider_instances.insert(collider, instance.id);
                entry.collider = Some(collider);
            }
        } else if instance.descriptor.can_collide
            && entry
                .collider
                .is_none_or(|collider| self.world.colliders.get(collider).is_none())
        {
            if let Some(collider) = entry.collider.take() {
                self.collider_instances.remove(&collider);
            }
            let collider = self.world.insert_collider(
                collider_builder(instance.descriptor, instance.transform),
                Some(entry.handle),
            );
            self.collider_instances.insert(collider, instance.id);
            entry.collider = Some(collider);
        }
        entry.descriptor = instance.descriptor;
        entry.transform = instance.transform;
        self.bodies.insert(instance.id, entry);
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for PhysicsWorld {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PhysicsWorld")
            .field("gravity", &self.gravity())
            .field("body_count", &self.world.bodies.len())
            .field("collider_count", &self.world.colliders.len())
            .finish()
    }
}

impl PhysicsInstance {
    pub(crate) fn from_instance(instance: &dyn Instance) -> Option<Self> {
        if let Some(part) = instance.downcast_ref::<Part>() {
            return Some(Self::from_traits(part.id(), part, part.shape()));
        }
        #[cfg(feature = "meshpart")]
        if let Some(part) = instance.downcast_ref::<MeshPart>() {
            return Some(Self::from_traits(part.id(), part, PartShape::Block));
        }
        instance
            .downcast_ref::<BasePart>()
            .map(|part| Self::from_traits(part.id(), part, PartShape::Block))
    }

    fn from_traits<T: HasBasePart + HasPVInstance>(
        id: InstanceId,
        part: &T,
        shape: PartShape,
    ) -> Self {
        Self {
            id,
            transform: part.pivot(),
            descriptor: BodyDescriptor {
                anchored: part.anchored(),
                can_collide: part.can_collide(),
                shape,
            },
        }
    }
}

pub(crate) fn apply_transform(instance: &mut dyn Instance, transform: Mat4) {
    if let Some(part) = instance.downcast_mut::<Part>() {
        part.with_pose(transform);
        return;
    }
    #[cfg(feature = "meshpart")]
    if let Some(part) = instance.downcast_mut::<MeshPart>() {
        part.with_pose(transform);
        return;
    }
    if let Some(part) = instance.downcast_mut::<BasePart>() {
        part.with_pose(transform);
    }
}

fn body_builder(instance: &PhysicsInstance) -> RigidBodyBuilder {
    let mut builder = if instance.descriptor.anchored {
        RigidBodyBuilder::fixed()
    } else {
        RigidBodyBuilder::dynamic()
    };
    let (_, rotation, translation) = instance.transform.to_scale_rotation_translation();
    builder = builder
        .translation(rapier_vector(translation))
        .rotation(rapier_vector(rotation.to_scaled_axis()));
    builder
}

fn body_type(anchored: bool) -> RigidBodyType {
    if anchored {
        RigidBodyType::Fixed
    } else {
        RigidBodyType::Dynamic
    }
}

fn collider_builder(
    descriptor: BodyDescriptor,
    transform: Mat4,
) -> rapier3d::prelude::ColliderBuilder {
    let half_size =
        (transform.to_scale_rotation_translation().0.abs() * 0.5).max(Vec3::splat(0.001));
    match descriptor.shape {
        PartShape::Ball => ColliderBuilder::ball(half_size.max_element()),
        PartShape::Cylinder => ColliderBuilder::cylinder(half_size.y, half_size.x.max(half_size.z)),
        PartShape::Block | PartShape::Wedge | PartShape::CornerWedge => {
            ColliderBuilder::cuboid(half_size.x, half_size.y, half_size.z)
        }
    }
}

fn rapier_pose(transform: Mat4) -> rapier3d::math::Pose {
    let (_, rotation, translation) = transform.to_scale_rotation_translation();
    rapier3d::math::Pose::new(
        rapier_vector(translation),
        rapier_vector(rotation.to_scaled_axis()),
    )
}

fn body_transform(body: &RigidBody) -> Mat4 {
    let translation = body.translation();
    Mat4::from_rotation_translation(
        Quat::from_xyzw(
            body.rotation().x,
            body.rotation().y,
            body.rotation().z,
            body.rotation().w,
        ),
        Vec3::new(translation.x, translation.y, translation.z),
    )
}

fn rapier_vector(value: Vec3) -> rapier3d::math::Vector {
    rapier3d::math::Vector::new(value.x, value.y, value.z)
}

fn from_rapier_vector(value: rapier3d::math::Vector) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

fn matrices_approximately_equal(left: Mat4, right: Mat4) -> bool {
    left.to_cols_array()
        .into_iter()
        .zip(right.to_cols_array())
        .all(|(left, right)| (left - right).abs() <= 0.0001)
}

fn vectors_approximately_equal(left: Vec3, right: Vec3) -> bool {
    left.to_array()
        .into_iter()
        .zip(right.to_array())
        .all(|(left, right)| {
            left.is_finite() && right.is_finite() && (left - right).abs() <= 0.0001
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoxOverlapQuery, HasPVInstance, HasPart, Workspace};

    #[test]
    fn dynamic_parts_fall_and_rest_on_anchored_parts() {
        let mut workspace = Workspace::new();

        let floor = Part::new()
            .with_size(Vec3::new(10.0, 1.0, 10.0))
            .with_position(Vec3::new(0.0, -0.5, 0.0));
        floor.set_parent(&mut workspace);

        let ball = Part::new()
            .with_shape(PartShape::Ball)
            .with_anchored(false)
            .with_position(Vec3::new(3.5, 3.0, 0.0));
        let ball_id = ball.set_parent(&mut workspace);

        for _ in 0..120 {
            workspace.update(1.0 / 60.0);
        }

        let ball = workspace
            .get::<Part>(ball_id)
            .expect("ball remains in workspace");
        assert!(ball.position().y < 3.0);
        assert!(ball.position().y > 0.35);
        assert!(ball.position().x > 3.0 && ball.position().x < 4.0);
        assert!(workspace.physics().body_handle(ball_id).is_some());
    }

    #[test]
    fn workspace_box_overlap_query_uses_rapier_colliders_and_filters_instances() {
        let mut workspace = Workspace::new();
        let fixed_id = Part::new()
            .with_position(Vec3::new(1.5, 0.0, 0.0))
            .set_parent(&mut workspace);
        let dynamic_id = Part::new()
            .with_anchored(false)
            .with_position(Vec3::new(-1.5, 0.0, 0.0))
            .set_parent(&mut workspace);
        let non_collidable_id = Part::new()
            .with_can_collide(false)
            .set_parent(&mut workspace);
        Part::new()
            .with_position(Vec3::new(5.0, 0.0, 0.0))
            .set_parent(&mut workspace);

        workspace.update(1.0 / 60.0);

        let params = BoxOverlapQuery::new(Mat4::from_scale(Vec3::splat(4.0)));
        let overlaps = workspace.get_part_bounds_in_box(&params);
        assert_eq!(overlaps.len(), 2);
        assert!(overlaps.contains(&fixed_id));
        assert!(overlaps.contains(&dynamic_id));
        assert!(!overlaps.contains(&non_collidable_id));

        let filtered_params = params.with_excluded_instances([fixed_id]);
        assert_eq!(
            workspace.get_part_bounds_in_box(&filtered_params),
            vec![dynamic_id]
        );

        let limited_params =
            BoxOverlapQuery::new(Mat4::from_scale(Vec3::splat(4.0))).with_max_parts(1);
        assert_eq!(workspace.get_part_bounds_in_box(&limited_params).len(), 1);
    }
}
