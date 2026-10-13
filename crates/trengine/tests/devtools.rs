#![cfg(all(feature = "devtools", not(target_arch = "wasm32")))]
use trengine::{
    App, AppCreationError, Engine, Frame, RendererError, Terrarium, trscene::FrameInput,
};

#[test]
fn inspector_runs_after_custom_app_rendering() -> Result<(), AppCreationError> {
    struct CustomApp;
    impl App for CustomApp {
        fn raw_input_hook(&mut self, _engine: &mut Engine, input: &mut FrameInput) {
            input.pressed.push("F12".into());
        }
        fn render(
            &mut self,
            _engine: &mut Engine,
            _input: &FrameInput,
            _frame: &mut Frame,
        ) -> Result<(), RendererError> {
            Ok(())
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
