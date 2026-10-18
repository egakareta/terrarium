use crate::{
    Instance,
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

fn pivot_with_pose(pivot: Mat4, pose: Mat4) -> Mat4 {
    let size = pivot.to_scale_rotation_translation().0;
    PVInstance::pose_from_transform(pose) * Mat4::from_scale(size)
}

fn pivot_with_size(pivot: Mat4, size: Vec3) -> Mat4 {
    PVInstance::pose_from_transform(pivot) * Mat4::from_scale(size)
}

fn pivot_with_position(pivot: Mat4, position: Vec3) -> Mat4 {
    let (size, rotation, _) = pivot.to_scale_rotation_translation();
    Mat4::from_rotation_translation(rotation, position) * Mat4::from_scale(size)
}

fn pivot_with_orientation(pivot: Mat4, orientation: Vec3) -> Mat4 {
    let size = pivot.to_scale_rotation_translation().0;
    Mat4::from_rotation_translation(
        Quat::from_euler(
            EulerRot::XYZ,
            orientation.x.to_radians(),
            orientation.y.to_radians(),
            orientation.z.to_radians(),
        ),
        pivot.w_axis.truncate(),
    ) * Mat4::from_scale(size)
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

    /// Reports whether this instance accepts the supplied pivot.
    #[doc(hidden)]
    fn accepts_pivot(&self, _pivot: Mat4) -> bool {
        true
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

    /// Moves this instance and all spatial descendants to a world-space pivot.
    ///
    /// Descendants receive the same transform delta, preserving their transforms
    /// relative to this instance, including scale. Non-spatial descendants are
    /// traversed so their spatial children also move.
    /// For instances other than [`Rig`], [`Self::with_pivot`] changes only the
    /// instance's own transform.
    ///
    /// ```
    /// use trscene::{HasPVInstance, Instance, Part, glam::{Mat4, Vec3}};
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
        self.set_pivot(pivot_with_pose(self.pivot(), pose));
        self
    }

    /// Replaces the per-axis scale while preserving the current position and orientation (rigid pose).
    fn with_size(mut self, size: Vec3) -> Self
    where
        Self: Sized,
    {
        self.set_pivot(pivot_with_size(self.pivot(), size));
        self
    }

    /// Replaces the translation while preserving the current orientation and size.
    fn with_position(mut self, position: Vec3) -> Self
    where
        Self: Sized,
    {
        self.set_pivot(pivot_with_position(self.pivot(), position));
        self
    }

    /// Replaces the XYZ Euler orientation in degrees while preserving position and size.
    fn with_orientation(mut self, orientation: Vec3) -> Self
    where
        Self: Sized,
    {
        self.set_pivot(pivot_with_orientation(self.pivot(), orientation));
        self
    }
}

/// An instance with a world-space pivot and hierarchy-aware transform operations.
///
/// Spatial instances are exposed through [`Instance::spatial`] so systems can
/// work with transforms without knowing the concrete instance type.
pub trait Spatial: Instance {
    /// Returns the instance's world-space pivot.
    fn world_pivot(&self) -> Mat4;

    /// Replaces the pivot using this instance's transform semantics.
    fn set_world_pivot(&mut self, pivot: Mat4);

    /// Moves this instance and its spatial descendants to a world-space pivot.
    fn move_to_world_pivot(&mut self, pivot: Mat4);

    /// Returns the rigid pose, ignoring the pivot's scale.
    fn rigid_pose(&self) -> Mat4;

    /// Returns the translation component of the pivot.
    fn world_position(&self) -> Vec3;

    /// Returns XYZ Euler orientation angles in degrees.
    fn world_orientation(&self) -> Vec3;

    /// Returns the normalized world-space direction of local `-Z`.
    fn world_forward(&self) -> Vec3;

    /// Reports whether this instance accepts the supplied pivot.
    #[doc(hidden)]
    fn accepts_world_pivot(&self, pivot: Mat4) -> bool;

    /// Replaces the position while preserving orientation and scale.
    ///
    /// Returns `false` if this instance rejects the resulting transform.
    fn set_world_position(&mut self, position: Vec3) -> bool {
        self.try_set_world_pivot(pivot_with_position(self.world_pivot(), position))
    }

    /// Replaces the XYZ Euler orientation in degrees while preserving position and scale.
    ///
    /// Returns `false` if this instance rejects the resulting transform.
    fn set_world_orientation(&mut self, orientation: Vec3) -> bool {
        self.try_set_world_pivot(pivot_with_orientation(self.world_pivot(), orientation))
    }

    /// Replaces the scale while preserving position and orientation.
    ///
    /// Returns `false` if this instance rejects the resulting transform.
    fn set_world_size(&mut self, size: Vec3) -> bool {
        self.try_set_world_pivot(pivot_with_size(self.world_pivot(), size))
    }

    /// Replaces position and orientation while preserving the current scale.
    ///
    /// Returns `false` if this instance rejects the resulting transform.
    fn set_rigid_pose(&mut self, pose: Mat4) -> bool {
        self.try_set_world_pivot(pivot_with_pose(self.world_pivot(), pose))
    }

    /// Replaces the pivot if this instance accepts it.
    fn try_set_world_pivot(&mut self, pivot: Mat4) -> bool {
        if !self.accepts_world_pivot(pivot) {
            return false;
        }
        self.set_world_pivot(pivot);
        true
    }

    /// Sets only this node's stored pivot, without applying rig-specific propagation.
    #[doc(hidden)]
    fn set_pivot_direct(&mut self, pivot: Mat4);
}

impl<T: HasPVInstance + Instance> Spatial for T {
    fn world_pivot(&self) -> Mat4 {
        HasPVInstance::pivot(self)
    }

    fn set_world_pivot(&mut self, pivot: Mat4) {
        HasPVInstance::set_pivot(self, pivot);
    }

    fn move_to_world_pivot(&mut self, pivot: Mat4) {
        HasPVInstance::pivot_to(self, pivot);
    }

    fn rigid_pose(&self) -> Mat4 {
        HasPVInstance::pose(self)
    }

    fn world_position(&self) -> Vec3 {
        HasPVInstance::position(self)
    }

    fn world_orientation(&self) -> Vec3 {
        HasPVInstance::orientation(self)
    }

    fn world_forward(&self) -> Vec3 {
        HasPVInstance::forward(self)
    }

    fn accepts_world_pivot(&self, pivot: Mat4) -> bool {
        HasPVInstance::accepts_pivot(self, pivot)
    }

    fn set_pivot_direct(&mut self, pivot: Mat4) {
        self.pv_mut().pivot = pivot;
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

/// Updates a spatial instance through its common transform interface.
/// Returns false for non-spatial instances or singular resulting transforms.
pub(crate) fn update_instance_transform(
    instance: &mut dyn Instance,
    update: InstanceTransformUpdate,
) -> bool {
    let Some(spatial) = instance.spatial_mut() else {
        return false;
    };
    match update {
        InstanceTransformUpdate::Position(value) => spatial.set_world_position(value),
        InstanceTransformUpdate::Orientation(value) => spatial.set_world_orientation(value),
        InstanceTransformUpdate::Size(value) => spatial.set_world_size(value),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum InstanceTransformUpdate {
    Position(Vec3),
    Orientation(Vec3),
    Size(Vec3),
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
    if let Some(spatial) = instance.spatial_mut() {
        spatial.set_pivot_direct(transform * spatial.world_pivot());
    }
    for child in instance.children_mut() {
        transform_subtree(child, transform);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasePart, Camera, Part, Workspace};

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
