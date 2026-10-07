use crate::{
    ChildrenMut, Color3, HasBasePart, HasPVInstance, Instance, InstanceData, PVInstance, Part,
    glam::Vec3,
};

/// A block character whose six parts share a movable torso-centered pivot.
///
/// Limbs are anchored until a character controller or joint system is supplied.
/// Use [`HasPVInstance::pivot_to`] to move the complete assembly. Builder methods
/// such as `with_position` change only the root; use `pivot_to` after constructing it.
#[derive(Clone, Debug)]
pub struct Humanoid {
    instance: InstanceData,
    pv_instance: PVInstance,
    /// Application-level ID of the player owning this character.
    pub player_id: String,
}

impl Humanoid {
    /// Creates a character at the origin with Head, Torso, LeftArm, RightArm,
    /// LeftLeg, and RightLeg parts parented directly to it.
    ///
    /// The feet are three units below the pivot and the head is two units above it.
    pub fn new(player_id: impl Into<String>) -> Self {
        let mut humanoid = Self {
            instance: InstanceData::new("Humanoid"),
            pv_instance: PVInstance::new(),
            player_id: player_id.into(),
        };
        for (name, size, position, color) in [
            (
                "Head",
                Vec3::new(2.0, 1.0, 1.0),
                Vec3::new(0.0, 1.5, 0.0),
                Color3::new(1.0, 0.8, 0.5),
            ),
            (
                "Torso",
                Vec3::new(2.0, 2.0, 1.0),
                Vec3::ZERO,
                Color3::new(0.2, 0.5, 0.9),
            ),
            (
                "LeftArm",
                Vec3::new(1.0, 2.0, 1.0),
                Vec3::new(-1.5, 0.0, 0.0),
                Color3::new(1.0, 0.8, 0.5),
            ),
            (
                "RightArm",
                Vec3::new(1.0, 2.0, 1.0),
                Vec3::new(1.5, 0.0, 0.0),
                Color3::new(1.0, 0.8, 0.5),
            ),
            (
                "LeftLeg",
                Vec3::new(1.0, 2.0, 1.0),
                Vec3::new(-0.5, -2.0, 0.0),
                Color3::new(0.2, 0.3, 0.4),
            ),
            (
                "RightLeg",
                Vec3::new(1.0, 2.0, 1.0),
                Vec3::new(0.5, -2.0, 0.0),
                Color3::new(0.2, 0.3, 0.4),
            ),
        ] {
            humanoid.add_child(
                Part::new()
                    .with_name(name)
                    .with_size(size)
                    .with_position(position)
                    .with_color(color),
            );
        }
        humanoid
    }
}

crate::impl_instance!(
    Humanoid,
    class_name = "Humanoid",
    data = instance,
    pv = pv_instance,
);

impl HasPVInstance for Humanoid {
    fn pv(&self) -> &PVInstance {
        &self.pv_instance
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        &mut self.pv_instance
    }

    fn spatial_children_mut(&mut self) -> Option<ChildrenMut<'_>> {
        Some(self.children_mut())
    }
}
