#[cfg(feature = "meshpart")]
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, OnceLock};

use slotmap::Key;

use crate::{
    egui_wgpu,
    trscene::{Color3, InstanceId, OutlineMode, RenderOutline, Workspace},
};

#[cfg(feature = "meshpart")]
mod viewport;

pub use trrender::{DEPTH_FORMAT, RendererError};
#[cfg(feature = "meshpart")]
pub use viewport::{Viewport, ViewportTexture};

#[cfg(not(target_arch = "wasm32"))]
static WGPU_INSTANCE_KEEPALIVE: OnceLock<Mutex<Option<crate::wgpu::Instance>>> = OnceLock::new();

#[cfg(feature = "meshpart")]
struct ViewportTarget {
    _color_texture: crate::wgpu::Texture,
    color_view: crate::wgpu::TextureView,
    _depth_texture: crate::wgpu::Texture,
    depth_view: crate::wgpu::TextureView,
    size: [u32; 2],
    texture_id: Option<crate::egui::TextureId>,
}

/// Terrarium's eframe adapter around the standalone WGPU renderer.
pub struct RenderAdapter {
    backend: trrender::Renderer,
    #[cfg(feature = "meshpart")]
    viewport_targets: HashMap<u64, ViewportTarget>,
}

impl RenderAdapter {
    /// Creates an eframe-backed renderer from the active WGPU render state.
    pub fn new(
        render_state: &egui_wgpu::RenderState,
        size: [u32; 2],
    ) -> Result<Self, RendererError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let keepalive = WGPU_INSTANCE_KEEPALIVE.get_or_init(|| Mutex::new(None));
            *keepalive
                .lock()
                .expect("renderer WGPU instance lock poisoned") =
                Some(render_state.instance.clone());
        }
        Ok(Self {
            backend: trrender::Renderer::new(
                &render_state.device,
                &render_state.queue,
                render_state.target_format,
                size,
            )?,
            #[cfg(feature = "meshpart")]
            viewport_targets: HashMap::default(),
        })
    }

    /// Builds a render snapshot, adds stencil outlines for selected instances, and submits it.
    pub fn prepare_eframe_scene(
        &mut self,
        workspace: &Workspace,
        size: [u32; 2],
        selected: &[InstanceId],
    ) -> Result<(), RendererError> {
        let scene = workspace.render_scene();
        let targets: Vec<_> = selected
            .iter()
            .copied()
            .filter(|&id| workspace.instance(id).is_some())
            .map(|id| id.data().as_ffi())
            .collect();
        let mut scene = scene;
        if !targets.is_empty() {
            scene.outlines.insert(
                0,
                RenderOutline {
                    mode: OutlineMode::Silhouette,
                    targets,
                    color: Color3::new(0.1, 0.45, 1.0),
                    width: 1.0,
                    threshold: 0.08,
                    always_on_top: true,
                },
            );
        }
        self.backend.prepare(&scene, size)
    }

    /// Draws the prepared output texture into eframe's WGPU paint pass.
    pub fn paint_eframe_scene<'a>(&mut self, pass: &mut crate::wgpu::RenderPass<'a>) {
        self.backend.paint(pass);
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
