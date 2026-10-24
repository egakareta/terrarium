use trscene::{glam::*, *};

instance! {
    /// A primitive with an additional authored spring property.
    #[derive(Clone, Debug)]
    pub struct SpringPart {
        #[extends]
        part: Part,
        /// Spring stiffness in newtons per meter.
        #[property(default = 20.0)]
        stiffness: f32,
    }
}

instance! {
    /// An assembly that retains the built-in rig's hierarchy-aware setters.
    #[derive(Clone, Debug)]
    pub struct SpringAssembly {
        #[extends]
        rig: Rig,
    }
}

fn paint<T: BasePartExt + ?Sized>(part: &mut T) {
    part.set_color(Color3::new(0.2, 0.6, 1.0));
}

fn main() {
    let mut part = SpringPart::default()
        .with_stiffness(40.0)
        .with_position(Vec3::X);
    paint(&mut part);

    let mut assembly = SpringAssembly::default();
    let part_id = assembly.add_child(part);
    let mut workspace = Workspace::new();
    let assembly_id = workspace.add_child(assembly);
    workspace
        .view_mut::<PVInstance>(assembly_id)
        .unwrap()
        .set_position(Vec3::new(5.0, 0.0, 0.0));
    workspace
        .view_mut::<BasePart>(part_id)
        .unwrap()
        .set_transparency(0.25);

    let spring = workspace.view::<SpringPart>(part_id).unwrap();
    assert_eq!(spring.position(), Vec3::new(6.0, 0.0, 0.0));
    assert_eq!(spring.id(), part_id);
    assert!(SpringPart::descriptor().is_a::<BasePart>());
    assert_eq!(workspace.views::<Part>().count(), 1);
    assert!(workspace.get::<Part>(part_id).is_none());
    println!(
        "{}: stiffness={}, position={}, transparency={}",
        spring.class_name(),
        spring.stiffness(),
        spring.position(),
        spring.transparency(),
    );
    assert!(
        workspace
            .render_scene()
            .nodes
            .iter()
            .flat_map(|node| &node.children)
            .any(|node| node.mesh.is_some())
    );
    println!("Custom primitive renders inside a custom rig; exact downcasts remain exact.");
}
