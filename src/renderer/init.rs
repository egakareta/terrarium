use super::*;

impl Renderer {
    /// Creates a renderer that draws into eframe's WGPU render pass.
    pub fn new(
        render_state: &crate::egui_wgpu::RenderState,
        size: [u32; 2],
    ) -> Result<Self, RendererError> {
        let format = render_state.target_format;
        #[cfg(not(target_arch = "wasm32"))]
        let _ = WGPU_INSTANCE_KEEPALIVE.set(render_state.instance.clone());
        let width = size[0].max(1);
        let height = size[1].max(1);
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let (depth_texture, depth_view) = create_depth_texture(&device, width, height);
        let scene_texture = create_eframe_scene_texture(&device, format, width, height);
        let scene_view = scene_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (outline_mask_texture, outline_mask_view) =
            create_outline_mask_texture(&device, width, height);
        let (shadow_texture, shadow_view, shadow_layer_views) = create_shadow_texture(&device);
        let (local_shadow_texture, local_shadow_view, local_shadow_layer_views) =
            create_local_shadow_texture(&device, 1);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: Some(wgpu::CompareFunction::LessEqual),
            anisotropy_clamp: 1,
            border_color: None,
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let local_lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("local lights uniform buffer"),
            size: std::mem::size_of::<LocalLightsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shadow_camera_size = std::mem::size_of::<ShadowCameraUniform>() as u64;
        let shadow_camera_stride = align_to(
            shadow_camera_size,
            u64::from(device.limits().min_uniform_buffer_offset_alignment),
        ) as u32;
        let shadow_camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow camera uniform buffer"),
            size: u64::from(shadow_camera_stride)
                * (SHADOW_CASCADE_COUNT + MAX_LOCAL_SHADOW_LAYERS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part instance buffer"),
            size: std::mem::size_of::<InstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let outline_instance_buffers = std::array::from_fn(|mode| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(match mode {
                    0 => "toon outline instance buffer",
                    1 => "silhouette outline instance buffer",
                    _ => "stencil outline instance buffer",
                }),
                size: std::mem::size_of::<InstanceRaw>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let outline_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline uniform buffer"),
            size: std::mem::size_of::<OutlineUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let skybox_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("skybox sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let outline_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("outline composite bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let outline_geometry_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("outline geometry bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        // Black 1x1 cubemap bound until the first skybox upload (and whenever
        // the workspace has no skybox). The shader multiplies IBL by zero in
        // that case, so its contents never contribute.
        let fallback_environment_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fallback environment cubemap"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for layer in 0..6u32 {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &fallback_environment_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &[0, 0, 0, 255],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
        let fallback_environment_view =
            fallback_environment_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("fallback environment cubemap view"),
                dimension: Some(wgpu::TextureViewDimension::Cube),
                array_layer_count: Some(6),
                ..Default::default()
            });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&fallback_environment_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&skybox_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: local_lights_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&local_shadow_view),
                },
            ],
        });
        let shadow_camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(shadow_camera_size),
                    },
                    count: None,
                }],
            });
        let shadow_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow camera bind group"),
            layout: &shadow_camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &shadow_camera_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(shadow_camera_size),
                }),
            }],
        });
        let mut material_bind_group_entries = Vec::with_capacity(MATERIAL_SLOT_COUNT * 2);
        for binding in 0..MATERIAL_SLOT_COUNT {
            material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            });
        }
        // One filtering sampler per material slot so each directional slot can
        // use its own `TextureFilter` (e.g. pixel-art `Nearest` on top, smooth
        // `Trilinear` elsewhere).
        for binding in 0..MATERIAL_SLOT_COUNT {
            material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
                binding: (MATERIAL_SLOT_COUNT + binding) as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material bind group layout"),
                entries: &material_bind_group_entries,
            });
        let mut material_samplers = HashMap::new();
        for filter in TextureFilter::ALL {
            material_samplers.insert(filter, create_material_sampler(&device, filter));
        }
        let material_factor_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material factor bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
        let (material_factors_texture, material_factors_view) =
            create_material_factor_texture(&device, 1);
        let material_factors_bind_group = create_material_factor_bind_group(
            &device,
            &material_factor_bind_group_layout,
            &material_factors_view,
        );
        let default_base_color = Texture::new(1, 1, vec![255, 255, 255, 255])?;
        let default_normal = Texture::linear(1, 1, vec![128, 128, 255, 255])?;
        let default_metallic_roughness = Texture::linear(1, 1, vec![0, 255, 0, 255])?;
        let default_emissive = Texture::new(1, 1, vec![255, 255, 255, 255])?;
        let textures = vec![
            GpuTexture {
                source: default_base_color.clone(),
            },
            GpuTexture {
                source: default_normal.clone(),
            },
            GpuTexture {
                source: default_metallic_roughness.clone(),
            },
            GpuTexture {
                source: default_emissive.clone(),
            },
        ];
        let mut texture_dedup = HashMap::new();
        texture_dedup.insert(default_base_color, GpuTextureHandle(0));
        texture_dedup.insert(default_normal, GpuTextureHandle(1));
        texture_dedup.insert(default_metallic_roughness, GpuTextureHandle(2));
        texture_dedup.insert(default_emissive, GpuTextureHandle(3));
        let default_material_textures = MaterialTextures {
            base_color: [GpuTextureHandle(0); MATERIAL_SLOT_COUNT],
            normal: [GpuTextureHandle(1); MATERIAL_SLOT_COUNT],
            metallic_roughness: [GpuTextureHandle(2); MATERIAL_SLOT_COUNT],
            emissive: [GpuTextureHandle(3); MATERIAL_SLOT_COUNT],
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PBR mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let shader_constants = [
            (
                "FRAMEBUFFER_IS_SRGB",
                if format.is_srgb() { 1.0 } else { 0.0 },
            ),
            ("SHADOW_MAP_SIZE", SHADOW_MAP_SIZE as f64),
        ];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[
                Some(&camera_bind_group_layout),
                Some(&material_bind_group_layout),
                Some(&material_factor_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let create_mesh_pipeline = |label, depth_write_enabled, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &shader_constants,
                        ..Default::default()
                    },
                    buffers: &[Some(Vertex::layout()), Some(InstanceRaw::layout())],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    // All primitive meshes are closed solids, so backfaces never
                    // contribute a visible pixel: they are always behind a front
                    // face and depth-rejected after shading. Culling them skips
                    // roughly half the fragment work with identical output.
                    cull_mode: Some(wgpu::Face::Back),
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write_enabled),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &shader_constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = create_mesh_pipeline("PBR mesh pipeline", true, wgpu::BlendState::REPLACE);
        let transparent_pipeline = create_mesh_pipeline(
            "transparent PBR mesh pipeline",
            false,
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow pipeline layout"),
                bind_group_layouts: &[Some(&shadow_camera_bind_group_layout)],
                immediate_size: 0,
            });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("directional shadow pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &shader_constants,
                    ..Default::default()
                },
                buffers: &[Some(Vertex::layout()), Some(InstanceRaw::shadow_layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Same reasoning as the main pass: shadow depth keeps the
                // nearest front face either way.
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 1,
                    slope_scale: 1.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline composite shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let outline_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline geometry shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("outline.wgsl").into()),
        });
        let outline_composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("outline composite pipeline layout"),
                bind_group_layouts: &[Some(&outline_bind_group_layout)],
                immediate_size: 0,
            });
        let outline_composite_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("outline composite pipeline"),
                layout: Some(&outline_composite_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &blit_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &blit_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let outline_mask_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("outline mask pipeline layout"),
                bind_group_layouts: &[Some(&camera_bind_group_layout)],
                immediate_size: 0,
            });
        let mask_depth_stencil = || wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let mask_vertex_buffers = [Some(Vertex::layout()), Some(InstanceRaw::layout())];
        let toon_mask_targets = [Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::RED,
        })];
        let silhouette_mask_targets = [Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: Some(wgpu::BlendState::REPLACE),
            write_mask: wgpu::ColorWrites::GREEN,
        })];
        let toon_mask_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("toon outline mask pipeline"),
            layout: Some(&outline_mask_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &outline_shader,
                entry_point: Some("vs_mask"),
                compilation_options: Default::default(),
                buffers: &mask_vertex_buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(mask_depth_stencil()),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &outline_shader,
                entry_point: Some("fs_mask"),
                compilation_options: Default::default(),
                targets: &toon_mask_targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        let silhouette_mask_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("silhouette outline mask pipeline"),
                layout: Some(&outline_mask_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &outline_shader,
                    entry_point: Some("vs_mask"),
                    compilation_options: Default::default(),
                    buffers: &mask_vertex_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(mask_depth_stencil()),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &outline_shader,
                    entry_point: Some("fs_mask"),
                    compilation_options: Default::default(),
                    targets: &silhouette_mask_targets,
                }),
                multiview_mask: None,
                cache: None,
            });
        let stencil_mask_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("stencil outline mask pipeline"),
                layout: Some(&outline_mask_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &outline_shader,
                    entry_point: Some("vs_mask"),
                    compilation_options: Default::default(),
                    buffers: &mask_vertex_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::Always,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: wgpu::StencilOperation::Replace,
                        },
                        back: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::Always,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: wgpu::StencilOperation::Replace,
                        },
                        read_mask: 0xff,
                        write_mask: 0xff,
                    },
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &outline_shader,
                    entry_point: Some("fs_mask"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::empty(),
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let stencil_outline_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("stencil outline pipeline layout"),
                bind_group_layouts: &[
                    Some(&camera_bind_group_layout),
                    Some(&outline_geometry_bind_group_layout),
                ],
                immediate_size: 0,
            });
        let stencil_outline_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("stencil outline pipeline"),
                layout: Some(&stencil_outline_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &outline_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(Vertex::layout()), Some(InstanceRaw::layout())],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Front),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::NotEqual,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: wgpu::StencilOperation::Keep,
                        },
                        back: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::NotEqual,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: wgpu::StencilOperation::Keep,
                        },
                        read_mask: 0xff,
                        write_mask: 0,
                    },
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &outline_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let skybox_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skybox shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("skybox.wgsl").into()),
        });
        let skybox_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("skybox bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let skybox_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skybox camera uniform buffer"),
            size: std::mem::size_of::<SkyboxCameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let skybox_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skybox cube vertex buffer"),
            contents: bytemuck::cast_slice(&SKYBOX_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let skybox_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skybox cube index buffer"),
            contents: bytemuck::cast_slice(&SKYBOX_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let skybox_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("skybox pipeline layout"),
                bind_group_layouts: &[Some(&skybox_bind_group_layout)],
                immediate_size: 0,
            });
        // The skybox shader only declares FRAMEBUFFER_IS_SRGB: passing the
        // mesh shader's SHADOW_MAP_SIZE override would fail pipeline creation.
        let skybox_shader_constants = [("FRAMEBUFFER_IS_SRGB", shader_constants[0].1)];
        let skybox_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skybox pipeline"),
            layout: Some(&skybox_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &skybox_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &skybox_shader_constants,
                    ..Default::default()
                },
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // The camera sits inside the cube, so either winding can face
                // the camera depending on the triangle.
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &skybox_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &skybox_shader_constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let eframe_scene = EframeSceneTarget::new(&device, format, width, height);
        let outline_bind_group = create_outline_bind_group(
            &device,
            &outline_bind_group_layout,
            &scene_view,
            &eframe_scene.sampler,
            &outline_mask_view,
            &outline_uniform_buffer,
        );
        let outline_geometry_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("outline geometry bind group"),
            layout: &outline_geometry_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: outline_uniform_buffer.as_entire_binding(),
            }],
        });

        let mut renderer = Self {
            device,
            queue,
            width,
            height,
            depth_texture,
            depth_view,
            scene_texture,
            scene_view,
            outline_mask_texture,
            outline_mask_view,
            _shadow_texture: shadow_texture,
            shadow_layer_views,
            _local_shadow_texture: local_shadow_texture,
            local_shadow_view,
            local_shadow_layer_views,
            _shadow_sampler: shadow_sampler,
            pipeline,
            transparent_pipeline,
            shadow_pipeline,
            outline_composite_pipeline,
            toon_mask_pipeline,
            silhouette_mask_pipeline,
            stencil_mask_pipeline,
            stencil_outline_pipeline,
            eframe_scene,
            skybox_pipeline,
            skybox_bind_group_layout,
            skybox_uniform_buffer,
            skybox_sampler,
            skybox_vertex_buffer,
            skybox_index_buffer,
            skybox_texture: None,
            skybox_view: None,
            skybox_bind_group: None,
            skybox_revision: None,
            camera_buffer,
            local_lights_buffer,
            outline_uniform_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            outline_bind_group_layout,
            outline_bind_group,
            outline_geometry_bind_group,
            shadow_view,
            _fallback_environment_texture: fallback_environment_texture,
            fallback_environment_view,
            shadow_camera_buffer,
            shadow_camera_bind_group,
            shadow_camera_stride,
            material_bind_group_layout,
            material_samplers,
            material_factor_bind_group_layout,
            material_factors_texture,
            material_factors_view,
            material_factors_bind_group,
            material_factor_vec4s: Vec::new(),
            material_factor_indices: HashMap::new(),
            material_factor_last: None,
            material_factor_last_uniform: None,
            textures,
            workspace_texture_handles: HashMap::new(),
            texture_dedup,
            default_material_textures,
            default_material_filters: [TextureFilter::default(); MATERIAL_SLOT_COUNT],
            packed_material_textures: HashMap::new(),
            gpu_material_textures: Vec::new(),
            instance_buffer,
            outline_instance_buffers,
            meshes: Vec::new(),
            primitive_meshes: [GpuMeshHandle(usize::MAX); PartShape::COUNT],
            #[cfg(feature = "meshpart")]
            meshpart_meshes: HashMap::new(),
            #[cfg(feature = "meshpart")]
            free_meshpart_meshes: Vec::new(),
            clear_color: wgpu::Color {
                r: 0.018,
                g: 0.028,
                b: 0.065,
                a: 1.0,
            },
            shadow_pass_enabled: true,
            local_shadow_layer_count: 0,
            last_frame: Instant::now(),
            fps_timer: Instant::now(),
            frame_count: 0,
            fps: 0.0,
            prepared_batches: Vec::new(),
            outline_batches: std::array::from_fn(|_| Vec::new()),
            local_light_scratch: Vec::new(),
            batch_scratch: Vec::new(),
            batch_indices_scratch: HashMap::new(),
            material_bind_groups: HashMap::new(),
        };
        for shape in PartShape::ALL {
            let mesh = renderer.add_mesh(&shape.mesh([1.0; 4]))?;
            renderer.primitive_meshes[shape.index()] = mesh;
        }
        Ok(renderer)
    }
}
