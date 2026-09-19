use super::*;

impl EframeSceneTarget {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("eframe scene texture layout"),
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
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("eframe scene sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eframe scene composite shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("eframe scene composite pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("eframe scene composite pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let texture = create_eframe_scene_texture(device, format, width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_eframe_scene_bind_group(device, &bind_group_layout, &view, &sampler);
        Self {
            _texture: texture,
            view,
            format,
            sampler,
            bind_group_layout,
            bind_group,
            pipeline,
        }
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let texture = create_eframe_scene_texture(device, self.format, width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_eframe_scene_bind_group(device, &self.bind_group_layout, &view, &self.sampler);
        self._texture = texture;
        self.view = view;
        self.bind_group = bind_group;
    }
}

pub(super) fn create_eframe_scene_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eframe scene texture bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub(super) fn create_eframe_scene_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eframe scene color"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn create_material_sampler(
    device: &wgpu::Device,
    filter: TextureFilter,
) -> wgpu::Sampler {
    let (mag_filter, min_filter, mipmap_filter, anisotropy, label) = match filter {
        TextureFilter::Nearest => (
            wgpu::FilterMode::Nearest,
            wgpu::FilterMode::Nearest,
            wgpu::MipmapFilterMode::Nearest,
            1,
            "material sampler (nearest)",
        ),
        TextureFilter::Bilinear => (
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Nearest,
            1,
            "material sampler (bilinear)",
        ),
        TextureFilter::Trilinear => (
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Linear,
            1,
            "material sampler (trilinear)",
        ),
        TextureFilter::Anisotropic4x => (
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Linear,
            4,
            "material sampler (anisotropic 4x)",
        ),
        TextureFilter::Anisotropic8x => (
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Linear,
            8,
            "material sampler (anisotropic 8x)",
        ),
        TextureFilter::Anisotropic16x => (
            wgpu::FilterMode::Linear,
            wgpu::FilterMode::Linear,
            wgpu::MipmapFilterMode::Linear,
            16,
            "material sampler (anisotropic 16x)",
        ),
    };
    let anisotropy_clamp = anisotropy;
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter,
        min_filter,
        mipmap_filter,
        lod_min_clamp: 0.0,
        lod_max_clamp: 32.0,
        compare: None,
        anisotropy_clamp,
        border_color: None,
    })
}

pub(super) fn texture_gpu_format(color_space: TextureColorSpace) -> wgpu::TextureFormat {
    match color_space {
        TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
    }
}

pub(super) fn create_material_factor_texture(
    device: &wgpu::Device,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material factor texture"),
        size: wgpu::Extent3d {
            width: MATERIAL_FACTOR_TEXTURE_WIDTH,
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: MATERIAL_FACTOR_TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

pub(super) fn create_material_factor_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material factor bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

pub(super) fn write_packed_mips(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    base_mips: &[Image],
    surface_mips: &[Image],
    emissive_mips: &[Image],
    base_color_space: TextureColorSpace,
) {
    for (mip_level, ((base_image, surface_image), emissive_image)) in base_mips
        .iter()
        .zip(surface_mips)
        .zip(emissive_mips)
        .enumerate()
    {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &base_image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(base_image.width * 4),
                rows_per_image: Some(base_image.height),
            },
            wgpu::Extent3d {
                width: base_image.width,
                height: base_image.height,
                depth_or_array_layers: 1,
            },
        );
        let surface_pixels = if base_color_space == TextureColorSpace::Srgb {
            encode_srgb_rgb(&surface_image.pixels)
        } else {
            surface_image.pixels.clone()
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 1 },
                aspect: wgpu::TextureAspect::All,
            },
            &surface_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(surface_image.width * 4),
                rows_per_image: Some(surface_image.height),
            },
            wgpu::Extent3d {
                width: surface_image.width,
                height: surface_image.height,
                depth_or_array_layers: 1,
            },
        );
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 2 },
                aspect: wgpu::TextureAspect::All,
            },
            &emissive_image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(emissive_image.width * 4),
                rows_per_image: Some(emissive_image.height),
            },
            wgpu::Extent3d {
                width: emissive_image.width,
                height: emissive_image.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

pub(super) fn pack_surface_pixels(
    normal: &Texture,
    metallic_roughness: &Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        let normal_y = (y as u64 * normal.height as u64 / height as u64) as u32;
        let metallic_roughness_y =
            (y as u64 * metallic_roughness.height as u64 / height as u64) as u32;
        for x in 0..width {
            let normal_x = (x as u64 * normal.width as u64 / width as u64) as u32;
            let metallic_roughness_x =
                (x as u64 * metallic_roughness.width as u64 / width as u64) as u32;
            let destination = (y as usize * width as usize + x as usize) * 4;
            let normal_pixel = (normal_y as usize * normal.width as usize + normal_x as usize) * 4;
            let metallic_roughness_pixel = (metallic_roughness_y as usize
                * metallic_roughness.width as usize
                + metallic_roughness_x as usize)
                * 4;
            pixels[destination] = normal.pixels[normal_pixel];
            pixels[destination + 1] = normal.pixels[normal_pixel + 1];
            pixels[destination + 2] = metallic_roughness.pixels[metallic_roughness_pixel + 2];
            pixels[destination + 3] = metallic_roughness.pixels[metallic_roughness_pixel + 1];
        }
    }
    pixels
}

pub(super) fn resize_texture_pixels(texture: &Texture, width: u32, height: u32) -> Vec<u8> {
    if texture.width == width && texture.height == height {
        return texture.pixels.clone();
    }

    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        let source_y = (y as u64 * texture.height as u64 / height as u64) as usize;
        for x in 0..width {
            let source_x = (x as u64 * texture.width as u64 / width as u64) as usize;
            let source = (source_y * texture.width as usize + source_x) * 4;
            let destination = (y as usize * width as usize + x as usize) * 4;
            pixels[destination..destination + 4]
                .copy_from_slice(&texture.pixels[source..source + 4]);
        }
    }
    pixels
}

pub(super) fn encode_srgb_rgb(pixels: &[u8]) -> Vec<u8> {
    let mut encoded = pixels.to_vec();
    for pixel in encoded.chunks_exact_mut(4) {
        pixel[0] = linear_to_srgb_byte(pixel[0]);
        pixel[1] = linear_to_srgb_byte(pixel[1]);
        pixel[2] = linear_to_srgb_byte(pixel[2]);
    }
    encoded
}

pub(super) fn linear_to_srgb_byte(value: u8) -> u8 {
    static LOOKUP: OnceLock<[u8; 256]> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        std::array::from_fn(|value| {
            let value = value as f32 / 255.0;
            let value = if value <= 0.0031308 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    })[value as usize]
}
pub(super) fn align_to(value: u64, alignment: u64) -> u64 {
    value.div_ceil(alignment) * alignment
}

/// Factors used to reconstruct normal-matrix columns from a `pivot * scale` model.
///
/// `Part::transform` is always a rigid pivot multiplied by an axis-aligned
/// scale, so with `M3 = R * S` each column is a unit rotation axis scaled by
/// its axis scale. Multiplying by the reciprocal squared length recovers
/// `R * S^-1`, which is exactly the inverse-transpose for this TRS form.
pub(super) fn normal_scales_from_model(model: &Mat4) -> [f32; 3] {
    let c0 = model.x_axis.truncate();
    let c1 = model.y_axis.truncate();
    let c2 = model.z_axis.truncate();
    [
        c0.length_squared().max(1e-12).recip(),
        c1.length_squared().max(1e-12).recip(),
        c2.length_squared().max(1e-12).recip(),
    ]
}

/// Extracts normalized clip planes from a DirectX-style (depth 0..1)
/// view-projection matrix. Each plane is `(normal, distance)` with points
/// inside satisfying `dot(normal, p) + distance >= 0`.
pub(super) fn frustum_planes(view_projection: Mat4) -> [Vec4; 6] {
    let m = view_projection.to_cols_array_2d();
    // Rows of the column-major matrix.
    let row = |i: usize| Vec4::new(m[0][i], m[1][i], m[2][i], m[3][i]);
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    let normalize = |p: Vec4| {
        let length = p.truncate().length().max(1e-12);
        p / length
    };
    [
        normalize(r3 + r0), // left
        normalize(r3 - r0), // right
        normalize(r3 + r1), // bottom
        normalize(r3 - r1), // top
        normalize(r2),      // near (0..1 depth)
        normalize(r3 - r2), // far
    ]
}

pub(super) enum FrustumRelation {
    Outside,
    Intersecting,
    Inside,
}

pub(super) fn aabb_frustum_relation(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> FrustumRelation {
    let center = (min + max) * 0.5;
    let extent = (max - min) * 0.5;
    let center = center.extend(1.0);
    let mut fully_inside = true;
    for plane in planes {
        let projected_extent = plane.truncate().abs().dot(extent);
        let distance = plane.dot(center);
        if distance < -projected_extent {
            return FrustumRelation::Outside;
        }
        fully_inside &= distance >= projected_extent;
    }
    if fully_inside {
        FrustumRelation::Inside
    } else {
        FrustumRelation::Intersecting
    }
}

pub(super) fn sphere_visible(planes: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    let center = center.extend(1.0);
    for plane in planes {
        if plane.dot(center) < -radius {
            return false;
        }
    }
    true
}

pub(super) fn create_shadow_texture(
    device: &wgpu::Device,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    [wgpu::TextureView; SHADOW_CASCADE_COUNT],
) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("directional shadow map"),
        size: wgpu::Extent3d {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
            depth_or_array_layers: SHADOW_CASCADE_COUNT as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHADOW_DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("directional shadow map array"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layer_views = std::array::from_fn(|cascade| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("directional shadow map cascade"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: cascade as u32,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });
    (texture, view, layer_views)
}

pub(super) fn create_local_shadow_texture(
    device: &wgpu::Device,
    size: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    [wgpu::TextureView; MAX_LOCAL_SHADOW_LAYERS],
) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("local light shadow map"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: MAX_LOCAL_SHADOW_LAYERS as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHADOW_DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("local light shadow map array"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layer_views = std::array::from_fn(|layer| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("local light shadow map layer"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer as u32,
            array_layer_count: Some(1),
            ..Default::default()
        })
    });
    (texture, view, layer_views)
}

pub(super) fn create_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
