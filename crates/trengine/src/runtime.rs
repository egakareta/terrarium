use serde::Serialize;
use serde_json::Value;

use crate::{trscene::FrameInput, *};

#[derive(Serialize)]
pub(crate) struct HostInfo {
    pub ui: String,
    pub bundled_fonts: bool,
}
#[derive(Serialize)]
pub(crate) struct FrameOutput {
    pub fps: f32,
    pub pixels_per_point: f32,
    pub gui_regions: Vec<[f32; 4]>,
    pub gui_input_regions: Vec<[f32; 4]>,
    pub close: bool,
    pub platform: egui::PlatformOutput,
}
pub(crate) trait HostApp {
    fn frame(&mut self, input: FrameInput) -> Result<(), AppCreationError>;
    #[cfg(not(target_arch = "wasm32"))]
    fn is_close_requested(&self) -> bool;
    fn info(&self) -> HostInfo;
    fn status(&self) -> FrameOutput;
    fn state(&self) -> Value;
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
    platform: egui::PlatformOutput,
}
impl<A: App> Runtime<A> {
    pub(crate) fn new(engine: Engine, app: A, ui: String, bundle_fonts: bool) -> Self {
        Self {
            engine,
            app,
            frame: Frame::default(),
            ui,
            bundle_fonts,
            platform: Default::default(),
        }
    }
}

impl<A: App> HostApp for Runtime<A> {
    #[cfg(not(target_arch = "wasm32"))]
    fn is_close_requested(&self) -> bool {
        self.frame.is_close_requested()
    }
    fn frame(&mut self, mut input: FrameInput) -> Result<(), AppCreationError> {
        self.app.raw_input_hook(&mut self.engine, &mut input);
        let context = self.engine.egui_context();
        let raw = self.engine.gui.borrow_mut().input(&input)?;
        let mut advanced = false;
        let mut result = Ok(());
        let output = context.run_ui(raw, |_ui| {
            #[cfg(feature = "devtools")]
            self.engine
                .devtools
                .show(_ui.ctx(), &mut self.engine.workspace);
            if !advanced {
                input.pointer_captured |= context.egui_wants_pointer_input();
                input.keyboard_captured |= context.egui_wants_keyboard_input();
                for touch in &mut input.touches {
                    touch.captured |= context.egui_wants_pointer_input();
                }
                self.app.logic(&mut self.engine, &input, &mut self.frame);
                result = self.app.render(&mut self.engine, &input, &mut self.frame);
                advanced = true;
            }
            self.engine.paint_touch_controls(&context);
        });
        self.platform = output.platform_output.clone();
        self.engine.gui.borrow_mut().update(output);
        result?;
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
    fn info(&self) -> HostInfo {
        HostInfo {
            ui: self.ui.clone(),
            bundled_fonts: self.bundle_fonts,
        }
    }
    fn status(&self) -> FrameOutput {
        let mut gui_regions = self.engine.gui.borrow().regions(false);
        let mut gui_input_regions = self.engine.gui.borrow().regions(true);
        let camera = self.engine.workspace.camera_controller();
        if camera.touch_controls_visible()
            && let Some([x, y, radius]) = camera.touch_geometry()
        {
            gui_regions.push([
                x - radius - 2.0,
                y - radius - 2.0,
                radius * 2.0 + 4.0,
                radius * 2.0 + 4.0,
            ]);
            gui_input_regions.push([
                x - radius - 2.0,
                y - radius - 2.0,
                radius * 2.0 + 4.0,
                radius * 2.0 + 4.0,
            ]);
        }
        FrameOutput {
            fps: self.engine.renderer().fps(),
            pixels_per_point: self.engine.egui_context().pixels_per_point(),
            gui_regions,
            gui_input_regions,
            close: self.frame.is_close_requested(),
            platform: self.platform.clone(),
        }
    }
    fn state(&self) -> Value {
        self.app.state(&self.engine)
    }
    fn command(&mut self, command: &str, payload: Value) -> Result<Value, AppCreationError> {
        if command == "terrarium.close" {
            self.frame.close();
            return Ok(Value::Null);
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
