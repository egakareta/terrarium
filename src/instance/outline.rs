use crate::{Color3, InstanceData};

/// The rendering technique used by an [`Outline`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutlineMode {
    /// Uses a hard screen-space edge over the selected instance group.
    #[default]
    Toon,
    /// Uses the selected instance group's rendered mask to trace its silhouette.
    Silhouette,
    /// Uses the GPU stencil buffer and an expanded back-face pass.
    Stencil,
}

impl OutlineMode {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Toon => 0,
            Self::Silhouette => 1,
            Self::Stencil => 2,
        }
    }
}

/// A render-only instance that outlines its parent and the parent's siblings.
///
/// An outline is attached to the instance tree just like any other child, but
/// it does not render geometry itself. For example, adding an outline child to
/// a part outlines that part and the other renderable children of the part's
/// parent. Multiple outline instances may select different groups and modes.
#[derive(Clone, Debug)]
pub struct Outline {
    instance: InstanceData,
    mode: OutlineMode,
    color: Color3,
    width: f32,
    threshold: f32,
}

impl Outline {
    /// Creates an outline using `mode`, with a black one-pixel line.
    pub fn new(mode: OutlineMode) -> Self {
        Self {
            instance: InstanceData::new("Outline"),
            mode,
            color: Color3::BLACK,
            width: 1.0,
            threshold: 0.08,
        }
    }

    /// Creates a toon outline.
    pub fn toon() -> Self {
        Self::new(OutlineMode::Toon)
    }

    /// Creates a silhouette outline.
    pub fn silhouette() -> Self {
        Self::new(OutlineMode::Silhouette)
    }

    /// Creates a stencil-buffer outline.
    pub fn stencil() -> Self {
        Self::new(OutlineMode::Stencil)
    }

    /// Returns the outline technique.
    pub fn mode(&self) -> OutlineMode {
        self.mode
    }

    /// Returns the outline color.
    pub fn color(&self) -> Color3 {
        self.color
    }

    /// Returns the screen-space line width in pixels.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Returns the edge threshold used by toon and silhouette detection.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Sets the outline technique.
    pub fn with_mode(mut self, mode: OutlineMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the outline color.
    pub fn with_color(mut self, color: Color3) -> Self {
        self.color = color;
        self
    }

    /// Sets the screen-space line width in pixels.
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Sets the edge threshold used by toon and silhouette detection.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }
}

crate::impl_instance!(Outline, class_name = "Outline", data = instance,);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instance;

    #[test]
    fn convenience_constructors_select_each_outline_mode() {
        assert_eq!(Outline::toon().mode(), OutlineMode::Toon);
        assert_eq!(Outline::silhouette().mode(), OutlineMode::Silhouette);
        assert_eq!(Outline::stencil().mode(), OutlineMode::Stencil);
    }

    #[test]
    fn outline_style_builders_preserve_instance_identity_and_properties() {
        let outline = Outline::toon()
            .with_name("marker")
            .with_color(Color3::new(1.0, 0.25, 0.0))
            .with_width(3.0)
            .with_threshold(0.2);
        assert_eq!(outline.name(), "marker");
        assert_eq!(outline.color(), Color3::new(1.0, 0.25, 0.0));
        assert_eq!(outline.width(), 3.0);
        assert_eq!(outline.threshold(), 0.2);
        assert!(outline.id() != outline.clone().id());
    }
}
