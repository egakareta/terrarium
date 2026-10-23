#![cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]

use terrarium::{AppCreationError, Engine, Instance, Part, Terrarium, Workspace, with_workspace};

#[with_workspace]
fn initialize(engine: &mut Engine) -> Result<(), AppCreationError> {
    fn populate(workspace: &mut Workspace) {
        workspace.add_child(Part::new().with_name("nested function"));
    }

    struct Scene<'a> {
        workspace: &'a mut Workspace,
    }

    workspace.set_name("Scene".into());
    workspace.add_child(Part::new().with_name("first"));
    engine.with_clear_color([0.1, 0.2, 0.3, 1.0]);
    workspace.lighting.with_clock_time(18.0);
    populate(workspace);
    Scene { workspace }
        .workspace
        .add_child(Part::new().with_name("shorthand field"));

    assert_eq!(workspace.name(), "Scene");
    assert_eq!(
        format!("{workspace}", workspace = workspace.name()),
        "Scene"
    );
    assert_eq!(vec![workspace.name().to_owned(); 2], ["Scene", "Scene"]);
    assert_eq!(vec![workspace.name().to_owned()], ["Scene"]);

    let mut add_from_closure = || {
        workspace.add_child(Part::new().with_name("closure"));
    };
    add_from_closure();
    add_with_alias(engine);
    add_with_renamed_parameter(engine);
    SceneContext { engine }.populate();
    Ok(())
}

type Runtime = Engine;

#[terrarium::with_workspace(runtime)]
fn add_with_alias(runtime: &mut Runtime) {
    workspace.add_child(Part::new().with_name("alias"));
}

#[with_workspace]
fn add_with_renamed_parameter(runtime: &mut Engine) {
    workspace.add_child(Part::new().with_name("renamed parameter"));
}

struct SceneContext<'a> {
    engine: &'a mut Engine,
}

impl SceneContext<'_> {
    fn workspace_mut(&mut self) -> &mut Workspace {
        self.engine.workspace_mut()
    }

    #[with_workspace(self)]
    fn populate(&mut self) {
        workspace.add_child(Part::new().with_name("method"));
    }
}

#[with_workspace]
fn local_bindings_keep_their_meaning(engine: &mut Engine) {
    let original_name = workspace.name().to_owned();
    {
        let workspace = workspace.name().to_owned();
        assert_eq!(workspace, original_name);
    }
    let local_length = |workspace: &str| workspace.len();
    let length = local_length("local");
    assert_eq!(length, 5);

    let mut total = 0;
    for workspace in [1, 2, 3] {
        total += workspace;
    }
    let mut values = [Some(4), None].into_iter();
    while let Some(Some(workspace)) = values.next() {
        total += workspace;
    }
    let value = Some(total);
    if let Some(workspace) = value {
        assert_eq!(workspace, 10);
    } else {
        workspace.set_name("unexpected".into());
    }
    for value in [value, None] {
        match value {
            Some(workspace) if workspace > 0 => assert_eq!(workspace, 10),
            None if workspace.name() == original_name => {}
            _ => workspace.set_name("unexpected".into()),
        }
    }
    {
        let engine = 7;
        assert_eq!(engine, 7);
    }
    assert_eq!(workspace.name(), original_name);
    let workspace = Workspace::new().with_name("local workspace");
    assert_eq!(workspace.name(), "local workspace");
}

#[test]
fn workspace_shorthand_preserves_scene_edits_and_engine_access() -> Result<(), AppCreationError> {
    let mut engine = Terrarium::new()
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(0))
        .run(initialize)?
        .unwrap();

    local_bindings_keep_their_meaning(&mut engine);
    assert_eq!(engine.clear_color(), [0.1, 0.2, 0.3, 1.0]);
    assert_eq!(engine.workspace().name(), "Scene");
    assert_eq!(engine.workspace().lighting.clock_time(), 18.0);
    assert_eq!(
        engine
            .workspace()
            .get_all::<Part>()
            .map(Instance::name)
            .collect::<Vec<_>>(),
        [
            "first",
            "nested function",
            "shorthand field",
            "closure",
            "alias",
            "renamed parameter",
            "method"
        ]
    );
    Ok(())
}
