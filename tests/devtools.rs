#![cfg(feature = "devtools")]

use terrarium::{
    BasePart, Color3, HasBasePart, HasLight, HasMaterials, HasPVInstance, HasPart, Instance, Part,
    PartShape, PointLight, Workspace,
    devtools::{Devtools, Inspect, PropertyError, PropertyValue},
    egui::{self, Event, Key, Modifiers},
    glam::Vec3,
};

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
    workspace: &mut Workspace,
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
        |ui| devtools.show(ui.ctx(), workspace),
    );
    output.textures_delta.clear();
}

fn pointer_frame(
    context: &egui::Context,
    devtools: &mut Devtools,
    workspace: &mut Workspace,
    mut events: Vec<Event>,
    modifiers: Modifiers,
) -> egui::FullOutput {
    events.insert(0, Event::ModifiersChanged(modifiers));
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events,
            ..Default::default()
        },
        |ui| devtools.show(ui.ctx(), workspace),
    );
    output.textures_delta.clear();
    output
}

fn label_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    fn search(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| search(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| search(&shape.shape, label))
        .unwrap_or_else(|| panic!("missing label: {label}"))
}

#[test]
fn tree_rows_handle_ctrl_click_and_shift_click() {
    let mut workspace = Workspace::new();
    let ids: Vec<_> = ["first", "second", "third"]
        .into_iter()
        .map(|name| workspace.add_child(Part::new().with_name(name)))
        .collect();
    let mut devtools = Devtools::default();
    let context = egui::Context::default();
    devtools.set_visible(true);
    for _ in 0..2 {
        pointer_frame(
            &context,
            &mut devtools,
            &mut workspace,
            vec![],
            Modifiers::NONE,
        );
    }
    let mut output = pointer_frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![],
        Modifiers::NONE,
    );
    for (label, modifiers, selected) in [
        ("first (Part)", Modifiers::NONE, vec![ids[0]]),
        ("third (Part)", Modifiers::SHIFT, ids.clone()),
        ("second (Part)", Modifiers::CTRL, vec![ids[0], ids[2]]),
    ] {
        let position = label_position(&output, label);
        pointer_frame(
            &context,
            &mut devtools,
            &mut workspace,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ],
            modifiers,
        );
        pointer_frame(
            &context,
            &mut devtools,
            &mut workspace,
            vec![Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
            modifiers,
        );
        assert_eq!(devtools.selected(), selected);
        output = pointer_frame(
            &context,
            &mut devtools,
            &mut workspace,
            vec![],
            Modifiers::NONE,
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn terrarium_keeps_devtools_available_when_app_rendering_is_overridden()
-> Result<(), terrarium::AppCreationError> {
    struct CustomApp;
    impl terrarium::App for CustomApp {
        fn raw_input_hook(
            &mut self,
            _engine: &mut terrarium::Engine,
            _context: &egui::Context,
            input: &mut egui::RawInput,
        ) {
            input.events.push(key(Key::F12, false));
        }
        fn render(
            &mut self,
            _engine: &mut terrarium::Engine,
            _ui: &mut egui::Ui,
            _frame: &mut terrarium::eframe::Frame,
        ) {
        }
    }
    let engine = terrarium::Terrarium::new()
        .with_headless(Some(1))
        .with_env_logger(false)
        .run(|engine| {
            assert!(!engine.devtools.is_visible());
            Ok::<_, terrarium::AppCreationError>(CustomApp)
        })?
        .unwrap();
    assert!(engine.devtools.is_visible());
    Ok(())
}

#[test]
fn f12_toggles_the_initially_hidden_inspector_and_ignores_repeat() {
    let mut workspace = Workspace::new();
    let mut devtools = Devtools::default();
    let context = egui::Context::default();
    assert!(!devtools.is_visible());
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![key(Key::F12, false)],
    );
    assert!(devtools.is_visible());
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![key(Key::F12, true)],
    );
    assert!(devtools.is_visible());
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![release(Key::F12)],
    );
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![key(Key::F12, false)],
    );
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
    devtools.select(order[4], Modifiers::SHIFT, &[order[4]]);
    assert_eq!(devtools.selected(), &[order[4]]);
}

#[test]
fn multi_selection_edits_common_fields_and_reports_mixed_values() {
    let mut workspace = Workspace::new();
    let part = workspace.add_child(Part::new().with_color(Color3::BLACK));
    let light = workspace.add_child(PointLight::new());
    let mut devtools = Devtools::default();
    devtools.select(part, Modifiers::NONE, &[part, light]);
    devtools.select(light, Modifiers::CTRL, &[part, light]);
    assert!(
        devtools
            .properties(&workspace)
            .iter()
            .find(|p| p.property.path == "color")
            .unwrap()
            .mixed
    );
    assert!(
        !devtools
            .properties(&workspace)
            .iter()
            .any(|p| p.property.path == "shape")
    );
    devtools
        .edit_property(
            &mut workspace,
            "color",
            PropertyValue::Color([0.2, 0.4, 0.6]),
        )
        .unwrap();
    assert_eq!(
        workspace.get::<Part>(part).unwrap().color(),
        Color3::new(0.2, 0.4, 0.6)
    );
    assert_eq!(
        workspace.get::<PointLight>(light).unwrap().color(),
        Color3::new(0.2, 0.4, 0.6)
    );
    assert!(
        !devtools
            .properties(&workspace)
            .iter()
            .find(|p| p.property.path == "color")
            .unwrap()
            .mixed
    );
    assert_eq!(
        devtools.edit_property(
            &mut workspace,
            "color",
            PropertyValue::Color([f32::NAN, 0.0, 0.0])
        ),
        Err(PropertyError::InvalidValue)
    );
    assert_eq!(
        workspace.get::<Part>(part).unwrap().color(),
        Color3::new(0.2, 0.4, 0.6)
    );
    assert_eq!(
        devtools.edit_property(&mut workspace, "id", PropertyValue::Text("changed".into())),
        Err(PropertyError::NotFound)
    );
    assert!(workspace.instance(part).is_some());
}

#[test]
fn clock_time_edits_replace_the_sun_override_and_wrap_the_displayed_time() {
    let mut workspace = Workspace::new();
    workspace.lighting.with_sun_direction(Vec3::Y);
    let original_time = workspace.lighting.clock_time();
    let root = workspace.id();
    let mut devtools = Devtools::default();
    devtools.select(root, Modifiers::NONE, &[root]);
    assert_eq!(
        devtools.edit_property(
            &mut workspace,
            "lighting.clock_time",
            PropertyValue::Number(f32::NAN),
        ),
        Err(PropertyError::InvalidValue)
    );
    assert_eq!(workspace.lighting.clock_time(), original_time);
    assert_eq!(workspace.lighting.sun_direction(), Vec3::Y);
    devtools
        .edit_property(
            &mut workspace,
            "lighting.clock_time",
            PropertyValue::Number(30.0),
        )
        .unwrap();
    let mut expected = terrarium::Lighting::new();
    expected.with_clock_time(30.0);
    assert_eq!(workspace.lighting.clock_time(), 6.0);
    assert!(
        workspace
            .lighting
            .sun_direction()
            .abs_diff_eq(expected.sun_direction(), 1e-6)
    );
    assert!(devtools.properties(&workspace).iter().any(|field| {
        field.property.path == "lighting.clock_time"
            && field.property.value == PropertyValue::Number(6.0)
    }));
}

#[test]
fn reflected_edits_change_transforms_shapes_materials_and_attributes() {
    let mut workspace = Workspace::new();
    let mut parent = Part::new();
    let child = parent.add_child(Part::new().with_position(Vec3::splat(5.0)));
    let parent = workspace.add_child(parent);
    let mut devtools = Devtools::default();
    devtools.select(parent, Modifiers::NONE, &[parent, child]);
    devtools
        .edit_property(
            &mut workspace,
            "position",
            PropertyValue::Vector(vec![1.0, 2.0, 3.0]),
        )
        .unwrap();
    devtools
        .edit_property(
            &mut workspace,
            "shape",
            PropertyValue::Choice {
                selected: "Ball".into(),
                options: vec![],
            },
        )
        .unwrap_err();
    let shape = devtools
        .properties(&workspace)
        .into_iter()
        .find(|p| p.property.path == "shape")
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
    assert_eq!(part.material_slot(terrarium::Face::Top).roughness(), 0.8);
    assert_eq!(part.get_attribute("health"), Some(&serde_json::json!(75)));
    assert_eq!(
        workspace.get::<Part>(child).unwrap().position(),
        Vec3::splat(5.0)
    );
    assert_eq!(
        devtools.edit_property(
            &mut workspace,
            "attributes",
            PropertyValue::Json("{".into())
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
fn delete_destroys_nested_selection_and_preserves_the_workspace_root() {
    let mut workspace = Workspace::new();
    let mut parent = Part::new();
    let child = parent.add_child(Part::new());
    let parent = workspace.add_child(parent);
    let sibling = workspace.add_child(Part::new());
    let root = workspace.id();
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
        &mut workspace,
        vec![key(Key::Delete, false)],
    );
    assert!(workspace.instance(parent).is_none());
    assert!(workspace.instance(child).is_none());
    assert!(workspace.instance(sibling).is_some());
    assert_eq!(workspace.id(), root);
    assert_eq!(devtools.selected(), &[root]);
}

#[test]
fn delete_leaves_instances_alive_while_text_input_is_focused_or_inspector_is_hidden() {
    let mut workspace = Workspace::new();
    let part = workspace.add_child(Part::new());
    let mut devtools = Devtools::default();
    let context = egui::Context::default();
    devtools.select(part, Modifiers::NONE, &[part]);
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![key(Key::Delete, false)],
    );
    assert!(workspace.instance(part).is_some());
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![release(Key::Delete)],
    );
    devtools.set_visible(true);
    let mut text = "input".to_owned();
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
        devtools.show(ui.ctx(), &mut workspace);
    });
    output.textures_delta.clear();
    frame(
        &context,
        &mut devtools,
        &mut workspace,
        vec![key(Key::Delete, false)],
    );
    assert!(workspace.instance(part).is_some());
}

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
fn derived_fields_are_editable_and_respect_readonly_and_skipped_fields() {
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
    assert_eq!(
        instance.set_property("source", PropertyValue::Text("replacement".into())),
        Err(PropertyError::NotFound)
    );
    assert!(
        !instance
            .properties()
            .iter()
            .any(|p| p.path == "runtime_counter")
    );
}
