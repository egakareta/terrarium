use std::ops::{Deref, DerefMut};

use crate::{
    Color3, DEFAULT_MATERIAL, Face, InstanceData, Material, MeshMaterialSlots, PVInstance,
    PartShape,
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

    /// Creates a base part without assigning a display name.
    pub fn unnamed() -> Self {
        Self::new("")
    }
}

impl Default for BasePart {
    fn default() -> Self {
        Self::unnamed()
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
    pub(crate) material_slots: MeshMaterialSlots,
}

impl Part {
    /// Creates a visible part using a unit [`PartShape::Block`] and the default
    /// material.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            basepart: BasePart::new(name),
            shape: PartShape::Block,
            material_slots: MeshMaterialSlots::default(),
        }
    }

    /// Creates a visible part without assigning a display name.
    pub fn unnamed() -> Self {
        Self::new("")
    }

    /// Assigns the same material to all six directional slots.
    pub fn set_material(&mut self, material: Material) {
        for slot in Face::ALL {
            self.set_material_slot(slot, material);
        }
    }

    /// Assigns a material to a mesh-selected directional slot.
    ///
    /// Slots are directional, so they work for any [`PartShape`]: setting
    /// [`Face::Top`] affects the top-facing triangles of a block,
    /// cylinder cap, wedge slope, sphere pole, or custom mesh alike.
    pub fn set_material_slot(&mut self, slot: Face, material: Material) {
        self.material_slots.set(slot, material);
    }

    /// Returns the material if all directional slots have the same effective material.
    ///
    /// Unassigned slots use [`Material::default()`] when compared.
    pub fn material(&self) -> Option<&Material> {
        let material = self.material_slot(Face::ALL[0]);
        if Face::ALL
            .into_iter()
            .skip(1)
            .all(|slot| self.material_slot(slot) == material)
        {
            Some(material)
        } else {
            None
        }
    }

    /// Returns the effective material for a slot.
    ///
    /// An unassigned slot uses [`Material::default()`].
    pub fn material_slot(&self, slot: Face) -> &Material {
        self.material_slots.get(slot).unwrap_or(&DEFAULT_MATERIAL)
    }
}

impl Default for Part {
    fn default() -> Self {
        Self::unnamed()
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

    #[test]
    fn material_slot_falls_back_to_the_default_material_until_overridden() {
        let mut part = Part::new("part");
        assert_eq!(part.material_slot(Face::Top), &Material::default());
        assert_eq!(part.material(), Some(&Material::default()));

        let top = Material::from_color(Color3::new(1.0, 0.0, 0.0));
        part.set_material_slot(Face::Top, top);
        assert_eq!(part.material_slot(Face::Top), &top);
        assert_eq!(part.material(), None);
        assert_eq!(part.material_slot(Face::Bottom), &Material::default());
    }

    #[test]
    fn set_material_assigns_all_directional_slots() {
        let mut part = Part::new("part");
        let material = Material::from_color(Color3::new(1.0, 0.0, 0.0));

        part.set_material(material);

        for slot in Face::ALL {
            assert_eq!(part.material_slot(slot), &material);
        }
        assert_eq!(part.material(), Some(&material));
    }
}
