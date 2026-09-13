use std::ops::{Deref, DerefMut};

use crate::{
    Color3, InstanceData, Material, MaterialSlot, MeshMaterialSlots, PVInstance, PartShape,
    glam::{Mat4, Vec3},
};

/// A named, transformable scene node with optional collision metadata.
///
/// `BasePart` is useful as a parent for custom instance hierarchies. A visible
/// [`Part`] builds on it with a primitive shape and material.
#[derive(Clone, Debug)]
pub struct BasePart {
    pub(crate) instance: InstanceData,
    pub(crate) pv_instance: PVInstance,
    /// World-space scale applied to the unit primitive mesh.
    pub size: Vec3,
    /// Tint.
    pub color: Color3,
    /// Whether a physics system should treat this part as immovable.
    ///
    /// Terrarium does not currently implement physics; this flag is metadata
    /// for an application-level physics integration.
    pub anchored: bool,
    /// Whether a physics system should consider this part for collisions.
    ///
    /// Terrarium does not currently implement collision queries.
    pub can_collide: bool,
}

impl BasePart {
    /// Creates a base part with identity transform, unit size, white tint, and
    /// collision metadata enabled.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            instance: InstanceData::new(name),
            pv_instance: PVInstance::new(),
            size: Vec3::ONE,
            color: Color3::WHITE,
            anchored: true,
            can_collide: true,
        }
    }
}

crate::impl_instance!(BasePart, class_name = "BasePart", data = instance,);

impl Deref for BasePart {
    type Target = PVInstance;
    fn deref(&self) -> &Self::Target {
        &self.pv_instance
    }
}
impl DerefMut for BasePart {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pv_instance
    }
}

/// Provides the full object-to-world transform, including volume.
pub trait Transform {
    /// Returns the 4x4 transform matrix for this object:
    /// ```text
    /// [s_x, 0,   0,   x]
    /// [0,   s_y, 0,   y]
    /// [0,   0,   s_z, z]
    /// [0,   0,   0,   1]
    /// ```
    ///
    /// Unlike [`PVInstance::pivot`], this includes volume.
    fn transform(&self) -> Mat4;
}

impl Transform for BasePart {
    fn transform(&self) -> Mat4 {
        self.pivot() * Mat4::from_scale(self.size)
    }
}

/// A visible scene object rendered with one of the built-in primitive shapes.
///
/// A `Part` dereferences to [`BasePart`], so its name, hierarchy, transform,
/// size, tint, and collision metadata are available directly on the value.
#[derive(Clone, Debug)]
pub struct Part {
    basepart: BasePart,
    /// Primitive geometry used when the part is rendered.
    pub shape: PartShape,
    /// Material assigned to the base material slot.
    pub material: Material,
    /// Materials assigned to the mesh's non-base slots.
    pub material_slots: MeshMaterialSlots,
}

impl Part {
    /// Creates a visible part using a unit [`PartShape::Block`] and the default
    /// material.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            basepart: BasePart::new(name),
            shape: PartShape::Block,
            material: Material::default(),
            material_slots: MeshMaterialSlots::default(),
        }
    }

    /// Assigns a material to a mesh-selected slot.
    pub fn set_material_slot(&mut self, slot: MaterialSlot, material: Material) {
        if slot == MaterialSlot::Base {
            self.material = material;
        } else {
            self.material_slots.set(slot, material);
        }
    }
}

crate::impl_instance!(Part, class_name = "Part", data = basepart.instance,);

impl Deref for Part {
    type Target = BasePart;
    fn deref(&self) -> &Self::Target {
        &self.basepart
    }
}

impl DerefMut for Part {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.basepart
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    #[test]
    fn part_position_and_orientation_accessors_use_the_pivot() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let orientation = Vec3::new(10.0, 20.0, 30.0);
        let mut part = Part::new("part");

        part.set_position(position);
        part.set_orientation(orientation);

        assert_eq!(part.position(), position);
        assert!((part.orientation() - orientation).abs().max_element() < 0.0001);
    }
}
