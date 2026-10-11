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

#[cfg(not(target_arch = "wasm32"))]
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
                assert_preset(&engine.workspace, preset);
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        assert_preset(&engine.workspace, preset);
    }
    Ok(())
}

#[cfg(all(feature = "bevy", not(target_arch = "wasm32")))]
#[test]
fn bevy_without_a_supplied_workspace_uses_the_launcher_preset() -> Result<(), AppCreationError> {
    let engine = Terrarium::preset(WorkspacePreset::Baseplate)
        .with_bevy(terrarium::bevy::app::App::new())
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(1))
        .run(|engine| {
            assert_preset(&engine.workspace, WorkspacePreset::Baseplate);
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert_preset(&engine.workspace, WorkspacePreset::Baseplate);
    Ok(())
}

#[cfg(all(feature = "bevy", not(target_arch = "wasm32")))]
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
            assert_preset(&engine.workspace, WorkspacePreset::Empty);
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert_preset(&engine.workspace, WorkspacePreset::Empty);
    Ok(())
}
