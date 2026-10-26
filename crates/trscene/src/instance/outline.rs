use crate::*;

instance!({
    #[instance(default = false, inspect = false)]
    /// A render-only instance that outlines its parent and the parent's siblings.
    ///
    /// An outline is attached to the instance tree just like any other child, but
    /// it does not render geometry itself. For example, adding an outline child to
    /// a part outlines that part and the other renderable children of the part's
    /// parent. Multiple outline instances may select different groups and modes.
    #[derive(Clone, Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct Outline {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[data]
        instance: InstanceData,
        /// Rendering technique used by the outline.
        #[property]
        mode: OutlineMode,
        /// Color of the outline.
        #[property]
        color: Color3,
        /// Screen-space line width in pixels.
        #[property]
        width: f32,
        /// Edge threshold used by toon and silhouette detection.
        #[property]
        threshold: f32,
        /// Whether the outline renders through occluding geometry.
        #[property]
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
    }
});

#[cfg(test)]
mod tests {
    use super::*;

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
