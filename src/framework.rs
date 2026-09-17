use std::sync::{Arc, Mutex, MutexGuard};

use crate::{Renderer, RendererError, Workspace, eframe, egui, egui_wgpu};

const DEFAULT_CLEAR_COLOR: [f32; 4] = [0.018, 0.028, 0.065, 1.0];

struct SceneCallback {
    renderer: Arc<Mutex<Renderer>>,
}

impl egui_wgpu::CallbackTrait for SceneCallback {
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut egui_wgpu::wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
        self.renderer
            .lock()
            .expect("framework renderer lock poisoned")
            .paint_eframe_scene(render_pass);
    }
}

/// Handles integration with [`eframe`].
pub struct Framework {
    renderer: Arc<Mutex<Renderer>>,
    clear_color: [f32; 4],
}

impl Framework {
    /// Creates the framework from eframe's WGPU creation context.
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
        Ok(Self {
            renderer: Arc::new(Mutex::new(renderer)),
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
    pub fn renderer(&self) -> MutexGuard<'_, Renderer> {
        self.renderer
            .lock()
            .expect("framework renderer lock poisoned")
    }

    /// Processes camera input, advances the workspace, and requests the next frame.
    pub fn update(&self, context: &egui::Context, workspace: &mut Workspace) {
        let delta = context.input(|input| input.stable_dt.min(0.1));
        context.input(|input| workspace.process_eframe_input(input));
        workspace.update(delta);
        context.request_repaint();
    }

    /// Prepares a workspace for the available eframe UI region.
    pub fn prepare(
        &self,
        ui: &mut egui::Ui,
        workspace: &mut Workspace,
    ) -> Result<(), RendererError> {
        let rect = ui.max_rect();
        let pixels_per_point = ui.pixels_per_point();
        let size = [
            (rect.width() * pixels_per_point).round().max(1.0) as u32,
            (rect.height() * pixels_per_point).round().max(1.0) as u32,
        ];
        workspace.current_camera.resize(size[0], size[1]);
        self.renderer().prepare_eframe_scene(workspace, size)
    }

    /// Registers the paint callback for a workspace prepared with [`Self::prepare`].
    pub fn paint(&self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let callback = egui_wgpu::Callback::new_paint_callback(
            rect,
            SceneCallback {
                renderer: Arc::clone(&self.renderer),
            },
        );
        ui.painter().add(egui::Shape::Callback(callback));
    }

    /// Prepares and paints a workspace into the available eframe UI region.
    pub fn render(
        &self,
        ui: &mut egui::Ui,
        workspace: &mut Workspace,
    ) -> Result<(), RendererError> {
        self.prepare(ui, workspace)?;
        self.paint(ui);
        Ok(())
    }

    /// Runs a new eframe application with the specified configuration and app creator.
    pub fn run(config: RunConfig<'_>, app_creator: eframe::AppCreator<'_>) -> eframe::Result {
        let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
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

/// Options controlling the behavior of the window.
pub struct RunConfig<'a> {
    /// The application title on native platforms.
    pub title: &'a str,
    /// The window size on native platforms.
    pub size: [u32; 2],
    /// The element to render the web application.
    #[cfg(target_arch = "wasm32")]
    pub canvas_id: &'a str,
    /// Configures wgpu instance/device/adapter/surface creation and renderloop.
    pub wgpu_options: Box<dyn FnOnce(&mut eframe::egui_wgpu::WgpuConfiguration) + 'a>,
}

impl<'a> Default for RunConfig<'a> {
    fn default() -> Self {
        Self {
            title: "app",
            size: [1280, 720],
            #[cfg(target_arch = "wasm32")]
            canvas_id: "app",
            wgpu_options: Box::new(|_| {}),
        }
    }
}

impl<'a> RunConfig<'a> {
    /// Configures wgpu instance/device/adapter/surface creation and renderloop.
    pub fn with_wgpu_options(
        mut self,
        f: impl FnOnce(&mut eframe::egui_wgpu::WgpuConfiguration) + 'a,
    ) -> Self {
        self.wgpu_options = Box::new(f);
        self
    }
}
