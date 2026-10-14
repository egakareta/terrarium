use std::{
    cell::{RefCell, RefMut},
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::{
    trscene::{FrameInput, Workspace, WorkspacePreset},
    *,
};

/// Error returned while creating or running an application.
pub type AppCreationError = Box<dyn std::error::Error + Send + Sync>;

/// Application frame controls shared by every runtime.
#[derive(Default)]
pub struct Frame {
    close_requested: bool,
}

impl Frame {
    /// Requests that the runtime stop after the current frame.
    pub fn close(&mut self) {
        self.close_requested = true;
    }
    /// Returns whether the application requested shutdown.
    pub fn is_close_requested(&self) -> bool {
        self.close_requested
    }
}

/// Owns a workspace and its GPU renderer independently of the UI host.
pub struct Engine {
    renderer: Rc<RefCell<RenderAdapter>>,
    pub(crate) gui: RefCell<crate::gui::Gui>,
    /// The workspace rendered by this engine.
    pub workspace: Workspace,
    /// The F12 workspace inspector, initially hidden.
    #[cfg(feature = "devtools")]
    pub devtools: trdevtools::Devtools,
    creation_context: CreationContext,
    clear_color: [f32; 4],
    #[cfg(feature = "bevy")]
    bevy_app: Option<bevy_app::App>,
}

impl Engine {
    /// Creates an engine from host-independent GPU resources and a workspace.
    pub fn new(
        context: &CreationContext,
        size: [u32; 2],
        workspace: Workspace,
    ) -> Result<Self, RendererError> {
        let renderer = RenderAdapter::new(&context.wgpu_render_state, size)?;
        let mut engine = Self {
            renderer: Rc::new(RefCell::new(renderer)),
            gui: RefCell::new(crate::gui::Gui::new(&context.wgpu_render_state)),
            workspace,
            #[cfg(feature = "devtools")]
            devtools: trdevtools::Devtools::default(),
            creation_context: context.clone(),
            clear_color: [0.018, 0.028, 0.065, 1.0],
            #[cfg(feature = "bevy")]
            bevy_app: None,
        };
        engine.with_clear_color(engine.clear_color);
        Ok(engine)
    }

    /// Returns the egui context used by internal tools.
    pub fn egui_context(&self) -> egui::Context {
        self.gui.borrow().context.clone()
    }
    pub(crate) fn paint_touch_controls(&self, context: &egui::Context) {
        let camera = self.workspace.camera_controller();
        if !camera.touch_controls_visible() {
            return;
        }
        let Some([x, y, radius]) = camera.touch_geometry() else {
            return;
        };
        let zoom = context.zoom_factor();
        let center = egui::pos2(x / zoom, y / zoom);
        let radius = radius / zoom;
        let painter = context.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("terrarium_camera_joystick"),
        ));
        painter.circle(
            center,
            radius,
            egui::Color32::from_black_alpha(80),
            egui::Stroke::new(2.0, egui::Color32::from_white_alpha(110)),
        );
        let tick = radius * 0.75;
        for direction in [
            egui::vec2(1.0, 0.0),
            egui::vec2(-1.0, 0.0),
            egui::vec2(0.0, 1.0),
            egui::vec2(0.0, -1.0),
        ] {
            painter.line_segment(
                [center + direction * tick, center + direction * (tick + 6.0)],
                egui::Stroke::new(2.0, egui::Color32::from_white_alpha(140)),
            );
        }
        let movement = camera.touch_movement();
        painter.circle(
            center + egui::vec2(movement[0], movement[1]) * radius * 0.65,
            radius * 0.3,
            egui::Color32::from_white_alpha(100),
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(160)),
        );
    }
    /// Returns the GPU resources associated with application creation.
    pub fn creation_context(&self) -> &CreationContext {
        &self.creation_context
    }
    /// Returns mutable access to the renderer for advanced operations.
    pub fn renderer(&self) -> RefMut<'_, RenderAdapter> {
        self.renderer.borrow_mut()
    }
    /// Returns the scene's RGBA clear color.
    pub fn clear_color(&self) -> [f32; 4] {
        self.clear_color
    }
    /// Sets the scene's RGBA clear color.
    pub fn with_clear_color(&mut self, color: [f32; 4]) -> &mut Self {
        self.clear_color = color;
        self.renderer().with_clear_color(wgpu::Color {
            r: color[0] as f64,
            g: color[1] as f64,
            b: color[2] as f64,
            a: color[3] as f64,
        });
        self
    }
    /// Processes camera input and advances the workspace.
    pub fn update(&mut self, input: &FrameInput) {
        self.workspace.camera_controller_mut().process_input(input);
        #[cfg(feature = "bevy")]
        if let Some(app) = self.bevy_app.as_mut() {
            crate::bevy::update_app(app, &mut self.workspace, input.delta_secs());
            return;
        }
        self.workspace.update(input.delta_secs());
    }
    /// Prepares the workspace for the host's current viewport.
    pub fn prepare(&mut self, input: &FrameInput) -> Result<(), RendererError> {
        let size = input.physical_size();
        self.workspace.current_camera.resize(size[0], size[1]);
        self.renderer
            .borrow_mut()
            .prepare_scene(&self.workspace, size)
    }
    /// Submits the scene for display by the host.
    pub fn render(&mut self, input: &FrameInput) -> Result<(), RendererError> {
        self.prepare(input)?;
        Ok(())
    }
    /// Returns the attached Bevy application, if any.
    #[cfg(feature = "bevy")]
    pub fn bevy_app(&self) -> Option<&bevy_app::App> {
        self.bevy_app.as_ref()
    }
    /// Returns mutable access to the attached Bevy application.
    #[cfg(feature = "bevy")]
    pub fn bevy_app_mut(&mut self) -> Option<&mut bevy_app::App> {
        self.bevy_app.as_mut()
    }
}

impl Deref for Engine {
    type Target = Workspace;
    fn deref(&self) -> &Workspace {
        &self.workspace
    }
}
impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}

/// Application behavior; UI components receive state and send commands through the web bridge.
pub trait App: 'static {
    /// Processes one application update.
    fn logic(&mut self, engine: &mut Engine, input: &FrameInput, frame: &mut Frame) {
        engine.update(input);
        self.after_update(engine, input, frame);
    }
    /// Runs after input and workspace updates.
    fn after_update(&mut self, _engine: &mut Engine, _input: &FrameInput, _frame: &mut Frame) {}
    /// Submits one application frame.
    fn render(
        &mut self,
        engine: &mut Engine,
        input: &FrameInput,
        _frame: &mut Frame,
    ) -> Result<(), RendererError> {
        engine.render(input)
    }
    /// Returns application state consumed by the React component.
    fn state(&self, _engine: &Engine) -> serde_json::Value {
        serde_json::Value::Null
    }
    /// Handles a command from the React component.
    fn command(
        &mut self,
        _engine: &mut Engine,
        command: &str,
        _payload: serde_json::Value,
    ) -> Result<serde_json::Value, AppCreationError> {
        Err(std::io::Error::other(format!("unknown application command: {command}")).into())
    }
    /// Adjusts normalized host input before camera or inspector processing.
    fn raw_input_hook(&mut self, _engine: &mut Engine, _input: &mut FrameInput) {}
}
impl App for () {}

/// The primary entry point for configuring and running a Terrarium application.
pub struct Terrarium<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    native_context: Option<tauri::Context<tauri::Wry>>,
    pub(crate) workspace_preset: WorkspacePreset,
    #[cfg(feature = "bevy")]
    bevy_app: Option<bevy_app::App>,
    pub(crate) title: &'a str,
    pub(crate) size: [u32; 2],
    pub(crate) canvas_id: &'a str,
    pub(crate) ui: &'a str,
    wgpu_options_customized: bool,
    wgpu_options: Box<dyn FnOnce(&mut WgpuConfiguration) + 'a>,
    pub(crate) env_logger: bool,
    pub(crate) headless: Option<Option<usize>>,
    pub(crate) console_error_panic_hook: bool,
    pub(crate) bundle_fonts: bool,
}
impl Default for Terrarium<'_> {
    fn default() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            native_context: None,
            workspace_preset: WorkspacePreset::default(),
            #[cfg(feature = "bevy")]
            bevy_app: None,
            title: "app",
            size: [1280, 720],
            canvas_id: "app",
            ui: "simple",
            wgpu_options_customized: false,
            wgpu_options: Box::new(|_| {}),
            env_logger: true,
            headless: None,
            console_error_panic_hook: true,
            bundle_fonts: cfg!(feature = "default-fonts"),
        }
    }
}
impl<'a> Terrarium<'a> {
    /// Creates a launcher with an empty workspace.
    pub fn new() -> Self {
        Self::default()
    }
    /// Creates a launcher with the selected starting geometry.
    pub fn preset(preset: WorkspacePreset) -> Self {
        Self {
            workspace_preset: preset,
            ..Self::default()
        }
    }
    /// Attaches a Bevy app while preserving the imperative workspace initializer.
    #[cfg(feature = "bevy")]
    pub fn with_bevy(mut self, app: bevy_app::App) -> Self {
        self.bevy_app = Some(app);
        self
    }
    #[cfg(feature = "bevy")]
    pub(crate) fn take_bevy_app(&mut self) -> Option<bevy_app::App> {
        self.bevy_app.take()
    }
    /// Creates an engine and runs the initialized app in Tauri or the browser.
    /// Native runs return the engine after shutdown; browser runs return immediately.
    pub fn run<A: App, E: Into<AppCreationError>>(
        mut self,
        initialize: impl FnOnce(&mut Engine) -> Result<A, E> + 'static,
    ) -> Result<Option<Engine>, AppCreationError> {
        let mut options = WgpuConfiguration::default();
        (self.wgpu_options)(&mut options);
        self.wgpu_options = Box::new(|_| {});
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.env_logger {
                let _ = env_logger::try_init();
            }
            if let Some(headless) = self.headless {
                let state = crate::native::render_state(options, self.wgpu_options_customized)?;
                let runtime = self.initialize(state, initialize)?;
                crate::native::run_headless(runtime, self.size, headless)
            } else {
                crate::native::start(self, options, initialize)
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            if self.console_error_panic_hook {
                console_error_panic_hook::set_once();
            }
            crate::web::start(self, options, initialize);
            Ok(None)
        }
    }
    pub(crate) fn initialize<A: App, E: Into<AppCreationError>>(
        &mut self,
        state: RenderState,
        initialize: impl FnOnce(&mut Engine) -> Result<A, E>,
    ) -> Result<crate::runtime::Runtime<A>, AppCreationError> {
        let mut engine = Engine::new(
            &CreationContext {
                wgpu_render_state: state,
            },
            self.size,
            Workspace::preset(self.workspace_preset),
        )?;
        let fonts = if self.bundle_fonts {
            trfont::font_definitions()
        } else {
            egui::FontDefinitions::default()
        };
        #[cfg(any(feature = "phosphor", feature = "devtools"))]
        let fonts = {
            let mut fonts = fonts;
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            fonts
        };
        #[cfg(feature = "devtools")]
        engine.devtools.set_base_font_definitions(fonts.clone());
        engine.egui_context().set_fonts(fonts);
        #[cfg(feature = "bevy")]
        if let Some(app) = self.bevy_app.take() {
            engine.bevy_app = Some(crate::bevy::attach_app(app, &mut engine.workspace)?);
        }
        let app = initialize(&mut engine).map_err(Into::into)?;
        #[cfg(feature = "bevy")]
        if let Some(app) = engine.bevy_app.as_mut() {
            crate::bevy::finalize_app(app, &mut engine.workspace);
        }
        Ok(crate::runtime::Runtime::new(
            engine,
            app,
            self.ui.to_owned(),
            self.bundle_fonts,
        ))
    }
    /// Runs without a native window for a frame count, or until `Frame::close` when `None`.
    pub fn with_headless(mut self, frames: Option<usize>) -> Self {
        self.headless = Some(frames);
        self
    }
    /// Sets the native application title.
    pub fn with_title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }
    /// Sets the initial window size.
    pub fn with_size(mut self, size: [u32; 2]) -> Self {
        self.size = size.map(|v| v.max(1));
        self
    }
    /// Sets the scene canvas element identifier.
    pub fn with_canvas_id(mut self, id: &'a str) -> Self {
        self.canvas_id = id;
        self
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn take_native_context(&mut self) -> Option<tauri::Context<tauri::Wry>> {
        self.native_context.take()
    }

    /// Uses a consumer-provided Tauri context and frontend assets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_tauri_context(mut self, context: tauri::Context<tauri::Wry>) -> Self {
        self.native_context = Some(context);
        self
    }
    /// Selects a registered React application component.
    pub fn with_ui(mut self, ui: &'a str) -> Self {
        self.ui = ui;
        self
    }
    /// Configures WGPU backend, adapter, features, and limits.
    pub fn with_wgpu_options(mut self, options: impl FnOnce(&mut WgpuConfiguration) + 'a) -> Self {
        self.wgpu_options_customized = true;
        self.wgpu_options = Box::new(options);
        self
    }
    /// Controls native environment logger initialization.
    pub fn with_env_logger(mut self, enabled: bool) -> Self {
        self.env_logger = enabled;
        self
    }
    /// Controls browser panic reporting.
    pub fn with_console_error_panic_hook(mut self, enabled: bool) -> Self {
        self.console_error_panic_hook = enabled;
        self
    }
    /// Controls use of the bundled fonts by the React host.
    pub fn with_bundled_fonts(mut self, enabled: bool) -> Self {
        self.bundle_fonts = enabled;
        self
    }
}
