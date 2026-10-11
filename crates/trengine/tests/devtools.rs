#![cfg(all(feature = "devtools", not(target_arch = "wasm32")))]

use trengine::{App, AppCreationError, Engine, Terrarium, egui};

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
            assert!(!engine.devtools.is_visible());
            Ok::<_, AppCreationError>(CustomApp)
        })?
        .unwrap();

    assert!(engine.devtools.is_visible());
    Ok(())
}
