use trdevtools::{Devtools, Inspect, PropertyError, PropertyValue};
use trscene::{
    BasePart, Color3, FrameInput, HasBasePart, HasLight, HasMaterials, HasPVInstance, HasPart,
    Instance, Modifiers, Part, PartShape, PointLight, Workspace, glam::Vec3,
};

#[derive(Clone, Debug, Inspect)]
struct CustomInstance {
    #[inspect(flatten)]
    base: BasePart,
    health: f32,
    #[inspect(readonly)]
    source: &'static str,
    #[inspect(skip)]
    runtime_counter: usize,
}

#[test]
fn f12_toggles_the_inspector_once_per_key_press() {
    let mut workspace = Workspace::new();
    let mut devtools = Devtools::default();
    let mut input = FrameInput::default();
    assert!(!devtools.is_visible());
    input.pressed.push("F12".into());
    devtools.process_input(&input, &mut workspace);
    assert!(devtools.is_visible());
    input.pressed.clear();
    devtools.process_input(&input, &mut workspace);
    assert!(devtools.is_visible());
    input.pressed.push("F12".into());
    devtools.process_input(&input, &mut workspace);
    assert!(!devtools.is_visible());
}

#[test]
fn selection_supports_toggle_ranges_and_additive_ranges() {
    let mut workspace = Workspace::new();
    let order: Vec<_> = (0..5).map(|_| workspace.add_child(Part::new())).collect();
    let mut devtools = Devtools::default();

    devtools.select(order[1], Modifiers::NONE, &order);
    devtools.select(order[3], Modifiers::SHIFT, &order);
    assert_eq!(devtools.selected(), &order[1..=3]);
    devtools.select(order[2], Modifiers::CTRL, &order);
    assert_eq!(devtools.selected(), &[order[1], order[3]]);
    devtools.select(order[4], Modifiers::CTRL | Modifiers::SHIFT, &order);
    assert_eq!(
        devtools.selected(),
        &[order[1], order[3], order[2], order[4]]
    );
    devtools.select(order[0], Modifiers::NONE, &order);
    assert_eq!(devtools.selected(), &[order[0]]);
}

#[test]
fn edits_common_properties_and_rejects_invalid_values_without_mutation() {
    let mut workspace = Workspace::new();
    let part = workspace.add_child(Part::new().with_color(Color3::BLACK));
    let light = workspace.add_child(PointLight::new());
    let mut devtools = Devtools::default();
    devtools.select(part, Modifiers::NONE, &[part, light]);
    devtools.select(light, Modifiers::CTRL, &[part, light]);

    let color = devtools
        .properties(&workspace)
        .into_iter()
        .find(|property| property.property.path == "color")
        .unwrap();
    assert!(color.mixed);
    assert!(
        !devtools
            .properties(&workspace)
            .iter()
            .any(|property| property.property.path == "shape")
    );

    let edited_color = Color3::new(0.2, 0.4, 0.6);
    devtools
        .edit_property(
            &mut workspace,
            "color",
            PropertyValue::Color([edited_color.r, edited_color.g, edited_color.b]),
        )
        .unwrap();
    assert_eq!(workspace.get::<Part>(part).unwrap().color(), edited_color);
    assert_eq!(
        workspace.get::<PointLight>(light).unwrap().color(),
        edited_color
    );
    assert!(
        !devtools
            .properties(&workspace)
            .into_iter()
            .find(|property| property.property.path == "color")
            .unwrap()
            .mixed
    );

    assert_eq!(
        devtools.edit_property(
            &mut workspace,
            "color",
            PropertyValue::Color([f32::NAN, 0.0, 0.0]),
        ),
        Err(PropertyError::InvalidValue)
    );
    assert_eq!(workspace.get::<Part>(part).unwrap().color(), edited_color);
    assert!(workspace.instance(part).is_some());
}

#[test]
fn nested_properties_edit_transforms_materials_and_attributes() {
    let mut workspace = Workspace::new();
    let parent = workspace.add_child(Part::new());
    let mut devtools = Devtools::default();
    devtools.select(parent, Modifiers::NONE, &[parent]);

    devtools
        .edit_property(
            &mut workspace,
            "position",
            PropertyValue::Vector(vec![1.0, 2.0, 3.0]),
        )
        .unwrap();
    let shape = devtools
        .properties(&workspace)
        .into_iter()
        .find(|property| property.property.path == "shape")
        .unwrap()
        .property
        .value;
    let PropertyValue::Choice { options, .. } = shape else {
        panic!("shape is an enum");
    };
    devtools
        .edit_property(
            &mut workspace,
            "shape",
            PropertyValue::Choice {
                selected: "Ball".into(),
                options,
            },
        )
        .unwrap();
    devtools
        .edit_property(
            &mut workspace,
            "material_slots.Top.roughness",
            PropertyValue::Number(0.8),
        )
        .unwrap();
    devtools
        .edit_property(
            &mut workspace,
            "attributes",
            PropertyValue::Json(r#"{"health":75}"#.into()),
        )
        .unwrap();

    let part = workspace.get::<Part>(parent).unwrap();
    assert_eq!(part.position(), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(part.shape(), PartShape::Ball);
    assert_eq!(part.material_slot(trscene::Face::Top).roughness(), 0.8);
    assert_eq!(part.get_attribute("health"), Some(&serde_json::json!(75)));
    assert_eq!(
        devtools.edit_property(
            &mut workspace,
            "attributes",
            PropertyValue::Json("{".into()),
        ),
        Err(PropertyError::InvalidValue)
    );
    assert_eq!(
        workspace
            .get::<Part>(parent)
            .unwrap()
            .get_attribute("health"),
        Some(&serde_json::json!(75))
    );
}

#[test]
fn setters_preserve_field_invariants_and_root_edits_are_reflected() {
    let mut workspace = Workspace::new();
    let part = workspace.add_child(Part::new());
    let root = workspace.id();
    let mut devtools = Devtools::default();

    devtools.select(part, Modifiers::NONE, &[root, part]);
    devtools
        .edit_property(&mut workspace, "transparency", PropertyValue::Number(2.0))
        .unwrap();
    assert_eq!(workspace.get::<Part>(part).unwrap().transparency(), 1.0);

    devtools.select(root, Modifiers::NONE, &[root, part]);
    let original_time = workspace.lighting.clock_time();
    assert_eq!(
        devtools.edit_property(
            &mut workspace,
            "lighting.clock_time",
            PropertyValue::Number(f32::NAN),
        ),
        Err(PropertyError::InvalidValue)
    );
    assert_eq!(workspace.lighting.clock_time(), original_time);
    devtools
        .edit_property(
            &mut workspace,
            "lighting.clock_time",
            PropertyValue::Number(30.0),
        )
        .unwrap();
    assert_eq!(workspace.lighting.clock_time(), 6.0);
}

#[test]
fn deleting_nested_selection_preserves_root_and_unselected_siblings() {
    let mut workspace = Workspace::new();
    let mut parent = Part::new();
    let child = parent.add_child(Part::new());
    let parent = workspace.add_child(parent);
    let sibling = workspace.add_child(Part::new());
    let root = workspace.id();
    let order = [root, parent, child, sibling];
    let mut devtools = Devtools::default();

    devtools.set_visible(true);
    devtools.select(root, Modifiers::NONE, &order);
    devtools.select(parent, Modifiers::CTRL, &order);
    devtools.select(child, Modifiers::CTRL, &order);
    devtools.process_input(
        &FrameInput {
            pressed: vec!["Delete".into()],
            ..Default::default()
        },
        &mut workspace,
    );

    assert!(workspace.instance(parent).is_none());
    assert!(workspace.instance(child).is_none());
    assert!(workspace.instance(sibling).is_some());
    assert_eq!(workspace.id(), root);
    assert_eq!(devtools.selected(), &[root]);
}

#[test]
fn derives_flatten_readonly_and_skip_field_behavior() {
    let mut instance = CustomInstance {
        base: BasePart::new(),
        health: 100.0,
        source: "asset",
        runtime_counter: 3,
    };

    instance
        .set_property("health", PropertyValue::Number(75.0))
        .unwrap();
    instance
        .set_property("name", PropertyValue::Text("custom".into()))
        .unwrap();
    assert_eq!(instance.health, 75.0);
    assert_eq!(instance.base.name(), "custom");
    assert_eq!(instance.runtime_counter, 3);
    assert_eq!(instance.source, "asset");
    assert_eq!(
        instance.set_property("source", PropertyValue::Text("replacement".into())),
        Err(PropertyError::NotFound)
    );
    assert!(
        !instance
            .properties()
            .iter()
            .any(|property| property.path == "runtime_counter")
    );
}
