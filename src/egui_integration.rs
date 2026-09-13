use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State;
use winit::{event::WindowEvent, window::Window};

/// Owns the egui platform state and GPU renderer for one window.
///
/// This is kept inside [`crate::Renderer`] so applications only need to
/// manage their renderer and window.
pub(crate) struct EguiIntegration {
    context: egui::Context,
    state: State,
    renderer: Option<EguiRenderer>,
}

pub(crate) struct EguiFrame {
    paint_jobs: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
    pixels_per_point: f32,
}

impl EguiIntegration {
    /// Creates an egui integration for the supplied window.
    pub(crate) fn new(window: &Window) -> Self {
        let context = egui::Context::default();
        let state = State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            context,
            state,
            renderer: None,
        }
    }

    /// Returns the egui context used by this integration.
    pub(crate) fn context(&self) -> &egui::Context {
        &self.context
    }

    /// Returns the egui context used by this integration for configuration.
    pub(crate) fn context_mut(&mut self) -> &mut egui::Context {
        &mut self.context
    }

    /// Forwards a window event to egui and returns whether egui consumed it.
    pub(crate) fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    pub(crate) fn begin_frame<F>(&mut self, window: &Window, run_ui: F) -> EguiFrame
    where
        F: FnMut(&mut egui::Ui),
    {
        let input = self.state.take_egui_input(window);
        let mut output = self.context.run_ui(input, run_ui);
        self.state
            .handle_platform_output(window, std::mem::take(&mut output.platform_output));
        let pixels_per_point = output.pixels_per_point;
        let paint_jobs = self.context.tessellate(output.shapes, pixels_per_point);
        EguiFrame {
            paint_jobs,
            textures_delta: output.textures_delta,
            pixels_per_point,
        }
    }

    pub(crate) fn render_frame(
        &mut self,
        mut frame: EguiFrame,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) -> Vec<wgpu::CommandBuffer> {
        let renderer = self
            .renderer
            .get_or_insert_with(|| EguiRenderer::new(device, format, RendererOptions::default()));
        for (id, image_deltas) in frame.textures_delta.set.drain() {
            for image_delta in image_deltas {
                renderer.update_texture(device, queue, id, &image_delta);
            }
        }
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point: frame.pixels_per_point,
        };
        let user_command_buffers = renderer.update_buffers(
            device,
            queue,
            encoder,
            &frame.paint_jobs,
            &screen_descriptor,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            renderer.render(&mut pass, &frame.paint_jobs, &screen_descriptor);
        }
        for id in frame.textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
        user_command_buffers
    }
}
