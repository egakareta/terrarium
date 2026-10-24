#![cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]

use terrarium::*;

#[test]
fn engine_owns_the_instance_tree_and_scopes_workspace_access() -> Result<(), AppCreationError> {
    let clear_color = [0.15, 0.25, 0.35, 1.0];
    let mut engine = Terrarium::new()
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(0))
        .run(|engine| {
            let parent = engine
                .workspace_mut()
                .add_child_ref(Part::new().with_name("parent"));
            parent.add_child(Part::new().with_name("child"));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();

    engine = engine.with_clear_color(clear_color);
    assert_eq!(engine.clear_color(), clear_color);

    let root: &dyn Instance = &engine;
    assert!(root.is_a::<Engine>());
    assert_eq!(root.class_name(), "Engine");
    assert_eq!(root.parent(), None);
    assert_eq!(root.children()[0].id(), engine.workspace().id());
    assert_eq!(engine.workspace().parent(), Some(root.id()));
    assert_eq!(root.view::<Engine>(root.id()).unwrap().id(), root.id());
    assert_eq!(
        root.view::<Workspace>(engine.workspace().id())
            .unwrap()
            .id(),
        engine.workspace().id()
    );
    assert_eq!(
        root.descendants().map(Instance::name).collect::<Vec<_>>(),
        ["Workspace", "parent", "child"]
    );
    assert!(engine.try_clone_box().is_none());

    let sibling = engine.add_child(Part::new().with_name("service"));
    assert!(engine.workspace().view::<dyn Instance>(sibling).is_none());
    assert!(
        engine
            .workspace_mut()
            .view_mut::<dyn Instance>(sibling)
            .is_none()
    );
    assert!(
        engine
            .workspace()
            .view::<dyn Instance>(engine.id())
            .is_none()
    );

    let mut clone = engine.workspace().clone();
    assert_eq!(clone.parent(), None);
    let cloned_child = clone
        .get_all::<Part>()
        .find(|part| part.name() == "child")
        .unwrap()
        .id();
    assert!(
        clone
            .view_mut::<dyn Instance>(cloned_child)
            .unwrap()
            .destroy()
    );
    assert_eq!(engine.workspace().get_all::<Part>().count(), 2);

    let workspace = engine.workspace().id();
    assert!(!engine.workspace_mut().destroy());
    assert!(!Instance::remove_child(&mut engine, workspace));
    assert!(engine.view_mut::<dyn Instance>(sibling).unwrap().destroy());
    assert!(engine.view::<dyn Instance>(sibling).is_none());
    assert_eq!(engine.workspace().id(), workspace);
    Ok(())
}

#[test]
fn selected_owned_controllers_drive_the_scene_and_can_be_replaced() -> Result<(), AppCreationError>
{
    for use_bevy in [false, true] {
        #[cfg(not(feature = "bevy"))]
        if use_bevy {
            continue;
        }
        let launcher = Terrarium::new();
        #[cfg(feature = "bevy")]
        let launcher = if use_bevy {
            launcher.with_bevy(bevy::app::App::new())
        } else {
            launcher
        };
        let mut engine = launcher
            .with_size([32, 32])
            .with_env_logger(false)
            .with_headless(Some(0))
            .run(|engine| {
                engine
                    .workspace_mut()
                    .add_child(Rig::new().with_name("Player"));
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        let rig = engine.workspace().get_all::<Rig>().next().unwrap().id();
        let stored = engine.workspace_mut().add_child(PlayerController::new(rig));
        let context = egui::Context::default();
        let frame = |engine: &mut Engine, pressed| {
            let mut output = context.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::W,
                        physical_key: Some(egui::Key::W),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| engine.update(ui.ctx()),
            );
            output.textures_delta.clear();
        };
        frame(&mut engine, true);
        assert_eq!(
            engine.workspace().get::<Rig>(rig).unwrap().position(),
            glam::Vec3::ZERO
        );
        engine.set_controller(PlayerController::new(rig));
        frame(&mut engine, true);
        let moved = engine.workspace().get::<Rig>(rig).unwrap().position();
        assert!(moved.z < 0.0);
        engine
            .workspace_mut()
            .get_mut::<PlayerController>(stored)
            .unwrap()
            .destroy();
        frame(&mut engine, true);
        assert!(engine.workspace().get::<Rig>(rig).unwrap().position().z < moved.z);
        let position = engine.workspace().get::<Rig>(rig).unwrap().position();
        engine = engine.with_controller(CameraController::default());
        frame(&mut engine, true);
        assert_eq!(
            engine.workspace().get::<Rig>(rig).unwrap().position(),
            position
        );
        engine = engine.with_controller(PlayerController::new(rig));
        frame(&mut engine, true);
        assert!(engine.workspace().get::<Rig>(rig).unwrap().position().z < position.z);

        #[cfg(feature = "physics")]
        {
            let player = engine
                .controller_mut()
                .downcast_mut::<PlayerController>()
                .unwrap();
            player.camera.subject_offset = glam::Vec3::Y;
            player.camera.subject_distance = 8.0;
            let subject = engine.workspace_mut().get_mut::<Rig>(rig).unwrap();
            subject.with_anchored(false);
            subject.with_position(glam::Vec3::Y * 4.0);
            subject.add_child_local(Part::new().with_position(glam::Vec3::Y * 0.5));
            engine.workspace_mut().current_camera.set_subject(Some(rig));

            frame(&mut engine, true);

            let position = engine.workspace().get::<Rig>(rig).unwrap().position();
            assert!(position.z < 0.0);
            assert!(position.y < 4.0);
            let camera = &engine.workspace().current_camera;
            let to_subject = position + glam::Vec3::Y - camera.position();
            assert!((to_subject.length() - 8.0).abs() < 1e-5);
            assert!(camera.forward().dot(to_subject.normalize()) > 0.9999);
            #[cfg(feature = "sound")]
            assert!(
                glam::Vec3::from_array(engine.workspace().soundscape().listener().position)
                    .abs_diff_eq(camera.position(), 1e-5)
            );
        }
    }
    Ok(())
}
