use trscene::{BasePart, Part as Primitive, Workspace, glam::Mat4 as Transform, *};

instance!({
    /// A custom type built through a renamed runtime dependency and parent import.
    #[derive(Clone, Debug)]
    pub struct RenamedPart {
        #[extends]
        primitive: Primitive,
        /// Application-owned metadata.
        #[property(default = 7)]
        tag: u32,
    }

    impl RenamedPart {
        #[r#override(PVInstance::set_pivot)]
        fn set_pivot(&mut self, mut pivot: Transform) {
            pivot.w_axis.y = 0.0;
            self.pv_instance_mut().set_pivot(pivot);
        }
    }
});

mod models {
    use super::*;

    instance!({
        #[instance(path = crate::models)]
        /// A descendant whose module does not import its parent's argument types.
        #[derive(Clone, Debug)]
        pub struct DerivedPart {
            #[extends]
            part: RenamedPart,
        }
    });
}

#[test]
fn renamed_dependencies_and_parent_imports_preserve_composed_behavior() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(
        models::DerivedPart::default()
            .with_tag(12)
            .with_transparency(0.4)
            .with_position(glam::Vec3::Y),
    );
    let part = workspace.view::<BasePart>(id).unwrap();
    assert_eq!(part.id(), id);
    assert_eq!(part.class_name(), "DerivedPart");
    assert_eq!(part.transparency(), 0.4);
    assert_eq!(part.position(), glam::Vec3::ZERO);
    assert_eq!(workspace.view::<RenamedPart>(id).unwrap().tag(), 12);
}
