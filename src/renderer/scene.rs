use super::*;
use crate::{HasBasePart, HasPVInstance, HasPart, Instance, Outline};

fn collect_outline_targets(
    workspace: &Workspace,
) -> (
    [HashMap<InstanceId, OutlineSpec>; 3],
    [Option<OutlineSpec>; 3],
) {
    let mut targets = std::array::from_fn(|_| HashMap::new());
    let mut specs = [None; 3];
    for outline in workspace.get_all::<Outline>() {
        let Some(parent_id) = outline.parent() else {
            continue;
        };
        let Some(parent) = workspace.instance(parent_id) else {
            continue;
        };
        let Some(container_id) = parent.parent() else {
            continue;
        };
        let mode = outline.mode().index();
        let spec = OutlineSpec {
            color: outline.color(),
            width: outline.width(),
            threshold: outline.threshold(),
        };
        specs[mode].get_or_insert(spec);
        targets[mode].insert(parent_id, spec);
        if container_id == workspace.id() {
            for sibling in workspace.children() {
                targets[mode].insert(sibling.id(), spec);
            }
        } else if let Some(container) = workspace.instance(container_id) {
            for sibling in container.children() {
                targets[mode].insert(sibling.id(), spec);
            }
        }
    }
    (targets, specs)
}

fn outline_color(color: Color3) -> [f32; 4] {
    [
        color_channel(color.r),
        color_channel(color.g),
        color_channel(color.b),
        1.0,
    ]
}

fn outline_params(spec: Option<OutlineSpec>) -> [f32; 4] {
    spec.map_or([0.0; 4], |spec| {
        [
            finite_or(spec.width, 1.0).max(1.0),
            finite_or(spec.threshold, 0.08).clamp(0.0, 1.0),
            0.0,
            0.0,
        ]
    })
}

fn append_outline_instance(
    batches: &mut [Vec<OutlineRenderBatch>; 3],
    targets: &[HashMap<InstanceId, OutlineSpec>; 3],
    id: InstanceId,
    mesh: GpuMeshHandle,
    instance: InstanceRaw,
) {
    for mode in 0..3 {
        if !targets[mode].contains_key(&id) {
            continue;
        }
        if let Some(batch) = batches[mode].iter_mut().find(|batch| batch.mesh == mesh) {
            batch.instances.push(instance);
        } else {
            batches[mode].push(OutlineRenderBatch {
                mesh,
                instances: vec![instance],
                instance_start: 0,
            });
        }
    }
}

impl Renderer {
    pub(super) fn prepare_scene(&mut self, workspace: &Workspace) -> Result<(), RendererError> {
        self.prepared_batches.clear();
        let (outline_targets, outline_specs) = collect_outline_targets(workspace);
        self.queue.write_buffer(
            &self.outline_uniform_buffer,
            0,
            bytemuck::bytes_of(&OutlineUniform {
                toon_params: outline_params(outline_specs[0]),
                toon_color: outline_specs[0].map_or([0.0; 4], |spec| outline_color(spec.color)),
                silhouette_params: outline_params(outline_specs[1]),
                silhouette_color: outline_specs[1]
                    .map_or([0.0; 4], |spec| outline_color(spec.color)),
                stencil_params: outline_params(outline_specs[2]),
                stencil_color: outline_specs[2].map_or([0.0; 4], |spec| outline_color(spec.color)),
            }),
        );
        let mut outline_batches = std::mem::take(&mut self.outline_batches);
        for batches in &mut outline_batches {
            batches.clear();
        }
        let camera_vp = workspace.current_camera.view_projection_matrix();
        let camera_planes = frustum_planes(camera_vp);
        let local_shadows = self.prepare_local_lights(workspace, &camera_planes);
        let lighting = &workspace.lighting;
        let light_direction = lighting.render_sun_direction();
        let (light_vps, shadow_cascade_splits, shadow_texel_sizes) =
            light_view_projections(&workspace.current_camera, light_direction);
        let daylight = lighting.daylight_factor();
        let style_scale = match lighting.lighting_style {
            crate::LightingStyle::Realistic => 1.0,
            crate::LightingStyle::Soft => 0.72,
        };
        let brightness = finite_nonnegative(lighting.brightness);
        let direct_scale = brightness * 1.5 * daylight * style_scale;
        let exposure = finite_or(lighting.exposure_compensation, 0.0)
            .clamp(-16.0, 16.0)
            .exp2();
        let fog_start = finite_nonnegative(lighting.fog_start);
        let fog_end = finite_nonnegative(lighting.fog_end).max(fog_start + 0.001);
        self.shadow_pass_enabled = lighting.shadows_enabled();
        // Image-based lighting comes from the workspace skybox cubemap, which
        // carries a full CPU-generated mip chain: smooth surfaces sample sharp
        // reflections at LOD 0 while rough surfaces sample blurred mips.
        let (environment_mip_count, has_environment) = match lighting.skybox() {
            Some(skybox) => (skybox.face_size().ilog2() as f32 + 1.0, 1.0),
            None => (1.0, 0.0),
        };
        let camera_uniform = CameraUniform {
            view_projection: camera_vp.to_cols_array_2d(),
            light_view_projections: light_vps.map(|matrix| matrix.to_cols_array_2d()),
            camera_position: workspace
                .current_camera
                .pivot()
                .w_axis
                .truncate()
                .extend(1.0)
                .to_array(),
            camera_forward: workspace.current_camera.forward().extend(0.0).to_array(),
            light_direction: light_direction.extend(0.0).to_array(),
            light_color: [
                1.0 * direct_scale,
                0.93 * direct_scale,
                0.83 * direct_scale,
                0.0,
            ],
            ambient_color: scaled_color(lighting.ambient, 0.07),
            outdoor_ambient_color: scaled_color(
                lighting.outdoor_ambient,
                0.07 * (0.08 + daylight * 0.92),
            ),
            color_shift_top: lighting_color(lighting.color_shift_top),
            color_shift_bottom: lighting_color(lighting.color_shift_bottom),
            shadow_color: lighting_color(lighting.shadow_color),
            lighting_params: [
                daylight,
                if lighting.shadows_enabled() { 1.0 } else { 0.0 },
                finite_nonnegative(lighting.shadow_softness).min(1.0),
                exposure,
            ],
            fog_color: lighting_color(lighting.fog_color),
            fog_params: [fog_start, fog_end, 0.0, 0.0],
            shadow_cascade_splits: pack_shadow_values(shadow_cascade_splits),
            shadow_texel_sizes: pack_shadow_values(shadow_texel_sizes),
            environment_params: [
                environment_mip_count,
                finite_nonnegative(lighting.environment_diffuse_scale),
                finite_nonnegative(lighting.environment_specular_scale),
                has_environment,
            ],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        for (cascade, light_vp) in light_vps.iter().enumerate() {
            let uniform = ShadowCameraUniform {
                light_view_projection: light_vp.to_cols_array_2d(),
            };
            self.queue.write_buffer(
                &self.shadow_camera_buffer,
                cascade as u64 * u64::from(self.shadow_camera_stride),
                bytemuck::bytes_of(&uniform),
            );
        }
        let mut sky_view = workspace.current_camera.pivot().inverse();
        sky_view.w_axis = Vec4::new(0.0, 0.0, 0.0, 1.0);
        let sky_view_projection = workspace.current_camera.projection_matrix() * sky_view;
        self.queue.write_buffer(
            &self.skybox_uniform_buffer,
            0,
            bytemuck::bytes_of(&SkyboxCameraUniform {
                view_projection: sky_view_projection.to_cols_array_2d(),
                exposure: [exposure * (0.08 + daylight * 0.92), 0.0, 0.0, 0.0],
            }),
        );
        if self.skybox_revision != Some(lighting.skybox_revision()) {
            self.sync_skybox(lighting.skybox())?;
            self.skybox_revision = Some(lighting.skybox_revision());
        }
        let light_planes = light_vps.map(frustum_planes);
        let default_textures = self.default_material_textures;
        let default_filters = self.default_material_filters;

        #[cfg(feature = "meshpart")]
        let stale_meshparts: Vec<_> = self
            .meshpart_meshes
            .keys()
            .copied()
            .filter(|&id| workspace.get::<MeshPart>(id).is_none())
            .collect();
        #[cfg(feature = "meshpart")]
        for id in stale_meshparts {
            if let Some(cached) = self.meshpart_meshes.remove(&id) {
                self.free_meshpart_meshes.push(cached.handle);
            }
        }

        // Keep the finite set of default-material batches and their allocations
        // alive across frames. Custom material combinations can be unbounded,
        // so rebuild those rather than retaining stale scratch storage forever.
        let mut batches = std::mem::take(&mut self.batch_scratch);
        let mut batch_indices = std::mem::take(&mut self.batch_indices_scratch);
        batches.retain(|batch| {
            !batch.transparent
                && batch.textures == default_textures
                && ((self.primitive_meshes.contains(&batch.mesh)
                    && usize::from(batch.visibility_mask) < VISIBILITY_MASK_COUNT)
                    || (usize::from(batch.visibility_mask) >= VISIBILITY_MASK_COUNT
                        && !batch.instances.is_empty()))
        });
        batch_indices.clear();
        for batch in &mut batches {
            batch.instances.clear();
        }
        // Fast path for the common untextured case: index directly by shape
        // and pass visibility instead of hashing a large material key per part.
        let mut default_batches = [[None; VISIBILITY_MASK_COUNT]; PartShape::COUNT];
        for (batch_index, batch) in batches.iter().enumerate() {
            if usize::from(batch.visibility_mask) >= VISIBILITY_MASK_COUNT {
                batch_indices.insert(
                    (
                        batch.mesh,
                        default_textures,
                        default_filters,
                        batch.visibility_mask,
                    ),
                    batch_index,
                );
            }
        }
        for shape in PartShape::ALL {
            let mesh = self.primitive_meshes[shape.index()];
            for (batch_index, batch) in batches.iter().enumerate() {
                if batch.mesh == mesh && usize::from(batch.visibility_mask) < VISIBILITY_MASK_COUNT
                {
                    default_batches[shape.index()][batch.visibility_mask as usize] =
                        Some(batch_index);
                }
            }
        }
        let camera_position = workspace.current_camera.pivot().w_axis.truncate();
        let camera_forward = workspace.current_camera.forward();
        let mut parts = workspace.get_all::<Part>();
        let mut group = Vec::with_capacity(CULL_GROUP_SIZE);
        loop {
            group.clear();
            let mut bounds_min = Vec3::splat(f32::INFINITY);
            let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
            for part in parts.by_ref().take(CULL_GROUP_SIZE) {
                let pivot = part.pivot();
                let max_scale = part.size().max_element().max(0.0);
                let radius = part.shape().bounding_radius() * max_scale * 1.01;
                let center = pivot.w_axis.truncate();
                let extent = Vec3::splat(radius);
                bounds_min = bounds_min.min(center - extent);
                bounds_max = bounds_max.max(center + extent);
                group.push(PartCandidate {
                    part,
                    pivot,
                    center,
                    radius,
                    visibility_mask: 0,
                });
            }
            if group.is_empty() {
                break;
            }

            let mut classify =
                |planes: &[Vec4; 6], visibility_bit: u16, excluded: Option<InstanceId>| {
                    match aabb_frustum_relation(planes, bounds_min, bounds_max) {
                        FrustumRelation::Inside => {
                            for candidate in &mut group {
                                if Some(candidate.part.id()) != excluded {
                                    candidate.visibility_mask |= visibility_bit;
                                }
                            }
                        }
                        FrustumRelation::Intersecting => {
                            for candidate in &mut group {
                                if Some(candidate.part.id()) != excluded
                                    && sphere_visible(planes, candidate.center, candidate.radius)
                                {
                                    candidate.visibility_mask |= visibility_bit;
                                }
                            }
                        }
                        FrustumRelation::Outside => {}
                    }
                };
            classify(&camera_planes, 1, None);
            for (cascade, planes) in light_planes.iter().enumerate() {
                classify(planes, 1 << (cascade + 1), None);
            }
            for (layer, planes) in local_shadows
                .planes
                .iter()
                .take(self.local_shadow_layer_count)
                .enumerate()
            {
                let visibility_bit = 1 << (LOCAL_SHADOW_VISIBILITY_OFFSET + layer);
                classify(
                    planes,
                    visibility_bit,
                    local_shadows.excluded_casters[layer],
                );
            }

            for candidate in &group {
                let part = candidate.part;
                let pivot = candidate.pivot;
                let transparency = finite_or(part.transparency(), 0.0).clamp(0.0, 1.0);
                let opacity = 1.0 - transparency;
                if opacity <= 0.0 {
                    continue;
                }
                let transparent = transparency > 0.0;
                let mut visibility_mask = candidate.visibility_mask;
                if transparent {
                    // Transparent geometry is blended after opaque geometry and
                    // must not write to or appear in shadow depth buffers.
                    visibility_mask &= 1;
                }
                if visibility_mask == 0 {
                    continue;
                }
                let has_custom_textures = visibility_mask & 1 != 0
                    && part.material_slots.slots.iter().flatten().any(|material| {
                        material.textures().base_color.is_some()
                            || material.textures().normal.is_some()
                            || material.textures().metallic_roughness.is_some()
                            || material.textures().emissive.is_some()
                    });
                let custom_textures = if has_custom_textures {
                    let textures = self.material_textures(workspace, &part.material_slots)?;
                    (textures != default_textures).then_some(textures)
                } else {
                    None
                };
                // Filters only matter when textures are bound: 1x1 default
                // textures sample identically under any filter, so untextured
                // parts keep sharing the fast-path default batch.
                let custom_filters = if custom_textures.is_some() {
                    let filters = Self::material_filters(&part.material_slots);
                    (filters != default_filters).then_some(filters)
                } else {
                    None
                };
                let sort_depth = (candidate.center - camera_position).dot(camera_forward);
                let batch_index = if transparent {
                    let batch_index = batches.len();
                    batches.push(RenderBatch {
                        mesh: self.primitive_meshes[part.shape().index()],
                        textures: custom_textures.unwrap_or(default_textures),
                        filters: custom_filters.unwrap_or(default_filters),
                        visibility_mask,
                        transparent: true,
                        sort_depth,
                        instances: Vec::new(),
                        instance_start: 0,
                    });
                    batch_index
                } else if let Some(textures) = custom_textures {
                    let filters = custom_filters.unwrap_or(default_filters);
                    let mesh = self.primitive_meshes[part.shape().index()];
                    let key = (mesh, textures, filters, visibility_mask);
                    if let Some(&batch_index) = batch_indices.get(&key) {
                        batch_index
                    } else {
                        let batch_index = batches.len();
                        batch_indices.insert(key, batch_index);
                        batches.push(RenderBatch {
                            mesh,
                            textures,
                            filters,
                            visibility_mask,
                            transparent: false,
                            sort_depth: 0.0,
                            instances: Vec::new(),
                            instance_start: 0,
                        });
                        batch_index
                    }
                } else {
                    let mesh = self.primitive_meshes[part.shape().index()];
                    if usize::from(visibility_mask) < VISIBILITY_MASK_COUNT {
                        let slot =
                            &mut default_batches[part.shape().index()][visibility_mask as usize];
                        if let Some(batch_index) = *slot {
                            batch_index
                        } else {
                            let batch_index = batches.len();
                            *slot = Some(batch_index);
                            batches.push(RenderBatch {
                                mesh,
                                textures: default_textures,
                                filters: default_filters,
                                visibility_mask,
                                transparent: false,
                                sort_depth: 0.0,
                                instances: Vec::new(),
                                instance_start: 0,
                            });
                            batch_index
                        }
                    } else {
                        let key = (mesh, default_textures, default_filters, visibility_mask);
                        if let Some(&batch_index) = batch_indices.get(&key) {
                            batch_index
                        } else {
                            let batch_index = batches.len();
                            batch_indices.insert(key, batch_index);
                            batches.push(RenderBatch {
                                mesh,
                                textures: default_textures,
                                filters: default_filters,
                                visibility_mask,
                                transparent: false,
                                sort_depth: 0.0,
                                instances: Vec::new(),
                                instance_start: 0,
                            });
                            batch_index
                        }
                    }
                };
                let model = Mat4::from_cols(
                    pivot.x_axis * part.size().x,
                    pivot.y_axis * part.size().y,
                    pivot.z_axis * part.size().z,
                    pivot.w_axis,
                );
                let (normal_scales, tint, material_set) = if visibility_mask & 1 != 0 {
                    let mut tint = part.color().rgba();
                    tint[3] = opacity;
                    (
                        normal_scales_from_model(&model),
                        tint,
                        self.material_set_index(&part.material_slots),
                    )
                } else {
                    ([0.0; 3], [0.0; 4], 0)
                };
                let instance = InstanceRaw {
                    model: [
                        model.x_axis.truncate().to_array(),
                        model.y_axis.truncate().to_array(),
                        model.z_axis.truncate().to_array(),
                        model.w_axis.truncate().to_array(),
                    ],
                    normal_scales,
                    tint,
                    material_set,
                };
                batches[batch_index].instances.push(instance);
                if visibility_mask & 1 != 0 && opacity > 0.0 {
                    append_outline_instance(
                        &mut outline_batches,
                        &outline_targets,
                        part.id(),
                        batches[batch_index].mesh,
                        instance,
                    );
                }
            }
        }

        #[cfg(feature = "meshpart")]
        for meshpart in workspace.get_all::<MeshPart>() {
            let pivot = meshpart.pivot();
            let center = pivot.w_axis.truncate();
            let transparency = finite_or(meshpart.transparency(), 0.0).clamp(0.0, 1.0);
            let opacity = 1.0 - transparency;
            if opacity <= 0.0 {
                continue;
            }
            let transparent = transparency > 0.0;
            let radius = meshpart.bounding_radius() * meshpart.size().abs().max_element() * 1.01;
            let mut visibility_mask = 0;
            if sphere_visible(&camera_planes, center, radius) {
                visibility_mask |= 1;
            }
            for (cascade, planes) in light_planes.iter().enumerate() {
                if sphere_visible(planes, center, radius) {
                    visibility_mask |= 1 << (cascade + 1);
                }
            }
            for (layer, planes) in local_shadows
                .planes
                .iter()
                .take(self.local_shadow_layer_count)
                .enumerate()
            {
                if local_shadows.excluded_casters[layer] != Some(meshpart.id())
                    && sphere_visible(planes, center, radius)
                {
                    visibility_mask |= 1 << (LOCAL_SHADOW_VISIBILITY_OFFSET + layer);
                }
            }
            if visibility_mask == 0 {
                continue;
            }
            if transparent {
                // Transparent geometry is blended after opaque geometry and
                // must not write to or appear in shadow depth buffers.
                visibility_mask &= 1;
            }
            if visibility_mask == 0 {
                continue;
            }

            let Some(mesh) = self.meshpart_mesh(meshpart)? else {
                continue;
            };
            let has_custom_textures = visibility_mask & 1 != 0
                && meshpart
                    .material_slots
                    .slots
                    .iter()
                    .flatten()
                    .any(|material| {
                        material.textures().base_color.is_some()
                            || material.textures().normal.is_some()
                            || material.textures().metallic_roughness.is_some()
                            || material.textures().emissive.is_some()
                    });
            let textures = if has_custom_textures {
                self.material_textures(workspace, &meshpart.material_slots)?
            } else {
                default_textures
            };
            let filters = if textures == default_textures {
                default_filters
            } else {
                Self::material_filters(&meshpart.material_slots)
            };
            let sort_depth = (center - camera_position).dot(camera_forward);
            let key = (mesh, textures, filters, visibility_mask);
            let batch_index = if transparent {
                let batch_index = batches.len();
                batches.push(RenderBatch {
                    mesh,
                    textures,
                    filters,
                    visibility_mask,
                    transparent: true,
                    sort_depth,
                    instances: Vec::new(),
                    instance_start: 0,
                });
                batch_index
            } else if let Some(&batch_index) = batch_indices.get(&key) {
                batch_index
            } else {
                let batch_index = batches.len();
                batch_indices.insert(key, batch_index);
                batches.push(RenderBatch {
                    mesh,
                    textures,
                    filters,
                    visibility_mask,
                    transparent: false,
                    sort_depth: 0.0,
                    instances: Vec::new(),
                    instance_start: 0,
                });
                batch_index
            };
            let model = Mat4::from_cols(
                pivot.x_axis * meshpart.size().x,
                pivot.y_axis * meshpart.size().y,
                pivot.z_axis * meshpart.size().z,
                pivot.w_axis,
            );
            let (normal_scales, tint, material_set) = if visibility_mask & 1 != 0 {
                let mut tint = meshpart.color().rgba();
                tint[3] = opacity;
                (
                    normal_scales_from_model(&model),
                    tint,
                    self.material_set_index(&meshpart.material_slots),
                )
            } else {
                ([0.0; 3], [0.0; 4], 0)
            };
            let instance = InstanceRaw {
                model: [
                    model.x_axis.truncate().to_array(),
                    model.y_axis.truncate().to_array(),
                    model.z_axis.truncate().to_array(),
                    model.w_axis.truncate().to_array(),
                ],
                normal_scales,
                tint,
                material_set,
            };
            batches[batch_index].instances.push(instance);
            if visibility_mask & 1 != 0 {
                append_outline_instance(
                    &mut outline_batches,
                    &outline_targets,
                    meshpart.id(),
                    mesh,
                    instance,
                );
            }
        }

        let total_instances: usize = batches.iter().map(|batch| batch.instances.len()).sum();
        self.upload_material_factors();
        batches.sort_unstable_by(|left, right| {
            left.transparent.cmp(&right.transparent).then_with(|| {
                if left.transparent {
                    right.sort_depth.total_cmp(&left.sort_depth)
                } else {
                    (left.mesh.0, left.visibility_mask).cmp(&(right.mesh.0, right.visibility_mask))
                }
            })
        });
        let mut instance_start = 0;
        for batch in &mut batches {
            batch.instance_start = instance_start;
            instance_start += batch.instances.len();
        }
        self.ensure_instance_capacity(total_instances);
        let upload_size = std::mem::size_of::<InstanceRaw>() as u64 * total_instances as u64;
        if let Some(upload_size) = wgpu::BufferSize::new(upload_size)
            && let Some(mut upload) =
                self.queue
                    .write_buffer_with(&self.instance_buffer, 0, upload_size)
        {
            let mut byte_offset = 0;
            for batch in &batches {
                let bytes = bytemuck::cast_slice(&batch.instances);
                upload
                    .slice(byte_offset..byte_offset + bytes.len())
                    .copy_from_slice(bytes);
                byte_offset += bytes.len();
            }
        } else {
            for batch in &batches {
                self.queue.write_buffer(
                    &self.instance_buffer,
                    batch.instance_start as u64 * std::mem::size_of::<InstanceRaw>() as u64,
                    bytemuck::cast_slice(&batch.instances),
                );
            }
        }

        self.prepared_batches.clear();
        self.prepared_batches.reserve(batches.len());
        let default_packed = self.pack_material_textures(default_textures)?;
        self.material_bind_group(default_packed, default_filters);
        for batch in &batches {
            if batch.instances.is_empty() {
                continue;
            }
            let packed = if batch.textures == default_textures {
                default_packed
            } else {
                let packed = self.pack_material_textures(batch.textures)?;
                // Populate the bind-group cache once per material; steady-state
                // frames create zero bind groups.
                self.material_bind_group(packed, batch.filters);
                packed
            };
            self.prepared_batches.push(PreparedRenderBatch {
                mesh: batch.mesh,
                packed_textures: packed,
                filters: batch.filters,
                visibility_mask: batch.visibility_mask,
                transparent: batch.transparent,
                instance_start: batch.instance_start,
                instance_count: batch.instances.len() as u32,
            });
        }
        for (mode, mode_batches) in outline_batches.iter_mut().enumerate() {
            mode_batches.sort_unstable_by_key(|batch| batch.mesh.0);
            let mut instance_start = 0;
            for batch in mode_batches.iter_mut() {
                batch.instance_start = instance_start;
                instance_start += batch.instances.len();
            }
            self.ensure_outline_instance_capacity(mode, instance_start);
            for batch in mode_batches {
                self.queue.write_buffer(
                    &self.outline_instance_buffers[mode],
                    batch.instance_start as u64 * std::mem::size_of::<InstanceRaw>() as u64,
                    bytemuck::cast_slice(&batch.instances),
                );
            }
        }
        self.outline_batches = outline_batches;
        self.batch_scratch = batches;
        self.batch_indices_scratch = batch_indices;
        // Keep scratch capacities warm for the next frame's batch count.
        self.batch_scratch.reserve(PartShape::COUNT);
        Ok(())
    }
}
