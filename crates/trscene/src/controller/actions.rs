use crate::glam::{Mat4, Quat, Vec2, Vec3};

/// Movement, look, and sprint values independent of any input device.
///
/// These values separate input collection from movement and look behavior. They
/// can come from [`super::ControllerInput`] or be set directly by a script.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControllerActions {
    /// Right, up, and forward axes. Horizontal values use the supplied heading;
    /// up is world-aligned. Each axis is clamped to `-1..=1` when applied.
    pub movement: Vec3,
    /// Horizontal and vertical look displacement in physical pixels.
    pub look: Vec2,
    /// Applies the built-in `2.5` movement-speed multiplier when true.
    pub sprint: bool,
}

impl ControllerActions {
    /// Returns `2.5` when sprinting and `1.0` otherwise.
    pub fn speed_multiplier(self) -> f32 {
        if self.sprint { 2.5 } else { 1.0 }
    }

    /// Converts movement axes into a frame displacement.
    ///
    /// Forward follows local `-Z`, strafe follows the horizontal right axis, and
    /// elevation follows world `+Y`. Invalid input produces no displacement;
    /// finite frame durations are clamped to `0..=0.1` seconds.
    pub fn translation(self, rotation: Quat, speed: f32, delta_seconds: f32) -> Vec3 {
        if !self.movement.is_finite() || !rotation.is_finite() || !speed.is_finite() {
            return Vec3::ZERO;
        }
        let axes = self.movement.clamp(Vec3::NEG_ONE, Vec3::ONE);
        let right = (rotation * Vec3::X * Vec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
        let direction = rotation * Vec3::NEG_Z * axes.z + right * axes.x + Vec3::Y * axes.y;
        direction.normalize_or_zero()
            * axes.length().min(1.0)
            * speed
            * self.speed_multiplier()
            * frame_delta(delta_seconds)
    }

    /// Applies pixel-scaled look to a pose and returns an upright orientation.
    ///
    /// Horizontal displacement changes yaw; vertical displacement changes pitch,
    /// clamped to 89 degrees from the horizon. Non-finite look or sensitivity is ignored.
    pub fn look_rotation(self, pose: Mat4, sensitivity: f32) -> Quat {
        let look = self.look_delta(sensitivity);
        let forward = pose.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
        let pitch = (forward.y.clamp(-1.0, 1.0).asin() - look.y)
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        Quat::from_rotation_y(yaw(pose) - look.x) * Quat::from_rotation_x(pitch)
    }

    /// Applies horizontal look to a pose and returns an upright yaw rotation.
    ///
    /// Vertical look and the pose's pitch and roll are discarded.
    pub fn yaw_rotation(self, pose: Mat4, sensitivity: f32) -> Quat {
        Quat::from_rotation_y(yaw(pose) - self.look_delta(sensitivity).x)
    }

    fn look_delta(self, sensitivity: f32) -> Vec2 {
        if self.look.is_finite() && sensitivity.is_finite() {
            let delta = self.look * sensitivity;
            if delta.is_finite() {
                return delta;
            }
        }
        Vec2::ZERO
    }
}

fn yaw(pose: Mat4) -> f32 {
    let right = pose.transform_vector3(Vec3::X);
    // The right axis remains defined when the forward axis points vertically.
    if right.x * right.x + right.z * right.z > 1e-10 {
        (-right.z).atan2(right.x)
    } else {
        let forward = pose.transform_vector3(Vec3::NEG_Z);
        (-forward.x).atan2(-forward.z)
    }
}

pub(crate) fn frame_delta(delta_seconds: f32) -> f32 {
    if delta_seconds.is_finite() {
        delta_seconds.clamp(0.0, 0.1)
    } else {
        0.0
    }
}
