use crate::*;

instance! {
    /// A named, transformable scene node with optional collision metadata.
    ///
    /// `BasePart` is useful as a parent for custom instance hierarchies. A visible
    /// [`Part`] builds on it with primitive geometry.
    #[derive(Clone, Debug)]
    pub struct BasePart {
        #[data]
        pub(crate) instance: InstanceData,
        #[compose]
        pub(crate) pv_instance: PVInstance,
        /// Directional materials used when rendering this part.
        #[property(borrow)]
        pub(crate) material_slots: MeshMaterialSlots,
        /// Tint applied when rendering this part.
        #[property(default = Color3::WHITE)]
        color: Color3,
        /// Transparency, where `0.0` is opaque and `1.0` is invisible. Values
        /// are clamped to `0.0..=1.0`; non-finite values become `0.0`.
        #[property(default = 0.0, sanitize = sanitize_transparency)]
        transparency: f32,
        /// Whether a physics system should treat this part as immovable. The
        /// default physics integration creates a fixed body when this is `true`
        /// and a dynamic body when this is `false`. Inside a [`crate::Rig`],
        /// the rig's anchored state controls the shared body.
        #[property(default = true)]
        anchored: bool,
        /// Whether a physics system should consider this part for collisions.
        /// The default physics integration omits this part's collider when
        /// `false`, while still simulating its rigid body.
        #[property(default = true)]
        can_collide: bool,
    }

    impl BasePart {
        /// Returns mutable directional material storage.
        pub fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots {
            &mut self.material_slots
        }

        /// Returns a material when all directional slots have the same effective value.
        /// Unassigned slots use [`Material::default()`].
        pub fn material(&self) -> Option<&Material> {
            let material = self.material_slot(Face::ALL[0]);
            Face::ALL
                .into_iter()
                .all(|slot| self.material_slot(slot) == material)
                .then_some(material)
        }

        /// Sets every directional material slot.
        pub fn set_material(&mut self, material: Material) {
            for slot in Face::ALL {
                self.material_slots.set(slot, material);
            }
        }

        /// Returns the effective material for a directional slot, falling back to the default.
        pub fn material_slot(&self, slot: Face) -> &Material {
            self.material_slots().get(slot).unwrap_or(&DEFAULT_MATERIAL)
        }

        /// Sets the material for a directional slot selected by the mesh.
        pub fn set_material_slot(&mut self, slot: Face, material: Material) {
            self.material_slots.set(slot, material);
        }

        /// Creates a base part with identity pose, unit scale, white tint,
        /// opaque rendering, and collision metadata enabled.
        pub fn new() -> Self {
            Self::default()
        }
    }
}

fn sanitize_transparency(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

instance! {
    /// A visible scene object rendered with one of the built-in primitive shapes.
    #[derive(Clone, Debug)]
    pub struct Part {
        #[extends]
        basepart: BasePart,
        /// Primitive geometry used when the part is rendered.
        #[property(default = PartShape::Block)]
        shape: PartShape,
    }

    impl Part {
        /// Creates a visible part using a unit [`PartShape::Block`] and the default material.
        pub fn new() -> Self {
            Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{glam::*, *};

    #[test]
    fn part_position_and_orientation_accessors_use_the_pivot() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let orientation = Vec3::new(10.0, 20.0, 30.0);
        let part = Part::new()
            .with_name("part")
            .with_position(position)
            .with_orientation(orientation);

        assert_eq!(part.position(), position);
        assert!((part.orientation() - orientation).abs().max_element() < 0.0001);
    }

    #[test]
    fn pivot_carries_scale_through_pose_updates() {
        let size = Vec3::new(2.0, 3.0, 4.0);
        let position = Vec3::new(1.0, 2.0, 3.0);
        let orientation = Vec3::new(10.0, 20.0, 30.0);
        let part = Part::new()
            .with_size(size)
            .with_position(position)
            .with_orientation(orientation);

        let pivot = part.pivot();
        let rotation = Quat::from_euler(
            EulerRot::XYZ,
            orientation.x.to_radians(),
            orientation.y.to_radians(),
            orientation.z.to_radians(),
        );
        let expected = Mat4::from_rotation_translation(rotation, position) * Mat4::from_scale(size);

        assert!(
            (pivot.to_scale_rotation_translation().0 - size)
                .abs()
                .max_element()
                < 0.0001
        );
        assert_eq!(pivot.w_axis.truncate(), position);
        for (actual, expected) in pivot
            .to_cols_array()
            .into_iter()
            .zip(expected.to_cols_array())
        {
            assert!((actual - expected).abs() < 0.0001);
        }
        assert_eq!(part.base_part().pivot(), pivot);

        let moved = part.with_pose(Mat4::from_translation(Vec3::new(4.0, 5.0, 6.0)));
        assert!(
            (moved.pivot().to_scale_rotation_translation().0 - size)
                .abs()
                .max_element()
                < 0.0001
        );
    }

    #[test]
    fn material_slot_falls_back_to_the_default_material_until_overridden() {
        let part = Part::new().with_name("part");
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
        let part = Part::new().with_name("part");
        let material = Material::from_color(Color3::new(1.0, 0.0, 0.0));

        let part = part.with_material(material);

        for slot in Face::ALL {
            assert_eq!(part.material_slot(slot), &material);
        }
        assert_eq!(part.material(), Some(&material));
    }

    #[test]
    fn transparency_is_available_on_base_part_subtypes() {
        let part = Part::new().with_transparency(0.35);
        assert_eq!(part.transparency(), 0.35);

        let part = Part::new().with_transparency(2.0);
        assert_eq!(part.transparency(), 1.0);
    }
}
