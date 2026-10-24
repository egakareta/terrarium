use terrarium::{egui::Modifiers, trdevtools::*, *};

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

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
#[test]
fn engine_explorer_opens_the_workspace_and_inspects_the_root()
-> Result<(), terrarium::AppCreationError> {
    use terrarium::{AppCreationError, Terrarium, egui};

    fn contains_text(shape: &egui::Shape, target: &str) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text().contains(target),
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains_text(shape, target)),
            _ => false,
        }
    }

    let mut engine = Terrarium::new()
        .with_size([1280, 720])
        .with_env_logger(false)
        .with_headless(Some(0))
        .run(|engine| {
            engine
                .workspace_mut()
                .add_child_ref(Part::new().with_name("Scene child"))
                .add_child(Part::new().with_name("Collapsed descendant"));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    let context = engine.creation_context().egui_ctx.clone();
    let mut devtools = std::mem::take(&mut engine.devtools);
    let root = engine.id();
    devtools.select(root, Modifiers::NONE, &[root]);
    for _ in 0..2 {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                ..Default::default()
            },
            |ui| devtools.show_docked(ui, &mut engine),
        );
        for label in [
            "Engine (Engine)",
            "Workspace (Workspace)",
            "Scene child (Part)",
        ] {
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|shape| contains_text(&shape.shape, label)),
                "missing row: {label}"
            );
        }
        assert!(
            !output
                .shapes
                .iter()
                .any(|shape| contains_text(&shape.shape, "Collapsed descendant (Part)"))
        );
        output.textures_delta.clear();
    }
    assert_eq!(devtools.selected(), &[root]);
    devtools.edit_property(
        &mut engine,
        "name",
        PropertyValue::Text("Application".into()),
    )?;
    assert_eq!(engine.name(), "Application");

    let workspace = engine.workspace().id();
    let child = engine.workspace().children()[0].id();
    let order = [root, workspace, child];
    devtools.select(root, Modifiers::NONE, &order);
    devtools.select(child, Modifiers::SHIFT, &order);
    devtools.destroy_selection(&mut engine);
    assert_eq!(devtools.selected(), &[root, workspace]);
    assert!(engine.workspace().children().is_empty());
    Ok(())
}
