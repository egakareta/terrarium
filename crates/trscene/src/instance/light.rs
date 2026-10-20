use crate::{Color3, Face, InstanceData};

crate::instance! {
    #[instance(default = false, inspect = false)]
    /// Properties shared by all local light types.
    ///
    /// Light values are sanitized by the renderer: invalid color channels become
    /// zero, brightness is nonnegative, and angles and ranges are clamped to valid
    /// values.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct LightProperties {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[data]
        pub(crate) instance: InstanceData,
        color: Color3,
        brightness: f32,
        shadows: bool,
    }
    impl LightProperties {
        /// Returns shared access to the underlying [`LightProperties`].
        pub fn light(&self) -> &LightProperties {
            crate::Project::<LightProperties>::project(self)
        }

        /// Returns mutable access to the underlying [`LightProperties`].
        pub fn light_mut(&mut self) -> &mut LightProperties {
            crate::Project::<LightProperties>::project_mut(self)
        }

        /// RGB color emitted by the light. Invalid color channels are
        /// sanitized by the renderer.
        #[property]
        pub fn color(&self) -> Color3 {
            self.light().color
        }
        /// Sets `color`.
        pub fn set_color(&mut self, color: Color3) {
            self.light_mut().color = color;
        }
        /// Returns this value with `color` set.
        pub fn with_color(mut self, color: Color3) -> Self
        where
            Self: Sized,
        {
            self.set_color(color);
            self
        }

        /// Intensity multiplier for the emitted light. Negative brightness is
        /// sanitized by the renderer.
        #[property]
        pub fn brightness(&self) -> f32 {
            self.light().brightness
        }
        /// Sets `brightness`.
        pub fn set_brightness(&mut self, brightness: f32) {
            self.light_mut().brightness = brightness;
        }
        /// Returns this value with `brightness` set.
        pub fn with_brightness(mut self, brightness: f32) -> Self
        where
            Self: Sized,
        {
            self.set_brightness(brightness);
            self
        }

        /// Whether this light may use the renderer's bounded local-shadow budget.
        #[property]
        pub fn shadows(&self) -> bool {
            self.light().shadows
        }
        /// Sets `shadows`.
        pub fn set_shadows(&mut self, shadows: bool) {
            self.light_mut().shadows = shadows;
        }
        /// Returns this value with `shadows` set.
        pub fn with_shadows(mut self, shadows: bool) -> Self
        where
            Self: Sized,
        {
            self.set_shadows(shadows);
            self
        }
    }
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

crate::instance! {
    #[instance(default = false, inspect = false)]
    /// An omnidirectional light positioned at the center of its nearest physical
    /// ancestor.
    ///
    /// A point light renders when it is a descendant of a [`crate::BasePart`],
    /// [`crate::Part`], or `MeshPart` (when that feature is enabled).
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct PointLight {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        light: LightProperties,
        range: f32,
    }
    impl PointLight {
        /// Returns shared access to the underlying [`PointLight`].
        pub fn point_light(&self) -> &PointLight {
            crate::Project::<PointLight>::project(self)
        }

        /// Returns mutable access to the underlying [`PointLight`].
        pub fn point_light_mut(&mut self) -> &mut PointLight {
            crate::Project::<PointLight>::project_mut(self)
        }

        /// Maximum distance in world units affected by the light. The
        /// renderer clamps this to its supported range.
        #[property]
        pub fn range(&self) -> f32 {
            self.point_light().range
        }
        /// Sets `range`.
        pub fn set_range(&mut self, range: f32) {
            self.point_light_mut().range = range;
        }
        /// Returns this value with `range` set.
        pub fn with_range(mut self, range: f32) -> Self
        where
            Self: Sized,
        {
            self.set_range(range);
            self
        }
    }
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

crate::instance! {
    #[instance(default = false, inspect = false)]
    /// A cone light emitted from one face of its nearest physical ancestor.
    ///
    /// The renderer derives a finite influence distance from brightness so spot
    /// lights remain bounded without an additional range property.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct SpotLight {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        light: LightProperties,
        face: Face,
        angle: f32,
    }
    impl SpotLight {
        /// Returns shared access to the underlying [`SpotLight`].
        pub fn spot_light(&self) -> &SpotLight {
            crate::Project::<SpotLight>::project(self)
        }

        /// Returns mutable access to the underlying [`SpotLight`].
        pub fn spot_light_mut(&mut self) -> &mut SpotLight {
            crate::Project::<SpotLight>::project_mut(self)
        }

        /// Part face from which the cone emits.
        #[property]
        pub fn face(&self) -> Face {
            self.spot_light().face
        }
        /// Sets `face`.
        pub fn set_face(&mut self, face: Face) {
            self.spot_light_mut().face = face;
        }
        /// Returns this value with `face` set.
        pub fn with_face(mut self, face: Face) -> Self
        where
            Self: Sized,
        {
            self.set_face(face);
            self
        }

        /// Full cone angle in degrees. The renderer clamps this to `0..=180`.
        #[property]
        pub fn angle(&self) -> f32 {
            self.spot_light().angle
        }
        /// Sets `angle`.
        pub fn set_angle(&mut self, angle: f32) {
            self.spot_light_mut().angle = angle;
        }
        /// Returns this value with `angle` set.
        pub fn with_angle(mut self, angle: f32) -> Self
        where
            Self: Sized,
        {
            self.set_angle(angle);
            self
        }
    }
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

crate::instance! {
    #[instance(default = false, inspect = false)]
    /// An area light emitted across one face of its nearest physical ancestor.
    ///
    /// Surface lights use the ancestor's face dimensions while shading, producing
    /// softer distance falloff than a point-like spot light without extra draws.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct SurfaceLight {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        light: LightProperties,
        face: Face,
        angle: f32,
    }
    impl SurfaceLight {
        /// Returns shared access to the underlying [`SurfaceLight`].
        pub fn surface_light(&self) -> &SurfaceLight {
            crate::Project::<SurfaceLight>::project(self)
        }

        /// Returns mutable access to the underlying [`SurfaceLight`].
        pub fn surface_light_mut(&mut self) -> &mut SurfaceLight {
            crate::Project::<SurfaceLight>::project_mut(self)
        }

        /// Part face that acts as the emitting surface.
        #[property]
        pub fn face(&self) -> Face {
            self.surface_light().face
        }
        /// Sets `face`.
        pub fn set_face(&mut self, face: Face) {
            self.surface_light_mut().face = face;
        }
        /// Returns this value with `face` set.
        pub fn with_face(mut self, face: Face) -> Self
        where
            Self: Sized,
        {
            self.set_face(face);
            self
        }

        /// Full emission angle in degrees. The renderer clamps this to `0..=180`.
        #[property]
        pub fn angle(&self) -> f32 {
            self.surface_light().angle
        }
        /// Sets `angle`.
        pub fn set_angle(&mut self, angle: f32) {
            self.surface_light_mut().angle = angle;
        }
        /// Returns this value with `angle` set.
        pub fn with_angle(mut self, angle: f32) -> Self
        where
            Self: Sized,
        {
            self.set_angle(angle);
            self
        }
    }
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

/// Compatibility name for the generated LightProperties API.
pub use LightPropertiesExt as HasLight;
/// Compatibility name for the generated PointLight API.
pub use PointLightExt as HasPointLight;
/// Compatibility name for the generated SpotLight API.
pub use SpotLightExt as HasSpotLight;
/// Compatibility name for the generated SurfaceLight API.
pub use SurfaceLightExt as HasSurfaceLight;

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
