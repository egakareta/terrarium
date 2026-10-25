#![cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]

use terrarium::*;

instance! {
    #[instance(default = false)]
    #[derive(Clone, Debug)]
    struct PlacePart {
        #[extends]
        controller: Controller,
        part: InstanceId,
    }

    impl PlacePart {
        #[r#override(Controller::step)]
        fn step(&mut self, workspace: &mut Workspace, _delta_seconds: f32) {
            workspace
                .get_mut::<Part>(self.part)
                .unwrap()
                .with_position(glam::Vec3::X);
        }
    }
}

#[test]
fn engine_owns_the_instance_tree_and_scopes_workspace_access() -> Result<(), AppCreationError> {
    let clear_color = [0.15, 0.25, 0.35, 1.0];
    let mut engine = Terrarium::new()
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless_frames(Some(Some(0)))
        .run(|engine| {
            let controller = engine.controller();
            let controller_id = controller.id();
            assert_eq!(controller.class_name(), "CameraController");
            assert_eq!(controller.parent(), None);
            assert!(engine.view::<dyn Instance>(controller_id).is_none());
            assert!(
                engine
                    .controller()
                    .as_instance()
                    .downcast_ref::<CameraController>()
                    .is_some()
            );
            let parent_id = {
                let parent = engine
                    .workspace_mut()
                    .add_child_ref(Part::new().with_name("parent"));
                parent.add_child(Part::new().with_name("child"));
                parent.id()
            };
            engine.set_controller(Box::new(PlacePart {
                controller: Controller::default().with_name("PlacePart"),
                part: parent_id,
            }));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();

    engine = engine.with_clear_color(clear_color);
    assert_eq!(engine.clear_color(), clear_color);

    let root: &dyn Instance = &engine;
    let controller = engine.controller();
    let controller_id = controller.id();
    assert_eq!(controller.class_name(), "PlacePart");
    assert_eq!(controller.parent(), None);
    assert!(controller.children().is_empty());
    assert!(root.view::<dyn Instance>(controller_id).is_none());
    let cloned_controller = controller.as_instance().clone_box();
    assert_ne!(cloned_controller.id(), controller_id);
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
    assert!(
        !root
            .descendants()
            .any(|instance| instance.id() == controller_id)
    );
    assert!(engine.try_clone_box().is_none());

    let parent_id = engine
        .get_all::<Part>()
        .find(|part| part.name() == "parent")
        .unwrap()
        .id();
    engine.update(&egui::Context::default());
    assert_eq!(
        engine
            .workspace()
            .get::<Part>(parent_id)
            .unwrap()
            .position(),
        glam::Vec3::X
    );

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
