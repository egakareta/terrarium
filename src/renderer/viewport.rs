use super::*;

/// An offscreen scene that can be rendered into an egui texture or a CPU-side [`Image`].
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

impl Renderer {
    pub(super) fn render_viewport(
        &mut self,
        viewport: &Viewport,
        render_state: &crate::egui_wgpu::RenderState,
    ) -> Result<ViewportTexture, RendererError> {
        let size = viewport.size;
        let viewport_id = viewport.workspace.id();
        self.ensure_viewport_target(viewport_id, size, render_state);

        self.prepare_scene(&viewport.workspace)?;
        {
            let target = self
                .viewport_targets
                .get(&viewport_id)
                .expect("viewport target is created before rendering");
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("viewport command encoder"),
                });
            self.encode_shadow_pass(&mut encoder);
            self.encode_scene_pass(
                &mut encoder,
                &target.color_view,
                &target.depth_view,
                color(viewport.clear_color),
            );
            self.queue.submit(Some(encoder.finish()));
        }

        let (texture_id, register_texture) = {
            let target = self
                .viewport_targets
                .get(&viewport_id)
                .expect("viewport target is created before rendering");
            let mut egui_renderer = render_state.renderer.write();
            match target.texture_id {
                Some(texture_id) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(
                        &render_state.device,
                        &target.color_view,
                        wgpu::FilterMode::Linear,
                        texture_id,
                    );
                    (texture_id, false)
                }
                None => (
                    egui_renderer.register_native_texture(
                        &render_state.device,
                        &target.color_view,
                        wgpu::FilterMode::Linear,
                    ),
                    true,
                ),
            }
        };
        if register_texture {
            self.viewport_targets
                .get_mut(&viewport_id)
                .expect("viewport target is created before rendering")
                .texture_id = Some(texture_id);
        }

        Ok(ViewportTexture {
            id: texture_id,
            size,
        })
    }

    fn ensure_viewport_target(
        &mut self,
        viewport_id: InstanceId,
        size: [u32; 2],
        render_state: &crate::egui_wgpu::RenderState,
    ) {
        let needs_target = self
            .viewport_targets
            .get(&viewport_id)
            .is_none_or(|target| target.size != size);
        if !needs_target {
            return;
        }

        if let Some(target) = self.viewport_targets.remove(&viewport_id)
            && let Some(texture_id) = target.texture_id
        {
            render_state.renderer.write().free_texture(&texture_id);
        }

        let color_texture =
            create_eframe_scene_texture(&self.device, self.eframe_scene.format, size[0], size[1]);
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (depth_texture, depth_view) = create_depth_texture(&self.device, size[0], size[1]);
        self.viewport_targets.insert(
            viewport_id,
            ViewportTarget {
                _color_texture: color_texture,
                color_view,
                _depth_texture: depth_texture,
                depth_view,
                size,
                texture_id: None,
            },
        );
    }
}

fn normalized_size(size: [u32; 2]) -> [u32; 2] {
    [size[0].max(1), size[1].max(1)]
}

fn color(color: [f32; 4]) -> wgpu::Color {
    wgpu::Color {
        r: color[0] as f64,
        g: color[1] as f64,
        b: color[2] as f64,
        a: color[3] as f64,
    }
}

/// A GPU-backed viewport texture registered with egui.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportTexture {
    id: egui::TextureId,
    size: [u32; 2],
}

impl ViewportTexture {
    /// Returns the egui texture identifier used to display this texture.
    pub fn id(self) -> egui::TextureId {
        self.id
    }

    /// Returns the texture dimensions in pixels.
    pub fn size(self) -> [u32; 2] {
        self.size
    }
}

impl crate::Engine {
    /// Renders a viewport into a GPU texture registered with an egui renderer.
    pub fn render_viewport(
        &mut self,
        viewport: &Viewport,
    ) -> Result<ViewportTexture, RendererError> {
        let render_state = self
            .creation_context()
            .wgpu_render_state
            .as_ref()
            .ok_or(RendererError::MissingEframeWgpuRenderState)?
            .clone();
        self.renderer().render_viewport(viewport, &render_state)
    }
}
