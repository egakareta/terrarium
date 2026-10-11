use super::*;

impl Renderer {
    pub(super) fn sync_skybox(&mut self, skybox: Option<&Skybox>) -> Result<(), RendererError> {
        let Some(skybox) = skybox else {
            self.skybox_texture = None;
            self.skybox_view = None;
            self.skybox_bind_group = None;
            self.refresh_environment_binding();
            return Ok(());
        };
        let face_size = skybox.face_size();
        let first = Texture::from_image(
            skybox.face(Face::ALL_CUBEMAP[0]).clone(),
            TextureColorSpace::Srgb,
        )?;
        let mip_levels = first.mip_levels()?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skybox cubemap"),
            size: wgpu::Extent3d {
                width: face_size,
                height: face_size,
                depth_or_array_layers: Face::ALL_CUBEMAP.len() as u32,
            },
            mip_level_count: mip_levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, face) in Face::ALL_CUBEMAP.into_iter().enumerate() {
            let face_texture = if layer == 0 {
                first.clone()
            } else {
                Texture::from_image(skybox.face(face).clone(), TextureColorSpace::Srgb)?
            };
            for (level, mip) in face_texture.mip_levels()?.iter().enumerate() {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &mip.pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip.width * 4),
                        rows_per_image: Some(mip.height),
                    },
                    wgpu::Extent3d {
                        width: mip.width,
                        height: mip.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("skybox cubemap view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(Face::ALL_CUBEMAP.len() as u32),
            ..Default::default()
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skybox bind group"),
            layout: &self.skybox_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.skybox_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.skybox_sampler),
                },
            ],
        });
        self.skybox_texture = Some(texture);
        self.skybox_view = Some(view);
        self.skybox_bind_group = Some(bind_group);
        self.refresh_environment_binding();
        Ok(())
    }

    /// Rebinds the PBR environment map to the current skybox view, or to the
    /// black fallback cubemap when no skybox is set.
    pub(super) fn refresh_environment_binding(&mut self) {
        let camera_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &self.camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self._shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        self.skybox_view
                            .as_ref()
                            .unwrap_or(&self.fallback_environment_view),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.skybox_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: self.local_lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&self.local_shadow_view),
                },
            ],
        });
        self.camera_bind_group = camera_bind_group;
    }
}
