use approx::assert_abs_diff_eq;
use terrarium::{glam::Vec3, *};

fn assert_preset(workspace: &Workspace, preset: WorkspacePreset) {
    match preset {
        WorkspacePreset::Empty => assert!(workspace.instances().next().is_none()),
        WorkspacePreset::Baseplate => {
            let plate = workspace
                .get_all::<Part>()
                .find(|part| part.name() == "Baseplate")
                .expect("baseplate preset must provide a floor");
            assert!(plate.anchored());
            assert!(plate.can_collide());
            assert_eq!(plate.shape(), PartShape::Block);
            let size = plate.pivot().to_scale_rotation_translation().0;
            assert_eq!(size, Vec3::new(512.0, 1.0, 512.0));
            assert_abs_diff_eq!(plate.position().y + size.y / 2.0, 0.0);
        }
    }
}

#[test]
fn workspace_defaults_and_empty_presets_have_no_starting_geometry() {
    assert_eq!(WorkspacePreset::default(), WorkspacePreset::Empty);
    for workspace in [
        Workspace::new(),
        Workspace::default(),
        Workspace::preset(WorkspacePreset::Empty),
        Workspace::preset(WorkspacePreset::default()),
    ] {
        assert_preset(&workspace, WorkspacePreset::Empty);
    }
}

#[test]
fn baseplate_preset_provides_a_floor_without_changing_the_camera() {
    let workspace = Workspace::preset(WorkspacePreset::Baseplate);
    assert_preset(&workspace, WorkspacePreset::Baseplate);
    assert_eq!(workspace.current_camera.pose(), Camera::default().pose());
}

#[cfg(feature = "physics")]
#[test]
fn baseplate_supports_dynamic_parts() {
    let mut workspace = Workspace::preset(WorkspacePreset::Baseplate);
    let part = workspace.add_child(
        Part::new()
            .with_anchored(false)
            .with_position(Vec3::new(0.0, 3.0, 0.0)),
    );
    for _ in 0..180 {
        workspace.update(1.0 / 60.0);
    }
    let height = workspace.get::<Part>(part).unwrap().position().y;
    assert!(height > 0.4 && height < 0.6, "resting height: {height}");
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
#[test]
fn launchers_apply_presets_before_the_initializer() -> Result<(), AppCreationError> {
    for (launcher, preset) in [
        (Terrarium::new(), WorkspacePreset::Empty),
        (Terrarium::default(), WorkspacePreset::Empty),
        (
            Terrarium::preset(WorkspacePreset::Empty),
            WorkspacePreset::Empty,
        ),
        (
            Terrarium::preset(WorkspacePreset::Baseplate),
            WorkspacePreset::Baseplate,
        ),
    ] {
        let engine = launcher
            .with_size([32, 32])
            .with_env_logger(false)
            .with_headless(Some(1))
            .run(move |engine| {
                assert_eq!(engine.workspace().parent(), Some(engine.id()));
                assert_preset(engine.workspace(), preset);
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        assert_eq!(engine.workspace().parent(), Some(engine.id()));
        assert_preset(engine.workspace(), preset);
    }
    Ok(())
}

#[cfg(all(
    feature = "bevy",
    not(all(target_family = "wasm", not(target_os = "wasi")))
))]
#[test]
fn bevy_without_a_supplied_workspace_uses_the_launcher_preset() -> Result<(), AppCreationError> {
    let engine = Terrarium::preset(WorkspacePreset::Baseplate)
        .with_bevy(terrarium::bevy::app::App::new())
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(1))
        .run(|engine| {
            assert_eq!(engine.workspace().parent(), Some(engine.id()));
            assert_preset(engine.workspace(), WorkspacePreset::Baseplate);
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert_eq!(engine.workspace().parent(), Some(engine.id()));
    assert_preset(engine.workspace(), WorkspacePreset::Baseplate);
    Ok(())
}

#[cfg(all(
    feature = "bevy",
    not(all(target_family = "wasm", not(target_os = "wasi")))
))]
#[test]
fn a_supplied_empty_bevy_workspace_overrides_the_launcher_preset() -> Result<(), AppCreationError> {
    let mut app = terrarium::bevy::app::App::new();
    app.world_mut().insert_non_send(Workspace::new());
    let engine = Terrarium::preset(WorkspacePreset::Baseplate)
        .with_bevy(app)
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(1))
        .run(|engine| {
            assert_eq!(engine.workspace().parent(), Some(engine.id()));
            assert_preset(engine.workspace(), WorkspacePreset::Empty);
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert_eq!(engine.workspace().parent(), Some(engine.id()));
    assert_preset(engine.workspace(), WorkspacePreset::Empty);
    Ok(())
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
#[test]
fn engine_hierarchy_supports_instance_queries_and_edits() -> Result<(), AppCreationError> {
    let mut engine = Terrarium::new()
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(0))
        .run(|engine| {
            engine
                .workspace_mut()
                .add_child(Part::new().with_name("Scene part"));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap()
        .with_typed_attribute("version", 1_u32)?;

    let workspace_id = engine.workspace().id();
    let part_id = engine.workspace().get_all::<Part>().next().unwrap().id();
    let sibling = engine.add_child(Part::new().with_name("Engine child"));
    assert!(engine.workspace().instance(sibling).is_none());
    assert_eq!(engine.get_typed_attribute::<u32>("version").unwrap()?, 1);

    let root: &mut dyn Instance = &mut engine;
    assert_eq!(root.class_name(), "Engine");
    root.set_name("Application".to_owned());
    assert_eq!(root.name(), "Application");
    let workspace = root.find_descendant(workspace_id).unwrap();
    assert!(workspace.is::<Workspace>());
    assert_eq!(workspace.parent(), Some(root.id()));
    root.find_descendant_mut(part_id)
        .unwrap()
        .set_name("Renamed".to_owned());
    assert_eq!(root.find_descendant(part_id).unwrap().name(), "Renamed");
    assert!(root.find_descendant_mut(part_id).unwrap().destroy());
    assert!(root.find_descendant(part_id).is_none());
    assert!(root.find_descendant_mut(sibling).unwrap().destroy());
    assert!(root.find_descendant(sibling).is_none());
    assert!(engine.workspace().instance(part_id).is_none());
    assert_eq!(engine.workspace().id(), workspace_id);
    Ok(())
}
