use crate::{Color3, InstanceData, OutlineMode};

/// A render-only instance that outlines its parent and the parent's siblings.
///
/// An outline is attached to the instance tree just like any other child, but
/// it does not render geometry itself. For example, adding an outline child to
/// a part outlines that part and the other renderable children of the part's
/// parent. Multiple outline instances may select different groups and modes.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
pub struct Outline {
    #[cfg_attr(feature = "devtools", inspect(flatten))]
    instance: InstanceData,
    mode: OutlineMode,
    color: Color3,
    width: f32,
    threshold: f32,
    always_on_top: bool,
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
            always_on_top: false,
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

    crate::__instance_property! {
        self;
        /// Rendering technique used by the outline.
        pub getter mode() -> OutlineMode { self.mode }
        setter set_mode(mode: OutlineMode) {
            self.mode = mode;
        }
        builder with_mode;
    }

    crate::__instance_property! {
        self;
        /// Color of the outline.
        pub getter color() -> Color3 { self.color }
        setter set_color(color: Color3) {
            self.color = color;
        }
        builder with_color;
    }

    crate::__instance_property! {
        self;
        /// Screen-space line width in pixels.
        pub getter width() -> f32 { self.width }
        setter set_width(width: f32) {
            self.width = width;
        }
        builder with_width;
    }

    crate::__instance_property! {
        self;
        /// Edge threshold used by toon and silhouette detection.
        pub getter threshold() -> f32 { self.threshold }
        setter set_threshold(threshold: f32) {
            self.threshold = threshold;
        }
        builder with_threshold;
    }

    crate::__instance_property! {
        self;
        /// Whether the outline renders through occluding geometry.
        pub getter always_on_top() -> bool { self.always_on_top }
        setter set_always_on_top(always_on_top: bool) {
            self.always_on_top = always_on_top;
        }
        builder with_always_on_top;
    }
}

crate::impl_instance!(
    Outline,
    class_name = "Outline",
    data = instance,
    inspect = Outline,
);

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
            .with_threshold(0.2)
            .with_always_on_top(true);
        assert_eq!(outline.name(), "marker");
        assert_eq!(outline.color(), Color3::new(1.0, 0.25, 0.0));
        assert_eq!(outline.width(), 3.0);
        assert_eq!(outline.threshold(), 0.2);
        assert!(outline.always_on_top());
        assert!(outline.id() != outline.clone().id());
    }
}
