use super::*;

impl Renderer {
    pub(super) fn material_textures(
        &mut self,
        workspace: &Workspace,
        material_slots: &MeshMaterialSlots,
    ) -> Result<MaterialTextures, RendererError> {
        let resolve = |renderer: &mut Self,
                       handle: Option<TextureHandle>,
                       fallback: GpuTextureHandle|
         -> Result<GpuTextureHandle, RendererError> {
            handle.map_or(Ok(fallback), |handle| {
                renderer.upload_workspace_texture(workspace, handle)
            })
        };

        let base_color = self.default_material_textures.base_color[0];
        let normal = self.default_material_textures.normal[0];
        let metallic_roughness = self.default_material_textures.metallic_roughness[0];
        let emissive = self.default_material_textures.emissive[0];
        let mut textures = MaterialTextures {
            base_color: [base_color; MATERIAL_SLOT_COUNT],
            normal: [normal; MATERIAL_SLOT_COUNT],
            metallic_roughness: [metallic_roughness; MATERIAL_SLOT_COUNT],
            emissive: [emissive; MATERIAL_SLOT_COUNT],
        };
        for (index, material) in material_slots
            .slots
            .iter()
            .take(MATERIAL_SLOT_COUNT)
            .enumerate()
            .filter_map(|(index, material)| material.as_ref().map(|material| (index, material)))
        {
            let slot = index;
            textures.base_color[slot] = resolve(self, material.textures.base_color, base_color)?;
            textures.normal[slot] = resolve(self, material.textures.normal, normal)?;
            textures.metallic_roughness[slot] = resolve(
                self,
                material.textures.metallic_roughness,
                metallic_roughness,
            )?;
            textures.emissive[slot] = resolve(self, material.textures.emissive, emissive)?;
        }
        Ok(textures)
    }

    pub(super) fn material_filters(
        material_slots: &MeshMaterialSlots,
    ) -> [TextureFilter; MATERIAL_SLOT_COUNT] {
        let mut filters = [TextureFilter::default(); MATERIAL_SLOT_COUNT];
        for (index, material) in material_slots
            .slots
            .iter()
            .take(MATERIAL_SLOT_COUNT)
            .enumerate()
        {
            if let Some(material) = material {
                filters[index] = material.filter;
            }
        }
        filters
    }

    pub(super) fn material_set_index(&mut self, material_slots: &MeshMaterialSlots) -> u32 {
        // Fast path: no overrides means every face uses the default material,
        // so only the 9 default words need comparing.
        if material_slots.slots.is_empty() {
            let default = MaterialSetKey::base_bits(&DEFAULT_MATERIAL);
            if let Some((last_default, index)) = self.material_factor_last_uniform
                && last_default == default
            {
                return index;
            }
            let key = MaterialSetKey::uniform(default);
            let index = self.material_set_index_uncached(key);
            self.material_factor_last_uniform = Some((default, index));
            return index;
        }
        let key = MaterialSetKey::from_materials(material_slots);
        if let Some((last_key, index)) = self.material_factor_last
            && last_key == key
        {
            return index;
        }
        let index = self.material_set_index_uncached(key);
        self.material_factor_last = Some((key, index));
        index
    }

    pub(super) fn material_set_index_uncached(&mut self, key: MaterialSetKey) -> u32 {
        if let Some(&index) = self.material_factor_indices.get(&key) {
            return index;
        }
        let index = (self.material_factor_vec4s.len() / MATERIAL_VEC4S_PER_SET) as u32;
        self.material_factor_vec4s.extend(key.vec4s());
        self.material_factor_indices.insert(key, index);
        index
    }

    pub(super) fn upload_material_factors(&mut self) {
        if self.material_factor_vec4s.is_empty() {
            return;
        }
        let required_height =
            (self.material_factor_vec4s.len() / MATERIAL_VEC4S_PER_SET).max(1) as u32;
        if self.material_factors_texture.height() < required_height {
            let capacity = required_height.max(self.material_factors_texture.height().max(1) * 2);
            let (texture, view) = create_material_factor_texture(&self.device, capacity);
            self.material_factors_texture = texture;
            self.material_factors_view = view;
            self.material_factors_bind_group = create_material_factor_bind_group(
                &self.device,
                &self.material_factor_bind_group_layout,
                &self.material_factors_view,
            );
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.material_factors_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.material_factor_vec4s),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(
                    MATERIAL_FACTOR_TEXTURE_WIDTH * std::mem::size_of::<[f32; 4]>() as u32,
                ),
                rows_per_image: Some(required_height),
            },
            wgpu::Extent3d {
                width: MATERIAL_FACTOR_TEXTURE_WIDTH,
                height: required_height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub(super) fn upload_workspace_texture(
        &mut self,
        workspace: &Workspace,
        handle: TextureHandle,
    ) -> Result<GpuTextureHandle, RendererError> {
        let workspace_handle = (workspace.id(), handle);
        let texture = workspace
            .get_texture(handle)
            .ok_or(RendererError::InvalidTextureHandle { index: handle.0 })?;
        let version = workspace.texture_version(handle).unwrap_or(0);
        if let Some(&(gpu_handle, cached_version)) =
            self.workspace_texture_handles.get(&workspace_handle)
        {
            if cached_version == version {
                return Ok(gpu_handle);
            }
            // The workspace texture was edited after upload.
            if self.texture_source_is_shared(gpu_handle, texture) {
                // This source entry is shared with another logical texture (a
                // dedup hit or a built-in default), so preserve it for the
                // other owners and allocate a new entry.
                let new_handle = GpuTextureHandle(self.textures.len());
                texture.validate()?;
                self.textures.push(GpuTexture {
                    source: texture.clone(),
                });
                self.texture_dedup.insert(texture.clone(), new_handle);
                self.workspace_texture_handles
                    .insert(workspace_handle, (new_handle, version));
                return Ok(new_handle);
            }
            self.refresh_workspace_texture(gpu_handle, texture)?;
            self.workspace_texture_handles
                .insert(workspace_handle, (gpu_handle, version));
            return Ok(gpu_handle);
        }

        let gpu_handle = self.upload_dedup_texture(texture)?;
        self.workspace_texture_handles
            .insert(workspace_handle, (gpu_handle, version));
        Ok(gpu_handle)
    }

    /// Reports whether `gpu_handle` is sampled by another logical texture.
    ///
    /// That happens when distinct workspace textures deduplicated to the same
    /// GPU copy, or when a workspace texture matched a built-in default.
    /// Such copies must not be overwritten in place on edit.
    pub(super) fn texture_source_is_shared(
        &self,
        gpu_handle: GpuTextureHandle,
        texture: &Texture,
    ) -> bool {
        if self
            .default_material_textures
            .base_color
            .contains(&gpu_handle)
            || self.default_material_textures.normal.contains(&gpu_handle)
            || self
                .default_material_textures
                .metallic_roughness
                .contains(&gpu_handle)
            || self
                .default_material_textures
                .emissive
                .contains(&gpu_handle)
        {
            return true;
        }
        self.texture_dedup
            .iter()
            .any(|(known, &known_handle)| known_handle == gpu_handle && known != texture)
    }

    /// Stores an edited workspace texture and refreshes every packed material
    /// texture built from it.
    ///
    /// The caller guarantees `gpu_handle` is exclusively owned by this
    /// texture (see [`texture_source_is_shared`](Self::texture_source_is_shared)).
    /// Textures validate on the way in, so edits made through
    /// [`Workspace::get_texture_mut`] that break the RGBA8 invariant surface
    /// here as an error instead of panicking inside surface packing.
    pub(super) fn refresh_workspace_texture(
        &mut self,
        gpu_handle: GpuTextureHandle,
        texture: &Texture,
    ) -> Result<(), RendererError> {
        texture.validate()?;
        let old_source =
            std::mem::replace(&mut self.textures[gpu_handle.0].source, texture.clone());
        if self.texture_dedup.get(&old_source) == Some(&gpu_handle) {
            self.texture_dedup.remove(&old_source);
        }
        self.texture_dedup.insert(texture.clone(), gpu_handle);
        self.refresh_packed_textures(gpu_handle)
    }

    /// Rewrites every packed material texture that samples `changed`.
    ///
    /// Packed textures keep their GPU objects (and the views/bind groups over
    /// them) whenever the base dimensions and format still match; entries
    /// that outgrew their storage are recreated and their bind groups
    /// dropped so they rebuild with fresh views.
    pub(super) fn refresh_packed_textures(
        &mut self,
        changed: GpuTextureHandle,
    ) -> Result<(), RendererError> {
        let affected: Vec<MaterialTextures> = self
            .packed_material_textures
            .keys()
            .filter(|textures| {
                textures.base_color.contains(&changed)
                    || textures.normal.contains(&changed)
                    || textures.metallic_roughness.contains(&changed)
                    || textures.emissive.contains(&changed)
            })
            .copied()
            .collect();
        let mut recreated = Vec::new();
        let mut refreshed = HashSet::new();
        for textures in affected {
            let packed = self.packed_material_textures[&textures];
            for (slot, packed_handle) in packed.textures.into_iter().enumerate() {
                if !refreshed.insert(packed_handle) {
                    continue;
                }
                let base_texture = self.textures[textures.base_color[slot].0].source.clone();
                let surface_texture = Texture::linear(
                    base_texture.width,
                    base_texture.height,
                    pack_surface_pixels(
                        &self.textures[textures.normal[slot].0].source,
                        &self.textures[textures.metallic_roughness[slot].0].source,
                        base_texture.width,
                        base_texture.height,
                    ),
                )?;
                let emissive_texture = Texture::new(
                    base_texture.width,
                    base_texture.height,
                    resize_texture_pixels(
                        &self.textures[textures.emissive[slot].0].source,
                        base_texture.width,
                        base_texture.height,
                    ),
                )?;
                let base_mips = base_texture.mip_levels()?;
                let surface_mips = surface_texture.mip_levels()?;
                let emissive_mips = emissive_texture.mip_levels()?;
                let format = texture_gpu_format(base_texture.color_space);
                let entry = &self.gpu_material_textures[packed_handle.0];
                if entry._texture.size().width != base_texture.width
                    || entry._texture.size().height != base_texture.height
                    || entry._texture.format() != format
                    || entry._texture.mip_level_count() != base_mips.len() as u32
                {
                    let replacement = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("packed material texture"),
                        size: wgpu::Extent3d {
                            width: base_texture.width,
                            height: base_texture.height,
                            depth_or_array_layers: 3,
                        },
                        mip_level_count: base_mips.len() as u32,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    write_packed_mips(
                        &self.queue,
                        &replacement,
                        &base_mips,
                        &surface_mips,
                        &emissive_mips,
                        base_texture.color_space,
                    );
                    let view = replacement.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2Array),
                        array_layer_count: Some(3),
                        ..Default::default()
                    });
                    self.gpu_material_textures[packed_handle.0] = GpuMaterialTexture {
                        _texture: replacement,
                        view,
                    };
                    recreated.push(packed_handle);
                } else {
                    write_packed_mips(
                        &self.queue,
                        &entry._texture,
                        &base_mips,
                        &surface_mips,
                        &emissive_mips,
                        base_texture.color_space,
                    );
                }
            }
        }
        if !recreated.is_empty() {
            self.material_bind_groups
                .retain(|(packed, _), _| !packed.textures.iter().any(|t| recreated.contains(t)));
        }
        Ok(())
    }

    pub(super) fn upload_dedup_texture(
        &mut self,
        texture: &Texture,
    ) -> Result<GpuTextureHandle, RendererError> {
        if let Some(&gpu_handle) = self.texture_dedup.get(texture) {
            return Ok(gpu_handle);
        }
        texture.validate()?;
        let gpu_handle = GpuTextureHandle(self.textures.len());
        self.textures.push(GpuTexture {
            source: texture.clone(),
        });
        self.texture_dedup.insert(texture.clone(), gpu_handle);
        Ok(gpu_handle)
    }

    pub(super) fn upload_material_texture(
        &mut self,
        base_color: &Texture,
        surface: &Texture,
        emissive: &Texture,
    ) -> Result<PackedTextureHandle, RendererError> {
        let base_mips = base_color.mip_levels()?;
        let surface_mips = surface.mip_levels()?;
        let emissive_mips = emissive.mip_levels()?;
        let format = texture_gpu_format(base_color.color_space);
        let gpu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("packed material texture"),
            size: wgpu::Extent3d {
                width: base_color.width,
                height: base_color.height,
                depth_or_array_layers: 3,
            },
            mip_level_count: base_mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_packed_mips(
            &self.queue,
            &gpu_texture,
            &base_mips,
            &surface_mips,
            &emissive_mips,
            base_color.color_space,
        );
        let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(3),
            ..Default::default()
        });
        let handle = PackedTextureHandle(self.gpu_material_textures.len());
        self.gpu_material_textures.push(GpuMaterialTexture {
            _texture: gpu_texture,
            view,
        });
        Ok(handle)
    }

    pub(super) fn pack_material_textures(
        &mut self,
        textures: MaterialTextures,
    ) -> Result<PackedMaterialTextures, RendererError> {
        if let Some(&packed) = self.packed_material_textures.get(&textures) {
            return Ok(packed);
        }

        let mut packed_textures = [PackedTextureHandle(usize::MAX); MATERIAL_SLOT_COUNT];
        let mut packed_slots = HashMap::new();
        for (slot, packed_texture) in packed_textures.iter_mut().enumerate() {
            let slot_textures = (
                textures.base_color[slot],
                textures.normal[slot],
                textures.metallic_roughness[slot],
                textures.emissive[slot],
            );
            if let Some(&packed) = packed_slots.get(&slot_textures) {
                *packed_texture = packed;
                continue;
            }
            let base_texture = self.textures[textures.base_color[slot].0].source.clone();
            let surface_texture = Texture::linear(
                base_texture.width,
                base_texture.height,
                pack_surface_pixels(
                    &self.textures[textures.normal[slot].0].source,
                    &self.textures[textures.metallic_roughness[slot].0].source,
                    base_texture.width,
                    base_texture.height,
                ),
            )?;
            let emissive_texture = Texture::new(
                base_texture.width,
                base_texture.height,
                resize_texture_pixels(
                    &self.textures[textures.emissive[slot].0].source,
                    base_texture.width,
                    base_texture.height,
                ),
            )?;
            *packed_texture =
                self.upload_material_texture(&base_texture, &surface_texture, &emissive_texture)?;
            packed_slots.insert(slot_textures, *packed_texture);
        }
        let packed = PackedMaterialTextures {
            textures: packed_textures,
        };
        self.packed_material_textures.insert(textures, packed);
        Ok(packed)
    }

    pub(super) fn material_bind_group(
        &mut self,
        textures: PackedMaterialTextures,
        filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    ) -> &wgpu::BindGroup {
        let key = (textures, filters);
        if !self.material_bind_groups.contains_key(&key) {
            let bind_group = self.create_material_bind_group(textures, filters);
            self.material_bind_groups.insert(key, bind_group);
        }
        &self.material_bind_groups[&key]
    }

    pub(super) fn create_material_bind_group(
        &self,
        textures: PackedMaterialTextures,
        filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    ) -> wgpu::BindGroup {
        let mut entries = Vec::with_capacity(MATERIAL_SLOT_COUNT * 2);
        for (slot, texture) in textures.textures.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: slot as u32,
                resource: wgpu::BindingResource::TextureView(
                    &self.gpu_material_textures[texture.0].view,
                ),
            });
        }
        for (slot, filter) in filters.into_iter().enumerate() {
            let sampler = self
                .material_samplers
                .get(&filter)
                .expect("material sampler exists for every TextureFilter");
            entries.push(wgpu::BindGroupEntry {
                binding: (MATERIAL_SLOT_COUNT + slot) as u32,
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material bind group"),
            layout: &self.material_bind_group_layout,
            entries: &entries,
        })
    }
}
