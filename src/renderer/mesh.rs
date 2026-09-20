use super::*;

impl Renderer {
    /// Uploads a custom mesh and returns its GPU handle.
    pub fn add_mesh(&mut self, mesh: &Mesh) -> Result<GpuMeshHandle, RendererError> {
        let gpu_mesh = self.create_gpu_mesh(mesh)?;
        let handle = GpuMeshHandle(self.meshes.len());
        self.meshes.push(gpu_mesh);
        Ok(handle)
    }

    pub(super) fn create_gpu_mesh(&self, mesh: &Mesh) -> Result<GpuMesh, RendererError> {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return Err(RendererError::EmptyMesh);
        }
        for &index in &mesh.indices {
            if index as usize >= mesh.vertices.len() {
                return Err(RendererError::InvalidMeshIndex { index });
            }
        }

        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh vertex buffer"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh index buffer"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        Ok(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        })
    }

    #[cfg(feature = "meshpart")]
    pub(super) fn meshpart_mesh(
        &mut self,
        meshpart: &MeshPart,
    ) -> Result<Option<GpuMeshHandle>, RendererError> {
        use crate::HasMeshPart;

        if let Some(cached) = self.meshpart_meshes.get(&meshpart.id())
            && cached.revision == meshpart.mesh_revision()
        {
            return Ok(Some(cached.handle));
        }
        if (meshpart.mesh().vertices.is_empty() || meshpart.mesh().indices.is_empty())
            && meshpart.mesh_handle().gltf().is_some()
        {
            return Ok(None);
        }

        let gpu_mesh = self.create_gpu_mesh(meshpart.mesh())?;
        let handle = if let Some(cached) = self.meshpart_meshes.get(&meshpart.id()).copied() {
            self.meshes[cached.handle.0] = gpu_mesh;
            cached.handle
        } else if let Some(handle) = self.free_meshpart_meshes.pop() {
            self.meshes[handle.0] = gpu_mesh;
            handle
        } else {
            let handle = GpuMeshHandle(self.meshes.len());
            self.meshes.push(gpu_mesh);
            handle
        };
        self.meshpart_meshes.insert(
            meshpart.id(),
            CachedMeshPart {
                revision: meshpart.mesh_revision(),
                handle,
            },
        );
        Ok(Some(handle))
    }
    pub(super) fn ensure_instance_capacity(&mut self, instance_count: usize) {
        let required_size =
            std::mem::size_of::<InstanceRaw>() as u64 * instance_count.max(1) as u64;
        if self.instance_buffer.size() >= required_size {
            return;
        }

        self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part instance buffer"),
            size: required_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    pub(super) fn ensure_outline_instance_capacity(&mut self, mode: usize, instance_count: usize) {
        let required_size =
            std::mem::size_of::<InstanceRaw>() as u64 * instance_count.max(1) as u64;
        if self.outline_instance_buffers[mode].size() >= required_size {
            return;
        }

        self.outline_instance_buffers[mode] = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline instance buffer"),
            size: required_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
    pub(super) fn draw_batches<'a>(
        &self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &wgpu::RenderPipeline,
        camera_bind_group: &wgpu::BindGroup,
        dynamic_offsets: &[wgpu::DynamicOffset],
        use_materials: bool,
        visibility_bit: u16,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera_bind_group, dynamic_offsets);
        if use_materials {
            pass.set_bind_group(2, &self.material_factors_bind_group, &[]);
        }
        let mut bound_mesh = None;
        let mut bound_material = None;
        let mut index = 0;
        while index < self.prepared_batches.len() {
            let batch = &self.prepared_batches[index];
            if batch.instance_count == 0 || batch.visibility_mask & visibility_bit == 0 {
                index += 1;
                continue;
            }
            let mut instance_count = batch.instance_count;
            let mut next = index + 1;
            while let Some(candidate) = self.prepared_batches.get(next) {
                if candidate.visibility_mask & visibility_bit == 0
                    || candidate.mesh != batch.mesh
                    || (use_materials
                        && (candidate.packed_textures != batch.packed_textures
                            || candidate.filters != batch.filters))
                    || candidate.instance_start != batch.instance_start + instance_count as usize
                {
                    break;
                }
                instance_count += candidate.instance_count;
                next += 1;
            }
            let Some(mesh) = self.meshes.get(batch.mesh.0) else {
                index = next;
                continue;
            };
            if bound_mesh != Some(batch.mesh) {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                bound_mesh = Some(batch.mesh);
            }
            let instance_start =
                batch.instance_start as u64 * std::mem::size_of::<InstanceRaw>() as u64;
            let instance_end =
                instance_start + instance_count as u64 * std::mem::size_of::<InstanceRaw>() as u64;
            pass.set_vertex_buffer(1, self.instance_buffer.slice(instance_start..instance_end));
            let batch_material = (batch.packed_textures, batch.filters);
            if use_materials
                && bound_material != Some(batch_material)
                && let Some(bind_group) = self.material_bind_groups.get(&batch_material)
            {
                pass.set_bind_group(1, bind_group, &[]);
                bound_material = Some(batch_material);
            }
            pass.draw_indexed(0..mesh.index_count, 0, 0..instance_count);
            index = next;
        }
    }

    pub(super) fn draw_scene<'a>(&self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_skybox(pass);
        self.draw_batches(pass, &self.pipeline, &self.camera_bind_group, &[], true, 1);
    }

    pub(super) fn draw_outline_batches<'a>(
        &self,
        pass: &mut wgpu::RenderPass<'a>,
        mode: OutlineMode,
        pipeline: &wgpu::RenderPipeline,
        use_outline_uniform: bool,
    ) {
        let mode = mode.index();
        let batches = &self.outline_batches[mode];
        if batches.is_empty() {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        if use_outline_uniform {
            pass.set_bind_group(1, &self.outline_geometry_bind_group, &[]);
        }
        let instance_buffer = &self.outline_instance_buffers[mode];
        let mut bound_mesh = None;
        for batch in batches {
            let Some(mesh) = self.meshes.get(batch.mesh.0) else {
                continue;
            };
            if bound_mesh != Some(batch.mesh) {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                bound_mesh = Some(batch.mesh);
            }
            let instance_start =
                batch.instance_start as u64 * std::mem::size_of::<InstanceRaw>() as u64;
            let instance_end = instance_start
                + batch.instances.len() as u64 * std::mem::size_of::<InstanceRaw>() as u64;
            pass.set_vertex_buffer(1, instance_buffer.slice(instance_start..instance_end));
            pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
        }
    }

    pub(super) fn draw_skybox<'a>(&self, pass: &mut wgpu::RenderPass<'a>) {
        let Some(bind_group) = &self.skybox_bind_group else {
            return;
        };
        pass.set_pipeline(&self.skybox_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, self.skybox_vertex_buffer.slice(..));
        pass.set_index_buffer(
            self.skybox_index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );
        pass.draw_indexed(0..SKYBOX_INDICES.len() as u32, 0, 0..1);
    }
    pub(super) fn draw_shadow_scene<'a>(
        &self,
        pass: &mut wgpu::RenderPass<'a>,
        uniform_index: usize,
        visibility_bit: u16,
    ) {
        self.draw_batches(
            pass,
            &self.shadow_pipeline,
            &self.shadow_camera_bind_group,
            &[uniform_index as u32 * self.shadow_camera_stride],
            false,
            visibility_bit,
        );
    }
}
