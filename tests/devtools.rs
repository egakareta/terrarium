use terrarium::{
    Instance, Part, Workspace,
    devtools::{Devtools, PropertyValue},
    egui::Modifiers,
};

#[test]
fn facade_exposes_the_standalone_inspector_crate() {
    let mut workspace = Workspace::new();
    let part = workspace.add_child(Part::new());
    let mut devtools = Devtools::default();

    devtools.select(part, Modifiers::NONE, &[part]);
    devtools
        .edit_property(&mut workspace, "name", PropertyValue::Text("edited".into()))
        .unwrap();

    assert_eq!(workspace.get::<Part>(part).unwrap().name(), "edited");
}
