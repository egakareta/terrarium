use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use thiserror::Error;
use web_time::Instant;

use crate::{
    DEPTH_FORMAT, Instance, InstanceId, MATERIAL_SLOT_COUNT, Material, Mesh, MeshMaterialSlots,
    Part, PartShape, Texture, TextureColorSpace, TextureError, TextureHandle, Transform, Vertex,
    Workspace,
    glam::{Mat4, Vec3, Vec4},
    wgpu::util::DeviceExt,
    winit::window::Window,
};

const SHADOW_MAP_SIZE: u32 = 2048;
const SHADOW_ORTHOGRAPHIC_EXTENT: f32 = 35.0;
const SHADOW_NEAR: f32 = 20.0;
const SHADOW_FAR: f32 = 80.0;

/// Errors returned while creating or using a renderer.
#[derive(Debug, Error)]
pub enum RendererError {
    /// The window surface could not be created.
    #[error("could not create the rendering surface: {0}")]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),
    /// No compatible GPU adapter was available.
    #[error("could not find a compatible GPU adapter: {0}")]
    AdapterRequest(#[from] wgpu::RequestAdapterError),
    /// The selected adapter could not create a device and queue.
    #[error("could not create the GPU device: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),
    /// The window surface exposed no texture format.
    #[error("the window surface did not expose any texture formats")]
    NoSurfaceFormat,
    /// A custom mesh had no vertices or indices.
    #[error("mesh must contain at least one vertex and one index")]
    EmptyMesh,
    /// A custom mesh index was not present in its vertex list.
    #[error("mesh index {index} is outside the vertex range")]
    InvalidMeshIndex {
        /// The invalid index value.
        index: u16,
    },
    /// A texture failed validation.
    #[error("invalid texture: {0}")]
    InvalidTexture(#[from] TextureError),
    /// A material referenced a texture that is not owned by its workspace.
    #[error("workspace texture handle {index} is not valid")]
    InvalidTextureHandle {
        /// The invalid workspace-local texture index.
        index: usize,
    },
    /// The surface reported a validation error while acquiring a frame.
    #[error("the surface reported a validation error while acquiring a frame")]
    SurfaceValidation,
    /// Waiting for GPU work failed.
    #[error("could not wait for submitted GPU work: {0}")]
    DevicePoll(#[from] wgpu::PollError),
    /// The renderer was created for eframe and cannot present directly.
    #[error("the renderer does not own a presentation surface")]
    NoSurface,
    /// Eframe did not provide the WGPU state required by the eframe renderer.
    #[cfg(feature = "eframe")]
    #[error("eframe did not provide a WGPU render state")]
    EframeRenderStateUnavailable,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    normal_0: [f32; 4],
    normal_1: [f32; 4],
    normal_2: [f32; 4],
    base_color: [f32; 4],
    metallic_roughness: [f32; 4],
    emissive: [f32; 4],
}

impl InstanceRaw {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            6 => Float32x4,
            7 => Float32x4,
            8 => Float32x4,
            9 => Float32x4,
            10 => Float32x4,
            11 => Float32x4,
            12 => Float32x4,
            13 => Float32x4,
            14 => Float32x4,
            15 => Float32x4,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRIBUTES,
        }
    }
}

/// A handle to mesh data stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshHandle(usize);
struct RenderBatch {
    shape: PartShape,
    textures: MaterialTextures,
    instances: Vec<InstanceRaw>,
    instance_start: usize,
}

struct PreparedRenderBatch {
    shape: PartShape,
    packed_textures: PackedMaterialTextures,
    instance_start: usize,
    instance_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GpuTextureHandle(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialTextures {
    base_color: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    normal: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    metallic_roughness: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PackedMaterialTextures {
    textures: [PackedTextureHandle; MATERIAL_SLOT_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PackedTextureHandle(usize);

struct GpuMaterialTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct GpuTexture {
    _texture: wgpu::Texture,
    source: Texture,
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// The wgpu state and built-in PBR mesh pipeline.
pub struct Renderer {
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_texture: Option<wgpu::Texture>,
    depth_view: Option<wgpu::TextureView>,
    _shadow_texture: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    _shadow_sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    shadow_camera_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
    material_sampler: wgpu::Sampler,
    textures: Vec<GpuTexture>,
    workspace_texture_handles: HashMap<(InstanceId, TextureHandle), GpuTextureHandle>,
    texture_dedup: HashMap<Texture, GpuTextureHandle>,
    default_material_textures: MaterialTextures,
    packed_material_textures: HashMap<MaterialTextures, PackedMaterialTextures>,
    gpu_material_textures: Vec<GpuMaterialTexture>,
    instance_buffer: wgpu::Buffer,
    instance_data: Vec<InstanceRaw>,
    meshes: Vec<GpuMesh>,
    primitive_meshes: [MeshHandle; PartShape::COUNT],
    clear_color: wgpu::Color,
    last_frame: Instant,
    fps_timer: Instant,
    frame_count: u32,
    fps: f32,
    prepared_batches: Vec<PreparedRenderBatch>,
    batch_scratch: Vec<RenderBatch>,
    batch_indices_scratch: HashMap<(PartShape, MaterialTextures), usize>,
    material_bind_groups: HashMap<PackedMaterialTextures, wgpu::BindGroup>,
}

#[repr(C)]
/// The camera and fixed lighting values consumed by the built-in shader.
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    /// Camera view-projection matrix in column-major form.
    pub view_projection: [[f32; 4]; 4],
    /// World-to-light clip matrix used to render and sample the directional shadow map.
    pub light_view_projection: [[f32; 4]; 4],
    /// Camera world position as an XYZ vector with an unused fourth component.
    pub camera_position: [f32; 4],
    /// World-space direction toward the fixed key light.
    pub light_direction: [f32; 4],
    /// RGB intensity of the fixed key light.
    pub light_color: [f32; 4],
    /// RGB intensity of the fixed ambient light.
    pub ambient_color: [f32; 4],
}

impl Renderer {
    /// Creates a renderer and keeps the supplied window alive through its surface.
    pub async fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        Self::new_with_present_mode(window, wgpu::PresentMode::Fifo).await
    }

    /// Creates a renderer with the requested surface presentation mode.
    ///
    /// FIFO presentation is used when the surface does not support the requested mode.
    /// [`Renderer::new`] retains the normal FIFO presentation behavior.
    pub async fn new_with_present_mode(
        window: Arc<Window>,
        requested_present_mode: wgpu::PresentMode,
    ) -> Result<Self, RendererError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: Default::default(),
            flags: wgpu::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
            backend_options: Default::default(),
            display: Default::default(),
            memory_budget_thresholds: Default::default(),
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        log::info!("selected wgpu adapter: {:?}", adapter.get_info());
        let required_limits = wgpu::Limits {
            max_sampled_textures_per_shader_stage: MATERIAL_SLOT_COUNT as u32 + 1,
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("terrarium device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(RendererError::NoSurfaceFormat)?;
        let present_mode = if capabilities.present_modes.contains(&requested_present_mode) {
            requested_present_mode
        } else {
            wgpu::PresentMode::Fifo
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        Self::from_gpu(Some(surface), device, queue, config)
    }

    /// Creates a renderer that draws into eframe's WGPU render pass.
    #[cfg(feature = "eframe")]
    pub fn new_eframe(
        render_state: &crate::egui_wgpu::RenderState,
        size: [u32; 2],
    ) -> Result<Self, RendererError> {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: render_state.target_format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size[0].max(1),
            height: size[1].max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        Self::from_gpu(
            None,
            render_state.device.clone(),
            render_state.queue.clone(),
            config,
        )
    }

    fn from_gpu(
        surface: Option<wgpu::Surface<'static>>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
    ) -> Result<Self, RendererError> {
        let (depth_texture, depth_view) = surface.as_ref().map_or((None, None), |_| {
            let (texture, view) = create_depth_texture(&device, &config);
            (Some(texture), Some(view))
        });
        let (shadow_texture, shadow_view) = create_shadow_texture(&device);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
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
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part instance buffer"),
            size: std::mem::size_of::<InstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
                            view_dimension: wgpu::TextureViewDimension::D2,
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
                ],
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
            ],
        });
        let shadow_camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let shadow_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow camera bind group"),
            layout: &shadow_camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let mut material_bind_group_entries = Vec::with_capacity(MATERIAL_SLOT_COUNT + 1);
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
        material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
            binding: MATERIAL_SLOT_COUNT as u32,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material bind group layout"),
                entries: &material_bind_group_entries,
            });
        let material_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let default_base_color = Texture::new(1, 1, vec![255, 255, 255, 255])?;
        let default_normal = Texture::linear(1, 1, vec![128, 128, 255, 255])?;
        let default_metallic_roughness = Texture::linear(1, 1, vec![0, 255, 0, 255])?;
        let textures = vec![
            upload_texture(&device, &queue, &default_base_color)?,
            upload_texture(&device, &queue, &default_normal)?,
            upload_texture(&device, &queue, &default_metallic_roughness)?,
        ];
        let mut texture_dedup = HashMap::new();
        texture_dedup.insert(default_base_color, GpuTextureHandle(0));
        texture_dedup.insert(default_normal, GpuTextureHandle(1));
        texture_dedup.insert(default_metallic_roughness, GpuTextureHandle(2));
        let default_material_textures = MaterialTextures {
            base_color: [GpuTextureHandle(0); MATERIAL_SLOT_COUNT],
            normal: [GpuTextureHandle(1); MATERIAL_SLOT_COUNT],
            metallic_roughness: [GpuTextureHandle(2); MATERIAL_SLOT_COUNT],
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PBR mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let shader_constants = [(
            "FRAMEBUFFER_IS_SRGB",
            if config.format.is_srgb() { 1.0 } else { 0.0 },
        )];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[
                Some(&camera_bind_group_layout),
                Some(&material_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PBR mesh pipeline"),
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
                depth_write_enabled: Some(true),
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
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
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
                buffers: &[Some(Vertex::layout()), Some(InstanceRaw::layout())],
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
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        let mut renderer = Self {
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            _shadow_texture: shadow_texture,
            shadow_view,
            _shadow_sampler: shadow_sampler,
            pipeline,
            shadow_pipeline,
            camera_buffer,
            camera_bind_group,
            shadow_camera_bind_group,
            material_bind_group_layout,
            material_sampler,
            textures,
            workspace_texture_handles: HashMap::new(),
            texture_dedup,
            default_material_textures,
            packed_material_textures: HashMap::new(),
            gpu_material_textures: Vec::new(),
            instance_buffer,
            instance_data: Vec::new(),
            meshes: Vec::new(),
            primitive_meshes: [MeshHandle(usize::MAX); PartShape::COUNT],
            clear_color: wgpu::Color {
                r: 0.018,
                g: 0.028,
                b: 0.065,
                a: 1.0,
            },
            last_frame: Instant::now(),
            fps_timer: Instant::now(),
            frame_count: 0,
            fps: 0.0,
            prepared_batches: Vec::new(),
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

    /// Sets the color used to clear the color attachment before each frame.
    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.clear_color = color;
    }

    /// Returns the average number of successfully presented frames per second over the last
    /// measurement interval.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Returns the elapsed time in seconds since the previous call, capped at 100 milliseconds.
    pub fn delta_secs(&mut self) -> f32 {
        let now = Instant::now();
        let delta_seconds = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        delta_seconds.min(0.1)
    }

    /// Blocks until all GPU work submitted before this call has completed.
    ///
    /// This is intended for deterministic measurements and should not be used in a real-time
    /// render loop, where allowing multiple frames in flight is preferable.
    pub fn wait_for_gpu(&self) -> Result<(), RendererError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map(|_| ())
            .map_err(RendererError::DevicePoll)
    }

    /// Reconfigures the surface and depth buffer for a new non-zero size.
    ///
    /// Zero dimensions are ignored, which is useful while a window is
    /// minimized.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
        if self.depth_texture.is_some() {
            let (depth_texture, depth_view) = create_depth_texture(&self.device, &self.config);
            self.depth_texture = Some(depth_texture);
            self.depth_view = Some(depth_view);
        }
    }

    /// Prepares a workspace for drawing in an eframe WGPU paint callback.
    #[cfg(feature = "eframe")]
    pub fn prepare_eframe_scene(
        &mut self,
        workspace: &Workspace,
        size: [u32; 2],
    ) -> Result<(), RendererError> {
        self.config.width = size[0].max(1);
        self.config.height = size[1].max(1);
        self.prepare_scene(workspace)?;
        self.submit_shadow_map();
        Ok(())
    }

    /// Draws the prepared workspace into an eframe WGPU render pass.
    #[cfg(feature = "eframe")]
    pub fn paint_eframe_scene<'a>(&mut self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_scene(pass);
        self.record_frame();
    }

    /// Uploads a custom mesh and returns its GPU handle.
    pub fn add_mesh(&mut self, mesh: &Mesh) -> Result<MeshHandle, RendererError> {
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
        let handle = MeshHandle(self.meshes.len());
        self.meshes.push(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        });
        Ok(handle)
    }

    fn material_textures(
        &mut self,
        workspace: &Workspace,
        material: &Material,
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

        let base_color = resolve(
            self,
            material.textures.base_color,
            self.default_material_textures.base_color[0],
        )?;
        let normal = resolve(
            self,
            material.textures.normal,
            self.default_material_textures.normal[0],
        )?;
        let metallic_roughness = resolve(
            self,
            material.textures.metallic_roughness,
            self.default_material_textures.metallic_roughness[0],
        )?;
        let mut textures = MaterialTextures {
            base_color: [base_color; MATERIAL_SLOT_COUNT],
            normal: [normal; MATERIAL_SLOT_COUNT],
            metallic_roughness: [metallic_roughness; MATERIAL_SLOT_COUNT],
        };
        for (slot, material) in material_slots
            .slots
            .iter()
            .take(MATERIAL_SLOT_COUNT - 1)
            .enumerate()
        {
            let slot = slot + 1;
            textures.base_color[slot] = resolve(self, material.textures.base_color, base_color)?;
            textures.normal[slot] = resolve(self, material.textures.normal, normal)?;
            textures.metallic_roughness[slot] = resolve(
                self,
                material.textures.metallic_roughness,
                metallic_roughness,
            )?;
        }
        Ok(textures)
    }

    fn upload_workspace_texture(
        &mut self,
        workspace: &Workspace,
        handle: TextureHandle,
    ) -> Result<GpuTextureHandle, RendererError> {
        let workspace_handle = (workspace.id(), handle);
        if let Some(&gpu_handle) = self.workspace_texture_handles.get(&workspace_handle) {
            return Ok(gpu_handle);
        }

        let texture = workspace
            .get_texture(handle)
            .ok_or(RendererError::InvalidTextureHandle { index: handle.0 })?;
        let gpu_handle = self.upload_dedup_texture(texture)?;
        self.workspace_texture_handles
            .insert(workspace_handle, gpu_handle);
        Ok(gpu_handle)
    }

    fn upload_dedup_texture(
        &mut self,
        texture: &Texture,
    ) -> Result<GpuTextureHandle, RendererError> {
        if let Some(&gpu_handle) = self.texture_dedup.get(texture) {
            return Ok(gpu_handle);
        }
        let gpu_handle = GpuTextureHandle(self.textures.len());
        self.textures
            .push(upload_texture(&self.device, &self.queue, texture)?);
        self.texture_dedup.insert(texture.clone(), gpu_handle);
        Ok(gpu_handle)
    }

    fn upload_material_texture(
        &mut self,
        base_color: &Texture,
        surface: &Texture,
    ) -> Result<PackedTextureHandle, RendererError> {
        let base_mips = base_color.mip_levels()?;
        let surface_mips = surface.mip_levels()?;
        let format = match base_color.color_space {
            TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
            TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
        };
        let gpu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("packed material texture"),
            size: wgpu::Extent3d {
                width: base_color.width,
                height: base_color.height,
                depth_or_array_layers: 2,
            },
            mip_level_count: base_mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip_level, (base_image, surface_image)) in
            base_mips.iter().zip(surface_mips).enumerate()
        {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu_texture,
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
            let surface_pixels = if base_color.color_space == TextureColorSpace::Srgb {
                encode_srgb_rgb(&surface_image.pixels)
            } else {
                surface_image.pixels
            };
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu_texture,
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
        }
        let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(2),
            ..Default::default()
        });
        let handle = PackedTextureHandle(self.gpu_material_textures.len());
        self.gpu_material_textures.push(GpuMaterialTexture {
            _texture: gpu_texture,
            view,
        });
        Ok(handle)
    }

    fn pack_material_textures(
        &mut self,
        textures: MaterialTextures,
    ) -> Result<PackedMaterialTextures, RendererError> {
        if let Some(&packed) = self.packed_material_textures.get(&textures) {
            return Ok(packed);
        }

        let mut packed_textures = [PackedTextureHandle(usize::MAX); MATERIAL_SLOT_COUNT];
        for slot in 0..MATERIAL_SLOT_COUNT {
            let base_texture = self.textures[textures.base_color[slot].0].source.clone();
            let normal_texture = self.textures[textures.normal[slot].0].source.clone();
            let metallic_roughness_texture = self.textures[textures.metallic_roughness[slot].0]
                .source
                .clone();
            let surface_texture = Texture::linear(
                base_texture.width,
                base_texture.height,
                pack_surface_pixels(
                    &normal_texture,
                    &metallic_roughness_texture,
                    base_texture.width,
                    base_texture.height,
                ),
            )?;
            packed_textures[slot] =
                self.upload_material_texture(&base_texture, &surface_texture)?;
        }
        let packed = PackedMaterialTextures {
            textures: packed_textures,
        };
        self.packed_material_textures.insert(textures, packed);
        Ok(packed)
    }

    fn material_bind_group(&mut self, textures: PackedMaterialTextures) -> &wgpu::BindGroup {
        if !self.material_bind_groups.contains_key(&textures) {
            let bind_group = self.create_material_bind_group(textures);
            self.material_bind_groups.insert(textures, bind_group);
        }
        &self.material_bind_groups[&textures]
    }

    fn create_material_bind_group(&self, textures: PackedMaterialTextures) -> wgpu::BindGroup {
        let mut entries = Vec::with_capacity(MATERIAL_SLOT_COUNT + 1);
        for (slot, texture) in textures.textures.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: slot as u32,
                resource: wgpu::BindingResource::TextureView(
                    &self.gpu_material_textures[texture.0].view,
                ),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: MATERIAL_SLOT_COUNT as u32,
            resource: wgpu::BindingResource::Sampler(&self.material_sampler),
        });
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material bind group"),
            layout: &self.material_bind_group_layout,
            entries: &entries,
        })
    }

    fn ensure_instance_capacity(&mut self, instance_count: usize) {
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

    fn prepare_scene(&mut self, workspace: &Workspace) -> Result<(), RendererError> {
        self.prepared_batches.clear();
        let camera_vp = workspace.current_camera.view_projection_matrix();
        let light_vp = light_view_projection(Vec3::new(-0.45, 0.85, 0.35));
        let camera_uniform = CameraUniform {
            view_projection: camera_vp.to_cols_array_2d(),
            light_view_projection: light_vp.to_cols_array_2d(),
            camera_position: workspace
                .current_camera
                .pivot()
                .w_axis
                .truncate()
                .extend(1.0)
                .to_array(),
            light_direction: [-0.45, 0.85, 0.35, 0.0],
            light_color: [3.0, 2.8, 2.5, 0.0],
            ambient_color: [0.035, 0.045, 0.06, 0.0],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        let camera_planes = frustum_planes(camera_vp);
        let light_planes = frustum_planes(light_vp);
        let default_textures = self.default_material_textures;

        // Reuse allocations across frames: clearing retains backing capacity,
        // so the steady state performs no batching allocations.
        for batch in &mut self.batch_scratch {
            batch.instances.clear();
        }
        // Temporarily take ownership to build this frame; returned below.
        let mut batches = std::mem::take(&mut self.batch_scratch);
        batches.clear();
        let mut batch_indices = std::mem::take(&mut self.batch_indices_scratch);
        batch_indices.clear();
        // Fast path for the common untextured case: index directly by shape
        // instead of hashing a 168-byte key per part.
        let mut default_batch_for_shape: [Option<usize>; PartShape::COUNT] =
            [None; PartShape::COUNT];
        for part in workspace.get_all::<Part>() {
            // Exact union culling: keep anything visible to the camera or able
            // to cast into view. Culled parts contribute zero pixels to either
            // pass, so this changes no rendered pixel.
            let max_scale = part.size.max_element().max(0.0);
            let radius = part.shape.bounding_radius() * max_scale * 1.01;
            let center = part.position();
            if !sphere_visible(&camera_planes, center, radius)
                && !sphere_visible(&light_planes, center, radius)
            {
                continue;
            }
            let textures =
                self.material_textures(workspace, &part.material, &part.material_slots)?;
            let batch_index = if textures == default_textures {
                let slot = part.shape.index();
                if let Some(batch_index) = default_batch_for_shape[slot] {
                    batch_index
                } else {
                    let batch_index = batches.len();
                    default_batch_for_shape[slot] = Some(batch_index);
                    batches.push(RenderBatch {
                        shape: part.shape,
                        textures,
                        instances: Vec::new(),
                        instance_start: 0,
                    });
                    batch_index
                }
            } else {
                let key = (part.shape, textures);
                if let Some(&batch_index) = batch_indices.get(&key) {
                    batch_index
                } else {
                    let batch_index = batches.len();
                    batch_indices.insert(key, batch_index);
                    batches.push(RenderBatch {
                        shape: part.shape,
                        textures,
                        instances: Vec::new(),
                        instance_start: 0,
                    });
                    batch_index
                }
            };
            let model = part.transform();
            let (normal_0, normal_1, normal_2) = normal_columns_from_model(&model);
            let material = part.material;
            let tint = part.color.rgba();
            batches[batch_index].instances.push(InstanceRaw {
                model: model.to_cols_array_2d(),
                normal_0: [normal_0[0], normal_0[1], normal_0[2], 0.0],
                normal_1: [normal_1[0], normal_1[1], normal_1[2], 0.0],
                normal_2: [normal_2[0], normal_2[1], normal_2[2], 0.0],
                base_color: [
                    material.base_color[0] * tint[0],
                    material.base_color[1] * tint[1],
                    material.base_color[2] * tint[2],
                    material.base_color[3] * tint[3],
                ],
                metallic_roughness: [
                    material.metallic.clamp(0.0, 1.0),
                    material.roughness.clamp(0.04, 1.0),
                    0.0,
                    0.0,
                ],
                emissive: [
                    material.emissive[0],
                    material.emissive[1],
                    material.emissive[2],
                    0.0,
                ],
            });
        }

        self.instance_data.clear();
        // Reserve once so repeated frames never reallocate the flattened list.
        let total_instances: usize = batches.iter().map(|batch| batch.instances.len()).sum();
        self.instance_data.reserve(total_instances);
        for batch in &mut batches {
            batch.instance_start = self.instance_data.len();
            self.instance_data.extend_from_slice(&batch.instances);
        }
        self.ensure_instance_capacity(self.instance_data.len());
        if !self.instance_data.is_empty() {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instance_data),
            );
        }

        self.prepared_batches.clear();
        self.prepared_batches.reserve(batches.len());
        for batch in &batches {
            let packed = self.pack_material_textures(batch.textures)?;
            // Populate the bind-group cache once per material; steady-state
            // frames create zero bind groups.
            self.material_bind_group(packed);
            self.prepared_batches.push(PreparedRenderBatch {
                shape: batch.shape,
                packed_textures: packed,
                instance_start: batch.instance_start,
                instance_count: batch.instances.len() as u32,
            });
        }
        // Return scratch storage for reuse next frame.
        for batch in &mut batches {
            batch.instances.clear();
        }
        self.batch_scratch = batches;
        self.batch_indices_scratch = batch_indices;
        // Keep scratch capacities warm for the next frame's batch count.
        self.batch_scratch.reserve(PartShape::COUNT);
        Ok(())
    }

    fn draw_batches<'a>(
        &self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &wgpu::RenderPipeline,
        camera_bind_group: &wgpu::BindGroup,
        use_materials: bool,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        for batch in &self.prepared_batches {
            if batch.instance_count == 0 {
                continue;
            }
            let mesh_handle = self.primitive_meshes[batch.shape.index()];
            let Some(mesh) = self.meshes.get(mesh_handle.0) else {
                continue;
            };
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            let instance_start =
                batch.instance_start as u64 * std::mem::size_of::<InstanceRaw>() as u64;
            let instance_end = instance_start
                + batch.instance_count as u64 * std::mem::size_of::<InstanceRaw>() as u64;
            pass.set_vertex_buffer(1, self.instance_buffer.slice(instance_start..instance_end));
            if use_materials
                && let Some(bind_group) = self.material_bind_groups.get(&batch.packed_textures)
            {
                pass.set_bind_group(1, bind_group, &[]);
            }
            pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instance_count);
        }
    }

    fn draw_scene<'a>(&self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_batches(pass, &self.pipeline, &self.camera_bind_group, true);
    }

    fn draw_shadow_scene<'a>(&self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_batches(
            pass,
            &self.shadow_pipeline,
            &self.shadow_camera_bind_group,
            false,
        );
    }

    fn encode_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("directional shadow pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.draw_shadow_scene(&mut pass);
    }

    #[cfg(feature = "eframe")]
    fn submit_shadow_map(&self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("shadow command encoder"),
            });
        self.encode_shadow_pass(&mut encoder);
        self.queue.submit(Some(encoder.finish()));
    }

    fn record_frame(&mut self) {
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed;
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
    }

    /// Renders a workspace using its current camera. A lost or outdated surface is reconfigured
    /// and retried on the next frame; minimized and occluded windows simply skip their frame.
    pub fn render(&mut self, workspace: &Workspace) -> Result<(), RendererError> {
        self.render_with_overlay(
            workspace,
            |_device, _queue, _encoder, _view, _format, _size| Vec::new(),
        )
    }

    /// Renders a workspace and gives an overlay access to the frame before it is presented.
    ///
    /// The callback can encode additional commands into the frame, such as an egui render pass,
    /// and return command buffers that must be submitted alongside the scene command buffer.
    pub fn render_with_overlay<F>(
        &mut self,
        workspace: &Workspace,
        draw_overlay: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(
            &wgpu::Device,
            &wgpu::Queue,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
            wgpu::TextureFormat,
            [u32; 2],
        ) -> Vec<wgpu::CommandBuffer>,
    {
        self.render_with_overlay_internal(workspace, |renderer, encoder, view| {
            draw_overlay(
                &renderer.device,
                &renderer.queue,
                encoder,
                view,
                renderer.config.format,
                [renderer.config.width, renderer.config.height],
            )
        })
    }

    pub(crate) fn render_with_overlay_internal<F>(
        &mut self,
        workspace: &Workspace,
        draw_overlay: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(
            &mut Self,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
        ) -> Vec<wgpu::CommandBuffer>,
    {
        let frame = {
            let Some(surface) = &self.surface else {
                return Err(RendererError::NoSurface);
            };
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    surface.configure(&self.device, &self.config);
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(RendererError::SurfaceValidation);
                }
            }
        };

        self.prepare_scene(workspace)?;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let color_attachment = wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(self.clear_color),
                store: wgpu::StoreOp::Store,
            },
        };
        let Some(depth_view) = self.depth_view.as_ref() else {
            return Err(RendererError::NoSurface);
        };
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene command encoder"),
            });
        self.encode_shadow_pass(&mut encoder);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene render pass"),
                color_attachments: &[Some(color_attachment)],
                depth_stencil_attachment: Some(depth_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.draw_scene(&mut pass);
        }
        let overlay_command_buffers = draw_overlay(self, &mut encoder, &view);
        self.queue.submit(
            overlay_command_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        self.queue.present(frame);
        self.record_frame();
        Ok(())
    }
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &Texture,
) -> Result<GpuTexture, TextureError> {
    let format = match texture.color_space {
        TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
    };
    let mip_levels = texture.mip_levels()?;
    let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip_level, image) in mip_levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Ok(GpuTexture {
        _texture: gpu_texture,
        source: texture.clone(),
    })
}

fn pack_surface_pixels(
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

fn encode_srgb_rgb(pixels: &[u8]) -> Vec<u8> {
    let mut encoded = pixels.to_vec();
    for pixel in encoded.chunks_exact_mut(4) {
        pixel[0] = linear_to_srgb_byte(pixel[0]);
        pixel[1] = linear_to_srgb_byte(pixel[1]);
        pixel[2] = linear_to_srgb_byte(pixel[2]);
    }
    encoded
}

fn linear_to_srgb_byte(value: u8) -> u8 {
    let value = value as f32 / 255.0;
    let value = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn light_view_projection(light_direction: Vec3) -> Mat4 {
    let light_direction = light_direction.normalize_or_zero();
    let light_position = light_direction * 50.0;
    let up = if light_direction.dot(Vec3::Y).abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = crate::glam::camera::rh::view::look_at_mat4(light_position, Vec3::ZERO, up);
    let projection = crate::glam::camera::rh::proj::directx::orthographic(
        -SHADOW_ORTHOGRAPHIC_EXTENT,
        SHADOW_ORTHOGRAPHIC_EXTENT,
        -SHADOW_ORTHOGRAPHIC_EXTENT,
        SHADOW_ORTHOGRAPHIC_EXTENT,
        SHADOW_NEAR,
        SHADOW_FAR,
    );
    projection * view
}

/// Normal-matrix columns for a `pivot * scale` model without a full inverse.
///
/// `Part::transform` is always a rigid pivot multiplied by an axis-aligned
/// scale, so with `M3 = R * S` each column is a unit rotation axis scaled by
/// its axis scale. Dividing by the squared length recovers `R * S^-1`, which
/// is exactly the inverse-transpose for this TRS form at a fraction of the
/// cost of `Mat4::inverse`.
fn normal_columns_from_model(model: &Mat4) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let c0 = model.x_axis.truncate();
    let c1 = model.y_axis.truncate();
    let c2 = model.z_axis.truncate();
    let n0 = c0 / c0.length_squared().max(1e-12);
    let n1 = c1 / c1.length_squared().max(1e-12);
    let n2 = c2 / c2.length_squared().max(1e-12);
    (n0.to_array(), n1.to_array(), n2.to_array())
}

/// Extracts normalized clip planes from a DirectX-style (depth 0..1)
/// view-projection matrix. Each plane is `(normal, distance)` with points
/// inside satisfying `dot(normal, p) + distance >= 0`.
fn frustum_planes(view_projection: Mat4) -> [Vec4; 6] {
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

fn sphere_visible(planes: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    for plane in planes {
        if plane.truncate().dot(center) + plane.w < -radius {
            return false;
        }
    }
    true
}

fn create_shadow_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("directional shadow map"),
        size: wgpu::Extent3d {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_packing_preserves_normal_xy_and_material_channels() {
        let normal = Texture::linear(1, 1, vec![10, 20, 30, 255]).unwrap();
        let metallic_roughness = Texture::linear(1, 1, vec![40, 50, 60, 255]).unwrap();

        assert_eq!(
            pack_surface_pixels(&normal, &metallic_roughness, 1, 1),
            vec![10, 20, 60, 50]
        );
    }

    #[test]
    fn material_shader_validates_with_explicit_texture_gradients() {
        let module = wgpu::naga::front::wgsl::parse_str(include_str!("shader.wgsl"))
            .expect("material shader should parse");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        validator
            .validate(&module)
            .expect("material shader should validate");
    }

    #[test]
    fn cheap_normal_columns_match_inverse_transpose_for_trs() {
        use crate::glam::{Quat, Vec3};
        let rotation = Quat::from_euler(crate::glam::EulerRot::XYZ, 0.4, -0.7, 0.2);
        let pivot = Mat4::from_rotation_translation(rotation, Vec3::new(1.0, -2.0, 3.0));
        let size = Vec3::new(0.82, 1.1, 0.6);
        let model = pivot * Mat4::from_scale(size);
        let (n0, n1, n2) = normal_columns_from_model(&model);
        let reference = model.inverse().transpose().to_cols_array_2d();
        for (computed, expected) in [n0, n1, n2].iter().zip([
            [reference[0][0], reference[0][1], reference[0][2]],
            [reference[1][0], reference[1][1], reference[1][2]],
            [reference[2][0], reference[2][1], reference[2][2]],
        ]) {
            for (a, b) in computed.iter().zip(expected.iter()) {
                assert!((a - b).abs() < 1e-5, "got {computed:?}, want {expected:?}");
            }
        }
    }

    #[test]
    fn frustum_culling_keeps_visible_and_rejects_outside() {
        use crate::Camera;
        let camera = Camera::new(Vec3::new(0.0, 2.0, 6.0), Vec3::ZERO, 16.0 / 9.0);
        let planes = frustum_planes(camera.view_projection_matrix());
        assert!(sphere_visible(&planes, Vec3::ZERO, 0.5));
        // Far behind the camera must be culled.
        assert!(!sphere_visible(&planes, Vec3::new(0.0, 2.0, 20.0), 0.5));
        // Far beyond far plane must be culled.
        assert!(!sphere_visible(
            &planes,
            camera.pivot().w_axis.truncate() + camera.forward() * 500.0,
            0.5
        ));
    }
}
