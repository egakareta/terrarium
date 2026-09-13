use std::ops::{Deref, DerefMut};

use glam::{Mat4, Vec3};

use crate::{Color3, InstanceData, PVInstance};

#[derive(Clone, Debug)]
pub struct BasePart {
    pub(crate) instance: InstanceData,
    pub(crate) pv_instance: PVInstance,
    pub size: Vec3,
    /// Tint.
    pub color: Color3,
    pub anchored: bool,
    pub can_collide: bool,
}

impl BasePart {
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
