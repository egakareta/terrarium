/// An RGB color used by a [`Part`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color3 {
    /// Red channel, normally in the inclusive range `0.0..=1.0`.
    pub r: f32,
    /// Green channel, normally in the inclusive range `0.0..=1.0`.
    pub g: f32,
    /// Blue channel, normally in the inclusive range `0.0..=1.0`.
    pub b: f32,
}

impl Default for Color3 {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

impl Color3 {
    /// Opaque white.
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0);

    /// Creates an RGB color from three channel values.
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Converts this color to the renderer's opaque RGBA representation.
    pub fn rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, 1.0]
    }
}
