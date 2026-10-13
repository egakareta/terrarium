#[cfg(feature = "meshpart")]
use crate::MeshPart;
use crate::{
    BasePart, Camera, Instance, Part, Rig,
    glam::{EulerRot, Mat4, Quat, Vec3},
};

/// An object that has a physical location in the world.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct PVInstance {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    pivot: Mat4,
}

impl Default for PVInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl PVInstance {
    /// Creates an instance at the identity transform.
    pub fn new() -> Self {
        Self {
            pivot: Mat4::IDENTITY,
        }
    }

    /// Creates a new instance from a world-space transform.
    pub fn from_world_transform(transform: Mat4) -> Self {
        Self { pivot: transform }
    }

    pub(crate) fn pose_from_transform(transform: Mat4) -> Mat4 {
        let (_, rotation, translation) = transform.to_scale_rotation_translation();
        Mat4::from_rotation_translation(rotation, translation)
    }
}

/// Access to the underlying [`PVInstance`].
pub trait HasPVInstance {
    /// Returns shared access to the underlying [`PVInstance`].
    fn pv(&self) -> &PVInstance;

    /// Returns mutable access to the underlying [`PVInstance`].
    fn pv_mut(&mut self) -> &mut PVInstance;

    /// Sets the world-space pivot in place.
    ///
    /// The default implementation changes only this instance. [`Rig`] also
    /// moves its descendants, preserving their relative transforms.
    fn set_pivot(&mut self, pivot: Mat4) {
        self.pv_mut().pivot = pivot;
    }

    /// The world-space transform of the instance's pivot.
    fn pivot(&self) -> Mat4 {
        self.pv().pivot
    }

    /// Returns the position and orientation (rigid pose), ignoring the pivot's scale.
    fn pose(&self) -> Mat4 {
        PVInstance::pose_from_transform(self.pivot())
    }

    /// Returns the translation component of the pivot.
    fn position(&self) -> Vec3 {
        self.pivot().w_axis.truncate()
    }

    /// Returns XYZ Euler orientation angles in degrees.
    fn orientation(&self) -> Vec3 {
        let (_, rotation, _) = self.pivot().to_scale_rotation_translation();
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);

        Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
    }

    /// Returns the normalized world-space direction of local `-Z`.
    fn forward(&self) -> Vec3 {
        self.pose()
            .transform_vector3(Vec3::NEG_Z)
            .normalize_or_zero()
    }

    /// Moves this instance and all built-in spatial descendants to a world-space pivot.
    ///
    /// Descendants receive the same transform delta, preserving their transforms
    /// relative to this instance, including scale. Non-spatial descendants are
    /// traversed so their spatial children also move.
    /// For instances other than [`Rig`], [`Self::with_pivot`] changes only the
    /// instance's own transform.
    ///
    /// ```
    /// use terrarium::{HasPVInstance, Instance, Part, glam::{Mat4, Vec3}};
    ///
    /// let mut parent = Part::new();
    /// let child = parent.add_child(Part::new().with_position(Vec3::X));
    /// parent.pivot_to(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)));
    /// assert_eq!(parent.position(), Vec3::new(10.0, 0.0, 0.0));
    /// assert_eq!(
    ///     parent.find_descendant(child).unwrap().downcast_ref::<Part>().unwrap().position(),
    ///     Vec3::new(11.0, 0.0, 0.0),
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the current pivot cannot be inverted or the transform delta is
    /// not finite.
    fn pivot_to(&mut self, pivot: Mat4)
    where
        Self: Instance,
    {
        pivot_hierarchy_to(self, pivot);
    }

    /// Replaces this instance's position, orientation and scale.
    ///
    /// Descendants keep their world-space transforms by default. [`Rig`]
    /// moves its descendants with it. Use [`Self::pivot_to`] to move any
    /// instance hierarchy together.
    fn with_pivot(mut self, pivot: Mat4) -> Self
    where
        Self: Sized,
    {
        self.set_pivot(pivot);
        self
    }

    /// Replaces the position and orientation (rigid pose) while preserving the current scale.
    fn with_pose(mut self, pose: Mat4) -> Self
    where
        Self: Sized,
    {
        let pivot = self.pivot();
        let size = pivot.to_scale_rotation_translation().0;
        self.set_pivot(PVInstance::pose_from_transform(pose) * Mat4::from_scale(size));
        self
    }

    /// Replaces the per-axis scale while preserving the current position and orientation (rigid pose).
    fn with_size(mut self, size: Vec3) -> Self
    where
        Self: Sized,
    {
        let pivot = self.pivot();
        self.set_pivot(PVInstance::pose_from_transform(pivot) * Mat4::from_scale(size));
        self
    }

    /// Replaces the translation while preserving the current orientation and size.
    fn with_position(mut self, position: Vec3) -> Self
    where
        Self: Sized,
    {
        let pivot = self.pivot();
        let (_, rotation, _) = pivot.to_scale_rotation_translation();
        let size = pivot.to_scale_rotation_translation().0;
        self.set_pivot(
            Mat4::from_rotation_translation(rotation, position) * Mat4::from_scale(size),
        );
        self
    }

    /// Replaces the XYZ Euler orientation in degrees while preserving position and size.
    fn with_orientation(mut self, orientation: Vec3) -> Self
    where
        Self: Sized,
    {
        let pivot = self.pivot();
        let position = self.position();
        let size = pivot.to_scale_rotation_translation().0;
        self.set_pivot(
            Mat4::from_rotation_translation(
                Quat::from_euler(
                    EulerRot::XYZ,
                    orientation.x.to_radians(),
                    orientation.y.to_radians(),
                    orientation.z.to_radians(),
                ),
                position,
            ) * Mat4::from_scale(size),
        );
        self
    }
}

impl HasPVInstance for PVInstance {
    fn pv(&self) -> &PVInstance {
        self
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        self
    }
}

impl<T: HasPVInstance + ?Sized> HasPVInstance for &mut T {
    fn pv(&self) -> &PVInstance {
        (**self).pv()
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        (**self).pv_mut()
    }

    fn set_pivot(&mut self, pivot: Mat4) {
        (**self).set_pivot(pivot);
    }
}

pub(crate) fn pivot_hierarchy_to<T: HasPVInstance + Instance + ?Sized>(
    instance: &mut T,
    pivot: Mat4,
) {
    let current = instance.pivot();
    if current == pivot {
        return;
    }
    let transform = pivot * current.inverse();
    assert!(
        transform.is_finite(),
        "pivot_to requires an invertible current pivot and a finite transform delta"
    );
    instance.pv_mut().pivot = pivot;
    for child in instance.children_mut() {
        transform_subtree(child, transform);
    }
}

fn transform_subtree(instance: &mut dyn Instance, transform: Mat4) {
    if let Some(part) = instance.downcast_mut::<Part>() {
        transform_pivot(part, transform);
    } else if let Some(part) = instance.downcast_mut::<BasePart>() {
        transform_pivot(part, transform);
    } else if let Some(camera) = instance.downcast_mut::<Camera>() {
        transform_pivot(camera, transform);
    } else if let Some(rig) = instance.downcast_mut::<Rig>() {
        transform_pivot(rig, transform);
    }
    #[cfg(feature = "meshpart")]
    if let Some(part) = instance.downcast_mut::<MeshPart>() {
        transform_pivot(part, transform);
    }

    for child in instance.children_mut() {
        transform_subtree(child, transform);
    }
}

fn transform_pivot(instance: &mut impl HasPVInstance, transform: Mat4) {
    let pivot = transform * instance.pivot();
    instance.pv_mut().pivot = pivot;
}

pub(crate) fn instance_pivot(instance: &dyn Instance) -> Option<Mat4> {
    if let Some(part) = instance.downcast_ref::<Part>() {
        return Some(part.pivot());
    }
    if let Some(part) = instance.downcast_ref::<BasePart>() {
        return Some(part.pivot());
    }
    if let Some(camera) = instance.downcast_ref::<Camera>() {
        return Some(camera.pivot());
    }
    if let Some(rig) = instance.downcast_ref::<Rig>() {
        return Some(rig.pivot());
    }
    #[cfg(feature = "meshpart")]
    if let Some(part) = instance.downcast_ref::<MeshPart>() {
        return Some(part.pivot());
    }
    None
}

pub(crate) fn pivot_instance_to(instance: &mut dyn Instance, pivot: Mat4) -> bool {
    if let Some(part) = instance.downcast_mut::<Part>() {
        part.pivot_to(pivot);
    } else if let Some(part) = instance.downcast_mut::<BasePart>() {
        part.pivot_to(pivot);
    } else if let Some(camera) = instance.downcast_mut::<Camera>() {
        camera.pivot_to(pivot);
    } else if let Some(rig) = instance.downcast_mut::<Rig>() {
        rig.pivot_to(pivot);
    } else {
        #[cfg(feature = "meshpart")]
        if let Some(part) = instance.downcast_mut::<MeshPart>() {
            part.pivot_to(pivot);
            return true;
        }
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Workspace;

    #[test]
    fn pivot_to_moves_nested_spatial_descendants_through_non_spatial_nodes() {
        let initial_pose =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.4), Vec3::new(3.0, 4.0, 5.0));
        let target_pose =
            Mat4::from_rotation_translation(Quat::from_rotation_z(1.2), Vec3::new(-2.0, 6.0, 1.0));
        let parent_scale = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
        let part_offset = Mat4::from_scale_rotation_translation(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_rotation_x(0.7),
            Vec3::new(2.0, 0.0, -1.0),
        );
        let camera_offset =
            Mat4::from_rotation_translation(Quat::from_rotation_y(-0.3), Vec3::new(-1.0, 2.0, 3.0));
        let base_offset = Mat4::from_translation(Vec3::new(5.0, -1.0, 0.0));
        let mut parent = BasePart::new().with_pivot(initial_pose * parent_scale);
        let mut part = Part::new().with_pivot(initial_pose * part_offset);
        let camera_id = part.add_child(Camera::default().with_pivot(initial_pose * camera_offset));
        let part_id = parent.add_child(part);
        let mut light = crate::PointLight::new();
        let base_id = light.add_child(BasePart::new().with_pivot(initial_pose * base_offset));
        let light_id = parent.add_child(light);
        #[cfg(feature = "meshpart")]
        let mesh_id = parent.add_child(
            crate::MeshPart::new(crate::Mesh::cube(1.0, [1.0; 4]))
                .with_pivot(initial_pose * part_offset),
        );
        let mut workspace = Workspace::new();
        let parent_id = parent.set_parent(&mut workspace);
        let unrelated_pivot = Mat4::from_translation(Vec3::splat(20.0));
        let unrelated_id = Part::new()
            .with_pivot(unrelated_pivot)
            .set_parent(&mut workspace);

        let target = target_pose * parent_scale;
        workspace
            .get_mut::<BasePart>(parent_id)
            .unwrap()
            .pivot_to(target);

        assert_eq!(
            workspace.get::<BasePart>(parent_id).unwrap().pivot(),
            target
        );
        assert!(
            workspace
                .get::<Part>(part_id)
                .unwrap()
                .pivot()
                .abs_diff_eq(target_pose * part_offset, 0.0001)
        );
        assert!(
            workspace
                .get::<Camera>(camera_id)
                .unwrap()
                .pivot()
                .abs_diff_eq(target_pose * camera_offset, 0.0001)
        );
        assert!(
            workspace
                .get::<BasePart>(base_id)
                .unwrap()
                .pivot()
                .abs_diff_eq(target_pose * base_offset, 0.0001)
        );
        #[cfg(feature = "meshpart")]
        assert!(
            workspace
                .get::<crate::MeshPart>(mesh_id)
                .unwrap()
                .pivot()
                .abs_diff_eq(target_pose * part_offset, 0.0001)
        );
        assert_eq!(
            workspace.get::<Part>(unrelated_id).unwrap().pivot(),
            unrelated_pivot
        );
        assert_eq!(
            workspace.instance(part_id).unwrap().parent(),
            Some(parent_id)
        );
        assert_eq!(
            workspace.instance(camera_id).unwrap().parent(),
            Some(part_id)
        );
        assert_eq!(
            workspace.instance(base_id).unwrap().parent(),
            Some(light_id)
        );

        let camera_pivot = workspace.get::<Camera>(camera_id).unwrap().pivot();
        workspace
            .get_mut::<BasePart>(parent_id)
            .unwrap()
            .pivot_to(target);
        assert_eq!(
            workspace.get::<Camera>(camera_id).unwrap().pivot(),
            camera_pivot
        );
    }

    #[test]
    fn pivot_to_scales_descendant_offsets_and_sizes() {
        let mut parent = Part::new().with_position(Vec3::new(1.0, 0.0, 0.0));
        let child_id = parent.add_child(
            BasePart::new()
                .with_position(Vec3::new(3.0, 0.0, 0.0))
                .with_size(Vec3::new(2.0, 3.0, 4.0)),
        );
        parent.pivot_to(Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::IDENTITY,
            Vec3::new(10.0, 0.0, 0.0),
        ));

        let child = parent
            .find_descendant(child_id)
            .unwrap()
            .downcast_ref::<BasePart>()
            .unwrap();
        let expected = Mat4::from_scale_rotation_translation(
            Vec3::new(4.0, 6.0, 8.0),
            Quat::IDENTITY,
            Vec3::new(14.0, 0.0, 0.0),
        );
        assert!(child.pivot().abs_diff_eq(expected, 0.0001));
    }

    #[test]
    #[should_panic(expected = "pivot_to requires an invertible current pivot")]
    fn pivot_to_rejects_a_singular_current_pivot() {
        let mut parent = Part::new().with_size(Vec3::ZERO);
        parent.add_child(Part::new());
        parent.pivot_to(Mat4::IDENTITY);
    }

    #[test]
    fn with_pivot_changes_only_the_instance_transform() {
        let mut parent = BasePart::new();
        let child_pivot = Mat4::from_translation(Vec3::new(2.0, 3.0, 4.0));
        let camera_pivot = Mat4::from_rotation_y(0.8);
        let mut child = Part::new().with_pivot(child_pivot);
        let camera_id = child.add_child(Camera::default().with_pivot(camera_pivot));
        let child_id = parent.add_child(child);
        let target = Mat4::from_rotation_translation(
            Quat::from_rotation_z(1.0),
            Vec3::new(10.0, 20.0, 30.0),
        );

        let parent = parent.with_pivot(target);

        assert_eq!(parent.pivot(), target);
        assert_eq!(
            parent
                .find_descendant(child_id)
                .unwrap()
                .downcast_ref::<Part>()
                .unwrap()
                .pivot(),
            child_pivot
        );
        assert_eq!(
            parent
                .find_descendant(camera_id)
                .unwrap()
                .downcast_ref::<Camera>()
                .unwrap()
                .pivot(),
            camera_pivot
        );
    }
}
