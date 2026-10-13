#[cfg(feature = "meshpart")]
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use crate::{RenderState, trscene::Workspace};

#[cfg(feature = "meshpart")]
mod viewport;

pub use trrender::{DEPTH_FORMAT, RendererError};
#[cfg(feature = "meshpart")]
pub use viewport::{Viewport, ViewportTexture};

/// Terrarium's host-independent adapter around the standalone WGPU renderer.
pub struct RenderAdapter {
    backend: trrender::Renderer,
    state: RenderState,
    #[cfg(not(target_arch = "wasm32"))]
    viewport_background: wgpu::RenderPipeline,
    #[cfg(feature = "meshpart")]
    viewport_targets: HashMap<u64, trrender::Renderer>,
}

impl RenderAdapter {
    /// Creates a renderer from shared GPU resources.
    pub fn new(state: &RenderState, size: [u32; 2]) -> Result<Self, RendererError> {
        Ok(Self {
            backend: trrender::Renderer::new(
                &state.device,
                &state.queue,
                state.target_format,
                size,
            )?,
            state: state.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            viewport_background: viewport_background(state),
            #[cfg(feature = "meshpart")]
            viewport_targets: HashMap::new(),
        })
    }

    /// Builds a borrowed render snapshot from a workspace and submits it.
    pub fn prepare_scene(
        &mut self,
        workspace: &Workspace,
        size: [u32; 2],
    ) -> Result<(), RendererError> {
        self.backend.prepare(&workspace.render_scene(), size)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn paint_native(
        &mut self,
        target: &wgpu::TextureView,
        regions: &[crate::native_surface::Rect],
        viewport: Option<u64>,
    ) {
        let mut encoder =
            self.state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Terrarium native compositor"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let regions: Vec<_> = regions
                .iter()
                .map(|rect| [rect.x, rect.y, rect.width, rect.height])
                .collect();
            if viewport.is_some() {
                pass.set_pipeline(&self.viewport_background);
                for &[x, y, width, height] in &regions {
                    pass.set_scissor_rect(x, y, width, height);
                    pass.draw(0..3, 0..1);
                }
            }
            let renderer = match viewport {
                #[cfg(feature = "meshpart")]
                Some(id) => self.viewport_targets.get_mut(&id),
                #[cfg(not(feature = "meshpart"))]
                Some(_) => None,
                None => Some(&mut self.backend),
            };
            if let Some(renderer) = renderer {
                renderer.paint_regions(&mut pass, &regions);
            }
        }
        self.state.queue.submit(Some(encoder.finish()));
    }

    #[cfg(all(feature = "meshpart", target_arch = "wasm32"))]
    pub(crate) fn paint_viewport(
        &mut self,
        target: &wgpu::TextureView,
        id: u64,
    ) -> Result<(), RendererError> {
        let renderer = self
            .viewport_targets
            .get_mut(&id)
            .ok_or_else(|| RendererError::PixelReadback("viewport has been released".into()))?;
        let mut encoder =
            self.state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("WebGPU viewport compositor"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.paint(&mut pass);
        }
        self.state.queue.submit(Some(encoder.finish()));
        Ok(())
    }

    /// Composites the prepared scene into the host's output attachment.
    pub fn paint_scene(&mut self, target: &wgpu::TextureView) {
        let mut encoder =
            self.state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Terrarium host compositor"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Terrarium host output"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.backend.paint(&mut pass);
        }
        self.state.queue.submit(Some(encoder.finish()));
    }
}

impl Deref for RenderAdapter {
    type Target = trrender::Renderer;
    fn deref(&self) -> &Self::Target {
        &self.backend
    }
}
impl DerefMut for RenderAdapter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.backend
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn viewport_background(state: &RenderState) -> wgpu::RenderPipeline {
    let shader = state
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native viewport background"),
            source: wgpu::ShaderSource::Wgsl(include_str!("viewport_background.wgsl").into()),
        });
    state
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("native viewport background"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: state.target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
}
