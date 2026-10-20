#![cfg(all(
    feature = "devtools",
    not(all(target_family = "wasm", not(target_os = "wasi")))
))]

use trengine::{
    App, AppCreationError, Engine, Terrarium, egui,
    trdevtools::{Devtools, PropertyValue},
    trscene::{Instance, Part},
};

#[test]
fn inspector_runs_after_custom_app_rendering() -> Result<(), AppCreationError> {
    struct CustomApp;

    impl App for CustomApp {
        fn raw_input_hook(
            &mut self,
            _engine: &mut Engine,
            _context: &egui::Context,
            input: &mut egui::RawInput,
        ) {
            input.events.push(egui::Event::Key {
                key: egui::Key::F12,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }

        fn render(
            &mut self,
            _engine: &mut Engine,
            _ui: &mut egui::Ui,
            _frame: &mut trengine::eframe::Frame,
        ) {
        }
    }

    let engine = Terrarium::new()
        .with_headless(Some(1))
        .with_env_logger(false)
        .run(|engine| {
            engine.devtools.set_visible(false);
            assert!(!engine.devtools.is_visible());
            Ok::<_, AppCreationError>(CustomApp)
        })?
        .unwrap();

    assert!(engine.devtools.is_visible());
    Ok(())
}

#[test]
fn inspector_can_edit_the_engine_and_its_workspace_descendants() -> Result<(), AppCreationError> {
    let mut engine = Terrarium::new()
        .with_size([32, 32])
        .with_headless(Some(0))
        .with_env_logger(false)
        .run(|engine| {
            engine.workspace_mut().add_child(Part::new());
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    let root = engine.id();
    let workspace = engine.workspace().id();
    let part = engine.workspace().children()[0].id();
    let inspector_root = trdevtools::DevtoolsHost::root(&mut engine);
    assert_eq!(inspector_root.id(), root);
    assert_eq!(inspector_root.children()[0].id(), workspace);
    let rows = [root, workspace, part];
    let mut inspector = Devtools::default();

    for (id, name) in [(root, "Application"), (workspace, "Scene"), (part, "Cube")] {
        inspector.select(id, egui::Modifiers::NONE, &rows);
        inspector.edit_property(&mut engine, "name", PropertyValue::Text(name.to_owned()))?;
    }
    assert_eq!(engine.name(), "Application");
    assert_eq!(engine.workspace().name(), "Scene");
    assert_eq!(engine.workspace().instance(part).unwrap().name(), "Cube");

    inspector.destroy_selection(&mut engine);
    assert!(engine.workspace().instance(part).is_none());
    inspector.select(workspace, egui::Modifiers::NONE, &rows);
    inspector.select(root, egui::Modifiers::COMMAND, &rows);
    inspector.destroy_selection(&mut engine);
    assert_eq!(engine.workspace().id(), workspace);
    assert_eq!(engine.id(), root);
    Ok(())
}
