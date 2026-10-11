use crate::{Color3, Skybox, glam::Vec3};

/// The high-level lighting model used by a [`Lighting`] configuration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LightingStyle {
    /// Physically based lighting with directional shadows.
    #[default]
    Realistic,
    /// A softer model with less directional contrast.
    Soft,
}

/// Errors returned when parsing a time-of-day value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeOfDayError {
    /// The value was not in `HH:MM:SS` form or contained an invalid component.
    InvalidFormat,
}

impl std::fmt::Display for TimeOfDayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat => formatter.write_str("time of day must be HH:MM:SS"),
        }
    }
}

impl std::error::Error for TimeOfDayError {}

/// Scene-wide lighting settings.
///
/// Properties are automatically sanitized, keeping malformed configuration from
/// reaching WGSL.
#[derive(Clone, Debug)]
pub struct Lighting {
    /// Indoor/global ambient color.
    pub ambient: Color3,
    /// Multiplier for direct sunlight.
    pub brightness: f32,
    /// Tint applied to downward-facing surfaces.
    pub color_shift_bottom: Color3,
    /// Tint applied to upward-facing surfaces.
    pub color_shift_top: Color3,
    /// Diffuse contribution from the environment cubemap.
    pub environment_diffuse_scale: f32,
    /// Specular contribution from the environment cubemap.
    pub environment_specular_scale: f32,
    /// Exposure compensation in stops.
    pub exposure_compensation: f32,
    /// Fog color.
    pub fog_color: Color3,
    /// Distance at which fog starts.
    pub fog_start: f32,
    /// Distance at which fog reaches full strength.
    pub fog_end: f32,
    /// Geographic latitude used to tilt the sun path, in degrees.
    pub geographic_latitude: f32,
    /// Whether directional shadows are enabled.
    pub global_shadows: bool,
    /// The requested lighting style.
    pub lighting_style: LightingStyle,
    /// Outdoor ambient color.
    pub outdoor_ambient: Color3,
    /// Tint used for fully shadowed direct light.
    pub shadow_color: Color3,
    /// Directional shadow filter size from hard (`0`) to soft (`1`).
    pub shadow_softness: f32,
    skybox: Option<Skybox>,
    skybox_revision: u64,
    clock_time: f32,
    direction_override: Option<Vec3>,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            ambient: Color3::BLACK,
            brightness: 2.0,
            color_shift_bottom: Color3::BLACK,
            color_shift_top: Color3::BLACK,
            environment_diffuse_scale: 1.0,
            environment_specular_scale: 1.0,
            exposure_compensation: 0.0,
            fog_color: Color3::new(0.75, 0.75, 0.75),
            fog_start: 0.0,
            fog_end: 100_000.0,
            geographic_latitude: 0.0,
            global_shadows: true,
            lighting_style: LightingStyle::Realistic,
            outdoor_ambient: Color3::new(0.5, 0.5, 0.5),
            shadow_color: Color3::BLACK,
            shadow_softness: 0.2,
            #[cfg(feature = "default-skybox")]
            skybox: Some(Skybox::default()),
            #[cfg(not(feature = "default-skybox"))]
            skybox: None,
            #[cfg(feature = "default-skybox")]
            skybox_revision: 1,
            #[cfg(not(feature = "default-skybox"))]
            skybox_revision: 0,
            clock_time: 12.0,
            direction_override: None,
        }
    }
}

impl Lighting {
    /// Default direction toward the midday sun.
    pub const DEFAULT_SUN_DIRECTION: Vec3 = Vec3::new(-0.45, 0.85, 0.35);

    /// Creates lighting settings.
    ///
    /// Equivalent to [`Lighting::default()`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the lighting skybox, if one is set.
    ///
    /// New lighting configurations default to an embedded cross-layout
    /// skybox. When no skybox is set, the renderer clears to its clear color.
    pub fn skybox(&self) -> Option<&Skybox> {
        self.skybox.as_ref()
    }

    /// Replaces the lighting skybox.
    ///
    pub fn with_skybox(&mut self, skybox: Skybox) -> &mut Self {
        self.skybox = Some(skybox);
        self.skybox_revision = self.skybox_revision.wrapping_add(1);
        self
    }

    /// Removes the lighting skybox.
    pub fn clear_skybox(&mut self) {
        self.skybox = None;
        self.skybox_revision = self.skybox_revision.wrapping_add(1);
    }

    /// Returns the revision of the lighting skybox.
    pub fn skybox_revision(&self) -> u64 {
        self.skybox_revision
    }

    /// Returns the time of day in hours in `[0, 24)`.
    pub fn clock_time(&self) -> f32 {
        self.clock_time
    }

    /// Sets the time of day in hours and moves the sun along its path.
    ///
    pub fn with_clock_time(&mut self, hours: f32) -> &mut Self {
        self.clock_time = wrap_clock_time(hours);
        self.direction_override = None;
        self
    }

    /// Advances the time of day by `hours_per_second` for the elapsed real time.
    ///
    /// Non-finite inputs are ignored. Positive rates move time forward and
    /// negative rates move it backward.
    pub fn advance_clock_time(&mut self, delta_seconds: f32, hours_per_second: f32) -> &mut Self {
        if delta_seconds.is_finite() && hours_per_second.is_finite() {
            self.with_clock_time(self.clock_time + delta_seconds * hours_per_second);
        }
        self
    }

    /// Returns the time as a `HH:MM:SS` string.
    pub fn time_of_day(&self) -> String {
        let total_seconds = (self.clock_time * 3600.0).round() as u32 % (24 * 60 * 60);
        let hours = total_seconds / 3600;
        let minutes = total_seconds / 60 % 60;
        let seconds = total_seconds % 60;
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }

    /// Parses and sets a `HH:MM:SS` time.
    ///
    pub fn with_time_of_day(&mut self, time: &str) -> Result<&mut Self, TimeOfDayError> {
        let mut components = time.split(':');
        let hours = components
            .next()
            .and_then(|value| value.parse::<u32>().ok());
        let minutes = components
            .next()
            .and_then(|value| value.parse::<u32>().ok());
        let seconds = components
            .next()
            .and_then(|value| value.parse::<u32>().ok());
        if components.next().is_some()
            || hours.is_none()
            || minutes.is_none()
            || seconds.is_none()
            || minutes.unwrap_or(60) >= 60
            || seconds.unwrap_or(60) >= 60
            || hours.unwrap_or(24) >= 24
        {
            return Err(TimeOfDayError::InvalidFormat);
        }
        self.with_clock_time(
            hours.unwrap() as f32
                + minutes.unwrap() as f32 / 60.0
                + seconds.unwrap() as f32 / 3600.0,
        );
        Ok(self)
    }

    /// Returns minutes elapsed since midnight.
    pub fn get_minutes_after_midnight(&self) -> f32 {
        self.clock_time * 60.0
    }

    /// Sets the time using minutes elapsed since midnight.
    ///
    pub fn with_minutes_after_midnight(&mut self, minutes: f32) -> &mut Self {
        self.with_clock_time(minutes / 60.0);
        self
    }

    /// Returns the unit vector pointing from the scene toward the sun.
    pub fn sun_direction(&self) -> Vec3 {
        self.direction_override
            .unwrap_or_else(|| self.sun_direction_from_clock_time_and_latitude())
    }

    /// Returns the direction of the sun.
    pub fn get_sun_direction(&self) -> Vec3 {
        self.sun_direction()
    }

    /// Sets an explicit sun direction and updates `ClockTime` to its nearest
    /// matching hour.
    ///
    pub fn with_sun_direction(&mut self, direction: Vec3) -> &mut Self {
        let direction = sanitize_direction(direction);
        self.direction_override = Some(direction);
        self.clock_time = nearest_clock_time(direction);
        self
    }

    /// Returns the direction of the moon, opposite the sun direction.
    pub fn moon_direction(&self) -> Vec3 {
        -self.sun_direction()
    }

    /// Returns the direction of the moon.
    pub fn get_moon_direction(&self) -> Vec3 {
        self.moon_direction()
    }

    /// Returns an approximate normalized lunar phase in `[0, 1)`.
    ///
    /// Terrarium has no calendar service, so this uses the current day cycle
    /// as a deterministic phase rather than claiming a real astronomical date.
    pub fn get_moon_phase(&self) -> f32 {
        (self.clock_time / 24.0 + 0.5).rem_euclid(1.0)
    }

    /// Returns the sun direction for a time using the default path tilt.
    pub fn sun_direction_from_clock_time(hours: f32) -> Vec3 {
        base_sun_direction(wrap_clock_time(hours))
    }

    /// Returns the sun direction for the current clock and latitude.
    pub(crate) fn render_sun_direction(&self) -> Vec3 {
        self.sun_direction()
    }

    /// Returns the fraction of direct sunlight visible above the horizon.
    pub(crate) fn daylight_factor(&self) -> f32 {
        let height = self.sun_direction().y;
        let normalized = ((height + 0.12) / 0.42).clamp(0.0, 1.0);
        normalized * normalized * (3.0 - 2.0 * normalized)
    }

    /// Returns whether the configured renderer should sample directional
    /// shadows.
    pub(crate) fn shadows_enabled(&self) -> bool {
        self.global_shadows
    }

    fn sun_direction_from_clock_time_and_latitude(&self) -> Vec3 {
        let base = base_sun_direction(self.clock_time);
        let latitude = if self.geographic_latitude.is_finite() {
            self.geographic_latitude.clamp(-90.0, 90.0).to_radians()
        } else {
            0.0
        };
        let (sin, cos) = latitude.sin_cos();
        sanitize_direction(Vec3::new(
            base.x,
            base.y * cos - base.z * sin,
            base.y * sin + base.z * cos,
        ))
    }
}

fn wrap_clock_time(hours: f32) -> f32 {
    if hours.is_finite() {
        hours.rem_euclid(24.0)
    } else {
        12.0
    }
}

fn base_sun_direction(hours: f32) -> Vec3 {
    let angle = (hours - 12.0) / 12.0 * std::f32::consts::PI;
    let noon = Lighting::DEFAULT_SUN_DIRECTION
        .try_normalize()
        .unwrap_or(Vec3::Y);
    sanitize_direction(noon * angle.cos() - Vec3::X * angle.sin())
}

fn sanitize_direction(direction: Vec3) -> Vec3 {
    if direction.is_finite() {
        direction.try_normalize().unwrap_or(Vec3::Y)
    } else {
        Vec3::Y
    }
}

fn nearest_clock_time(direction: Vec3) -> f32 {
    let direction = sanitize_direction(direction);
    let mut best_hour = 12.0;
    let mut best_dot = f32::NEG_INFINITY;
    for minutes in 0..(24 * 60) {
        let hours = minutes as f32 / 60.0;
        let dot = base_sun_direction(hours).dot(direction);
        if dot > best_dot {
            best_dot = dot;
            best_hour = hours;
        }
    }
    best_hour
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_of_day_and_minutes_round_trip() {
        let mut lighting = Lighting::default();
        assert_eq!(lighting.time_of_day(), "12:00:00");

        lighting.with_time_of_day("23:15:30").unwrap();
        assert!((lighting.clock_time() - 23.258333).abs() < 0.00001);
        assert!((lighting.get_minutes_after_midnight() - 1395.5).abs() < 0.0001);
        assert_eq!(lighting.time_of_day(), "23:15:30");

        lighting.with_minutes_after_midnight(90.0);
        assert_eq!(lighting.time_of_day(), "01:30:00");
        assert_eq!(
            lighting.with_time_of_day("not-a-time").map(|_| ()),
            Err(TimeOfDayError::InvalidFormat)
        );

        lighting.with_clock_time(23.5);
        lighting.advance_clock_time(4.0, 0.25);
        assert!((lighting.clock_time() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn night_removes_direct_sunlight_and_latitude_tilts_the_path() {
        let mut lighting = Lighting::default();
        lighting.with_clock_time(0.0);
        assert_eq!(lighting.daylight_factor(), 0.0);
        assert!(lighting.sun_direction().y < 0.0);

        lighting.with_clock_time(12.0);
        let equator_noon = lighting.sun_direction();
        assert!(lighting.daylight_factor() > 0.99);

        lighting.geographic_latitude = 45.0;
        lighting.with_clock_time(12.0);
        assert!((lighting.sun_direction() - equator_noon).length() > 0.01);
    }
}
