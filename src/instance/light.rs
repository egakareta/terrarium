use crate::{Color3, Face, InstanceData};

/// Properties shared by all local light types.
///
/// Light values are sanitized by the renderer: invalid color channels become
/// zero, brightness is nonnegative, and angles and ranges are clamped to valid
/// values.
#[derive(Clone, Debug)]
pub struct LightProperties {
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

    /// Returns the RGB color emitted by the light.
    fn color(&self) -> Color3 {
        self.light().color
    }

    /// Returns the intensity multiplier for the emitted light.
    fn brightness(&self) -> f32 {
        self.light().brightness
    }

    /// Returns whether this light may use the renderer's bounded local-shadow
    /// budget.
    fn shadows(&self) -> bool {
        self.light().shadows
    }

    /// Sets the RGB color emitted by the light.
    fn with_color(mut self, color: Color3) -> Self
    where
        Self: Sized,
    {
        self.light_mut().color = color;
        self
    }

    /// Sets the intensity multiplier for the emitted light.
    fn with_brightness(mut self, brightness: f32) -> Self
    where
        Self: Sized,
    {
        self.light_mut().brightness = brightness;
        self
    }

    /// Sets whether this light may use the renderer's bounded local-shadow budget.
    fn with_shadows(mut self, shadows: bool) -> Self
    where
        Self: Sized,
    {
        self.light_mut().shadows = shadows;
        self
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

    /// Returns the maximum distance in world units affected by this light.
    fn range(&self) -> f32 {
        self.point_light().range
    }

    /// Sets the maximum distance in world units affected by this light.
    fn with_range(mut self, range: f32) -> Self
    where
        Self: Sized,
    {
        self.point_light_mut().range = range;
        self
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

    /// Returns the part face from which the cone emits.
    fn face(&self) -> Face {
        self.spot_light().face
    }

    /// Returns the full cone angle in degrees.
    fn angle(&self) -> f32 {
        self.spot_light().angle
    }

    /// Sets the part face from which the cone emits.
    fn with_face(mut self, face: Face) -> Self
    where
        Self: Sized,
    {
        self.spot_light_mut().face = face;
        self
    }

    /// Sets the full cone angle in degrees.
    fn with_angle(mut self, angle: f32) -> Self
    where
        Self: Sized,
    {
        self.spot_light_mut().angle = angle;
        self
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

    /// Returns the part face that acts as the emitting surface.
    fn face(&self) -> Face {
        self.surface_light().face
    }

    /// Returns the full emission angle in degrees.
    fn angle(&self) -> f32 {
        self.surface_light().angle
    }

    /// Sets the part face that acts as the emitting surface.
    fn with_face(mut self, face: Face) -> Self
    where
        Self: Sized,
    {
        self.surface_light_mut().face = face;
        self
    }

    /// Sets the full emission angle in degrees.
    fn with_angle(mut self, angle: f32) -> Self
    where
        Self: Sized,
    {
        self.surface_light_mut().angle = angle;
        self
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
pub struct PointLight {
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

crate::impl_instance!(PointLight, class_name = "PointLight", data = light.instance,);

/// A cone light emitted from one face of its nearest physical ancestor.
///
/// The renderer derives a finite influence distance from brightness so spot
/// lights remain bounded without an additional range property.
#[derive(Clone, Debug)]
pub struct SpotLight {
    light: LightProperties,
    /// Part face from which the cone emits.
    face: Face,
    /// Full cone angle in degrees, clamped to `0..=180` while rendering.
    angle: f32,
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

crate::impl_instance!(SpotLight, class_name = "SpotLight", data = light.instance,);

/// An area light emitted across one face of its nearest physical ancestor.
///
/// Surface lights use the ancestor's face dimensions while shading, producing
/// softer distance falloff than a point-like spot light without extra draws.
#[derive(Clone, Debug)]
pub struct SurfaceLight {
    light: LightProperties,
    /// Part face that acts as the emitting surface.
    face: Face,
    /// Full emission angle in degrees, clamped to `0..=180` while rendering.
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

        let mut parent = BasePart::new().named("fixture");
        let light = parent.add_child_ref(
            PointLight::new()
                .with_color(Color3::new(1.0, 0.5, 0.25))
                .with_brightness(3.0)
                .with_shadows(true)
                .with_range(12.0)
                .named("bulb"),
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
