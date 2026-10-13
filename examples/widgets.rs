//! React widget gallery. Select a demo with `cargo run --example widgets -- taffy_grid`.
use terrarium::{App, AppCreationError, Engine, Terrarium, serde_json};

struct WidgetsApp {
    widget: String,
}
impl App for WidgetsApp {
    fn state(&self, _engine: &Engine) -> serde_json::Value {
        serde_json::json!({"widget": self.widget})
    }
    fn command(
        &mut self,
        _engine: &mut Engine,
        command: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, AppCreationError> {
        if command != "widget" {
            return Err(format!("unknown widget command: {command}").into());
        }
        self.widget = payload
            .as_str()
            .ok_or("widget must be a string")?
            .to_owned();
        Ok(serde_json::Value::Null)
    }
    fn render(
        &mut self,
        _engine: &mut Engine,
        _input: &terrarium::FrameInput,
        _frame: &mut terrarium::Frame,
    ) -> Result<(), terrarium::RendererError> {
        Ok(())
    }
}
fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let widget = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "taffy_egui_widgets".to_owned());
    #[cfg(target_arch = "wasm32")]
    let widget = "taffy_egui_widgets".to_owned();
    Terrarium::new()
        .with_ui("widgets")
        .run(move |_| Ok::<_, AppCreationError>(WidgetsApp { widget }))
        .unwrap();
}
