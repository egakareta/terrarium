use crate::{
    Color3, DEFAULT_MATERIAL, Face, InstanceData, Material, MeshMaterialSlots, PVInstance,
    PartShape,
};

crate::instance! {
    #[instance(inspect = false)]
    /// A named, transformable scene node with optional collision metadata.
    ///
    /// `BasePart` is useful as a parent for custom instance hierarchies. A visible
    /// [`Part`] builds on it with a primitive shape and material.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct BasePart {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[data]
        pub(crate) instance: InstanceData,
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[compose]
        pub(crate) pv_instance: PVInstance,
        /// Tint applied when rendering this part.
        #[property(default = Color3::WHITE)]
        color: Color3,
        /// Transparency, where `0.0` is opaque and `1.0` is invisible. Values
        /// are clamped to `0.0..=1.0`; non-finite values become `0.0`.
        #[property(default = 0.0, sanitize = sanitize_transparency)]
        #[cfg_attr(feature = "devtools", inspect(setter = with_transparency))]
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
        /// Creates a base part with identity pose, unit scale, white tint,
        /// opaque rendering, and collision metadata enabled.
        pub fn new() -> Self {
            Self::default()
        }
        /// Returns shared access to the underlying [`BasePart`].
        pub fn base_part(&self) -> &BasePart {
            crate::Project::<BasePart>::project(self)
        }
        /// Returns mutable access to the underlying [`BasePart`].
        pub fn base_part_mut(&mut self) -> &mut BasePart {
            crate::Project::<BasePart>::project_mut(self)
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

/// Access to directional material slots.
pub trait MaterialsExt: crate::Project<MeshMaterialSlots> {
    /// Returns shared access to the directional material slots.
    fn material_slots(&self) -> &MeshMaterialSlots {
        self.project()
    }

    /// Returns mutable access to the directional material slots.
    fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots {
        self.project_mut()
    }

    crate::__instance_property! {
        self;
        /// A material is available only when all six directional slots have
        /// the same effective value. Unassigned slots use [`Material::default()`].
        getter material() -> Option<&Material> {
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
        setter set_material(material: Material) {
            for slot in Face::ALL {
                self.material_slots_mut().set(slot, material);
            }
        }
        builder with_material;
    }

    crate::__instance_property! {
        self;
        /// Effective material for a directional slot. An unassigned slot uses
        /// [`Material::default()`]. Slots are mesh-selected, so they work for
        /// any [`PartShape`].
        getter material_slot(slot: Face) -> &Material {
            self.material_slots().get(slot).unwrap_or(&DEFAULT_MATERIAL)
        }
        setter set_material_slot(slot: Face, material: Material) {
            self.material_slots_mut().set(slot, material);
        }
        builder with_material_slot;
    }
}

impl<T: MaterialsExt + ?Sized> MaterialsExt for &mut T {
    fn material_slots(&self) -> &MeshMaterialSlots {
        (**self).material_slots()
    }

    fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots {
        (**self).material_slots_mut()
    }

    fn material(&self) -> Option<&Material> {
        (**self).material()
    }

    fn set_material(&mut self, material: Material) {
        (**self).set_material(material);
    }

    fn material_slot(&self, slot: Face) -> &Material {
        (**self).material_slot(slot)
    }

    fn set_material_slot(&mut self, slot: Face, material: Material) {
        (**self).set_material_slot(slot, material);
    }
}

crate::instance! {
    #[instance(inspect = false)]
    /// A visible scene object rendered with one of the built-in primitive shapes.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct Part {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        basepart: BasePart,
        /// Primitive geometry used when the part is rendered.
        #[property(default = PartShape::Block)]
        shape: PartShape,
        #[compose]
        pub(crate) material_slots: MeshMaterialSlots,
    }

    impl Part {
        /// Creates a visible part using a unit [`PartShape::Block`] and the default material.
        pub fn new() -> Self {
            Self::default()
        }
        /// Returns shared access to the underlying [`Part`].
        pub fn part(&self) -> &Part {
            crate::Project::<Part>::project(self)
        }
        /// Returns mutable access to the underlying [`Part`].
        pub fn part_mut(&mut self) -> &mut Part {
            crate::Project::<Part>::project_mut(self)
        }
    }
}

/// Compatibility name for the generated base-part API.
pub use BasePartExt as HasBasePart;
/// Compatibility name for the material API.
pub use MaterialsExt as HasMaterials;
/// Compatibility name for the generated primitive-part API.
pub use PartExt as HasPart;

impl crate::Project<MeshMaterialSlots> for MeshMaterialSlots {
    fn project(&self) -> &MeshMaterialSlots {
        self
    }
    fn project_mut(&mut self) -> &mut MeshMaterialSlots {
        self
    }
}
impl MaterialsExt for MeshMaterialSlots {}
/// A scene instance exposing directional material slots.
pub trait MaterialsView: MaterialsExt + crate::InstanceReceiver {}
impl<T: MaterialsExt + crate::InstanceReceiver + ?Sized> MaterialsView for T {}
impl crate::InstanceType for MeshMaterialSlots {
    type Api = dyn MaterialsView;
    fn descriptor() -> &'static crate::InstanceClass {
        &crate::InstanceClass {
            name: "MeshMaterialSlots",
            type_id: std::any::TypeId::of::<Self>,
            bases: &[],
        }
    }
}
/// Supplies the material capability's schema to composed instances.
#[doc(hidden)]
#[macro_export]
macro_rules! __terrarium_material_slots {
    ($callback:path, [$($state:tt)*]) => {
        $callback! { $($state)* [{ MeshMaterialSlots [$crate::MeshMaterialSlots] [$crate::MaterialsExt] [$crate::MaterialsView] [$crate::__MaterialInspection] [material_slots material_slots_mut material set_material with_material material_slot set_material_slot with_material_slot] false [] }] }
    };
}

/// Inspector adapter for directional material storage.
#[doc(hidden)]
#[cfg(feature = "devtools")]
pub struct __MaterialInspection;
#[cfg(feature = "devtools")]
impl __MaterialInspection {
    /// Returns fields supplied by directional material storage.
    pub fn __instance_properties<R: MaterialsExt + ?Sized>(
        receiver: &R,
    ) -> Vec<crate::devtools::Property> {
        crate::devtools::Inspect::properties(receiver.material_slots())
            .into_iter()
            .map(|property| property.prefixed("material_slots"))
            .collect()
    }
    /// Applies an edit to directional material storage.
    pub fn __instance_set_property<R: MaterialsExt + ?Sized>(
        receiver: &mut R,
        path: &str,
        value: &crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        let tail = crate::devtools::property_tail(path, "material_slots")
            .ok_or(crate::devtools::PropertyError::NotFound)?;
        crate::devtools::Inspect::set_property(receiver.material_slots_mut(), tail, value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HasPVInstance, Instance,
        glam::{EulerRot, Mat4, Quat, Vec3},
    };

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
