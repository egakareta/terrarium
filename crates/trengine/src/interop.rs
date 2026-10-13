use serde::Serialize;
use serde_json::Value;
#[cfg(feature = "devtools")]
use serde_json::json;
#[cfg(feature = "devtools")]
use slotmap::Key;

use crate::{trscene::FrameInput, *};

/// State sent to the React host after a rendered frame.
#[derive(Serialize)]
pub struct UiSnapshot {
    /// Registered application component name.
    pub ui: String,
    /// Consumer-provided application state.
    pub app: Value,
    /// Measured rendered frames per second.
    pub fps: f32,
    /// Whether the application requested shutdown.
    pub close: bool,
    /// Whether to use Terrarium's bundled fonts.
    pub bundled_fonts: bool,
    /// Current touchscreen joystick movement.
    pub touch_movement: [f32; 2],
    /// Joystick center and radius in viewport-local CSS pixels.
    pub touch_geometry: Option<[f32; 3]>,
    /// Whether a touchscreen is present and touch input is enabled.
    pub touch_enabled: bool,
    /// Inspector tree and properties when devtools are enabled.
    pub devtools: Value,
}

pub(crate) trait HostApp {
    fn frame(&mut self, input: FrameInput) -> Result<(), AppCreationError>;
    fn snapshot(&self) -> UiSnapshot;
    fn command(&mut self, command: &str, payload: Value) -> Result<Value, AppCreationError>;
    fn engine(&self) -> &Engine;
    #[cfg(not(target_arch = "wasm32"))]
    fn into_engine(self: Box<Self>) -> Engine;
    #[cfg(all(feature = "meshpart", target_arch = "wasm32"))]
    fn viewport_pixels(&self, id: u64) -> Result<Vec<u8>, AppCreationError>;
}

pub(crate) struct Runtime<A> {
    engine: Engine,
    app: A,
    frame: Frame,
    ui: String,
    bundle_fonts: bool,
}
impl<A: App> Runtime<A> {
    pub(crate) fn new(engine: Engine, app: A, ui: String, bundle_fonts: bool) -> Self {
        Self {
            engine,
            app,
            frame: Frame::default(),
            ui,
            bundle_fonts,
        }
    }
}

impl<A: App> HostApp for Runtime<A> {
    fn frame(&mut self, mut input: FrameInput) -> Result<(), AppCreationError> {
        self.app.raw_input_hook(&mut self.engine, &mut input);
        self.app.logic(&mut self.engine, &input, &mut self.frame);
        self.app.render(&mut self.engine, &input, &mut self.frame)?;
        #[cfg(feature = "devtools")]
        self.engine
            .devtools
            .process_input(&input, &mut self.engine.workspace);
        #[cfg(feature = "bevy")]
        if self.engine.bevy_app().is_some_and(|app| {
            app.should_exit().is_some()
                || app
                    .world()
                    .non_send::<crate::bevy::FrameContext>()
                    .is_close_requested()
        }) {
            self.frame.close();
        }
        Ok(())
    }
    fn snapshot(&self) -> UiSnapshot {
        UiSnapshot {
            ui: self.ui.clone(),
            app: self.app.state(&self.engine),
            fps: self.engine.renderer().fps(),
            close: self.frame.is_close_requested(),
            bundled_fonts: self.bundle_fonts,
            touch_movement: self.engine.workspace.camera_controller().touch_movement(),
            touch_geometry: self.engine.workspace.camera_controller().touch_geometry(),
            touch_enabled: self
                .engine
                .workspace
                .camera_controller()
                .touch_controls_visible(),
            #[cfg(feature = "devtools")]
            devtools: inspector_snapshot(&self.engine),
            #[cfg(not(feature = "devtools"))]
            devtools: Value::Null,
        }
    }
    fn command(&mut self, command: &str, payload: Value) -> Result<Value, AppCreationError> {
        if command == "terrarium.close" {
            self.frame.close();
            return Ok(Value::Null);
        }
        #[cfg(feature = "devtools")]
        if command.starts_with("devtools.") {
            return inspector_command(&mut self.engine, command, payload);
        }
        self.app.command(&mut self.engine, command, payload)
    }
    fn engine(&self) -> &Engine {
        &self.engine
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn into_engine(self: Box<Self>) -> Engine {
        self.engine
    }
    #[cfg(all(feature = "meshpart", target_arch = "wasm32"))]
    fn viewport_pixels(&self, id: u64) -> Result<Vec<u8>, AppCreationError> {
        self.engine
            .renderer()
            .viewport_pixels(id)
            .map_err(Into::into)
    }
}

#[cfg(feature = "devtools")]
fn inspector_snapshot(engine: &Engine) -> Value {
    use trscene::Instance;
    fn node(instance: &dyn Instance) -> Value {
        json!({
            "id": instance.id().data().as_ffi().to_string(), "name": instance.name(),
            "class": instance.class_name(), "children": instance.children().iter().map(|child| node(child.as_ref())).collect::<Vec<_>>()
        })
    }
    let tools = &engine.devtools;
    json!({
        "visible": tools.is_visible(),
        "tree": if tools.is_visible() { node(&engine.workspace) } else { Value::Null },
        "selected": tools.selected().iter().map(|id| id.data().as_ffi().to_string()).collect::<Vec<_>>(),
        "properties": if tools.is_visible() { serde_json::to_value(tools.properties(&engine.workspace)).unwrap_or(Value::Null) } else { Value::Null }
    })
}

#[cfg(feature = "devtools")]
fn inspector_command(
    engine: &mut Engine,
    command: &str,
    payload: Value,
) -> Result<Value, AppCreationError> {
    use trscene::{InstanceId, Modifiers};
    match command {
        "devtools.visible" => {
            engine
                .devtools
                .set_visible(payload.as_bool().ok_or("visible must be a boolean")?);
        }
        "devtools.select" => {
            let parse_id = |value: &Value| -> Result<InstanceId, AppCreationError> {
                let id = value
                    .as_str()
                    .ok_or("instance id must be a string")?
                    .parse::<u64>()?;
                Ok(InstanceId::from(slotmap::KeyData::from_ffi(id)))
            };
            let id = parse_id(&payload["id"])?;
            let order = payload["order"]
                .as_array()
                .ok_or("visible order must be an array")?
                .iter()
                .map(parse_id)
                .collect::<Result<Vec<_>, _>>()?;
            let modifiers = serde_json::from_value::<Modifiers>(payload["modifiers"].clone())?;
            engine.devtools.select(id, modifiers, &order);
        }
        "devtools.selection" => {
            let selected = payload
                .as_array()
                .ok_or("selection must be an array")?
                .iter()
                .map(|value| {
                    let id = value
                        .as_str()
                        .ok_or("instance id must be a string")?
                        .parse::<u64>()?;
                    Ok(InstanceId::from(slotmap::KeyData::from_ffi(id)))
                })
                .collect::<Result<Vec<_>, AppCreationError>>()?;
            engine.devtools.set_selection(&engine.workspace, selected);
        }
        "devtools.edit" => {
            let path = payload["path"]
                .as_str()
                .ok_or("property path must be a string")?;
            let value = serde_json::from_value(payload["value"].clone())?;
            engine
                .devtools
                .edit_property(&mut engine.workspace, path, value)?;
        }
        "devtools.delete" => engine.devtools.destroy_selection(&mut engine.workspace),
        _ => return Err(format!("unknown inspector command: {command}").into()),
    }
    Ok(Value::Null)
}
