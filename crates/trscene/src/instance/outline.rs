use crate::{Color3, InstanceData, OutlineMode};

crate::instance! {
    #[instance(default = false, inspect = false, inherent = true)]
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
        #[data]
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

        /// Rendering technique used by the outline.
        #[property]
        pub fn mode(&self) -> OutlineMode {
            self.mode
        }
        /// Sets `mode`.
        pub fn set_mode(&mut self, mode: OutlineMode) {
            self.mode = mode;
        }
        /// Returns this value with `mode` set.
        pub fn with_mode(mut self, mode: OutlineMode) -> Self
        where
            Self: Sized,
        {
            self.set_mode(mode);
            self
        }

        /// Color of the outline.
        #[property]
        pub fn color(&self) -> Color3 {
            self.color
        }
        /// Sets `color`.
        pub fn set_color(&mut self, color: Color3) {
            self.color = color;
        }
        /// Returns this value with `color` set.
        pub fn with_color(mut self, color: Color3) -> Self
        where
            Self: Sized,
        {
            self.set_color(color);
            self
        }

        /// Screen-space line width in pixels.
        #[property]
        pub fn width(&self) -> f32 {
            self.width
        }
        /// Sets `width`.
        pub fn set_width(&mut self, width: f32) {
            self.width = width;
        }
        /// Returns this value with `width` set.
        pub fn with_width(mut self, width: f32) -> Self
        where
            Self: Sized,
        {
            self.set_width(width);
            self
        }

        /// Edge threshold used by toon and silhouette detection.
        #[property]
        pub fn threshold(&self) -> f32 {
            self.threshold
        }
        /// Sets `threshold`.
        pub fn set_threshold(&mut self, threshold: f32) {
            self.threshold = threshold;
        }
        /// Returns this value with `threshold` set.
        pub fn with_threshold(mut self, threshold: f32) -> Self
        where
            Self: Sized,
        {
            self.set_threshold(threshold);
            self
        }

        /// Whether the outline renders through occluding geometry.
        #[property]
        pub fn always_on_top(&self) -> bool {
            self.always_on_top
        }
        /// Sets `always_on_top`.
        pub fn set_always_on_top(&mut self, always_on_top: bool) {
            self.always_on_top = always_on_top;
        }
        /// Returns this value with `always_on_top` set.
        pub fn with_always_on_top(mut self, always_on_top: bool) -> Self
        where
            Self: Sized,
        {
            self.set_always_on_top(always_on_top);
            self
        }
    }
}

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
