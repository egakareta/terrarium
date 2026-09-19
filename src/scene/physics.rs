use std::{collections::HashMap, fmt};

use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, IntegrationParameters, PhysicsWorld as RapierPhysicsWorld,
    RigidBody, RigidBodyBuilder, RigidBodyHandle, RigidBodyType,
};

#[cfg(feature = "meshpart")]
use crate::MeshPart;
use crate::{
    BasePart, Instance, InstanceId, Part, PartShape,
    glam::{Mat4, Quat, Vec3},
};

/// A Rapier-backed physics simulation synchronized with workspace parts.
///
/// [`Workspace`](crate::Workspace) creates one automatically when the `physics` feature is
/// enabled. Parts with [`BasePart::anchored`] set to `false` are simulated as dynamic rigid
/// bodies; anchored parts are fixed rigid bodies. Use [`Self::rapier_mut`] for forces, impulses,
/// joints, and other advanced Rapier operations.
pub struct PhysicsWorld {
    world: RapierPhysicsWorld,
    bodies: HashMap<InstanceId, BodyEntry>,
    last_transforms: HashMap<InstanceId, Mat4>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BodyDescriptor {
    anchored: bool,
    can_collide: bool,
    size: Vec3,
    shape: PartShape,
}

#[derive(Clone, Copy, Debug)]
struct BodyEntry {
    handle: RigidBodyHandle,
    collider: Option<ColliderHandle>,
    descriptor: BodyDescriptor,
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
            last_transforms: HashMap::new(),
        }
    }

    /// Returns the gravity applied to dynamic bodies, in world units per second squared.
    pub fn gravity(&self) -> Vec3 {
        from_rapier_vector(self.world.gravity)
    }

    /// Sets the gravity applied to dynamic bodies, in world units per second squared.
    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.world.gravity = rapier_vector(gravity);
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

    pub(crate) fn step(
        &mut self,
        instances: &[PhysicsInstance],
        delta_seconds: f32,
    ) -> Vec<(InstanceId, Mat4)> {
        self.sync_instances(instances);

        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.world.integration_parameters.dt = delta_seconds.min(0.1);
            self.world.step();
        }

        let mut transforms = Vec::new();
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
            transforms.push((instance.id, transform));
        }
        transforms
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
                self.bodies.remove(&instance.id);
                self.last_transforms.remove(&instance.id);
                self.insert_instance(instance);
            }
        }
    }

    fn insert_instance(&mut self, instance: &PhysicsInstance) {
        let builder = body_builder(instance);
        let handle = self.world.insert_body(builder);
        let collider = if instance.descriptor.can_collide {
            Some(
                self.world
                    .insert_collider(collider_builder(instance.descriptor), Some(handle)),
            )
        } else {
            None
        };
        self.bodies.insert(
            instance.id,
            BodyEntry {
                handle,
                collider,
                descriptor: instance.descriptor,
            },
        );
        self.last_transforms.insert(instance.id, instance.transform);
    }

    fn update_instance(&mut self, instance: &PhysicsInstance, mut entry: BodyEntry) {
        let Some(body) = self.world.bodies.get_mut(entry.handle) else {
            return;
        };

        if entry.descriptor.anchored != instance.descriptor.anchored {
            body.set_body_type(body_type(instance.descriptor.anchored), true);
        }

        if instance.descriptor.anchored {
            body.set_position(rapier_pose(instance.transform), true);
            self.last_transforms.insert(instance.id, instance.transform);
        } else if self
            .last_transforms
            .get(&instance.id)
            .is_none_or(|last| !matrices_approximately_equal(*last, instance.transform))
        {
            body.set_position(rapier_pose(instance.transform), true);
            body.set_linvel(rapier_vector(Vec3::ZERO), true);
            body.set_angvel(rapier_vector(Vec3::ZERO), true);
        }

        let collider_changed = entry.descriptor.can_collide != instance.descriptor.can_collide
            || entry.descriptor.size != instance.descriptor.size
            || entry.descriptor.shape != instance.descriptor.shape;
        if collider_changed {
            if let Some(collider) = entry.collider.take() {
                self.world.remove_collider(collider);
            }
            if instance.descriptor.can_collide {
                entry.collider = Some(
                    self.world
                        .insert_collider(collider_builder(instance.descriptor), Some(entry.handle)),
                );
            }
        } else if instance.descriptor.can_collide
            && entry
                .collider
                .is_none_or(|collider| self.world.colliders.get(collider).is_none())
        {
            entry.collider = Some(
                self.world
                    .insert_collider(collider_builder(instance.descriptor), Some(entry.handle)),
            );
        }
        entry.descriptor = instance.descriptor;
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
            return Some(Self::from_base_part(part.id(), part, part.shape));
        }
        #[cfg(feature = "meshpart")]
        if let Some(part) = instance.downcast_ref::<MeshPart>() {
            return Some(Self::from_base_part(part.id(), part, PartShape::Block));
        }
        instance
            .downcast_ref::<BasePart>()
            .map(|part| Self::from_base_part(part.id(), part, PartShape::Block))
    }

    fn from_base_part(id: InstanceId, part: &BasePart, shape: PartShape) -> Self {
        Self {
            id,
            transform: part.pivot(),
            descriptor: BodyDescriptor {
                anchored: part.anchored,
                can_collide: part.can_collide,
                size: part.size,
                shape,
            },
        }
    }
}

pub(crate) fn apply_transform(instance: &mut dyn Instance, transform: Mat4) {
    if let Some(part) = instance.downcast_mut::<Part>() {
        part.pivot_to(transform);
        return;
    }
    #[cfg(feature = "meshpart")]
    if let Some(part) = instance.downcast_mut::<MeshPart>() {
        part.pivot_to(transform);
        return;
    }
    if let Some(part) = instance.downcast_mut::<BasePart>() {
        part.pivot_to(transform);
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

fn collider_builder(descriptor: BodyDescriptor) -> rapier3d::prelude::ColliderBuilder {
    let half_size = (descriptor.size.abs() * 0.5).max(Vec3::splat(0.001));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;

    #[test]
    fn dynamic_parts_fall_and_rest_on_anchored_parts() {
        let mut workspace = Workspace::new();

        let mut floor = Part::new();
        floor.size = Vec3::new(10.0, 1.0, 10.0);
        floor.set_position(Vec3::new(0.0, -0.5, 0.0));
        floor.set_parent(&mut workspace);

        let mut ball = Part::new();
        ball.shape = PartShape::Ball;
        ball.anchored = false;
        ball.set_position(Vec3::new(0.0, 3.0, 0.0));
        let ball_id = ball.set_parent(&mut workspace);

        for _ in 0..120 {
            workspace.update(1.0 / 60.0);
        }

        let ball = workspace
            .get::<Part>(ball_id)
            .expect("ball remains in workspace");
        assert!(ball.position().y < 3.0);
        assert!(ball.position().y > 0.35);
        assert!(workspace.physics().body_handle(ball_id).is_some());
    }
}
