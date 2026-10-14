use wasm_bindgen::{JsCast, prelude::*};
use web_sys::HtmlCanvasElement;

use crate::{
    runtime::HostApp,
    trscene::{FrameInput, WorkspacePreset},
    *,
};

/// Browser-side renderer and application interop exposed to the React host.
#[wasm_bindgen]
pub struct WebBridge {
    runtime: Box<dyn HostApp>,
    software_adapter: bool,
    gui_surface: Option<(
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
        HtmlCanvasElement,
    )>,
    surface: wgpu::Surface<'static>,
    configuration: wgpu::SurfaceConfiguration,
    canvas: HtmlCanvasElement,
    #[cfg(feature = "meshpart")]
    viewports: std::collections::HashMap<u32, WebViewport>,
    #[cfg(feature = "meshpart")]
    next_viewport: u32,
}

#[cfg(feature = "meshpart")]
struct WebViewport {
    texture: u64,
    surface: wgpu::Surface<'static>,
    configuration: wgpu::SurfaceConfiguration,
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

#[wasm_bindgen]
impl WebBridge {
    /// Mounts an onscreen canvas for egui above the application's web UI.
    pub fn mount_gui(&mut self, canvas: HtmlCanvasElement) -> Result<(), JsValue> {
        let state = &self.runtime.engine().creation_context().wgpu_render_state;
        let surface = state
            .instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(js_error)?;
        let mut configuration = surface
            .get_default_config(&state.adapter, 1, 1)
            .ok_or_else(|| js_error("egui surface configuration is unavailable"))?;
        configuration.format = state.target_format;
        configuration.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        canvas.set_width(1);
        canvas.set_height(1);
        surface.configure(&state.device, &configuration);
        self.gui_surface = Some((surface, configuration, canvas));
        Ok(())
    }
    /// Whether WebGPU selected a CPU fallback adapter.
    pub fn software_adapter(&self) -> bool {
        self.software_adapter
    }
    /// Returns the renderer's measured frames per second.
    pub fn fps(&self) -> f32 {
        self.runtime.engine().renderer().fps()
    }
    /// Returns the selected GPU backend for diagnostics.
    pub fn backend(&self) -> String {
        format!(
            "{:?}",
            self.runtime
                .engine()
                .creation_context()
                .wgpu_render_state
                .adapter
                .get_info()
                .backend
        )
    }

    /// Resolves when previously submitted GPU work completes.
    ///
    /// CPU fallback hosts use this to bound submissions; hardware rendering does not wait.
    pub fn wait_for_gpu(&self) -> js_sys::Promise {
        let state = &self.runtime.engine().creation_context().wgpu_render_state;
        if state.adapter.get_info().backend != wgpu::Backend::BrowserWebGpu {
            return js_sys::Promise::resolve(&JsValue::UNDEFINED);
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        state.queue.on_submitted_work_done(move || {
            let _ = sender.send(());
        });
        let _ = state.device.poll(wgpu::PollType::Poll);
        wasm_bindgen_futures::future_to_promise(async move {
            receiver.await.map_err(js_error)?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Advances and presents one frame, returning UI state.
    pub fn frame(&mut self, input: JsValue) -> Result<JsValue, JsValue> {
        self.render(input)?;
        self.state()
    }

    /// Advances and presents a frame, returning FPS, shutdown status, and egui platform output.
    pub fn render(&mut self, input: JsValue) -> Result<JsValue, JsValue> {
        let input: FrameInput = serde_wasm_bindgen::from_value(input).map_err(js_error)?;
        let size = input.physical_size();
        let state = &self.runtime.engine().creation_context().wgpu_render_state;
        if self.configuration.width != size[0] || self.configuration.height != size[1] {
            self.canvas.set_width(size[0]);
            self.canvas.set_height(size[1]);
            self.configuration.width = size[0];
            self.configuration.height = size[1];
            self.surface.configure(&state.device, &self.configuration);
        }
        self.runtime.frame(input).map_err(js_error)?;
        let mut reconfigure = false;
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let target = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                self.runtime.engine().renderer().paint_scene(&target);
                if self.gui_surface.is_none() {
                    self.runtime.engine().gui.borrow_mut().paint(
                        &target,
                        &[[0, 0, size[0], size[1]]],
                        false,
                    );
                }
                self.runtime
                    .engine()
                    .creation_context()
                    .wgpu_render_state
                    .queue
                    .present(texture);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                reconfigure = true
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(js_error("WebGPU surface validation failed"));
            }
        }
        if reconfigure {
            let state = &self.runtime.engine().creation_context().wgpu_render_state;
            self.surface.configure(&state.device, &self.configuration);
        }
        if let Some((surface, configuration, canvas)) = &mut self.gui_surface {
            let state = &self.runtime.engine().creation_context().wgpu_render_state;
            if [configuration.width, configuration.height] != size {
                canvas.set_width(size[0]);
                canvas.set_height(size[1]);
                configuration.width = size[0];
                configuration.height = size[1];
                surface.configure(&state.device, configuration);
            }
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    let target = texture.texture.create_view(&Default::default());
                    self.runtime.engine().gui.borrow_mut().paint(
                        &target,
                        &[[0, 0, size[0], size[1]]],
                        true,
                    );
                    state.queue.present(texture);
                }
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    surface.configure(&state.device, configuration)
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(js_error("egui surface validation failed"));
                }
            }
        }
        #[cfg(feature = "meshpart")]
        for viewport in self.viewports.values_mut() {
            let state = &self.runtime.engine().creation_context().wgpu_render_state;
            match viewport.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    let target = texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    self.runtime
                        .engine()
                        .renderer()
                        .paint_viewport(&target, viewport.texture)
                        .map_err(js_error)?;
                    state.queue.present(texture);
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    viewport
                        .surface
                        .configure(&state.device, &viewport.configuration)
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(js_error("viewport surface validation failed"));
                }
            }
        }
        let _ = self
            .runtime
            .engine()
            .creation_context()
            .wgpu_render_state
            .device
            .poll(wgpu::PollType::Poll);
        self.runtime
            .status()
            .serialize(
                &serde_wasm_bindgen::Serializer::json_compatible()
                    .serialize_large_number_types_as_bigints(true),
            )
            .map_err(js_error)
    }
    /// Attaches a WebGPU canvas to a viewport; WebGL hosts use asynchronous readback.
    pub fn mount_viewport(
        &mut self,
        id: &str,
        canvas: HtmlCanvasElement,
    ) -> Result<Option<u32>, JsValue> {
        #[cfg(feature = "meshpart")]
        {
            let state = &self.runtime.engine().creation_context().wgpu_render_state;
            if state.adapter.get_info().backend != wgpu::Backend::BrowserWebGpu {
                return Ok(None);
            }
            let surface = state
                .instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
                .map_err(js_error)?;
            let mut configuration = surface
                .get_default_config(
                    &state.adapter,
                    canvas.width().max(1),
                    canvas.height().max(1),
                )
                .ok_or_else(|| js_error("viewport surface configuration is unavailable"))?;
            configuration.format = state.target_format;
            configuration.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
            surface.configure(&state.device, &configuration);
            let texture = id.parse().map_err(js_error)?;
            let key = self.next_viewport;
            self.next_viewport = key
                .checked_add(1)
                .ok_or_else(|| js_error("viewport identifiers exhausted"))?;
            self.viewports.insert(
                key,
                WebViewport {
                    texture,
                    surface,
                    configuration,
                },
            );
            Ok(Some(key))
        }
        #[cfg(not(feature = "meshpart"))]
        {
            let _ = (id, canvas);
            Ok(None)
        }
    }
    /// Detaches a previously mounted viewport canvas.
    pub fn unmount_viewport(&mut self, key: u32) {
        #[cfg(feature = "meshpart")]
        {
            self.viewports.remove(&key);
        }
        #[cfg(not(feature = "meshpart"))]
        {
            let _ = key;
        }
    }
    /// Returns current UI state without advancing the simulation.
    pub fn state(&self) -> Result<JsValue, JsValue> {
        self.runtime
            .state()
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(js_error)
    }
    /// Returns the frontend component and initial host configuration.
    pub fn info(&self) -> Result<JsValue, JsValue> {
        self.runtime
            .info()
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(js_error)
    }
    /// Enables egui's generated accessibility tree for assistive hosts and diagnostics.
    pub fn enable_accessibility(&self) {
        self.runtime.engine().egui_context().enable_accesskit();
    }
    /// Returns the latest standard egui platform output, including accessibility updates.
    pub fn platform_output(&self) -> Result<JsValue, JsValue> {
        self.runtime
            .status()
            .platform
            .serialize(
                &serde_wasm_bindgen::Serializer::json_compatible()
                    .serialize_large_number_types_as_bigints(true),
            )
            .map_err(js_error)
    }
    /// Dispatches an application command.
    pub fn command(&mut self, command: &str, payload: JsValue) -> Result<JsValue, JsValue> {
        let payload = if payload.is_undefined() {
            serde_json::Value::Null
        } else {
            serde_wasm_bindgen::from_value(payload).map_err(js_error)?
        };
        let result = self.runtime.command(command, payload).map_err(js_error)?;
        serde_wasm_bindgen::to_value(&result).map_err(js_error)
    }
    /// Reads an offscreen viewport, returning `undefined` while readback is pending.
    pub fn viewport(&self, id: &str) -> Result<JsValue, JsValue> {
        #[cfg(feature = "meshpart")]
        {
            match self.runtime.viewport_pixels(id.parse().map_err(js_error)?) {
                Ok(pixels) => Ok(js_sys::Uint8Array::from(pixels.as_slice()).into()),
                Err(error)
                    if error
                        .downcast_ref::<RendererError>()
                        .is_some_and(|e| matches!(e, RendererError::PixelReadbackPending)) =>
                {
                    Ok(JsValue::UNDEFINED)
                }
                Err(error) => Err(js_error(error)),
            }
        }
        #[cfg(not(feature = "meshpart"))]
        {
            let _ = id;
            Err(js_error("meshpart feature is disabled"))
        }
    }
}

pub(crate) fn start<A: App, E: Into<AppCreationError>>(
    mut config: Terrarium<'_>,
    mut options: WgpuConfiguration,
    initialize: impl FnOnce(&mut Engine) -> Result<A, E> + 'static,
) {
    let canvas_id = config.canvas_id.to_owned();
    let ui = config.ui.to_owned();
    let size = config.size;
    let preset: WorkspacePreset = config.workspace_preset;
    let bundle_fonts = config.bundle_fonts;
    #[cfg(feature = "bevy")]
    let bevy_app = config.take_bevy_app();
    let _ = &mut config;
    wasm_bindgen_futures::spawn_local(async move {
        let result: Result<(), AppCreationError> = async {
            let window = web_sys::window().ok_or("browser window is unavailable")?;
            let canvas = window
                .document()
                .and_then(|document| document.get_element_by_id(&canvas_id))
                .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
                .ok_or("scene canvas is unavailable")?;
            canvas.set_width(size[0]);
            canvas.set_height(size[1]);
            let descriptor = std::mem::replace(
                &mut options.instance_descriptor,
                wgpu::InstanceDescriptor::new_without_display_handle(),
            );
            let instance = wgpu::util::new_instance_with_webgpu_detection(descriptor).await;
            let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))?;
            let state = RenderState::create(options, instance, Some(&surface)).await?;
            let info = state.adapter.get_info();
            let software_adapter = info.backend == wgpu::Backend::BrowserWebGpu
                && info.device_type == wgpu::DeviceType::Cpu;
            let mut configuration = surface
                .get_default_config(&state.adapter, size[0], size[1])
                .ok_or("surface configuration is unavailable")?;
            configuration.format = state.target_format;
            surface.configure(&state.device, &configuration);
            let mut launcher = Terrarium::preset(preset)
                .with_size(size)
                .with_ui(&ui)
                .with_bundled_fonts(bundle_fonts);
            #[cfg(feature = "bevy")]
            if let Some(app) = bevy_app {
                launcher = launcher.with_bevy(app);
            }
            let runtime = launcher.initialize(state, initialize)?;
            let bridge: JsValue = WebBridge {
                runtime: Box::new(runtime),
                software_adapter,
                gui_surface: None,
                surface,
                configuration,
                canvas,
                #[cfg(feature = "meshpart")]
                viewports: Default::default(),
                #[cfg(feature = "meshpart")]
                next_viewport: 0,
            }
            .into();
            js_sys::Reflect::set(&window, &JsValue::from_str("__TERRARIUM__"), &bridge)
                .map_err(|_| "failed to expose browser bridge")?;
            window
                .dispatch_event(
                    &web_sys::Event::new("terrarium-ready")
                        .map_err(|_| "failed to create readiness event")?,
                )
                .map_err(|_| "failed to dispatch readiness event")?;
            Ok(())
        }
        .await;
        if let Err(error) = result
            && let Some(window) = web_sys::window()
        {
            let _ = js_sys::Reflect::set(
                &window,
                &JsValue::from_str("__TERRARIUM_ERROR__"),
                &JsValue::from_str(&error.to_string()),
            );
            if let Ok(event) = web_sys::Event::new("terrarium-ready") {
                let _ = window.dispatch_event(&event);
            }
        }
    });
}

use serde::Serialize;
