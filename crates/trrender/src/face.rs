/// A direction on the six-sided local-space cube.
///
/// [`Top`](Self::Top) faces toward `+Y`,
/// [`Bottom`](Self::Bottom) toward `-Y`,
/// [`Front`](Self::Front) toward `+Z`,
/// [`Back`](Self::Back) toward `-Z`,
/// [`Right`](Self::Right) toward `+X`,
/// [`Left`](Self::Left) toward `-X`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Face {
    /// The face toward `+Y`.
    Top,
    /// The face toward `-Y`.
    Bottom,
    /// The face toward `+Z`.
    #[default]
    Front,
    /// The face toward `-Z`.
    Back,
    /// The face toward `-X`.
    Left,
    /// The face toward `+X`.
    Right,
}

impl Face {
    /// All faces in material-slot order.
    pub const ALL: [Self; 6] = [
        Self::Top,
        Self::Bottom,
        Self::Front,
        Self::Back,
        Self::Left,
        Self::Right,
    ];

    /// All faces in WebGPU cubemap-layer order.
    pub const ALL_CUBEMAP: [Self; 6] = [
        Self::Right,
        Self::Left,
        Self::Top,
        Self::Bottom,
        Self::Front,
        Self::Back,
    ];

    /// Returns the zero-based material-slot index for this face.
    pub const fn material_index(self) -> usize {
        match self {
            Self::Top => 0,
            Self::Bottom => 1,
            Self::Front => 2,
            Self::Back => 3,
            Self::Left => 4,
            Self::Right => 5,
        }
    }

    /// Returns the zero-based WebGPU cubemap-layer index for this face.
    pub const fn cubemap_index(self) -> usize {
        match self {
            Self::Right => 0,
            Self::Left => 1,
            Self::Top => 2,
            Self::Bottom => 3,
            Self::Front => 4,
            Self::Back => 5,
        }
    }

    /// Selects the face whose axis best matches a normal.
    ///
    /// The dominant axis of `normal` wins: `|y|` beats `|x|` beats `|z|` on
    /// ties, so an up-facing slope maps to [`Top`](Self::Top) rather than a
    /// side. A zero normal maps to [`Top`](Self::Top).
    pub fn from_normal(normal: [f32; 3]) -> Self {
        let [x, y, z] = normal;
        let ax = x.abs();
        let ay = y.abs();
        let az = z.abs();
        if ax == 0.0 && ay == 0.0 && az == 0.0 {
            return Self::Top;
        }
        if ay >= ax && ay >= az {
            if y >= 0.0 { Self::Top } else { Self::Bottom }
        } else if ax >= az {
            if x >= 0.0 { Self::Right } else { Self::Left }
        } else if z >= 0.0 {
            Self::Front
        } else {
            Self::Back
        }
    }
}
