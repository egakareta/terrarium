use crate::*;

instance!({
    #[instance(inspect = false)]
    /// Properties shared by all local light types.
    ///
    /// Light values are sanitized by the renderer: invalid color channels become
    /// zero, brightness is nonnegative, and angles and ranges are clamped to valid
    /// values.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct LightProperties {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[data]
        pub(crate) instance: InstanceData,
        /// RGB color emitted by the light. Invalid color channels are
        /// sanitized by the renderer.
        #[property(default = Color3::WHITE)]
        color: Color3,
        /// Intensity multiplier for the emitted light. Negative brightness is
        /// sanitized by the renderer.
        #[property(default = 1.0)]
        brightness: f32,
        /// Whether this light may use the renderer's bounded local-shadow budget.
        #[property]
        shadows: bool,
    }
});

instance!({
    #[instance(inspect = false)]
    /// An omnidirectional light positioned at the center of its nearest physical
    /// ancestor.
    ///
    /// A point light renders when it is a descendant of a [`crate::BasePart`],
    /// [`crate::Part`], or `MeshPart` (when that feature is enabled).
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct PointLight {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        light: LightProperties,
        /// Maximum distance in world units affected by the light. The
        /// renderer clamps this to its supported range.
        #[property(default = 8.0)]
        range: f32,
    }
});

impl PointLight {
    /// Creates a white point light with brightness `1`, range `8`, and shadows
    /// disabled.
    pub fn new() -> Self {
        Self::default()
    }
}

instance!({
    #[instance(inspect = false)]
    /// A cone light emitted from one face of its nearest physical ancestor.
    ///
    /// The renderer derives a finite influence distance from brightness so spot
    /// lights remain bounded without an additional range property.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct SpotLight {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        light: LightProperties,
        /// Part face from which the cone emits.
        #[property(default = Face::Front)]
        face: Face,
        /// Full cone angle in degrees. The renderer clamps this to `0..=180`.
        #[property(default = 90.0)]
        angle: f32,
    }
});

impl SpotLight {
    /// Creates a white front-facing spot light with brightness `1`, a
    /// 90-degree cone, and shadows disabled.
    ///
    /// The display name defaults to `"SpotLight"`; use
    /// [`Instance::with_name`](crate::Instance::with_name) to override it.
    pub fn new() -> Self {
        Self::default()
    }
}

instance!({
    #[instance(inspect = false)]
    /// An area light emitted across one face of its nearest physical ancestor.
    ///
    /// Surface lights use the ancestor's face dimensions while shading, producing
    /// softer distance falloff than a point-like spot light without extra draws.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct SurfaceLight {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[extends]
        light: LightProperties,
        /// Face from which this light is emitted.
        #[property(default = Face::Front)]
        face: Face,
        /// Emission cone angle in degrees.
        #[property(default = 90.0)]
        angle: f32,
    }
});

impl SurfaceLight {
    /// Creates a white front-facing surface light with brightness `1`, a
    /// 90-degree emission angle, and shadows disabled.
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
