use std::ops::{Deref, DerefMut};

use crate::{Color3, Face, InstanceData};

/// Properties shared by all local light types.
///
/// Light values are sanitized by the renderer: invalid color channels become
/// zero, brightness is nonnegative, and angles and ranges are clamped to valid
/// values.
#[derive(Clone, Debug)]
pub struct LightProperties {
    pub(crate) instance: InstanceData,
    /// RGB color emitted by the light.
    pub color: Color3,
    /// Intensity multiplier for the emitted light.
    pub brightness: f32,
    /// Whether this light may use the renderer's bounded local-shadow budget.
    pub shadows: bool,
}

impl LightProperties {
    fn new(name: impl Into<String>) -> Self {
        Self {
            instance: InstanceData::new(name),
            color: Color3::WHITE,
            brightness: 1.0,
            shadows: false,
        }
    }
}

/// An omnidirectional light positioned at the center of its nearest physical
/// ancestor.
///
/// A point light renders when it is a descendant of a [`crate::BasePart`],
/// [`crate::Part`], or `MeshPart` (when that feature is enabled).
#[derive(Clone, Debug)]
pub struct PointLight {
    light: LightProperties,
    /// Maximum distance in world units affected by this light.
    pub range: f32,
}

impl PointLight {
    /// Creates a white point light with brightness `1`, range `8`, and shadows
    /// disabled.
    pub fn new() -> Self {
        Self {
            light: LightProperties::new("PointLight"),
            range: 8.0,
        }
    }
}

impl Default for PointLight {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for PointLight {
    type Target = LightProperties;

    fn deref(&self) -> &Self::Target {
        &self.light
    }
}

impl DerefMut for PointLight {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.light
    }
}

crate::impl_instance!(PointLight, class_name = "PointLight", data = light.instance,);

/// A cone light emitted from one face of its nearest physical ancestor.
///
/// The renderer derives a finite influence distance from brightness so spot
/// lights remain bounded without an additional range property.
#[derive(Clone, Debug)]
pub struct SpotLight {
    light: LightProperties,
    /// Part face from which the cone emits.
    pub face: Face,
    /// Full cone angle in degrees, clamped to `0..=180` while rendering.
    pub angle: f32,
}

impl SpotLight {
    /// Creates a white front-facing spot light with brightness `1`, a
    /// 90-degree cone, and shadows disabled.
    ///
    /// The display name defaults to `"SpotLight"`; use
    /// [`Instance::named`](crate::Instance::named) to override it.
    pub fn new() -> Self {
        Self {
            light: LightProperties::new("SpotLight"),
            face: Face::Front,
            angle: 90.0,
        }
    }
}

impl Default for SpotLight {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for SpotLight {
    type Target = LightProperties;

    fn deref(&self) -> &Self::Target {
        &self.light
    }
}

impl DerefMut for SpotLight {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.light
    }
}

crate::impl_instance!(SpotLight, class_name = "SpotLight", data = light.instance,);

/// An area light emitted across one face of its nearest physical ancestor.
///
/// Surface lights use the ancestor's face dimensions while shading, producing
/// softer distance falloff than a point-like spot light without extra draws.
#[derive(Clone, Debug)]
pub struct SurfaceLight {
    light: LightProperties,
    /// Part face that acts as the emitting surface.
    pub face: Face,
    /// Full emission angle in degrees, clamped to `0..=180` while rendering.
    pub angle: f32,
}

impl SurfaceLight {
    /// Creates a white front-facing surface light with brightness `1`, a
    /// 90-degree emission angle, and shadows disabled.
    ///
    /// The display name defaults to `"SurfaceLight"`; use
    /// [`Instance::named`](crate::Instance::named) to override it.
    pub fn new() -> Self {
        Self {
            light: LightProperties::new("SurfaceLight"),
            face: Face::Front,
            angle: 90.0,
        }
    }
}

impl Default for SurfaceLight {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for SurfaceLight {
    type Target = LightProperties;

    fn deref(&self) -> &Self::Target {
        &self.light
    }
}

impl DerefMut for SurfaceLight {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.light
    }
}

crate::impl_instance!(
    SurfaceLight,
    class_name = "SurfaceLight",
    data = light.instance,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasePart, Instance};

    #[test]
    fn local_lights_are_instances_with_shared_editable_properties() {
        fn assert_instance<T: Instance>() {}

        assert_instance::<PointLight>();
        assert_instance::<SpotLight>();
        assert_instance::<SurfaceLight>();

        let mut parent = BasePart::new().named("fixture");
        let light = parent.add_child_with_ref(PointLight::new().named("bulb"), |light| {
            light.color = Color3::new(1.0, 0.5, 0.25);
            light.brightness = 3.0;
            light.shadows = true;
            light.range = 12.0;
        });
        assert_eq!(light.name(), "bulb");
        assert_eq!(light.color, Color3::new(1.0, 0.5, 0.25));
        assert_eq!(light.brightness, 3.0);
        assert!(light.shadows);
        assert_eq!(light.range, 12.0);
    }

    #[test]
    fn directional_lights_default_to_front_facing_ninety_degree_cones() {
        let spot = SpotLight::default();
        let surface = SurfaceLight::default();

        assert_eq!(spot.face, Face::Front);
        assert_eq!(spot.angle, 90.0);
        assert_eq!(surface.face, Face::Front);
        assert_eq!(surface.angle, 90.0);
    }
}
