/// An RGB color used by a [`Part`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Default for Color3 {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

impl Color3 {
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn rgba(self) -> [f32; 4] {
        [self.r, self.g, self.b, 1.0]
    }
}
