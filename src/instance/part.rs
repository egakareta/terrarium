use crate::{
    Color3, DEFAULT_MATERIAL, Face, HasPVInstance, InstanceData, Material, MeshMaterialSlots,
    PVInstance, PartShape,
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
    size: Vec3,
    color: Color3,
    anchored: bool,
    can_collide: bool,
}

impl BasePart {
    /// Creates a base part with identity transform, unit size, white tint, and
    /// collision metadata enabled.
    pub fn new() -> Self {
        Self {
            instance: InstanceData::new("BasePart"),
            pv_instance: PVInstance::new(),
            size: Vec3::ONE,
            color: Color3::WHITE,
            anchored: true,
            can_collide: true,
        }
    }
}

impl Default for BasePart {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_instance!(BasePart, class_name = "BasePart", data = instance,);

impl HasPVInstance for BasePart {
    fn pv(&self) -> &PVInstance {
        &self.pv_instance
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        &mut self.pv_instance
    }
}

/// Access to the underlying [`BasePart`].
pub trait HasBasePart {
    /// Returns shared access to the underlying [`BasePart`].
    fn base_part(&self) -> &BasePart;

    /// Returns mutable access to the underlying [`BasePart`].
    fn base_part_mut(&mut self) -> &mut BasePart;

    /// World-space scale applied to the unit primitive mesh.
    fn size(&self) -> Vec3 {
        self.base_part().size
    }

    /// Tint.
    fn color(&self) -> Color3 {
        self.base_part().color
    }

    /// Whether a physics system should treat this part as immovable.
    ///
    /// The default physics integration creates a fixed body when this is `true`
    /// and a dynamic body when this is `false`.
    fn anchored(&self) -> bool {
        self.base_part().anchored
    }

    /// Whether a physics system should consider this part for collisions.
    ///
    /// The default physics integration omits this part's collider when this is
    /// `false`, while still simulating its rigid body.
    fn can_collide(&self) -> bool {
        self.base_part().can_collide
    }

    /// Sets the world-space scale applied to the unit primitive mesh.
    fn with_size(mut self, size: Vec3) -> Self
    where
        Self: Sized,
    {
        self.base_part_mut().size = size;
        self
    }

    /// Sets the tint.
    fn with_color(mut self, color: Color3) -> Self
    where
        Self: Sized,
    {
        self.base_part_mut().color = color;
        self
    }

    /// Sets whether a physics system should treat this part as immovable.
    ///
    /// The default physics integration creates a fixed body when this is `true`
    /// and a dynamic body when this is `false`.
    fn with_anchored(mut self, anchored: bool) -> Self
    where
        Self: Sized,
    {
        self.base_part_mut().anchored = anchored;
        self
    }

    /// Sets whether a physics system should consider this part for collisions.
    ///
    /// The default physics integration omits this part's collider when this is
    /// `false`, while still simulating its rigid body.
    fn with_can_collide(mut self, can_collide: bool) -> Self
    where
        Self: Sized,
    {
        self.base_part_mut().can_collide = can_collide;
        self
    }
}

impl HasBasePart for BasePart {
    fn base_part(&self) -> &BasePart {
        self
    }

    fn base_part_mut(&mut self) -> &mut BasePart {
        self
    }
}

impl<T: HasBasePart + ?Sized> HasBasePart for &mut T {
    fn base_part(&self) -> &BasePart {
        (**self).base_part()
    }

    fn base_part_mut(&mut self) -> &mut BasePart {
        (**self).base_part_mut()
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
    /// Unlike [`HasPVInstance::pivot`], this includes volume.
    fn transform(&self) -> Mat4;
}

impl Transform for BasePart {
    fn transform(&self) -> Mat4 {
        self.pivot() * Mat4::from_scale(self.size())
    }
}

/// Access to directional material slots.
pub trait HasMaterials {
    /// Returns shared access to the directional material slots.
    fn material_slots(&self) -> &MeshMaterialSlots;

    /// Returns mutable access to the directional material slots.
    fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots;

    /// Returns the material if all directional slots have the same effective material.
    ///
    /// Unassigned slots use [`Material::default()`] when compared.
    fn material(&self) -> Option<&Material> {
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
    fn material_slot(&self, slot: Face) -> &Material {
        self.material_slots().get(slot).unwrap_or(&DEFAULT_MATERIAL)
    }

    /// Assigns the same material to all six directional slots.
    fn with_material(mut self, material: Material) -> Self
    where
        Self: Sized,
    {
        for slot in Face::ALL {
            self.material_slots_mut().set(slot, material);
        }
        self
    }

    /// Assigns a material to a mesh-selected directional slot.
    ///
    /// Slots are directional, so they work for any [`PartShape`]: setting
    /// [`Face::Top`] affects the top-facing triangles of a block,
    /// cylinder cap, wedge slope, sphere pole, or custom mesh alike.
    fn with_material_slot(mut self, slot: Face, material: Material) -> Self
    where
        Self: Sized,
    {
        self.material_slots_mut().set(slot, material);
        self
    }
}

impl<T: HasMaterials + ?Sized> HasMaterials for &mut T {
    fn material_slots(&self) -> &MeshMaterialSlots {
        (**self).material_slots()
    }

    fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots {
        (**self).material_slots_mut()
    }
}

/// A visible scene object rendered with one of the built-in primitive shapes.
#[derive(Clone, Debug)]
pub struct Part {
    basepart: BasePart,
    shape: PartShape,
    pub(crate) material_slots: MeshMaterialSlots,
}

impl Part {
    /// Creates a visible part using a unit [`PartShape::Block`] and the default
    /// material.
    pub fn new() -> Self {
        Self {
            basepart: <BasePart as crate::Instance>::named(BasePart::new(), "Part"),
            shape: PartShape::Block,
            material_slots: MeshMaterialSlots::default(),
        }
    }
}

impl Default for Part {
    fn default() -> Self {
        Self::new()
    }
}

/// Access to the underlying [`Part`].
pub trait HasPart {
    /// Returns shared access to the underlying [`Part`].
    fn part(&self) -> &Part;
    /// Returns mutable access to the underlying [`Part`].
    fn part_mut(&mut self) -> &mut Part;

    /// Primitive geometry used when the part is rendered.
    fn shape(&self) -> PartShape {
        self.part().shape
    }

    /// Sets the primitive geometry used when the part is rendered.
    fn with_shape(mut self, shape: PartShape) -> Self
    where
        Self: Sized,
    {
        self.part_mut().shape = shape;
        self
    }
}

impl HasPart for Part {
    fn part(&self) -> &Part {
        self
    }

    fn part_mut(&mut self) -> &mut Part {
        self
    }
}

impl<T: HasPart + ?Sized> HasPart for &mut T {
    fn part(&self) -> &Part {
        (**self).part()
    }
    fn part_mut(&mut self) -> &mut Part {
        (**self).part_mut()
    }
}

crate::impl_instance!(Part, class_name = "Part", data = basepart.instance,);

impl HasPVInstance for Part {
    fn pv(&self) -> &PVInstance {
        self.basepart.pv()
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        self.basepart.pv_mut()
    }
}

impl HasBasePart for Part {
    fn base_part(&self) -> &BasePart {
        &self.basepart
    }

    fn base_part_mut(&mut self) -> &mut BasePart {
        &mut self.basepart
    }
}

impl HasMaterials for Part {
    fn material_slots(&self) -> &MeshMaterialSlots {
        &self.material_slots
    }

    fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots {
        &mut self.material_slots
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::Instance as _;

    #[test]
    fn part_position_and_orientation_accessors_use_the_pivot() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let orientation = Vec3::new(10.0, 20.0, 30.0);
        let part = Part::new()
            .named("part")
            .with_position(position)
            .with_orientation(orientation);

        assert_eq!(part.position(), position);
        assert!((part.orientation() - orientation).abs().max_element() < 0.0001);
    }

    #[test]
    fn material_slot_falls_back_to_the_default_material_until_overridden() {
        let part = Part::new().named("part");
        assert_eq!(part.material_slot(Face::Top), &Material::default());
        assert_eq!(part.material(), Some(&Material::default()));

        let top = Material::from_color(Color3::new(1.0, 0.0, 0.0));
        let part = part.with_material_slot(Face::Top, top);
        assert_eq!(part.material_slot(Face::Top), &top);
        assert_eq!(part.material(), None);
        assert_eq!(part.material_slot(Face::Bottom), &Material::default());
    }

    #[test]
    fn with_material_assigns_all_directional_slots() {
        let part = Part::new().named("part");
        let material = Material::from_color(Color3::new(1.0, 0.0, 0.0));

        let part = part.with_material(material);

        for slot in Face::ALL {
            assert_eq!(part.material_slot(slot), &material);
        }
        assert_eq!(part.material(), Some(&material));
    }
}
