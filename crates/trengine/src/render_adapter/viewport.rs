use trscene::Instance;

use super::*;

instance!({
    #[instance(default = false, inspect = false)]
    /// An offscreen scene that can be rendered into an egui texture or a CPU-side [`trrender::Image`].
    ///
    /// A viewport owns a workspace, so callers can add materials, lights, or more
    /// instances through [`Self::workspace_mut`] after creating it from a mesh.
    #[derive(Clone, Debug)]
    pub struct Viewport {
        #[extends]
        workspace: Workspace,
        /// Output image dimensions in pixels.
        #[property(setter = apply_output_size)]
        output_size: [u32; 2],
        #[property]
        /// Color used to clear the viewport before drawing.
        clear_color: [f32; 4],
    }

    impl Viewport {
        /// Creates an empty viewport with a transparent background.
        pub fn new(size: [u32; 2]) -> Self {
            let size = normalized_size(size);
            let mut workspace = Workspace::new();
            workspace.lighting.clear_skybox();
            workspace.camera_mut().resize(size[0], size[1]);
            Self {
                workspace,
                output_size: size,
                clear_color: [0.0, 0.0, 0.0, 0.0],
            }
        }

        fn apply_output_size(&mut self, size: [u32; 2]) {
            self.output_size = normalized_size(size);
            self.workspace
                .camera_mut()
                .resize(self.output_size[0], self.output_size[1]);
        }
    }
});

trscene::__instance_inspection_items! {
    impl trscene::devtools::Inspect for Viewport {
        fn properties(&self) -> Vec<trscene::devtools::Property> {
            let mut properties = trscene::devtools::Inspect::properties(&self.workspace)
                .into_iter()
                .map(|property| property.prefixed("workspace"))
                .collect::<Vec<_>>();
            properties.extend(
                trscene::devtools::Inspect::properties(&self.clear_color)
                    .into_iter()
                    .map(|property| property.prefixed("clear_color")),
            );
            properties
        }

        fn set_property(
            &mut self,
            path: &str,
            value: trscene::devtools::PropertyValue,
        ) -> Result<(), trscene::devtools::PropertyError> {
            if let Some(tail) = trscene::devtools::property_tail(path, "workspace") {
                return trscene::devtools::Inspect::set_property(&mut self.workspace, tail, value);
            }
            if let Some(tail) = trscene::devtools::property_tail(path, "clear_color") {
                return trscene::devtools::Inspect::set_property(&mut self.clear_color, tail, value);
            }
            Err(trscene::devtools::PropertyError::NotFound)
        }

        fn validate_property(
            &self,
            path: &str,
            value: &trscene::devtools::PropertyValue,
        ) -> Result<(), trscene::devtools::PropertyError> {
            if let Some(tail) = trscene::devtools::property_tail(path, "workspace") {
                return trscene::devtools::Inspect::validate_property(&self.workspace, tail, value);
            }
            if let Some(tail) = trscene::devtools::property_tail(path, "clear_color") {
                return trscene::devtools::Inspect::validate_property(&self.clear_color, tail, value);
            }
            Err(trscene::devtools::PropertyError::NotFound)
        }
    }
}

impl RenderAdapter {
    pub(super) fn render_viewport(
        &mut self,
        viewport: &Viewport,
        render_state: &crate::egui_wgpu::RenderState,
    ) -> Result<ViewportTexture, RendererError> {
        let size = viewport.output_size();
        let viewport_id = viewport.id().data().as_ffi();
        self.ensure_viewport_target(viewport_id, size, render_state);
        let scene = viewport.workspace.render_scene();
        let target = self
            .viewport_targets
            .get(&viewport_id)
            .expect("viewport target is created before rendering");
        self.backend.render_to_view(
            &scene,
            &target.color_view,
            &target.depth_view,
            size,
            color(viewport.clear_color),
        )?;

        let (texture_id, register_texture) = {
            let mut egui_renderer = render_state.renderer.write();
            match target.texture_id {
                Some(texture_id) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(
                        &render_state.device,
                        &target.color_view,
                        crate::wgpu::FilterMode::Linear,
                        texture_id,
                    );
                    (texture_id, false)
                }
                None => (
                    egui_renderer.register_native_texture(
                        &render_state.device,
                        &target.color_view,
                        crate::wgpu::FilterMode::Linear,
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
        viewport_id: u64,
        size: [u32; 2],
        render_state: &crate::egui_wgpu::RenderState,
    ) {
        if self
            .viewport_targets
            .get(&viewport_id)
            .is_some_and(|target| target.size == size)
        {
            return;
        }

        if let Some(target) = self.viewport_targets.remove(&viewport_id)
            && let Some(texture_id) = target.texture_id
        {
            render_state.renderer.write().free_texture(&texture_id);
        }

        let color_texture = render_state
            .device
            .create_texture(&crate::wgpu::TextureDescriptor {
                label: Some("viewport color target"),
                size: crate::wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: crate::wgpu::TextureDimension::D2,
                format: render_state.target_format,
                usage: crate::wgpu::TextureUsages::RENDER_ATTACHMENT
                    | crate::wgpu::TextureUsages::TEXTURE_BINDING
                    | crate::wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
        let color_view = color_texture.create_view(&crate::wgpu::TextureViewDescriptor::default());
        let depth_texture = render_state
            .device
            .create_texture(&crate::wgpu::TextureDescriptor {
                label: Some("viewport depth target"),
                size: crate::wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: crate::wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: crate::wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
        let depth_view = depth_texture.create_view(&crate::wgpu::TextureViewDescriptor::default());
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

fn color(color: [f32; 4]) -> crate::wgpu::Color {
    crate::wgpu::Color {
        r: color[0] as f64,
        g: color[1] as f64,
        b: color[2] as f64,
        a: color[3] as f64,
    }
}

/// A GPU-backed viewport texture registered with egui.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportTexture {
    id: crate::egui::TextureId,
    size: [u32; 2],
}

impl ViewportTexture {
    /// Returns the egui texture identifier used to display this texture.
    pub fn id(self) -> crate::egui::TextureId {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_viewport_properties_preserve_output_size_behavior() {
        let mut viewport = Viewport::new([4, 2])
            .with_output_size([3, 0])
            .with_clear_color([0.25, 0.5, 0.75, 1.0]);

        assert_eq!(viewport.output_size(), [3, 1]);
        assert_eq!(viewport.clear_color(), [0.25, 0.5, 0.75, 1.0]);
        assert_eq!(viewport.workspace().camera().aspect(), 3.0);

        viewport.set_output_size([0, 0]);
        assert_eq!(viewport.output_size(), [1, 1]);
        assert_eq!(viewport.workspace().camera().aspect(), 1.0);
    }
}
