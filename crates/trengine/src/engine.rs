#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
use std::cell::RefMut;
#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
use std::rc::Rc;
#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
use std::rc::{Rc, Weak};
#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
};

use crate::{trscene::*, *};

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
#[wasm_bindgen::prelude::wasm_bindgen]
unsafe extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str, error: &wasm_bindgen::JsValue);
}

const DEFAULT_CLEAR_COLOR: [f32; 4] = [0.018, 0.028, 0.065, 1.0];
const DEFAULT_WINDOW_SIZE: [u32; 2] = [1280, 720];

/// Error returned while creating an application.
pub type AppCreationError = Box<dyn std::error::Error + Send + Sync>;

/// One-shot callback for configuring wgpu before the application starts.
pub type WgpuOptionsCallback = Box<dyn FnOnce(&mut egui_wgpu::WgpuConfiguration)>;

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
type RendererHandle = Rc<RefCell<RenderAdapter>>;

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
type RendererHandle = Arc<Mutex<RenderAdapter>>;

type EngineSlot = Rc<RefCell<Option<Engine>>>;

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
static DEFAULT_HEADLESS_RENDER_STATE: OnceLock<Result<egui_wgpu::RenderState, String>> =
    OnceLock::new();

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
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
#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
thread_local! {
    static RENDERERS: RefCell<Vec<Option<Weak<RefCell<RenderAdapter>>>>>
        = const { RefCell::new(Vec::new()) };
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
fn register_renderer(renderer: &RendererHandle) -> usize {
    RENDERERS.with(|renderers| {
        let mut renderers = renderers.borrow_mut();
        let id = renderers.len();
        renderers.push(Some(Rc::downgrade(renderer)));
        id
    })
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
fn unregister_renderer(id: usize) {
    RENDERERS.with(|renderers| {
        if let Some(renderer) = renderers.borrow_mut().get_mut(id) {
            *renderer = None;
        }
    });
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
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
    #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
    renderer: Arc<Mutex<RenderAdapter>>,
    #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
    renderer_id: usize,
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut egui_wgpu::wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        self.renderer
            .lock()
            .expect("engine renderer lock poisoned")
            .paint_eframe_scene(render_pass);

        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        with_renderer(self.renderer_id, |renderer| {
            renderer.paint_eframe_scene(render_pass);
        });
    }
}

instance! {
    #[instance(default = false, inspect = false)]
    /// The instance-tree root and owner of a [`Workspace`], [`Controller`], and renderer integrated
    /// with [`eframe`]. The default controller is a [`CameraController`].
    ///
    /// Add scene instances through [`Self::workspace_mut`]. The workspace cannot be
    /// destroyed independently, and the live engine does not support instance cloning.
    pub struct Engine {
        #[data]
        instance: Box<InstanceData>,
        /// The controller driven by this engine. Defaults to a camera controller.
        #[property(borrow)]
        pub controller: Box<dyn ControllerView>,
        renderer: RendererHandle,
        /// The docked workspace inspector.
        #[cfg(feature = "devtools")]
        pub devtools: trdevtools::Devtools,
        #[cfg(feature = "devtools")]
        title: String,
        #[cfg(feature = "devtools")]
        devtools_render_selection: Option<Vec<InstanceId>>,
        creation_context: eframe::CreationContext<'static>,
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        renderer_id: usize,
        #[property]
        /// The color used to clear the window and rendered scene.
        clear_color: [f32; 4],
        #[cfg(feature = "bevy")]
        bevy_app: Option<bevy_app::App>,
    }
}

trscene::__instance_inspection_items! {
    impl trscene::devtools::Inspect for Engine {
        fn properties(&self) -> Vec<trscene::devtools::Property> {
            trscene::devtools::Inspect::properties(&self.instance)
        }

        fn validate_property(
            &self,
            path: &str,
            value: &trscene::devtools::PropertyValue,
        ) -> Result<(), trscene::devtools::PropertyError> {
            trscene::devtools::Inspect::validate_property(&self.instance, path, value)
        }

        fn set_property(
            &mut self,
            path: &str,
            value: trscene::devtools::PropertyValue,
        ) -> Result<(), trscene::devtools::PropertyError> {
            trscene::devtools::Inspect::set_property(&mut self.instance, path, value)
        }
    }
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("instance", &self.instance)
            .finish_non_exhaustive()
    }
}

impl InstanceClone for Engine {
    fn try_clone_box(&self) -> Option<Box<dyn Instance>> {
        None
    }
}

fn engine_workspace(instance: &mut InstanceData) -> &mut Workspace {
    instance
        .children_mut()
        .next()
        .expect("engine workspace")
        .downcast_mut::<Workspace>()
        .expect("engine workspace")
}

fn controller_delta(delta_seconds: f32) -> f32 {
    if delta_seconds.is_finite() {
        delta_seconds.clamp(0.0, 0.1)
    } else {
        0.0
    }
}

pub(crate) fn step_engine(
    workspace: &mut Workspace,
    controller: &mut dyn ControllerView,
    delta_seconds: f32,
) {
    workspace.step_tweens(delta_seconds);
    workspace.step_animations(delta_seconds);
    let controller_delta = controller_delta(delta_seconds);
    controller.step(workspace, controller_delta);
    #[cfg(feature = "physics")]
    workspace.step_physics(delta_seconds);
    controller.post_physics(workspace, controller_delta);
    #[cfg(feature = "sound")]
    for error in workspace.step_audio(delta_seconds) {
        log::error!("{error}");
    }
}

impl Engine {
    /// Returns the workspace rendered by this engine.
    pub fn workspace(&self) -> &Workspace {
        self.children()[0]
            .downcast_ref::<Workspace>()
            .expect("engine workspace")
    }

    /// Returns mutable access to the workspace rendered by this engine.
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        engine_workspace(&mut self.instance)
    }

    /// Feeds the current egui frame to the controller.
    pub fn process_input(&mut self, context: &egui::Context) {
        self.controller.process_input(context);
    }

    /// Sends a winit window event to the controller.
    ///
    /// Returns `true` if the controller reports using the event.
    pub fn process_window_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        self.controller.process_window_event(event)
    }

    /// Sends an unfiltered device event to the controller.
    pub fn process_device_event(&mut self, event: &winit::event::DeviceEvent) {
        self.controller.process_device_event(event);
    }

    /// Reports whether a bound movement key is down for the controller.
    pub fn has_movement_input(&self, input: &egui::InputState) -> bool {
        self.controller.has_movement_input(input)
    }

    /// Advances tweens, animations, the controller, physics, and audio.
    ///
    /// The controller runs after animation and before physics, with a post-physics
    /// phase for behavior such as following a dynamic camera subject. Its frame
    /// delta is clamped to `0..=0.1` seconds.
    pub fn step(&mut self, delta_seconds: f32) {
        step_engine(
            engine_workspace(&mut self.instance),
            self.controller.as_mut(),
            delta_seconds,
        );
    }

    #[cfg(feature = "devtools")]
    fn with_devtools_render_selection<R>(
        &mut self,
        selected: &[InstanceId],
        render: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.devtools_render_selection.replace(selected.to_vec());
        let result = render(self);
        self.devtools_render_selection = previous;
        result
    }

    /// Creates the engine with the supplied workspace from eframe's WGPU creation context.
    ///
    /// `fallback_size` is used on web and when no native window is available. The renderer is
    /// automatically resized to the UI region when [`Self::render`] is called.
    pub fn new(
        creation_context: &eframe::CreationContext<'_>,
        fallback_size: [u32; 2],
        workspace: Workspace,
    ) -> Result<Self, RendererError> {
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        let size = creation_context
            .winit_window()
            .map(|window| window.inner_size())
            .map_or(fallback_size, |size| [size.width, size.height]);
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        let size = fallback_size;
        let controller = CameraController::default();
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        let controller = {
            let mut controller = controller;
            if web_sys::window().is_some_and(|window| window.navigator().max_touch_points() > 0) {
                controller.set_touchscreen_detected(true);
            }
            controller
        };

        let render_state = creation_context
            .wgpu_render_state
            .as_ref()
            .ok_or(RendererError::MissingEframeWgpuRenderState)?;
        let renderer = RenderAdapter::new(render_state, [size[0].max(1), size[1].max(1)])?;
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        let renderer = Rc::new(RefCell::new(renderer));
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        let renderer = Arc::new(Mutex::new(renderer));

        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        let renderer_id = register_renderer(&renderer);

        let mut instance = InstanceData::new_root("Engine");
        instance.add_persistent_child(Box::new(workspace));

        Ok(Self {
            instance,
            controller: Box::new(controller),
            renderer,
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
            #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
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

    /// Returns the Bevy app attached through [`Terrarium::with_bevy_app`], if any.
    #[cfg(feature = "bevy")]
    pub fn bevy_app(&self) -> Option<&bevy_app::App> {
        self.bevy_app.as_ref()
    }

    /// Returns mutable access to the Bevy app attached through [`Terrarium::with_bevy_app`], if any.
    #[cfg(feature = "bevy")]
    pub fn bevy_app_mut(&mut self) -> Option<&mut bevy_app::App> {
        self.bevy_app.as_mut()
    }

    /// Returns locked access to the underlying renderer for advanced operations.
    #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
    pub fn renderer(&self) -> MutexGuard<'_, RenderAdapter> {
        self.renderer.lock().expect("engine renderer lock poisoned")
    }

    /// Returns mutable access to the underlying renderer for advanced operations.
    #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
    pub fn renderer(&self) -> RefMut<'_, RenderAdapter> {
        self.renderer.borrow_mut()
    }

    /// Processes controller input, advances the workspace, and requests the next frame.
    ///
    /// This calls [`Self::step`].
    pub fn update(&mut self, context: &egui::Context) {
        let delta = {
            #[cfg(feature = "devtools")]
            {
                let has_movement_input = context.input(|input| self.has_movement_input(input));
                self.devtools
                    .focus_scene_on_movement_input(context, has_movement_input);
            }
            let delta = process_engine_input(self.controller.as_mut(), context);
            #[cfg(feature = "bevy")]
            if let Some(app) = self.bevy_app.as_mut() {
                crate::bevy::step_app(
                    app,
                    engine_workspace(&mut self.instance),
                    &mut self.controller,
                    context,
                    delta,
                );
                context.request_repaint();
                return;
            }
            delta
        };
        self.step(delta);
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
        prepare_workspace(
            &self.renderer,
            self.clear_color,
            engine_workspace(&mut self.instance),
            ui,
            &selected,
        )
    }

    /// Registers the paint callback for the workspace prepared with [`Self::prepare`].
    pub fn paint(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        let callback = SceneCallback {
            renderer: Arc::clone(&self.renderer),
        };
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
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

    fn start(config: Terrarium, app_creator: eframe::AppCreator<'static>) -> eframe::Result {
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        if config.env_logger {
            let _ = env_logger::try_init();
        }
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        let _ = config.env_logger;

        let wgpu_options_customized = config.wgpu_options.is_some();
        let mut wgpu_options = egui_wgpu::WgpuConfiguration::default();
        if let Some(configure) = config.wgpu_options {
            configure(&mut wgpu_options);
        }
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        let window_size = config.size.unwrap_or(DEFAULT_WINDOW_SIZE);
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        {
            let headless_frame_limit = config.headless_frames;
            if let Some(mut frames_remaining) = headless_frame_limit {
                let render_state = if wgpu_options_customized {
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
                            egui::vec2(window_size[0] as f32, window_size[1] as f32),
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
                viewport: native_viewport(&config.title, window_size, config.size.is_some()),
                wgpu_options,
                ..Default::default()
            };

            eframe::run_native(&config.title, native_options, app_creator)
        }

        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        {
            use wasm_bindgen::JsCast as _;

            if config.console_error_panic_hook {
                console_error_panic_hook::set_once();
            }

            let canvas_id = config.canvas_id.clone();

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

fn process_engine_input(controller: &mut dyn ControllerView, context: &egui::Context) -> f32 {
    let delta = context.input(|input| input.stable_dt);
    let delta = if delta.is_finite() {
        delta.clamp(0.0, 0.1)
    } else {
        0.0
    };
    controller.process_input(context);
    delta
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
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
    clear_color: [f32; 4],
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
    workspace.camera_mut().resize(size[0], size[1]);
    #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
    let mut renderer = renderer.lock().expect("engine renderer lock poisoned");
    #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
    let mut renderer = renderer.borrow_mut();
    renderer.with_clear_color(egui_wgpu::wgpu::Color {
        r: clear_color[0] as f64,
        g: clear_color[1] as f64,
        b: clear_color[2] as f64,
        a: clear_color[3] as f64,
    });
    renderer.prepare_eframe_scene(workspace, size, selected)
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
fn native_viewport(title: &str, size: [u32; 2], size_explicit: bool) -> egui::ViewportBuilder {
    let viewport = egui::ViewportBuilder::default().with_title(title);
    if size_explicit {
        viewport.with_inner_size([size[0] as f32, size[1] as f32])
    } else {
        viewport.with_maximized(true)
    }
}

#[cfg(all(test, not(all(target_family = "wasm", not(target_os = "wasi")))))]
mod tests {
    use super::*;

    #[test]
    fn native_window_defaults_to_maximized_unless_sized() {
        let default = Terrarium::default();
        let default_viewport = native_viewport(
            &default.title,
            default.size.unwrap_or(DEFAULT_WINDOW_SIZE),
            default.size.is_some(),
        );
        assert_eq!(default_viewport.maximized, Some(true));
        assert_eq!(default_viewport.fullscreen, None);
        assert_eq!(default_viewport.inner_size, None);

        let sized = Terrarium::default().with_size([960, 540]);
        let sized_viewport = native_viewport(
            &sized.title,
            sized.size.unwrap_or(DEFAULT_WINDOW_SIZE),
            sized.size.is_some(),
        );
        assert_eq!(sized_viewport.fullscreen, None);
        assert_eq!(sized_viewport.inner_size, Some(egui::vec2(960.0, 540.0)));
    }

    #[test]
    fn terrarium_is_an_instance_root() {
        let mut terrarium = Terrarium::new();
        let instance: &dyn Instance = &terrarium;

        assert_eq!(instance.class_name(), "Terrarium");
        assert_eq!(instance.parent(), None);
        assert!(instance.children().is_empty());
        assert!(instance.try_clone_box().is_none());

        let child_id = terrarium.add_child(Part::new());
        assert_eq!(terrarium.view::<Part>(child_id).unwrap().id(), child_id);
    }
}

#[cfg(feature = "devtools")]
impl trdevtools::DevtoolsHost for Engine {
    fn scene_title(&self) -> &str {
        &self.title
    }

    fn fps(&self) -> Option<f32> {
        Some(self.renderer().fps())
    }

    fn workspace(&mut self) -> &mut Workspace {
        self.workspace_mut()
    }

    fn has_movement_input(&mut self, input: &egui::InputState) -> bool {
        Engine::has_movement_input(self, input)
    }

    fn root(&mut self) -> &mut dyn Instance {
        self
    }

    fn render_scene(&mut self, ui: &mut egui::Ui, selected: &[InstanceId]) {
        self.with_devtools_render_selection(selected, |engine| {
            if let Err(error) = engine.render(ui) {
                log::error!("scene preparation failed: {error}");
            }
        });
    }
}

impl eframe::App for Engine {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        EngineExt::clear_color(self)
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
        EngineExt::clear_color(engine)
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

    fn fps(&self) -> Option<f32> {
        Some(self.engine.renderer().fps())
    }

    fn workspace(&mut self) -> &mut Workspace {
        self.engine.workspace_mut()
    }

    fn has_movement_input(&mut self, input: &egui::InputState) -> bool {
        self.engine.has_movement_input(input)
    }

    fn root(&mut self) -> &mut dyn Instance {
        self.engine
    }

    fn render_scene(&mut self, ui: &mut egui::Ui, selected: &[InstanceId]) {
        let app = &mut self.app;
        let frame = &mut self.frame;
        self.engine
            .with_devtools_render_selection(selected, |engine| app.render(engine, ui, frame));
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
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        normalize_web_canvas_input(context, raw_input);
        with_engine(&self.engine, |engine| {
            self.app.raw_input_hook(engine, context, raw_input);
        });
    }
}

fn take_engine(slot: EngineSlot) -> Option<Engine> {
    Rc::try_unwrap(slot).ok()?.into_inner()
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
impl Drop for Engine {
    fn drop(&mut self) {
        unregister_renderer(self.renderer_id);
    }
}

/// Convenience access to the owned [`Workspace`].
///
/// Methods and fields not provided by [`Engine`] resolve to its workspace.
/// Instance methods operate on the engine root instead.
impl Deref for Engine {
    type Target = Workspace;
    fn deref(&self) -> &Self::Target {
        self.workspace()
    }
}

impl DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.workspace_mut()
    }
}

instance! {
    #[instance(inspect = false)]
    /// The primary entry point for configuring and running a Terrarium application.
    pub struct Terrarium {
        #[data]
        #[default(InstanceData::new_root("Terrarium"))]
        instance: Box<InstanceData>,
        /// Starting geometry applied before the application initializer runs.
        #[property]
        workspace_preset: WorkspacePreset,
        #[cfg(feature = "bevy")]
        #[property(borrow)]
        bevy_app: Option<bevy_app::App>,
        /// The application title on native platforms.
        #[property(default = "App".to_owned(), into)]
        title: String,
        /// Explicit window size. An unset size uses a maximized native window and the default fallback size.
        #[property(default = None, into)]
        size: Option<[u32; 2]>,
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        #[property(default = "app".to_owned(), into)]
        canvas_id: String,
        /// Optional one-shot wgpu configuration callback.
        #[property(borrow, default = None)]
        wgpu_options: Option<WgpuOptionsCallback>,
        /// Whether to automatically call [`env_logger::init()`].
        #[property(default = true)]
        env_logger: bool,
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        /// Native headless frame policy: `None` disables it, `Some(None)` runs until close, and
        /// `Some(Some(n))` runs at most `n` frames.
        #[property]
        headless_frames: Option<Option<usize>>,
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        #[property(default = true)]
        console_error_panic_hook: bool,
        /// Whether to install Terrarium's bundled fonts.
        #[property(default = cfg!(feature = "default-fonts"))]
        bundled_fonts: bool,
    }
}

impl Terrarium {
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
        Self::default().with_workspace_preset(preset)
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

        let size = launcher.size.unwrap_or(DEFAULT_WINDOW_SIZE);
        let preset = launcher.workspace_preset;
        #[cfg(feature = "devtools")]
        let title = launcher.title.clone();
        #[cfg(feature = "default-fonts")]
        let is_bundled_fonts = launcher.bundled_fonts;
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
            let workspace = Workspace::preset(preset);
            #[cfg(feature = "bevy")]
            let (workspace, bevy_app) = {
                let mut workspace = workspace;
                let app = bevy_app
                    .map(|app| {
                        crate::bevy::attach_app(app, &mut workspace, &creation_context.egui_ctx)
                    })
                    .transpose()?;
                (workspace, app)
            };
            let engine = Engine::new(creation_context, size, workspace)?;
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
            {
                engine.bevy_app = bevy_app;
            }
            store_engine(&app_engine_slot, engine);

            let app = with_engine(&app_engine_slot, initialize)
                .map_err(|error| -> AppCreationError { error.into() })?;
            #[cfg(feature = "bevy")]
            with_engine(&app_engine_slot, |engine| {
                if let Some(app) = engine.bevy_app.as_mut() {
                    crate::bevy::finalize_app(app, engine_workspace(&mut engine.instance));
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
}

impl std::fmt::Debug for Terrarium {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terrarium")
            .field("workspace_preset", &self.workspace_preset)
            .field("title", &self.title)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

trscene::__instance_inspection_items! {
    impl trscene::devtools::Inspect for Terrarium {
        fn properties(&self) -> Vec<trscene::devtools::Property> {
            trscene::devtools::Inspect::properties(&self.instance)
        }

        fn validate_property(
            &self,
            path: &str,
            value: &trscene::devtools::PropertyValue,
        ) -> Result<(), trscene::devtools::PropertyError> {
            trscene::devtools::Inspect::validate_property(&self.instance, path, value)
        }

        fn set_property(
            &mut self,
            path: &str,
            value: trscene::devtools::PropertyValue,
        ) -> Result<(), trscene::devtools::PropertyError> {
            trscene::devtools::Inspect::set_property(&mut self.instance, path, value)
        }
    }
}

impl InstanceClone for Terrarium {
    fn try_clone_box(&self) -> Option<Box<dyn Instance>> {
        None
    }
}
