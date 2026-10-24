#![cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]

use terrarium::*;

#[derive(Debug)]
struct PlacePart(InstanceId);

impl Controller for PlacePart {
    fn step(&mut self, workspace: &mut Workspace, _: f32) {
        workspace
            .get_mut::<Part>(self.0)
            .unwrap()
            .with_position(glam::Vec3::X);
    }
}

#[test]
fn engine_owns_the_instance_tree_and_scopes_workspace_access() -> Result<(), AppCreationError> {
    let clear_color = [0.15, 0.25, 0.35, 1.0];
    let mut engine = Terrarium::new()
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(0))
        .run(|engine| {
            assert!(
                engine
                    .controller()
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
            engine.set_controller(Box::new(PlacePart(parent_id)));
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
