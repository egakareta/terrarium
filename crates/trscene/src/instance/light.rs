use crate::{Color3, Face, InstanceData};

/// Properties shared by all local light types.
///
/// Light values are sanitized by the renderer: invalid color channels become
/// zero, brightness is nonnegative, and angles and ranges are clamped to valid
/// values.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct LightProperties {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    pub(crate) instance: InstanceData,
    color: Color3,
    brightness: f32,
    shadows: bool,
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

/// Access to the underlying [`LightProperties`].
pub trait HasLight {
    /// Returns shared access to the underlying [`LightProperties`].
    fn light(&self) -> &LightProperties;

    /// Returns mutable access to the underlying [`LightProperties`].
    fn light_mut(&mut self) -> &mut LightProperties;

    crate::__instance_property! {
        self;
        /// RGB color emitted by the light. Invalid color channels are
        /// sanitized by the renderer.
        getter color() -> Color3 { self.light().color }
        setter set_color(color: Color3) {
            self.light_mut().color = color;
        }
        builder with_color;
    }

    crate::__instance_property! {
        self;
        /// Intensity multiplier for the emitted light. Negative brightness is
        /// sanitized by the renderer.
        getter brightness() -> f32 { self.light().brightness }
        setter set_brightness(brightness: f32) {
            self.light_mut().brightness = brightness;
        }
        builder with_brightness;
    }

    crate::__instance_property! {
        self;
        /// Whether this light may use the renderer's bounded local-shadow budget.
        getter shadows() -> bool { self.light().shadows }
        setter set_shadows(shadows: bool) {
            self.light_mut().shadows = shadows;
        }
        builder with_shadows;
    }
}

impl<T: HasLight + ?Sized> HasLight for &mut T {
    fn light(&self) -> &LightProperties {
        (**self).light()
    }

    fn light_mut(&mut self) -> &mut LightProperties {
        (**self).light_mut()
    }
}

/// Access to a point light's type-specific properties.
pub trait HasPointLight {
    /// Returns shared access to the underlying [`PointLight`].
    fn point_light(&self) -> &PointLight;

    /// Returns mutable access to the underlying [`PointLight`].
    fn point_light_mut(&mut self) -> &mut PointLight;

    crate::__instance_property! {
        self;
        /// Maximum distance in world units affected by the light. The
        /// renderer clamps this to its supported range.
        getter range() -> f32 { self.point_light().range }
        setter set_range(range: f32) {
            self.point_light_mut().range = range;
        }
        builder with_range;
    }
}

impl<T: HasPointLight + ?Sized> HasPointLight for &mut T {
    fn point_light(&self) -> &PointLight {
        (**self).point_light()
    }

    fn point_light_mut(&mut self) -> &mut PointLight {
        (**self).point_light_mut()
    }
}

/// Access to a spot light's type-specific properties.
pub trait HasSpotLight {
    /// Returns shared access to the underlying [`SpotLight`].
    fn spot_light(&self) -> &SpotLight;

    /// Returns mutable access to the underlying [`SpotLight`].
    fn spot_light_mut(&mut self) -> &mut SpotLight;

    crate::__instance_property! {
        self;
        /// Part face from which the cone emits.
        getter face() -> Face { self.spot_light().face }
        setter set_face(face: Face) {
            self.spot_light_mut().face = face;
        }
        builder with_face;
    }

    crate::__instance_property! {
        self;
        /// Full cone angle in degrees. The renderer clamps this to `0..=180`.
        getter angle() -> f32 { self.spot_light().angle }
        setter set_angle(angle: f32) {
            self.spot_light_mut().angle = angle;
        }
        builder with_angle;
    }
}

impl<T: HasSpotLight + ?Sized> HasSpotLight for &mut T {
    fn spot_light(&self) -> &SpotLight {
        (**self).spot_light()
    }

    fn spot_light_mut(&mut self) -> &mut SpotLight {
        (**self).spot_light_mut()
    }
}

/// Access to a surface light's type-specific properties.
pub trait HasSurfaceLight {
    /// Returns shared access to the underlying [`SurfaceLight`].
    fn surface_light(&self) -> &SurfaceLight;

    /// Returns mutable access to the underlying [`SurfaceLight`].
    fn surface_light_mut(&mut self) -> &mut SurfaceLight;

    crate::__instance_property! {
        self;
        /// Part face that acts as the emitting surface.
        getter face() -> Face { self.surface_light().face }
        setter set_face(face: Face) {
            self.surface_light_mut().face = face;
        }
        builder with_face;
    }

    crate::__instance_property! {
        self;
        /// Full emission angle in degrees. The renderer clamps this to `0..=180`.
        getter angle() -> f32 { self.surface_light().angle }
        setter set_angle(angle: f32) {
            self.surface_light_mut().angle = angle;
        }
        builder with_angle;
    }
}

impl<T: HasSurfaceLight + ?Sized> HasSurfaceLight for &mut T {
    fn surface_light(&self) -> &SurfaceLight {
        (**self).surface_light()
    }

    fn surface_light_mut(&mut self) -> &mut SurfaceLight {
        (**self).surface_light_mut()
    }
}

/// An omnidirectional light positioned at the center of its nearest physical
/// ancestor.
///
/// A point light renders when it is a descendant of a [`crate::BasePart`],
/// [`crate::Part`], or `MeshPart` (when that feature is enabled).
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct PointLight {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    light: LightProperties,
    range: f32,
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

impl HasLight for PointLight {
    fn light(&self) -> &LightProperties {
        &self.light
    }

    fn light_mut(&mut self) -> &mut LightProperties {
        &mut self.light
    }
}

impl HasPointLight for PointLight {
    fn point_light(&self) -> &PointLight {
        self
    }

    fn point_light_mut(&mut self) -> &mut PointLight {
        self
    }
}

crate::impl_instance!(
    PointLight,
    class_name = "PointLight",
    data = light.instance,
    inspect = PointLight,
);

/// A cone light emitted from one face of its nearest physical ancestor.
///
/// The renderer derives a finite influence distance from brightness so spot
/// lights remain bounded without an additional range property.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct SpotLight {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    light: LightProperties,
    face: Face,
    angle: f32,
}

impl SpotLight {
    /// Creates a white front-facing spot light with brightness `1`, a
    /// 90-degree cone, and shadows disabled.
    ///
    /// The display name defaults to `"SpotLight"`; use
    /// [`Instance::with_name`](crate::Instance::with_name) to override it.
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

impl HasLight for SpotLight {
    fn light(&self) -> &LightProperties {
        &self.light
    }

    fn light_mut(&mut self) -> &mut LightProperties {
        &mut self.light
    }
}

impl HasSpotLight for SpotLight {
    fn spot_light(&self) -> &SpotLight {
        self
    }

    fn spot_light_mut(&mut self) -> &mut SpotLight {
        self
    }
}

crate::impl_instance!(
    SpotLight,
    class_name = "SpotLight",
    data = light.instance,
    inspect = SpotLight,
);

/// An area light emitted across one face of its nearest physical ancestor.
///
/// Surface lights use the ancestor's face dimensions while shading, producing
/// softer distance falloff than a point-like spot light without extra draws.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct SurfaceLight {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    light: LightProperties,
    face: Face,
    angle: f32,
}

impl SurfaceLight {
    /// Creates a white front-facing surface light with brightness `1`, a
    /// 90-degree emission angle, and shadows disabled.
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

impl HasLight for SurfaceLight {
    fn light(&self) -> &LightProperties {
        &self.light
    }

    fn light_mut(&mut self) -> &mut LightProperties {
        &mut self.light
    }
}

impl HasSurfaceLight for SurfaceLight {
    fn surface_light(&self) -> &SurfaceLight {
        self
    }

    fn surface_light_mut(&mut self) -> &mut SurfaceLight {
        self
    }
}

crate::impl_instance!(
    SurfaceLight,
    class_name = "SurfaceLight",
    data = light.instance,
    inspect = SurfaceLight,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasePart, HasLight, HasPointLight, HasSpotLight, HasSurfaceLight, Instance};

    #[test]
    fn local_lights_are_instances_with_shared_editable_properties() {
        fn assert_instance<T: Instance>() {}

        assert_instance::<PointLight>();
        assert_instance::<SpotLight>();
        assert_instance::<SurfaceLight>();

        let mut parent = BasePart::new().with_name("fixture");
        let light = parent.add_child_ref(
            PointLight::new()
                .with_color(Color3::new(1.0, 0.5, 0.25))
                .with_brightness(3.0)
                .with_shadows(true)
                .with_range(12.0)
                .with_name("bulb"),
        );
        assert_eq!(light.name(), "bulb");
        assert_eq!(light.color(), Color3::new(1.0, 0.5, 0.25));
        assert_eq!(light.brightness(), 3.0);
        assert!(light.shadows());
        assert_eq!(light.range(), 12.0);
    }

    #[test]
    fn directional_lights_default_to_front_facing_ninety_degree_cones() {
        let mut spot = SpotLight::default();
        let mut surface = SurfaceLight::default();

        assert_eq!(spot.face(), Face::Front);
        assert_eq!(spot.angle(), 90.0);
        assert_eq!(surface.face(), Face::Front);
        assert_eq!(surface.angle(), 90.0);

        (&mut spot).with_face(Face::Back).with_angle(55.0);
        (&mut surface).with_face(Face::Bottom).with_angle(45.0);

        assert_eq!(spot.face(), Face::Back);
        assert_eq!(spot.angle(), 55.0);
        assert_eq!(surface.face(), Face::Bottom);
        assert_eq!(surface.angle(), 45.0);
    }
}
