#[cfg(target_arch = "wasm32")]
use std::cell::RefMut;
#[cfg(not(target_arch = "wasm32"))]
use std::rc::Rc;
#[cfg(target_arch = "wasm32")]
use std::rc::{Rc, Weak};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
};

use crate::{
    trscene::{InstanceId, Workspace, WorkspacePreset},
    *,
};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
unsafe extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str, error: &wasm_bindgen::JsValue);
}

const DEFAULT_CLEAR_COLOR: [f32; 4] = [0.018, 0.028, 0.065, 1.0];

/// Error returned while creating an application.
pub type AppCreationError = Box<dyn std::error::Error + Send + Sync>;

#[cfg(target_arch = "wasm32")]
type RendererHandle = Rc<RefCell<RenderAdapter>>;

#[cfg(not(target_arch = "wasm32"))]
type RendererHandle = Arc<Mutex<RenderAdapter>>;

type EngineSlot = Rc<RefCell<Option<Engine>>>;

#[cfg(not(target_arch = "wasm32"))]
static DEFAULT_HEADLESS_RENDER_STATE: OnceLock<Result<egui_wgpu::RenderState, String>> =
    OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
fn default_headless_render_state(
    options: &egui_wgpu::WgpuConfiguration,
) -> Result<egui_wgpu::RenderState, eframe::Error> {
    let cached = DEFAULT_HEADLESS_RENDER_STATE.get_or_init(|| {
        let mut options = options.clone();
        if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_setup {
            let device_descriptor = setup.device_descriptor.clone();
            setup.device_descriptor = Arc::new(move |adapter| {
                let mut descriptor = device_descriptor(adapter);
                if adapter
                    .features()
                    .contains(egui_wgpu::wgpu::Features::PIPELINE_CACHE)
                {
                    descriptor.required_features |= egui_wgpu::wgpu::Features::PIPELINE_CACHE;
                }
                descriptor
            });
        }
        let instance = pollster::block_on(options.wgpu_setup.new_instance());
        pollster::block_on(egui_wgpu::RenderState::create(
            &options,
            &instance,
            None,
            egui_wgpu::RendererOptions::default(),
        ))
        .map_err(|error| error.to_string())
    });

    match cached {
        Ok(render_state) => Ok(render_state.clone()),
        Err(error) => Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
            error.clone(),
        )))),
    }
}

// WebGPU objects are not transferable between browser threads. Keep the renderer on its owning
// thread and let the Send + Sync callback carry only an index into this thread-local registry.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static RENDERERS: RefCell<Vec<Option<Weak<RefCell<RenderAdapter>>>>>
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
fn with_renderer<R>(id: usize, f: impl FnOnce(&mut RenderAdapter) -> R) -> R {
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
    renderer: Arc<Mutex<RenderAdapter>>,
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
    /// The docked workspace inspector.
    #[cfg(feature = "devtools")]
    pub devtools: trdevtools::Devtools,
    #[cfg(feature = "devtools")]
    title: String,
    #[cfg(feature = "devtools")]
    devtools_render_selection: Option<Vec<InstanceId>>,
    creation_context: eframe::CreationContext<'static>,
    #[cfg(target_arch = "wasm32")]
    renderer_id: usize,
    clear_color: [f32; 4],
    #[cfg(feature = "bevy")]
    bevy_app: Option<bevy_app::App>,
}

impl Engine {
    /// Creates the engine with the supplied workspace from eframe's WGPU creation context.
    ///
    /// `fallback_size` is used on web and when no native window is available. The renderer is
    /// automatically resized to the UI region when [`Self::render`] is called.
    pub fn new(
        creation_context: &eframe::CreationContext<'_>,
        fallback_size: [u32; 2],
        workspace: Workspace,
    ) -> Result<Self, RendererError> {
        #[cfg(not(target_arch = "wasm32"))]
        let size = creation_context
            .winit_window()
            .map(|window| window.inner_size())
            .map_or(fallback_size, |size| [size.width, size.height]);
        #[cfg(target_arch = "wasm32")]
        let size = fallback_size;
        #[cfg(target_arch = "wasm32")]
        let mut workspace = workspace;
        #[cfg(target_arch = "wasm32")]
        if web_sys::window().is_some_and(|window| window.navigator().max_touch_points() > 0) {
            workspace
                .camera_controller_mut()
                .set_touchscreen_detected(true);
        }

        let render_state = creation_context
            .wgpu_render_state
            .as_ref()
            .ok_or(RendererError::MissingEframeWgpuRenderState)?;
        let renderer = RenderAdapter::new(render_state, [size[0].max(1), size[1].max(1)])?;
        #[cfg(target_arch = "wasm32")]
        let renderer = Rc::new(RefCell::new(renderer));
        #[cfg(not(target_arch = "wasm32"))]
        let renderer = Arc::new(Mutex::new(renderer));

        #[cfg(target_arch = "wasm32")]
        let renderer_id = register_renderer(&renderer);

        Ok(Self {
            renderer,
            workspace,
            #[cfg(feature = "devtools")]
            devtools: {
                let mut devtools = trdevtools::Devtools::default();
                devtools.set_visible(true);
                devtools
            },
            #[cfg(feature = "devtools")]
            title: "Terrarium".to_owned(),
            #[cfg(feature = "devtools")]
            devtools_render_selection: None,
            creation_context: Self::owned_creation_context(creation_context),
            #[cfg(target_arch = "wasm32")]
            renderer_id,
            clear_color: DEFAULT_CLEAR_COLOR,
            #[cfg(feature = "bevy")]
            bevy_app: None,
        })
    }

    fn owned_creation_context(
        creation_context: &eframe::CreationContext<'_>,
    ) -> eframe::CreationContext<'static> {
        let mut owned = eframe::CreationContext::_new_kittest(creation_context.egui_ctx.clone());
        owned.integration_info = creation_context.integration_info.clone();
        owned.wgpu_render_state = creation_context.wgpu_render_state.clone();
        owned
    }

    /// Returns the eframe creation context associated with this engine.
    ///
    /// The context retains the egui context, integration information, and WGPU render state from
    /// application creation. It is owned by the engine so it remains available after the eframe
    /// app-creation callback returns.
    pub fn creation_context(&self) -> &eframe::CreationContext<'_> {
        &self.creation_context
    }

    /// Returns the Bevy app attached through [`Terrarium::with_bevy`], if any.
    #[cfg(feature = "bevy")]
    pub fn bevy_app(&self) -> Option<&bevy_app::App> {
        self.bevy_app.as_ref()
    }

    /// Returns mutable access to the Bevy app attached through [`Terrarium::with_bevy`], if any.
    #[cfg(feature = "bevy")]
    pub fn bevy_app_mut(&mut self) -> Option<&mut bevy_app::App> {
        self.bevy_app.as_mut()
    }

    /// Returns the color eframe should use to clear the window.
    pub fn clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Sets both eframe's window clear color and Terrarium's scene clear color.
    ///
    pub fn with_clear_color(&mut self, color: [f32; 4]) -> &mut Self {
        self.clear_color = color;
        self.renderer().with_clear_color(egui_wgpu::wgpu::Color {
            r: color[0] as f64,
            g: color[1] as f64,
            b: color[2] as f64,
            a: color[3] as f64,
        });
        self
    }

    /// Returns locked access to the underlying renderer for advanced operations.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn renderer(&self) -> MutexGuard<'_, RenderAdapter> {
        self.renderer.lock().expect("engine renderer lock poisoned")
    }

    /// Returns mutable access to the underlying renderer for advanced operations.
    #[cfg(target_arch = "wasm32")]
    pub fn renderer(&self) -> RefMut<'_, RenderAdapter> {
        self.renderer.borrow_mut()
    }

    /// Processes controller input, advances the workspace, and requests the next frame.
    pub fn update(&mut self, context: &egui::Context) {
        #[cfg(feature = "devtools")]
        self.devtools
            .focus_scene_on_camera_input(context, &self.workspace);
        let delta = process_workspace_input(&mut self.workspace, context);
        #[cfg(feature = "bevy")]
        if let Some(app) = self.bevy_app.as_mut() {
            crate::bevy::update_app(app, &mut self.workspace, context, delta);
            context.request_repaint();
            return;
        }
        self.workspace.update(delta);
        context.request_repaint();
    }

    /// Prepares the workspace for the available eframe UI region.
    pub fn prepare(&mut self, ui: &mut egui::Ui) -> Result<(), RendererError> {
        #[cfg(feature = "devtools")]
        let selected = self
            .devtools_render_selection
            .as_deref()
            .unwrap_or_else(|| self.devtools.selected())
            .to_vec();
        #[cfg(not(feature = "devtools"))]
        let selected: Vec<InstanceId> = Vec::new();
        prepare_workspace(&self.renderer, &mut self.workspace, ui, &selected)
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

    fn start(config: Terrarium<'_>, app_creator: eframe::AppCreator<'static>) -> eframe::Result {
        #[cfg(not(target_arch = "wasm32"))]
        if config.env_logger {
            let _ = env_logger::try_init();
        }
        #[cfg(target_arch = "wasm32")]
        let _ = config.env_logger;

        let mut wgpu_options = egui_wgpu::WgpuConfiguration::default();
        (config.wgpu_options)(&mut wgpu_options);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let headless_frame_limit = match config.headless {
                HeadlessMode::Disabled => None,
                HeadlessMode::Frames(frame_count) => Some(Some(frame_count)),
                HeadlessMode::UntilClose => Some(None),
            };
            if let Some(mut frames_remaining) = headless_frame_limit {
                let render_state = if config.wgpu_options_customized {
                    // TODO: maybe actual caching for custom wgpu_options?
                    let instance = pollster::block_on(wgpu_options.wgpu_setup.new_instance());
                    pollster::block_on(egui_wgpu::RenderState::create(
                        &wgpu_options,
                        &instance,
                        None,
                        egui_wgpu::RendererOptions::default(),
                    ))?
                } else {
                    default_headless_render_state(&wgpu_options)?
                };
                let context = egui::Context::default();
                let mut creation_context = eframe::CreationContext::_new_kittest(context.clone());
                creation_context.wgpu_render_state = Some(render_state);
                let mut app = app_creator(&creation_context).map_err(eframe::Error::AppCreation)?;
                let mut frame = eframe::Frame::_new_kittest();
                while !matches!(frames_remaining, Some(0)) {
                    let mut raw_input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(config.size[0] as f32, config.size[1] as f32),
                        )),
                        ..Default::default()
                    };
                    app.raw_input_hook(&context, &mut raw_input);
                    let mut output = context.run_ui(raw_input, |ui| {
                        app.logic(ui.ctx(), &mut frame);
                        app.ui(ui, &mut frame);
                    });
                    let close_requested = output
                        .viewport_output
                        .get(&egui::ViewportId::ROOT)
                        .is_some_and(|viewport| {
                            viewport.commands.contains(&egui::ViewportCommand::Close)
                        });
                    output.textures_delta.clear();

                    if close_requested {
                        break;
                    }
                    if let Some(frames_remaining) = &mut frames_remaining {
                        *frames_remaining -= 1;
                    }
                }
                return Ok(());
            }

            let native_options = eframe::NativeOptions {
                renderer: eframe::Renderer::Wgpu,
                viewport: native_viewport(config.title, config.size, config.size_explicit),
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
                canvas
                    .style()
                    .set_property("touch-action", "none")
                    .expect("failed to disable browser touch gestures on the canvas");

                let web_options = eframe::WebOptions {
                    renderer: eframe::Renderer::Wgpu,
                    wgpu_options,
                    ..Default::default()
                };

                if let Err(error) = eframe::WebRunner::new()
                    .start(canvas, web_options, app_creator)
                    .await
                {
                    console_error("failed to start eframe:", &error);
                }
            });

            Ok(())
        }
    }
}

fn process_workspace_input(workspace: &mut Workspace, context: &egui::Context) -> f32 {
    let delta = context.input(|input| input.stable_dt);
    let delta = if delta.is_finite() {
        delta.clamp(0.0, 0.1)
    } else {
        0.0
    };
    workspace.process_eframe_context(context);
    delta
}

#[cfg(target_arch = "wasm32")]
fn normalize_web_canvas_input(context: &egui::Context, raw_input: &mut egui::RawInput) {
    use wasm_bindgen::JsCast;

    let Some(canvas) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.query_selector("canvas").ok().flatten())
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        return;
    };

    let bounds = canvas.get_bounding_client_rect();
    let mut width = bounds.width() as f32;
    let mut height = bounds.height() as f32;
    if let Some(style) =
        web_sys::window().and_then(|window| window.get_computed_style(&canvas).ok().flatten())
    {
        let padding = |property| {
            style
                .get_property_value(property)
                .ok()
                .and_then(|value| value.trim_end_matches("px").parse::<f32>().ok())
                .unwrap_or_default()
        };
        width -= padding("padding-left") + padding("padding-right");
        height -= padding("padding-top") + padding("padding-bottom");
    }

    let zoom_factor = context.zoom_factor();
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || !zoom_factor.is_finite()
        || zoom_factor <= 0.0
    {
        return;
    }

    let pixels_per_point = (canvas.width() as f32 / width + canvas.height() as f32 / height) * 0.5;
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return;
    }

    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(width / zoom_factor, height / zoom_factor),
    ));
    raw_input
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(pixels_per_point);
}

fn configure_egui_style(context: &egui::Context) {
    let surface = egui::Color32::from_rgb(13, 20, 30);
    let window = egui::Color32::from_rgb(19, 29, 42);
    let widget = egui::Color32::from_rgb(26, 39, 53);
    let hovered = egui::Color32::from_rgb(35, 56, 70);
    let active = egui::Color32::from_rgb(28, 75, 76);
    let accent = egui::Color32::from_rgb(77, 215, 194);
    let text = egui::Color32::from_rgb(230, 238, 245);
    let muted_text = egui::Color32::from_rgb(148, 167, 183);
    let border = egui::Color32::from_rgb(48, 69, 84);

    context.all_styles_mut(|style| {
        style.animation_time = 0.16;

        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(text);
        visuals.weak_text_color = Some(muted_text);
        visuals.hyperlink_color = accent;
        visuals.faint_bg_color = egui::Color32::from_rgb(18, 29, 40);
        visuals.extreme_bg_color = egui::Color32::from_rgb(8, 12, 18);
        visuals.text_edit_bg_color = Some(widget);
        visuals.code_bg_color = widget;
        visuals.panel_fill = surface;
        visuals.window_fill = window;
        visuals.window_stroke = egui::Stroke::new(1.0, border);
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.selection.bg_fill = egui::Color32::from_rgb(34, 87, 85);
        visuals.selection.stroke = egui::Stroke::new(1.0, accent);

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = surface;
        widgets.noninteractive.weak_bg_fill = surface;
        widgets.noninteractive.fg_stroke.color = text;

        widgets.inactive.bg_fill = widget;
        widgets.inactive.weak_bg_fill = widget;
        widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border);
        widgets.inactive.fg_stroke.color = text;

        widgets.hovered.bg_fill = hovered;
        widgets.hovered.weak_bg_fill = hovered;
        widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
        widgets.hovered.fg_stroke.color = text;

        widgets.active.bg_fill = active;
        widgets.active.weak_bg_fill = active;
        widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
        widgets.active.fg_stroke.color = text;

        widgets.open.bg_fill = active;
        widgets.open.weak_bg_fill = active;
        widgets.open.bg_stroke = egui::Stroke::new(1.0, accent);
        widgets.open.fg_stroke.color = text;

        for widget in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(4);
        }
    });
    context.set_theme(egui::ThemePreference::Dark);
}

fn prepare_workspace(
    renderer: &RendererHandle,
    workspace: &mut Workspace,
    ui: &mut egui::Ui,
    selected: &[InstanceId],
) -> Result<(), RendererError> {
    let rect = ui.max_rect();
    let pixels_per_point = ui.pixels_per_point();
    let size = [
        (rect.width() * pixels_per_point).round().max(1.0) as u32,
        (rect.height() * pixels_per_point).round().max(1.0) as u32,
    ];
    workspace.current_camera.resize(size[0], size[1]);
    #[cfg(not(target_arch = "wasm32"))]
    let mut renderer = renderer.lock().expect("engine renderer lock poisoned");
    #[cfg(target_arch = "wasm32")]
    let mut renderer = renderer.borrow_mut();
    renderer.prepare_eframe_scene(workspace, size, selected)
}

#[cfg(not(target_arch = "wasm32"))]
fn native_viewport(title: &str, size: [u32; 2], size_explicit: bool) -> egui::ViewportBuilder {
    let viewport = egui::ViewportBuilder::default().with_title(title);
    if size_explicit {
        viewport.with_inner_size([size[0] as f32, size[1] as f32])
    } else {
        viewport.with_maximized(true)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn native_window_defaults_to_maximized_unless_sized() {
        let default = Terrarium::default();
        let default_viewport = native_viewport(default.title, default.size, default.size_explicit);
        assert_eq!(default_viewport.maximized, Some(true));
        assert_eq!(default_viewport.fullscreen, None);
        assert_eq!(default_viewport.inner_size, None);

        let sized = Terrarium::default().with_size([960, 540]);
        let sized_viewport = native_viewport(sized.title, sized.size, sized.size_explicit);
        assert_eq!(sized_viewport.fullscreen, None);
        assert_eq!(sized_viewport.inner_size, Some(egui::vec2(960.0, 540.0)));
    }
}

#[cfg(feature = "devtools")]
impl trdevtools::DevtoolsHost for Engine {
    fn scene_title(&self) -> &str {
        &self.title
    }

    fn workspace(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    fn render_scene(&mut self, ui: &mut egui::Ui, selected: &[InstanceId]) {
        let previous = self.devtools_render_selection.replace(selected.to_vec());
        if let Err(error) = self.render(ui) {
            log::error!("scene preparation failed: {error}");
        }
        self.devtools_render_selection = previous;
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
        #[cfg(feature = "devtools")]
        {
            let mut devtools = std::mem::take(&mut self.devtools);
            devtools.show_docked(ui, self);
            self.devtools = devtools;
        }
        #[cfg(not(feature = "devtools"))]
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

#[cfg(feature = "devtools")]
struct AppAdapterDevtoolsHost<'a, A> {
    engine: &'a mut Engine,
    app: &'a mut A,
    frame: &'a mut eframe::Frame,
}

#[cfg(feature = "devtools")]
impl<A: App> trdevtools::DevtoolsHost for AppAdapterDevtoolsHost<'_, A> {
    fn scene_title(&self) -> &str {
        &self.engine.title
    }

    fn workspace(&mut self) -> &mut Workspace {
        &mut self.engine.workspace
    }

    fn render_scene(&mut self, ui: &mut egui::Ui, selected: &[InstanceId]) {
        let previous = self
            .engine
            .devtools_render_selection
            .replace(selected.to_vec());
        self.app.render(self.engine, ui, self.frame);
        self.engine.devtools_render_selection = previous;
    }
}

struct AppAdapter<A> {
    engine: EngineSlot,
    app: A,
}

fn with_engine<R>(slot: &EngineSlot, f: impl FnOnce(&mut Engine) -> R) -> R {
    let mut engine = slot.borrow_mut();
    f(engine
        .as_mut()
        .expect("engine was not initialized before the app ran"))
}

fn store_engine(slot: &EngineSlot, engine: Engine) {
    *slot.borrow_mut() = Some(engine);
}

impl<A: App> eframe::App for AppAdapter<A> {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        with_engine(&self.engine, |engine| self.app.clear_color(engine, visuals))
    }

    fn logic(&mut self, context: &egui::Context, frame: &mut eframe::Frame) {
        with_engine(&self.engine, |engine| {
            self.app.logic(engine, context, frame);
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        with_engine(&self.engine, |engine| {
            #[cfg(feature = "devtools")]
            {
                let mut devtools = std::mem::take(&mut engine.devtools);
                {
                    let mut host = AppAdapterDevtoolsHost {
                        engine,
                        app: &mut self.app,
                        frame,
                    };
                    devtools.show_docked(ui, &mut host);
                }
                engine.devtools = devtools;
            }
            #[cfg(not(feature = "devtools"))]
            self.app.render(engine, ui, frame);
        });
    }

    fn raw_input_hook(&mut self, context: &egui::Context, raw_input: &mut egui::RawInput) {
        #[cfg(target_arch = "wasm32")]
        normalize_web_canvas_input(context, raw_input);
        with_engine(&self.engine, |engine| {
            self.app.raw_input_hook(engine, context, raw_input);
        });
    }
}

fn take_engine(slot: EngineSlot) -> Option<Engine> {
    Rc::try_unwrap(slot).ok()?.into_inner()
}

#[cfg(target_arch = "wasm32")]
impl Drop for Engine {
    fn drop(&mut self) {
        unregister_renderer(self.renderer_id);
    }
}

/// Convenience access to the owned [`Workspace`].
///
/// Method calls and field accesses that are not found on [`Engine`] itself
/// resolve to its workspace, so `engine.add_child(part)` works without
/// spelling out `engine.workspace`.
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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
enum HeadlessMode {
    Disabled,
    Frames(usize),
    UntilClose,
}

/// The primary entry point for configuring and running a Terrarium application.
pub struct Terrarium<'a> {
    workspace_preset: WorkspacePreset,
    #[cfg(feature = "bevy")]
    bevy_app: Option<bevy_app::App>,
    title: &'a str,
    size: [u32; 2],
    #[cfg(not(target_arch = "wasm32"))]
    size_explicit: bool,
    #[cfg(target_arch = "wasm32")]
    canvas_id: &'a str,
    wgpu_options: Box<dyn FnOnce(&mut egui_wgpu::WgpuConfiguration) + 'a>,
    wgpu_options_customized: bool,
    env_logger: bool,
    #[cfg(not(target_arch = "wasm32"))]
    headless: HeadlessMode,
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook: bool,
    bundle_fonts: bool,
}

impl<'a> Default for Terrarium<'a> {
    fn default() -> Self {
        Self {
            workspace_preset: WorkspacePreset::default(),
            #[cfg(feature = "bevy")]
            bevy_app: None,
            title: "App",
            size: [1280, 720],
            #[cfg(not(target_arch = "wasm32"))]
            size_explicit: false,
            #[cfg(target_arch = "wasm32")]
            canvas_id: "app",
            wgpu_options: Box::new(|_| {}),
            wgpu_options_customized: false,
            env_logger: true,
            #[cfg(not(target_arch = "wasm32"))]
            headless: HeadlessMode::Disabled,
            #[cfg(target_arch = "wasm32")]
            console_error_panic_hook: true,
            bundle_fonts: cfg!(feature = "default-fonts"),
        }
    }
}

impl<'a> Terrarium<'a> {
    /// Creates a Terrarium application launcher with an empty workspace and default settings.
    ///
    /// Equivalent to [`Terrarium::default()`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a launcher with the selected starting geometry.
    ///
    /// The preset is applied before the application initializer runs.
    /// [`WorkspacePreset::Empty`] is equivalent to [`Self::new`].
    /// A workspace supplied to Bevy takes precedence over this preset.
    pub fn preset(preset: WorkspacePreset) -> Self {
        Self {
            workspace_preset: preset,
            ..Self::default()
        }
    }

    /// Configures a Bevy app without changing the [`Self::run`] initializer or application callbacks.
    ///
    /// This only configures the launcher; the adapter is installed when `run(initialize)` creates
    /// the engine. Terrarium adds [`crate::bevy::TerrariumPlugin`] when needed, drives Bevy from
    /// [`Engine::update`], and still owns the event loop and renderer. Do not install Bevy's window
    /// or render plugins. A finalized Bevy app must already contain `TerrariumPlugin`.
    #[cfg(feature = "bevy")]
    pub fn with_bevy(mut self, app: bevy_app::App) -> Self {
        self.bevy_app = Some(app);
        self
    }

    /// Creates an [`Engine`] and runs the initialized [`App`].
    ///
    /// The initializer receives the engine after it has been created. Access eframe's creation
    /// context through [`Engine::creation_context`].
    ///
    /// The configured window size is used as the renderer's fallback size. Return `()` from the
    /// initializer when no additional application behavior is needed.
    ///
    /// Returns the created engine on native platforms, [`None`] on web platforms.
    pub fn run<A, E>(
        self,
        initialize: impl FnOnce(&mut Engine) -> Result<A, E> + 'static,
    ) -> Result<Option<Engine>, AppCreationError>
    where
        A: App,
        E: Into<AppCreationError>,
    {
        #[cfg(feature = "bevy")]
        let (bevy_app, launcher) = {
            let mut launcher = self;
            let app = launcher.bevy_app.take();
            (app, launcher)
        };
        #[cfg(not(feature = "bevy"))]
        let launcher = self;

        let size = launcher.size;
        let preset = launcher.workspace_preset;
        #[cfg(feature = "devtools")]
        let title = launcher.title.to_owned();
        #[cfg(feature = "default-fonts")]
        let is_bundled_fonts = launcher.bundle_fonts;
        let engine_slot: EngineSlot = Rc::new(RefCell::new(None));

        let app_engine_slot = engine_slot.clone();
        let app_creator: eframe::AppCreator<'static> = Box::new(move |creation_context| {
            #[cfg(feature = "default-fonts")]
            let font_definitions = is_bundled_fonts.then(trfont::font_definitions);
            #[cfg(not(feature = "default-fonts"))]
            let font_definitions: Option<egui::FontDefinitions> = None;
            #[cfg(feature = "devtools")]
            let font_definitions = {
                let mut fonts = font_definitions.unwrap_or_default();
                tregui::egui_phosphor::add_to_fonts(
                    &mut fonts,
                    tregui::egui_phosphor::Variant::Regular,
                );
                Some(fonts)
            };
            #[cfg(feature = "devtools")]
            let base_font_definitions = font_definitions
                .clone()
                .unwrap_or_else(egui::FontDefinitions::default);
            if let Some(font_definitions) = font_definitions {
                creation_context.egui_ctx.set_fonts(font_definitions);
            }
            configure_egui_style(&creation_context.egui_ctx);
            let engine = Engine::new(creation_context, size, Workspace::preset(preset))?;
            #[cfg(any(feature = "bevy", feature = "devtools"))]
            let mut engine = engine;
            #[cfg(feature = "devtools")]
            {
                engine.title = title;
            }
            #[cfg(feature = "devtools")]
            engine
                .devtools
                .set_base_font_definitions(base_font_definitions);
            #[cfg(feature = "bevy")]
            if let Some(app) = bevy_app {
                engine.bevy_app = Some(crate::bevy::attach_app(
                    app,
                    &mut engine.workspace,
                    &creation_context.egui_ctx,
                )?);
            }
            store_engine(&app_engine_slot, engine);

            let app = with_engine(&app_engine_slot, initialize)
                .map_err(|error| -> AppCreationError { error.into() })?;
            #[cfg(feature = "bevy")]
            with_engine(&app_engine_slot, |engine| {
                if let Some(app) = engine.bevy_app.as_mut() {
                    crate::bevy::finalize_app(app, &mut engine.workspace);
                }
            });
            Ok(Box::new(AppAdapter {
                engine: app_engine_slot.clone(),
                app,
            }) as Box<dyn eframe::App>)
        });

        Engine::start(launcher, app_creator)?;

        Ok(take_engine(engine_slot))
    }

    /// Runs headlessly instead of opening a native window.
    ///
    /// `Some(frame_count)` runs at most that many frames, while `None` runs indefinitely. A frame
    /// that sends [`egui::ViewportCommand::Close`] stops either mode early. A count of zero
    /// initializes the application without running a frame.
    ///
    /// This has no useful effect on web applications.
    pub fn with_headless(self, frame_count: Option<usize>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut this = self;
            this.headless = match frame_count {
                Some(frame_count) => HeadlessMode::Frames(frame_count),
                None => HeadlessMode::UntilClose,
            };
            this
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = frame_count;
            self
        }
    }

    /// The application title on native platforms.
    pub fn with_title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    /// The window size on native platforms.
    pub fn with_size(mut self, size: [u32; 2]) -> Self {
        self.size = size;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.size_explicit = true;
        }
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
        self.wgpu_options_customized = true;
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

    /// Whether to install Terrarium's bundled fonts.
    ///
    /// By default, this is enabled.
    pub fn with_bundled_fonts(mut self, enabled: bool) -> Self {
        self.bundle_fonts = enabled;
        self
    }
}
