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
    /// Opaque black.
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0);

    /// Opaque white.
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0);

    /// Opaque red.
    pub const RED: Self = Self::new(1.0, 0.0, 0.0);

    /// Opaque green.
    pub const GREEN: Self = Self::new(0.0, 1.0, 0.0);

    /// Opaque blue.
    pub const BLUE: Self = Self::new(0.0, 0.0, 1.0);

    /// Opaque yellow.
    pub const YELLOW: Self = Self::new(1.0, 1.0, 0.0);

    /// Opaque cyan.
    pub const CYAN: Self = Self::new(0.0, 1.0, 1.0);

    /// Opaque magenta.
    pub const MAGENTA: Self = Self::new(1.0, 0.0, 1.0);

    /// Opaque 50% gray.
    pub const GRAY: Self = Self::new(0.5, 0.5, 0.5);

    /// Opaque orange.
    pub const ORANGE: Self = Self::new(1.0, 0.5, 0.0);

    /// Opaque purple.
    pub const PURPLE: Self = Self::new(0.5, 0.0, 0.5);

    /// Opaque pink.
    pub const PINK: Self = Self::new(1.0, 0.75, 0.8);

    /// Opaque brown.
    pub const BROWN: Self = Self::new(0.55, 0.27, 0.07);

    /// Opaque lime.
    pub const LIME: Self = Self::new(0.75, 1.0, 0.0);

    /// Opaque teal.
    pub const TEAL: Self = Self::new(0.0, 0.5, 0.5);

    /// Opaque navy blue.
    pub const NAVY: Self = Self::new(0.0, 0.0, 0.5);

    /// Opaque maroon.
    pub const MAROON: Self = Self::new(0.5, 0.0, 0.0);

    /// Opaque olive.
    pub const OLIVE: Self = Self::new(0.5, 0.5, 0.0);

    /// Opaque silver.
    pub const SILVER: Self = Self::new(0.75, 0.75, 0.75);

    /// Opaque gold.
    pub const GOLD: Self = Self::new(1.0, 0.84, 0.0);

    /// Opaque coral.
    pub const CORAL: Self = Self::new(1.0, 0.5, 0.31);

    /// Opaque salmon.
    pub const SALMON: Self = Self::new(1.0, 0.5, 0.4);

    /// Opaque tomato red.
    pub const TOMATO: Self = Self::new(1.0, 0.39, 0.28);

    /// Opaque violet.
    pub const VIOLET: Self = Self::new(0.56, 0.0, 1.0);

    /// Opaque indigo.
    pub const INDIGO: Self = Self::new(0.29, 0.0, 0.51);

    /// Opaque turquoise.
    pub const TURQUOISE: Self = Self::new(0.25, 0.88, 0.82);

    /// Opaque beige.
    pub const BEIGE: Self = Self::new(0.96, 0.96, 0.86);

    /// Opaque khaki.
    pub const KHAKI: Self = Self::new(0.94, 0.9, 0.55);

    /// Opaque lavender.
    pub const LAVENDER: Self = Self::new(0.9, 0.9, 0.98);

    /// Opaque mint.
    pub const MINT: Self = Self::new(0.6, 1.0, 0.75);

    /// Opaque peach.
    pub const PEACH: Self = Self::new(1.0, 0.85, 0.73);

    /// Opaque plum.
    pub const PLUM: Self = Self::new(0.87, 0.63, 0.87);

    /// Opaque crimson.
    pub const CRIMSON: Self = Self::new(0.86, 0.08, 0.24);

    /// Opaque scarlet.
    pub const SCARLET: Self = Self::new(1.0, 0.14, 0.0);

    /// Opaque emerald.
    pub const EMERALD: Self = Self::new(0.31, 0.78, 0.47);

    /// Opaque forest green.
    pub const FOREST: Self = Self::new(0.13, 0.55, 0.13);

    /// Opaque sky blue.
    pub const SKY_BLUE: Self = Self::new(0.53, 0.81, 0.92);

    /// Opaque royal blue.
    pub const ROYAL_BLUE: Self = Self::new(0.25, 0.41, 0.88);

    /// Opaque midnight blue.
    pub const MIDNIGHT_BLUE: Self = Self::new(0.1, 0.1, 0.44);

    /// Opaque charcoal.
    pub const CHARCOAL: Self = Self::new(0.21, 0.21, 0.21);

    /// Creates an RGB color from three channel values.
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Converts this color to the renderer's opaque RGBA representation.
    pub fn rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, 1.0]
    }
}
