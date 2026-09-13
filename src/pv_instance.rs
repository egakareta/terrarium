use glam::{EulerRot, Mat4, Quat, Vec3};

/// An object that has a physical location in the world.
#[derive(Clone, Debug)]
pub struct PVInstance {
    pivot: Mat4,
}

impl Default for PVInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl PVInstance {
    pub fn new() -> Self {
        Self {
            pivot: Mat4::IDENTITY,
        }
    }

    /// Creates a new instance from a world-space transform.
    pub fn from_world_transform(transform: Mat4) -> Self {
        Self { pivot: transform }
    }

    /// The world-space transform of the instance's pivot.
    pub fn pivot(&self) -> Mat4 {
        self.pivot
    }

    /// Transforms the [`PVInstance`] along with all of its descendant [`PVInstance`]s such that the pivot is now located at the specified transform.
    pub fn pivot_to(&mut self, pivot: Mat4) {
        self.pivot = pivot;
    }

    pub fn position(&self) -> Vec3 {
        self.pivot.w_axis.truncate()
    }

    pub fn set_position(&mut self, position: Vec3) {
        let (_, rotation, _) = self.pivot.to_scale_rotation_translation();
        self.pivot = Mat4::from_rotation_translation(rotation, position);
    }

    pub fn orientation(&self) -> Vec3 {
        let (_, rotation, _) = self.pivot.to_scale_rotation_translation();
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);

        Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
    }

    pub fn set_orientation(&mut self, orientation: Vec3) {
        self.pivot = Mat4::from_rotation_translation(
            Quat::from_euler(
                EulerRot::XYZ,
                orientation.x.to_radians(),
                orientation.y.to_radians(),
                orientation.z.to_radians(),
            ),
            self.position(),
        );
    }

    pub fn forward(&self) -> Vec3 {
        self.pivot()
            .transform_vector3(Vec3::NEG_Z)
            .normalize_or_zero()
    }
}
