use trdevtools::{Devtools, DevtoolsHost, Inspect, PropertyError, PropertyValue};
use trscene::{
    BasePart, Color3, HasBasePart, HasLight, HasMaterials, HasPVInstance, HasPart, Instance,
    InstanceId, Part, PartShape, PointLight, Workspace,
    egui::{self, Event, Key, Modifiers},
    glam::Vec3,
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

struct TestDevtoolsHost {
    workspace: Workspace,
}

impl DevtoolsHost for TestDevtoolsHost {
    fn scene_title(&self) -> &str {
        "Sample Application"
    }

    fn workspace(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    fn render_scene(&mut self, ui: &mut egui::Ui, _selected: &[InstanceId]) {
        ui.label("Rendered scene content");
    }
}

fn key(key: Key, repeat: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers: Modifiers::NONE,
    }
}

fn release(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn frame(
    context: &egui::Context,
    devtools: &mut Devtools,
    host: &mut TestDevtoolsHost,
    events: Vec<Event>,
) {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events,
            ..Default::default()
        },
        |ui| devtools.show_docked(ui, host),
    );
    output.textures_delta.clear();
}

fn click(
    context: &egui::Context,
    devtools: &mut Devtools,
    host: &mut TestDevtoolsHost,
    position: egui::Pos2,
) {
    for pressed in [true, false] {
        frame(
            context,
            devtools,
            host,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

fn contains_text(shape: &egui::Shape, target: &str) -> bool {
    match shape {
        egui::Shape::Text(text) => text.galley.text().contains(target),
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains_text(shape, target)),
        _ => false,
    }
}

#[test]
fn hidden_inspector_renders_the_scene_and_development_notice() {
    let mut host = TestDevtoolsHost {
        workspace: Workspace::new(),
    };
    let mut devtools = Devtools::default();
    let context = egui::Context::default();
    let raw_input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 720.0),
        )),
        ..Default::default()
    };
    let mut first_output = context.run_ui(raw_input(), |ui| {
        devtools.show_docked(ui, &mut host);
    });
    first_output.textures_delta.clear();
    let mut output = context.run_ui(raw_input(), |ui| {
        devtools.show_docked(ui, &mut host);
    });

    let scene_is_visible = output
        .shapes
        .iter()
        .any(|clipped| contains_text(&clipped.shape, "Rendered scene content"));
    let notice_is_visible = output
        .shapes
        .iter()
        .any(|clipped| contains_text(&clipped.shape, "Development build"));
    output.textures_delta.clear();
    assert!(scene_is_visible);
    assert!(notice_is_visible);
    assert!(!devtools.is_visible());
}

#[test]
fn full_dock_uses_the_application_title_for_the_scene_tab() {
    let mut devtools = Devtools::default();
    devtools.set_visible(true);
    let mut host = TestDevtoolsHost {
        workspace: Workspace::new(),
    };
    let context = egui::Context::default();
    let raw_input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 720.0),
        )),
        ..Default::default()
    };
    let mut output = context.run_ui(raw_input(), |ui| {
        devtools.show_docked(ui, &mut host);
    });
    output.textures_delta.clear();
    let mut output = context.run_ui(raw_input(), |ui| {
        devtools.show_docked(ui, &mut host);
    });

    let title_is_visible = output
        .shapes
        .iter()
        .any(|clipped| contains_text(&clipped.shape, "Sample Application"));
    let scene_is_visible = output
        .shapes
        .iter()
        .any(|clipped| contains_text(&clipped.shape, "Rendered scene content"));
    output.textures_delta.clear();
    assert!(title_is_visible);
    assert!(scene_is_visible);
}

#[test]
fn full_dock_deletes_the_selected_instance_on_delete_key() {
    let mut workspace = Workspace::new();
    let part = workspace.add_child(Part::new());
    let mut devtools = Devtools::default();
    devtools.set_visible(true);
    devtools.select(part, Modifiers::NONE, &[part]);
    let mut host = TestDevtoolsHost { workspace };
    let context = egui::Context::default();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events: vec![key(Key::Delete, false)],
            ..Default::default()
        },
        |ui| devtools.show_docked(ui, &mut host),
    );
    output.textures_delta.clear();

    assert!(host.workspace.instance(part).is_none());
}

#[test]
fn f12_toggles_the_inspector_once_per_key_press() {
    let mut devtools = Devtools::default();
    let mut host = TestDevtoolsHost {
        workspace: Workspace::new(),
    };
    let context = egui::Context::default();

    assert!(!devtools.is_visible());
    frame(
        &context,
        &mut devtools,
        &mut host,
        vec![key(Key::F12, false)],
    );
    assert!(devtools.is_visible());
    frame(
        &context,
        &mut devtools,
        &mut host,
        vec![key(Key::F12, true)],
    );
    assert!(devtools.is_visible());
    frame(&context, &mut devtools, &mut host, vec![release(Key::F12)]);
    frame(
        &context,
        &mut devtools,
        &mut host,
        vec![key(Key::F12, false)],
    );
    assert!(!devtools.is_visible());
}

#[test]
fn bottom_right_button_shows_the_inspector() {
    let mut devtools = Devtools::default();
    let mut host = TestDevtoolsHost {
        workspace: Workspace::new(),
    };
    let context = egui::Context::default();
    frame(&context, &mut devtools, &mut host, Vec::new());
    frame(&context, &mut devtools, &mut host, Vec::new());

    click(
        &context,
        &mut devtools,
        &mut host,
        egui::pos2(1252.0, 692.0),
    );

    assert!(devtools.is_visible());
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
    let mut host = TestDevtoolsHost {
        workspace: Workspace::new(),
    };
    let mut parent = Part::new();
    let child = parent.add_child(Part::new());
    let parent = host.workspace.add_child(parent);
    let sibling = host.workspace.add_child(Part::new());
    let root = host.workspace.id();
    let order = [root, parent, child, sibling];
    let mut devtools = Devtools::default();
    let context = egui::Context::default();

    devtools.set_visible(true);
    devtools.select(root, Modifiers::NONE, &order);
    devtools.select(parent, Modifiers::CTRL, &order);
    devtools.select(child, Modifiers::CTRL, &order);
    frame(
        &context,
        &mut devtools,
        &mut host,
        vec![key(Key::Delete, false)],
    );

    assert!(host.workspace.instance(parent).is_none());
    assert!(host.workspace.instance(child).is_none());
    assert!(host.workspace.instance(sibling).is_some());
    assert_eq!(host.workspace.id(), root);
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
