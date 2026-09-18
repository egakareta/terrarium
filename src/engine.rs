#[cfg(target_arch = "wasm32")]
use std::cell::{RefCell, RefMut};
use std::ops::{Deref, DerefMut};
#[cfg(target_arch = "wasm32")]
use std::rc::{Rc, Weak};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{Renderer, RendererError, Workspace, eframe, egui, egui_wgpu};

const DEFAULT_CLEAR_COLOR: [f32; 4] = [0.018, 0.028, 0.065, 1.0];

/// Error returned while creating an application.
pub type AppCreationError = Box<dyn std::error::Error + Send + Sync>;

#[cfg(target_arch = "wasm32")]
type RendererHandle = Rc<RefCell<Renderer>>;

#[cfg(not(target_arch = "wasm32"))]
type RendererHandle = Arc<Mutex<Renderer>>;

// WebGPU objects are not transferable between browser threads. Keep the renderer on its owning
// thread and let the Send + Sync callback carry only an index into this thread-local registry.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static RENDERERS: RefCell<Vec<Option<Weak<RefCell<Renderer>>>>>
        = const { RefCell::new(Vec::new()) };
}

#[cfg(target_arch = "wasm32")]
fn register_renderer(renderer: &RendererHandle) -> usize {
    RENDERERS.with(|renderers| {
        let mut renderers = renderers.borrow_mut();
        let id = renderers.len();
        renderers.push(Some(Rc::downgrade(renderer)));
        id
    })
}

#[cfg(target_arch = "wasm32")]
fn unregister_renderer(id: usize) {
    RENDERERS.with(|renderers| {
        if let Some(renderer) = renderers.borrow_mut().get_mut(id) {
            *renderer = None;
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn with_renderer<R>(id: usize, f: impl FnOnce(&mut Renderer) -> R) -> R {
    let renderer = RENDERERS.with(|renderers| {
        renderers
            .borrow()
            .get(id)
            .and_then(Option::as_ref)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| panic!("engine renderer {id} is no longer available"))
    });
    f(&mut renderer.borrow_mut())
}

struct SceneCallback {
    #[cfg(not(target_arch = "wasm32"))]
    renderer: Arc<Mutex<Renderer>>,
    #[cfg(target_arch = "wasm32")]
    renderer_id: usize,
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut egui_wgpu::wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        self.renderer
            .lock()
            .expect("engine renderer lock poisoned")
            .paint_eframe_scene(render_pass);

        #[cfg(target_arch = "wasm32")]
        with_renderer(self.renderer_id, |renderer| {
            renderer.paint_eframe_scene(render_pass);
        });
    }
}

/// Owns a [`Workspace`] and renderer integrated with [`eframe`].
pub struct Engine {
    renderer: RendererHandle,
    /// The [`Workspace`] rendered by this engine.
    pub workspace: Workspace,
    #[cfg(target_arch = "wasm32")]
    renderer_id: usize,
    clear_color: [f32; 4],
}

impl Engine {
    /// Creates the engine from eframe's WGPU creation context.
    ///
    /// `fallback_size` is used on web and when no native window is available. The renderer is
    /// automatically resized to the UI region when [`Self::render`] is called.
    pub fn new(
        creation_context: &eframe::CreationContext<'_>,
        fallback_size: [u32; 2],
    ) -> Result<Self, RendererError> {
        #[cfg(not(target_arch = "wasm32"))]
        let size = creation_context
            .winit_window()
            .map(|window| window.inner_size())
            .map_or(fallback_size, |size| [size.width, size.height]);
        #[cfg(target_arch = "wasm32")]
        let size = fallback_size;

        let render_state = creation_context
            .wgpu_render_state
            .as_ref()
            .ok_or(RendererError::MissingEframeWgpuRenderState)?;
        let renderer = Renderer::new(render_state, [size[0].max(1), size[1].max(1)])?;
        #[cfg(target_arch = "wasm32")]
        let renderer = Rc::new(RefCell::new(renderer));
        #[cfg(not(target_arch = "wasm32"))]
        let renderer = Arc::new(Mutex::new(renderer));

        #[cfg(target_arch = "wasm32")]
        let renderer_id = register_renderer(&renderer);

        Ok(Self {
            renderer,
            workspace: Workspace::new(),
            #[cfg(target_arch = "wasm32")]
            renderer_id,
            clear_color: DEFAULT_CLEAR_COLOR,
        })
    }

    /// Returns the color eframe should use to clear the window.
    pub fn clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Sets both eframe's window clear color and Terrarium's scene clear color.
    pub fn set_clear_color(&mut self, color: [f32; 4]) {
        self.clear_color = color;
        self.renderer().set_clear_color(egui_wgpu::wgpu::Color {
            r: color[0] as f64,
            g: color[1] as f64,
            b: color[2] as f64,
            a: color[3] as f64,
        });
    }

    /// Returns locked access to the underlying renderer for advanced operations.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn renderer(&self) -> MutexGuard<'_, Renderer> {
        self.renderer.lock().expect("engine renderer lock poisoned")
    }

    /// Returns mutable access to the underlying renderer for advanced operations.
    #[cfg(target_arch = "wasm32")]
    pub fn renderer(&self) -> RefMut<'_, Renderer> {
        self.renderer.borrow_mut()
    }

    /// Processes camera input, advances the workspace, and requests the next frame.
    pub fn update(&mut self, context: &egui::Context) {
        let delta = context.input(|input| input.stable_dt.min(0.1));
        context.input(|input| self.workspace.process_eframe_input(input));
        self.workspace.update(delta);
        context.request_repaint();
    }

    /// Prepares the workspace for the available eframe UI region.
    pub fn prepare(&mut self, ui: &mut egui::Ui) -> Result<(), RendererError> {
        let rect = ui.max_rect();
        let pixels_per_point = ui.pixels_per_point();
        let size = [
            (rect.width() * pixels_per_point).round().max(1.0) as u32,
            (rect.height() * pixels_per_point).round().max(1.0) as u32,
        ];
        self.workspace.current_camera.resize(size[0], size[1]);
        self.renderer().prepare_eframe_scene(&self.workspace, size)
    }

    /// Registers the paint callback for the workspace prepared with [`Self::prepare`].
    pub fn paint(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        #[cfg(not(target_arch = "wasm32"))]
        let callback = SceneCallback {
            renderer: Arc::clone(&self.renderer),
        };
        #[cfg(target_arch = "wasm32")]
        let callback = SceneCallback {
            renderer_id: self.renderer_id,
        };
        let callback = egui_wgpu::Callback::new_paint_callback(rect, callback);
        ui.painter().add(egui::Shape::Callback(callback));
    }

    /// Prepares and paints the workspace into the available eframe UI region.
    pub fn render(&mut self, ui: &mut egui::Ui) -> Result<(), RendererError> {
        self.prepare(ui)?;
        self.paint(ui);
        Ok(())
    }

    fn start(config: RunConfig<'_>, app_creator: eframe::AppCreator<'static>) -> eframe::Result {
        if config.env_logger {
            env_logger::init();
        }

        let mut wgpu_options = egui_wgpu::WgpuConfiguration::default();
        (config.wgpu_options)(&mut wgpu_options);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let native_options = eframe::NativeOptions {
                renderer: eframe::Renderer::Wgpu,
                viewport: egui::ViewportBuilder::default()
                    .with_title(config.title)
                    .with_inner_size([config.size[0] as f32, config.size[1] as f32]),
                wgpu_options,
                ..Default::default()
            };

            eframe::run_native(config.title, native_options, app_creator)
        }

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast as _;

            if config.console_error_panic_hook {
                console_error_panic_hook::set_once();
            }

            let canvas_id = config.canvas_id.to_owned();

            wasm_bindgen_futures::spawn_local(async move {
                let canvas = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id(&canvas_id))
                    .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
                    .unwrap_or_else(|| panic!("failed to find canvas `{canvas_id}`"));

                let web_options = eframe::WebOptions {
                    renderer: eframe::Renderer::Wgpu,
                    wgpu_options,
                    ..Default::default()
                };

                eframe::WebRunner::new()
                    .start(canvas, web_options, app_creator)
                    .await
                    .expect("failed to start eframe");
            });

            Ok(())
        }
    }
}

impl eframe::App for Engine {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Engine::clear_color(self)
    }

    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.update(context);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Err(error) = self.render(ui) {
            log::error!("scene preparation failed: {error}");
        }
    }
}

/// Application behavior run by an [`Engine`].
///
/// The default methods run Terrarium's complete update and rendering lifecycle. Applications can
/// add logic through [`Self::after_update`] and egui content through [`Self::ui`] without
/// reimplementing that lifecycle, or override [`Self::logic`] and [`Self::render`] when they need
/// complete control over a phase.
pub trait App: 'static {
    /// Returns the color eframe should use to clear the window.
    fn clear_color(&self, engine: &Engine, _visuals: &egui::Visuals) -> [f32; 4] {
        engine.clear_color()
    }

    /// Processes one application update.
    fn logic(&mut self, engine: &mut Engine, context: &egui::Context, frame: &mut eframe::Frame) {
        engine.update(context);
        self.after_update(engine, context, frame);
    }

    /// Runs after Terrarium has processed input and updated the workspace.
    fn after_update(
        &mut self,
        _engine: &mut Engine,
        _context: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
    }

    /// Renders one application UI frame.
    fn render(&mut self, engine: &mut Engine, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Err(error) = engine.render(ui) {
            self.on_ui_error(engine, error, &ui.ctx().clone());
        }
        self.ui(engine, ui, frame);
    }

    /// Draws application UI after Terrarium has attempted to render the scene.
    fn ui(&mut self, _engine: &mut Engine, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {}

    /// Handles an error produced while preparing the scene for rendering.
    fn on_ui_error(
        &mut self,
        _engine: &mut Engine,
        error: RendererError,
        _context: &egui::Context,
    ) {
        log::error!("scene preparation failed: {error}");
    }

    /// Manipulates raw egui input before it is processed.
    fn raw_input_hook(
        &mut self,
        _engine: &mut Engine,
        _context: &egui::Context,
        _raw_input: &mut egui::RawInput,
    ) {
    }
}

impl App for () {}

struct AppAdapter<A> {
    engine: Engine,
    app: A,
}

impl<A: App> eframe::App for AppAdapter<A> {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.app.clear_color(&self.engine, visuals)
    }

    fn logic(&mut self, context: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(&mut self.engine, context, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.render(&mut self.engine, ui, frame);
    }

    fn raw_input_hook(&mut self, context: &egui::Context, raw_input: &mut egui::RawInput) {
        self.app
            .raw_input_hook(&mut self.engine, context, raw_input);
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Engine {
    fn drop(&mut self) {
        unregister_renderer(self.renderer_id);
    }
}

impl Deref for Engine {
    type Target = Workspace;
    fn deref(&self) -> &Self::Target {
        &self.workspace
    }
}

impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.workspace
    }
}

/// Options controlling the behavior of the window.
pub struct RunConfig<'a> {
    title: &'a str,
    size: [u32; 2],
    #[cfg(target_arch = "wasm32")]
    canvas_id: &'a str,
    wgpu_options: Box<dyn FnOnce(&mut egui_wgpu::WgpuConfiguration) + 'a>,
    env_logger: bool,
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook: bool,
}

impl<'a> Default for RunConfig<'a> {
    fn default() -> Self {
        Self {
            title: "app",
            size: [1280, 720],
            #[cfg(target_arch = "wasm32")]
            canvas_id: "app",
            wgpu_options: Box::new(|_| {}),
            env_logger: true,
            #[cfg(target_arch = "wasm32")]
            console_error_panic_hook: true,
        }
    }
}

impl<'a> RunConfig<'a> {
    /// Equivalent to [`RunConfig::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an [`Engine`] and runs the initialized [`App`].
    ///
    /// The configured window size is used as the renderer's fallback size. Return `()` from the
    /// initializer when no additional application behavior is needed.
    pub fn run<A, E>(
        self,
        initialize: impl FnOnce(&eframe::CreationContext<'_>, &mut Engine) -> Result<A, E> + 'static,
    ) -> eframe::Result
    where
        A: App,
        E: Into<AppCreationError>,
    {
        let size = self.size;
        Engine::start(
            self,
            Box::new(move |creation_context| {
                let mut engine = Engine::new(creation_context, size)?;
                let app = initialize(creation_context, &mut engine)
                    .map_err(|error| -> AppCreationError { error.into() })?;
                Ok(Box::new(AppAdapter { engine, app }))
            }),
        )
    }

    /// The application title on native platforms.
    pub fn with_title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    /// The window size on native platforms.
    pub fn with_size(mut self, size: [u32; 2]) -> Self {
        self.size = size;
        self
    }

    /// The element to render the web application.
    pub fn with_canvas_id(self, canvas_id: &'a str) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let mut this = self;
            this.canvas_id = canvas_id;
            this
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = canvas_id;
            self
        }
    }

    /// Configures wgpu instance/device/adapter/surface creation and renderloop.
    pub fn with_wgpu_options(
        mut self,
        f: impl FnOnce(&mut egui_wgpu::WgpuConfiguration) + 'a,
    ) -> Self {
        self.wgpu_options = Box::new(f);
        self
    }

    /// Whether to automatically call [`env_logger::init()`].
    ///
    /// By default, this is enabled.
    pub fn with_env_logger(mut self, enabled: bool) -> Self {
        self.env_logger = enabled;
        self
    }

    /// Whether to automatically call [`console_error_panic_hook::set_once()`].
    ///
    /// By default, this is enabled.
    pub fn with_console_error_panic_hook(self, enabled: bool) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let mut this = self;
            this.console_error_panic_hook = enabled;
            this
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = enabled;
            self
        }
    }
}
