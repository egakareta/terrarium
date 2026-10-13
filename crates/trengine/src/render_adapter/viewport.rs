use trscene::Instance;

use super::*;

/// An offscreen scene that can be rendered into a React viewport or a CPU-side [`trrender::Image`].
///
/// A viewport owns a workspace, so callers can add materials, lights, or more
/// instances through [`Self::workspace_mut`] after creating it from a mesh.
pub struct Viewport {
    workspace: Workspace,
    size: [u32; 2],
    clear_color: [f32; 4],
}

impl Viewport {
    /// Creates an empty viewport with a transparent background.
    pub fn new(size: [u32; 2]) -> Self {
        let size = normalized_size(size);
        let mut workspace = Workspace::new();
        workspace.lighting.clear_skybox();
        workspace.current_camera.resize(size[0], size[1]);
        Self {
            workspace,
            size,
            clear_color: [0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Returns the viewport's workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Returns mutable access to the viewport's workspace.
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    /// Returns the output image dimensions in pixels.
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Returns the color used to clear the viewport before drawing.
    pub fn clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Returns this viewport with a new output size in pixels.
    pub fn with_size(mut self, size: [u32; 2]) -> Self {
        self.set_size(size);
        self
    }

    /// Changes the output image dimensions in pixels.
    pub fn set_size(&mut self, size: [u32; 2]) -> &mut Self {
        self.size = normalized_size(size);
        self.workspace
            .current_camera
            .resize(self.size[0], self.size[1]);
        self
    }

    /// Returns this viewport with a new RGBA clear color.
    pub fn with_clear_color(mut self, color: [f32; 4]) -> Self {
        self.clear_color = color;
        self
    }

    /// Changes the RGBA color used to clear the viewport before drawing.
    pub fn set_clear_color(&mut self, color: [f32; 4]) -> &mut Self {
        self.clear_color = color;
        self
    }
}

impl RenderAdapter {
    pub(super) fn render_viewport(
        &mut self,
        viewport: &Viewport,
    ) -> Result<ViewportTexture, RendererError> {
        use slotmap::Key;
        let id = viewport.workspace.id().data().as_ffi();
        if !self.viewport_targets.contains_key(&id) {
            self.viewport_targets.insert(
                id,
                trrender::Renderer::new(
                    &self.state.device,
                    &self.state.queue,
                    self.state.target_format,
                    viewport.size,
                )?,
            );
        }
        let renderer = self
            .viewport_targets
            .get_mut(&id)
            .expect("viewport renderer was inserted");
        renderer.with_clear_color(color(viewport.clear_color));
        renderer.prepare(&viewport.workspace.render_scene(), viewport.size)?;
        Ok(ViewportTexture {
            id,
            size: viewport.size,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn viewport_pixels(&self, id: u64) -> Result<Vec<u8>, RendererError> {
        let renderer = self
            .viewport_targets
            .get(&id)
            .ok_or_else(|| RendererError::PixelReadback("viewport has been released".into()))?;
        Ok(renderer.read_pixels()?.into_iter().flatten().collect())
    }

    /// Releases an offscreen viewport's GPU resources.
    pub fn release_viewport(&mut self, texture: ViewportTexture) {
        self.viewport_targets.remove(&texture.id);
    }

    /// Reads a viewport's most recently rendered RGBA pixels.
    pub fn read_viewport_pixels(
        &self,
        texture: ViewportTexture,
    ) -> Result<Vec<[u8; 4]>, RendererError> {
        self.viewport_targets
            .get(&texture.id)
            .ok_or_else(|| RendererError::PixelReadback("viewport has been released".into()))?
            .read_pixels()
    }
}

fn normalized_size(size: [u32; 2]) -> [u32; 2] {
    [size[0].max(1), size[1].max(1)]
}

fn color(color: [f32; 4]) -> crate::wgpu::Color {
    crate::wgpu::Color {
        r: color[0] as f64,
        g: color[1] as f64,
        b: color[2] as f64,
        a: color[3] as f64,
    }
}

/// An offscreen viewport handle understood by native and browser hosts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ViewportTexture {
    id: u64,
    size: [u32; 2],
}

impl ViewportTexture {
    /// Returns a stable renderer-local viewport identifier.
    pub fn id(self) -> u64 {
        self.id
    }
    /// Returns the texture dimensions in pixels.
    pub fn size(self) -> [u32; 2] {
        self.size
    }
}

impl crate::Engine {
    /// Renders an offscreen scene for use by a React viewport component.
    pub fn render_viewport(
        &mut self,
        viewport: &Viewport,
    ) -> Result<ViewportTexture, RendererError> {
        self.renderer().render_viewport(viewport)
    }
}
