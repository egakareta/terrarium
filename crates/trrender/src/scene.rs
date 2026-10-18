use super::*;

fn collect_outline_targets(
    scene: &RenderScene<'_>,
) -> ([HashMap<u64, OutlineSpec>; 3], [Option<OutlineSpec>; 3]) {
    let mut targets = std::array::from_fn(|_| HashMap::default());
    let mut specs = [None; 3];
    for outline in &scene.outlines {
        let mode = outline.mode.index();
        let spec = OutlineSpec {
            color: outline.color,
            width: outline.width,
            threshold: outline.threshold,
            always_on_top: outline.always_on_top,
        };
        specs[mode].get_or_insert(spec);
        for &target in &outline.targets {
            targets[mode].insert(target, spec);
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
    targets: &[HashMap<u64, OutlineSpec>; 3],
    id: u64,
    mesh: GpuMeshHandle,
    instance: InstanceRaw,
) {
    for mode in 0..3 {
        let Some(spec) = targets[mode].get(&id) else {
            continue;
        };
        let mut instance = instance;
        instance.outline_always_on_top = u32::from(spec.always_on_top);
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
    pub(super) fn prepare_scene(&mut self, scene: &RenderScene<'_>) -> Result<(), RendererError> {
        self.prepared_batches.clear();
        let structural_rev = scene.structure_revision;
        let (outline_targets, specs) = collect_outline_targets(scene);
        self.queue.write_buffer(
            &self.outline_uniform_buffer,
            0,
            bytemuck::bytes_of(&OutlineUniform {
                toon_params: outline_params(specs[0]),
                toon_color: specs[0].map_or([0.0; 4], |spec| outline_color(spec.color)),
                silhouette_params: outline_params(specs[1]),
                silhouette_color: specs[1].map_or([0.0; 4], |spec| outline_color(spec.color)),
                stencil_params: outline_params(specs[2]),
                stencil_color: specs[2].map_or([0.0; 4], |spec| outline_color(spec.color)),
            }),
        );

        let mut outline_batches = std::mem::take(&mut self.outline_batches);
        for batches in &mut outline_batches {
            batches.clear();
        }
        let camera = &scene.camera;
        let camera_planes = frustum_planes(camera.view_projection);
        let local_shadows = if !self.light_had_any && self.light_revision == structural_rev {
            PreparedLocalShadows {
                planes: [[Vec4::ZERO; 6]; MAX_LOCAL_SHADOW_LAYERS],
                excluded_casters: [None; MAX_LOCAL_SHADOW_LAYERS],
            }
        } else {
            let shadows = self.prepare_local_lights(scene, &camera_planes);
            self.light_had_any =
                self.local_shadow_layer_count != 0 || !self.local_light_scratch.is_empty();
            self.light_revision = structural_rev;
            shadows
        };
        let lighting = scene.lighting;
        let light_direction = lighting.sun_direction();
        let (light_vps, shadow_cascade_splits, shadow_texel_sizes) =
            scene_light_view_projections(scene, light_direction, self.shadow_map_size);
        let daylight = lighting.daylight_factor();
        let direct_scale = lighting.render_direct_scale();
        let exposure = lighting.render_exposure();
        let fog_start = finite_nonnegative(lighting.fog_start);
        let fog_end = finite_nonnegative(lighting.fog_end).max(fog_start + 0.001);
        self.shadow_pass_enabled = lighting.shadows_enabled();
        let (environment_mip_count, has_environment) = match lighting.skybox() {
            Some(skybox) => (skybox.face_size().ilog2() as f32 + 1.0, 1.0),
            None => (1.0, 0.0),
        };
        let camera_uniform = CameraUniform {
            view_projection: camera.view_projection.to_cols_array_2d(),
            light_view_projections: light_vps.map(|matrix| matrix.to_cols_array_2d()),
            camera_position: camera.pivot.w_axis.truncate().extend(1.0).to_array(),
            camera_forward: camera.forward.extend(0.0).to_array(),
            light_direction: light_direction.extend(0.0).to_array(),
            light_color: [direct_scale, 0.93 * direct_scale, 0.83 * direct_scale, 0.0],
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
            self.queue.write_buffer(
                &self.shadow_camera_buffer,
                cascade as u64 * u64::from(self.shadow_camera_stride),
                bytemuck::bytes_of(&ShadowCameraUniform {
                    light_view_projection: light_vp.to_cols_array_2d(),
                }),
            );
        }
        let mut sky_view = camera.pose.inverse();
        sky_view.w_axis = Vec4::new(0.0, 0.0, 0.0, 1.0);
        let sky_view_projection = camera.projection * sky_view;
        self.queue.write_buffer(
            &self.skybox_uniform_buffer,
            0,
            bytemuck::bytes_of(&SkyboxCameraUniform {
                view_projection: sky_view_projection.to_cols_array_2d(),
                exposure: [exposure * (0.08 + daylight * 0.92), 0.0, 0.0, 0.0],
            }),
        );
        let skybox_key = (scene.id, lighting.skybox_revision());
        if self.skybox_revision != Some(skybox_key) {
            self.sync_skybox(lighting.skybox())?;
            self.skybox_revision = Some(skybox_key);
        }

        let light_planes = light_vps.map(frustum_planes);
        let default_textures = self.default_material_textures;
        let default_filters = self.default_material_filters;
        let camera_position = camera.position();
        let camera_forward = camera.forward;
        let mut live_custom_meshes = HashSet::default();
        for (node, _) in scene.nodes() {
            if node
                .mesh
                .as_ref()
                .is_some_and(|mesh| matches!(mesh.geometry, RenderGeometry::Custom { .. }))
            {
                live_custom_meshes.insert(node.id);
            }
        }
        let mut batches = std::mem::take(&mut self.batch_scratch);
        batches.retain(|batch| {
            batch.textures == default_textures && self.primitive_meshes.contains(&batch.mesh)
        });
        let mut batch_indices = std::mem::take(&mut self.batch_indices_scratch);
        batch_indices.clear();
        for (index, batch) in batches.iter_mut().enumerate() {
            batch.instances.clear();
            if batch.transparent {
                batch.sort_depth = f32::NEG_INFINITY;
            }
            batch_indices.insert(
                (
                    batch.mesh,
                    batch.textures,
                    batch.filters,
                    batch.visibility_mask,
                    batch.transparent,
                ),
                index,
            );
        }

        for (node, pivot) in scene.nodes() {
            let Some(object) = node.mesh.as_ref() else {
                continue;
            };
            let transparency = finite_or(object.transparency, 0.0).clamp(0.0, 1.0);
            if transparency >= 1.0 {
                continue;
            }
            let transparent = transparency > 0.0;
            let center = pivot.w_axis.truncate();
            let radius = scaled_bounding_radius(object.bounding_radius, pivot);
            let mut visibility_mask = if sphere_visible(&camera_planes, center, radius) {
                1
            } else {
                0
            };
            if !transparent {
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
                    if local_shadows.excluded_casters[layer] != Some(node.id)
                        && sphere_visible(planes, center, radius)
                    {
                        visibility_mask |= 1 << (LOCAL_SHADOW_VISIBILITY_OFFSET + layer);
                    }
                }
            }
            if transparent {
                visibility_mask &= 1;
            }
            if visibility_mask == 0 {
                continue;
            }

            let mesh = match object.geometry {
                RenderGeometry::Primitive(shape) => self.primitive_meshes[shape.index()],
                RenderGeometry::Custom { mesh, revision } => {
                    let Some(handle) = self.custom_mesh(node.id, mesh, revision)? else {
                        continue;
                    };
                    handle
                }
            };
            let has_custom_textures = visibility_mask & 1 != 0
                && object
                    .materials
                    .slots
                    .iter()
                    .flatten()
                    .any(Material::has_texture_maps);
            let textures = if has_custom_textures {
                self.material_textures(scene, object.materials)?
            } else {
                default_textures
            };
            let filters = if textures == default_textures {
                default_filters
            } else {
                Self::material_filters(object.materials)
            };
            let sort_depth = (center - camera_position).dot(camera_forward);
            let key = (mesh, textures, filters, visibility_mask, transparent);
            let batch_index = if let Some(&batch_index) = batch_indices.get(&key) {
                if transparent && sort_depth > batches[batch_index].sort_depth {
                    batches[batch_index].sort_depth = sort_depth;
                }
                batch_index
            } else {
                let batch_index = batches.len();
                batch_indices.insert(key, batch_index);
                batches.push(RenderBatch {
                    mesh,
                    textures,
                    filters,
                    visibility_mask,
                    transparent,
                    sort_depth: if transparent { sort_depth } else { 0.0 },
                    instances: Vec::new(),
                    instance_start: 0,
                });
                batch_index
            };
            let opacity = 1.0 - transparency;
            let (normal_scales, tint, material_set) = if visibility_mask & 1 != 0 {
                let mut tint = object.color.rgba();
                tint[3] = opacity;
                (
                    normal_scales_from_model(&pivot),
                    tint,
                    self.material_set_index(object.materials),
                )
            } else {
                ([0.0; 3], [0.0; 4], 0)
            };
            let instance = InstanceRaw {
                model: [
                    pivot.x_axis.truncate().to_array(),
                    pivot.y_axis.truncate().to_array(),
                    pivot.z_axis.truncate().to_array(),
                    pivot.w_axis.truncate().to_array(),
                ],
                normal_scales,
                tint,
                material_set,
                outline_always_on_top: 0,
            };
            batches[batch_index].instances.push(instance);
            if visibility_mask & 1 != 0 {
                append_outline_instance(
                    &mut outline_batches,
                    &outline_targets,
                    node.id,
                    mesh,
                    instance,
                );
            }
        }

        let stale_custom_meshes: Vec<_> = self
            .custom_meshes
            .keys()
            .copied()
            .filter(|id| !live_custom_meshes.contains(id))
            .collect();
        for id in stale_custom_meshes {
            if let Some(cached) = self.custom_meshes.remove(&id) {
                self.free_custom_meshes.push(cached.handle);
            }
        }

        let (mut sort_depths, mut sort_indices, mut sort_tmp) = (
            std::mem::take(&mut self.transparent_sort_depths),
            std::mem::take(&mut self.transparent_sort_indices),
            std::mem::take(&mut self.transparent_sort_tmp),
        );
        for batch in batches.iter_mut().filter(|batch| batch.transparent) {
            let len = batch.instances.len();
            if len > 1 {
                sort_depths.clear();
                sort_depths.reserve(len);
                for instance in &batch.instances {
                    let position = Vec3::new(
                        instance.model[3][0],
                        instance.model[3][1],
                        instance.model[3][2],
                    );
                    sort_depths.push((position - camera_position).dot(camera_forward));
                }
                sort_indices.clear();
                sort_indices.extend(0..len);
                sort_indices.sort_unstable_by(|&a, &b| sort_depths[b].total_cmp(&sort_depths[a]));
                sort_tmp.clear();
                sort_tmp.reserve(len);
                for &index in &sort_indices {
                    sort_tmp.push(batch.instances[index]);
                }
                std::mem::swap(&mut batch.instances, &mut sort_tmp);
                batch.sort_depth = sort_depths[sort_indices[0]];
            }
        }
        self.transparent_sort_depths = sort_depths;
        self.transparent_sort_indices = sort_indices;
        self.transparent_sort_tmp = sort_tmp;

        batches.sort_unstable_by(|left, right| {
            left.transparent.cmp(&right.transparent).then_with(|| {
                if left.transparent {
                    right.sort_depth.total_cmp(&left.sort_depth)
                } else {
                    (left.mesh.0, left.visibility_mask).cmp(&(right.mesh.0, right.visibility_mask))
                }
            })
        });
        let total_instances = batches.iter().map(|batch| batch.instances.len()).sum();
        self.upload_material_factors();
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
        self.batch_scratch.reserve(PartShape::COUNT);
        Ok(())
    }
}
