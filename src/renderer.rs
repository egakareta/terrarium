use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

use bytemuck::{Pod, Zeroable};
use thiserror::Error;
use web_time::Instant;

use crate::{
    Camera, DEPTH_FORMAT, Image, Instance, InstanceId, MATERIAL_SLOT_COUNT, Material, MaterialSlot,
    Mesh, MeshMaterialSlots, MeshPart, Part, PartShape, Texture, TextureColorSpace, TextureError,
    TextureFilter, TextureHandle, Vertex, Workspace,
    glam::{Mat4, Vec3, Vec4},
    wgpu::util::DeviceExt,
};

const SHADOW_MAP_SIZE: u32 = 3072;
const SHADOW_CASCADE_COUNT: usize = 7;
// Keep native backend code loaded until thread-local driver state is gone.
#[cfg(not(target_arch = "wasm32"))]
static WGPU_INSTANCE_KEEPALIVE: OnceLock<wgpu::Instance> = OnceLock::new();
const VISIBILITY_MASK_COUNT: usize = 1 << (SHADOW_CASCADE_COUNT + 1);
const CULL_GROUP_SIZE: usize = 64;
const SHADOW_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;
const SHADOW_DISTANCE: f32 = 80.0;
const SHADOW_CASTER_MARGIN: f32 = 20.0;
const SHADOW_RECEIVER_MARGIN: f32 = 5.0;

/// Errors returned while creating or using a renderer.
#[derive(Debug, Error)]
pub enum RendererError {
    /// eframe was not configured to use its WGPU renderer.
    #[error("eframe WGPU render state is unavailable")]
    MissingEframeWgpuRenderState,
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
    /// Waiting for GPU work failed.
    #[error("could not wait for submitted GPU work: {0}")]
    DevicePoll(#[from] wgpu::PollError),
    /// A requested render-target pixel was outside the target dimensions.
    #[error("pixel ({x}, {y}) is outside the render target {width}x{height}")]
    InvalidPixel {
        /// Horizontal pixel coordinate.
        x: u32,
        /// Vertical pixel coordinate.
        y: u32,
        /// Render-target width.
        width: u32,
        /// Render-target height.
        height: u32,
    },
    /// Reading a render-target pixel failed.
    #[error("could not read render-target pixel: {0}")]
    PixelReadback(String),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 3]; 4],
    normal_scales: [f32; 3],
    tint: [f32; 4],
    material_set: u32,
}

impl InstanceRaw {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            6 => Float32x3,
            7 => Float32x3,
            8 => Float32x3,
            9 => Float32x3,
            10 => Float32x3,
            11 => Float32x4,
            12 => Uint32,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRIBUTES,
        }
    }

    fn shadow_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            6 => Float32x3,
            7 => Float32x3,
            8 => Float32x3,
            9 => Float32x3,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRIBUTES,
        }
    }
}

/// A handle to mesh data stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MeshHandle(usize);
struct RenderBatch {
    mesh: MeshHandle,
    textures: MaterialTextures,
    filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    visibility_mask: u8,
    instances: Vec<InstanceRaw>,
    instance_start: usize,
}

struct PreparedRenderBatch {
    mesh: MeshHandle,
    packed_textures: PackedMaterialTextures,
    filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    visibility_mask: u8,
    instance_start: usize,
    instance_count: u32,
}

struct PartCandidate<'a> {
    part: &'a Part,
    pivot: Mat4,
    center: Vec3,
    radius: f32,
    visibility_mask: u8,
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

/// Packed `vec4` count for one deduplicated per-face PBR factor set: seven
/// slots (base + six directions) times three `vec4`s per slot (base color,
/// emissive RGB + roughness, metallic).
const MATERIAL_VEC4S_PER_SET: usize = MATERIAL_SLOT_COUNT * 3;

/// Width of the material-factor data texture: one texel per packed `vec4`,
/// so each deduplicated set occupies exactly one row.
const MATERIAL_FACTOR_TEXTURE_WIDTH: u32 = MATERIAL_VEC4S_PER_SET as u32;

/// GPU format of the material-factor data texture: exact `f32` storage
/// sampled with `textureLoad` (no filtering, so no float-filterable feature
/// is required).
const MATERIAL_FACTOR_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// Hashable per-face PBR factors for one part: bit patterns of base color
/// RGBA, metallic, roughness, and emissive RGB for each of the seven slots in
/// [`MaterialSlot`] order, with unset directional slots resolved to the base
/// material.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialSetKey([[u32; 9]; MATERIAL_SLOT_COUNT]);

impl MaterialSetKey {
    fn base_bits(material: &Material) -> [u32; 9] {
        [
            material.base_color[0].to_bits(),
            material.base_color[1].to_bits(),
            material.base_color[2].to_bits(),
            material.base_color[3].to_bits(),
            material.metallic.to_bits(),
            material.roughness.to_bits(),
            material.emissive[0].to_bits(),
            material.emissive[1].to_bits(),
            material.emissive[2].to_bits(),
        ]
    }

    /// A uniform set where every face uses the base material: the common case
    /// for parts without slot overrides.
    fn uniform(base: [u32; 9]) -> Self {
        Self([base; MATERIAL_SLOT_COUNT])
    }

    fn from_materials(material: &Material, material_slots: &MeshMaterialSlots) -> Self {
        if material_slots.slots.is_empty() {
            return Self::uniform(Self::base_bits(material));
        }
        let mut slots = [[0u32; 9]; MATERIAL_SLOT_COUNT];
        for (index, slot) in std::iter::once(MaterialSlot::Base)
            .chain(MaterialSlot::ALL_DIRECTIONS)
            .enumerate()
        {
            let material = if slot == MaterialSlot::Base {
                material
            } else {
                material_slots.get(slot).unwrap_or(material)
            };
            slots[index] = Self::base_bits(material);
        }
        Self(slots)
    }

    /// Expands the key back into the GPU `vec4` sequence consumed by
    /// `shader.wgsl`: per slot, base color, then emissive RGB + roughness,
    /// then metallic.
    fn vec4s(&self) -> [[f32; 4]; MATERIAL_VEC4S_PER_SET] {
        let mut vec4s = [[0.0; 4]; MATERIAL_VEC4S_PER_SET];
        for (index, bits) in self.0.iter().enumerate() {
            vec4s[index * 3] = [
                f32::from_bits(bits[0]),
                f32::from_bits(bits[1]),
                f32::from_bits(bits[2]),
                f32::from_bits(bits[3]),
            ];
            vec4s[index * 3 + 1] = [
                f32::from_bits(bits[6]),
                f32::from_bits(bits[7]),
                f32::from_bits(bits[8]),
                f32::from_bits(bits[5]),
            ];
            vec4s[index * 3 + 2] = [f32::from_bits(bits[4]), 0.0, 0.0, 0.0];
        }
        vec4s
    }
}

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

#[derive(Clone, Copy)]
struct CachedMeshPart {
    revision: u64,
    handle: MeshHandle,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShadowCameraUniform {
    light_view_projection: [[f32; 4]; 4],
}

struct EframeSceneTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

/// The eframe WGPU state and built-in PBR mesh pipeline.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    width: u32,
    height: u32,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    _shadow_texture: wgpu::Texture,
    shadow_layer_views: [wgpu::TextureView; SHADOW_CASCADE_COUNT],
    _shadow_sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    eframe_scene: EframeSceneTarget,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    shadow_camera_buffer: wgpu::Buffer,
    shadow_camera_bind_group: wgpu::BindGroup,
    shadow_camera_stride: u32,
    material_bind_group_layout: wgpu::BindGroupLayout,
    material_samplers: HashMap<TextureFilter, wgpu::Sampler>,
    material_factor_bind_group_layout: wgpu::BindGroupLayout,
    material_factors_texture: wgpu::Texture,
    material_factors_view: wgpu::TextureView,
    material_factors_bind_group: wgpu::BindGroup,
    material_factor_vec4s: Vec<[f32; 4]>,
    material_factor_indices: HashMap<MaterialSetKey, u32>,
    /// Hot cache for consecutive parts sharing one material set.
    material_factor_last: Option<(MaterialSetKey, u32)>,
    /// Hot cache for the override-free case, comparing only the 9
    /// base-material words instead of the full 63-word key.
    material_factor_last_uniform: Option<([u32; 9], u32)>,
    textures: Vec<GpuTexture>,
    workspace_texture_handles: HashMap<(InstanceId, TextureHandle), (GpuTextureHandle, u64)>,
    texture_dedup: HashMap<Texture, GpuTextureHandle>,
    default_material_textures: MaterialTextures,
    default_material_filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    packed_material_textures: HashMap<MaterialTextures, PackedMaterialTextures>,
    gpu_material_textures: Vec<GpuMaterialTexture>,
    instance_buffer: wgpu::Buffer,
    meshes: Vec<GpuMesh>,
    primitive_meshes: [MeshHandle; PartShape::COUNT],
    meshpart_meshes: HashMap<InstanceId, CachedMeshPart>,
    free_meshpart_meshes: Vec<MeshHandle>,
    clear_color: wgpu::Color,
    last_frame: Instant,
    fps_timer: Instant,
    frame_count: u32,
    fps: f32,
    prepared_batches: Vec<PreparedRenderBatch>,
    batch_scratch: Vec<RenderBatch>,
    batch_indices_scratch: HashMap<
        (
            MeshHandle,
            MaterialTextures,
            [TextureFilter; MATERIAL_SLOT_COUNT],
            u8,
        ),
        usize,
    >,
    material_bind_groups:
        HashMap<(PackedMaterialTextures, [TextureFilter; MATERIAL_SLOT_COUNT]), wgpu::BindGroup>,
}

#[repr(C)]
/// The camera and fixed lighting values consumed by the built-in shader.
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    /// Camera view-projection matrix in column-major form.
    pub view_projection: [[f32; 4]; 4],
    /// World-to-light clip matrices used by the directional shadow cascades.
    pub light_view_projections: [[[f32; 4]; 4]; SHADOW_CASCADE_COUNT],
    /// Camera world position as an XYZ vector with an unused fourth component.
    pub camera_position: [f32; 4],
    /// Camera world-space forward direction.
    pub camera_forward: [f32; 4],
    /// World-space direction toward the fixed key light.
    pub light_direction: [f32; 4],
    /// RGB intensity of the fixed key light.
    pub light_color: [f32; 4],
    /// RGB intensity of the fixed ambient light.
    pub ambient_color: [f32; 4],
    /// View-space far distance of each directional shadow cascade.
    pub shadow_cascade_splits: [[f32; 4]; 2],
    /// World-space width of one texel in each directional shadow cascade.
    pub shadow_texel_sizes: [[f32; 4]; 2],
}

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
        let (shadow_texture, shadow_view, shadow_layer_views) = create_shadow_texture(&device);
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
        let shadow_camera_size = std::mem::size_of::<ShadowCameraUniform>() as u64;
        let shadow_camera_stride = align_to(
            shadow_camera_size,
            u64::from(device.limits().min_uniform_buffer_offset_alignment),
        ) as u32;
        let shadow_camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow camera uniform buffer"),
            size: u64::from(shadow_camera_stride) * SHADOW_CASCADE_COUNT as u64,
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
                    format,
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
        let eframe_scene = EframeSceneTarget::new(&device, format, width, height);

        let mut renderer = Self {
            device,
            queue,
            width,
            height,
            depth_texture,
            depth_view,
            _shadow_texture: shadow_texture,
            shadow_layer_views,
            _shadow_sampler: shadow_sampler,
            pipeline,
            shadow_pipeline,
            eframe_scene,
            camera_buffer,
            camera_bind_group,
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
            meshes: Vec::new(),
            primitive_meshes: [MeshHandle(usize::MAX); PartShape::COUNT],
            meshpart_meshes: HashMap::new(),
            free_meshpart_meshes: Vec::new(),
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

    /// Returns the average number of successfully rendered frames per second over the last
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

    /// Reads an RGBA8 pixel from the most recently prepared eframe scene.
    ///
    /// Coordinates use a top-left origin.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_pixel(&self, x: u32, y: u32) -> Result<[u8; 4], RendererError> {
        if x >= self.width || y >= self.height {
            return Err(RendererError::InvalidPixel {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }

        let pixels = self.read_pixels()?;
        Ok(pixels[y as usize * self.width as usize + x as usize])
    }

    /// Reads all RGBA8 pixels from the most recently prepared eframe scene.
    ///
    /// Pixels are returned in row-major order with a top-left origin.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_pixels(&self) -> Result<Vec<[u8; 4]>, RendererError> {
        let unpadded_bytes_per_row = u64::from(self.width) * 4;
        let bytes_per_row = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row
            .div_ceil(u64::from(bytes_per_row))
            .checked_mul(u64::from(bytes_per_row))
            .ok_or_else(|| RendererError::PixelReadback("row size overflow".to_owned()))?;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("eframe scene pixel readback"),
            size: padded_bytes_per_row * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("eframe scene pixel readback encoder"),
            });
        encoder.copy_texture_to_buffer(
            self.eframe_scene._texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row as u32),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;
        receiver
            .recv()
            .map_err(|error| RendererError::PixelReadback(error.to_string()))?
            .map_err(|error| RendererError::PixelReadback(error.to_string()))?;

        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| RendererError::PixelReadback(error.to_string()))?;
        let row_bytes = unpadded_bytes_per_row as usize;
        let mut pixels = Vec::with_capacity(self.width as usize * self.height as usize);
        for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
            for pixel in row[..row_bytes].chunks_exact(4) {
                pixels.push(pixel.try_into().expect("one RGBA8 pixel is four bytes"));
            }
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }

    /// Resizes the eframe scene and depth buffer for a new non-zero size.
    ///
    /// Zero dimensions are ignored, which is useful while a window is
    /// minimized.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        let (depth_texture, depth_view) = create_depth_texture(&self.device, width, height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.eframe_scene.resize(&self.device, width, height);
    }

    /// Prepares a workspace for drawing in an eframe WGPU paint callback.
    pub fn prepare_eframe_scene(
        &mut self,
        workspace: &Workspace,
        size: [u32; 2],
    ) -> Result<(), RendererError> {
        let width = size[0].max(1);
        let height = size[1].max(1);
        if self.width != width || self.height != height {
            self.resize(width, height);
        }
        self.prepare_scene(workspace)?;
        self.submit_scene();
        Ok(())
    }

    /// Draws the prepared workspace into an eframe WGPU render pass.
    pub fn paint_eframe_scene<'a>(&mut self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.eframe_scene.pipeline);
        pass.set_bind_group(0, &self.eframe_scene.bind_group, &[]);
        pass.draw(0..3, 0..1);
        self.record_frame();
    }

    /// Uploads a custom mesh and returns its GPU handle.
    pub fn add_mesh(&mut self, mesh: &Mesh) -> Result<MeshHandle, RendererError> {
        let gpu_mesh = self.create_gpu_mesh(mesh)?;
        let handle = MeshHandle(self.meshes.len());
        self.meshes.push(gpu_mesh);
        Ok(handle)
    }

    fn create_gpu_mesh(&self, mesh: &Mesh) -> Result<GpuMesh, RendererError> {
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

    fn meshpart_mesh(&mut self, meshpart: &MeshPart) -> Result<MeshHandle, RendererError> {
        if let Some(cached) = self.meshpart_meshes.get(&meshpart.id())
            && cached.revision == meshpart.mesh_revision()
        {
            return Ok(cached.handle);
        }

        let gpu_mesh = self.create_gpu_mesh(meshpart.mesh())?;
        let handle = if let Some(cached) = self.meshpart_meshes.get(&meshpart.id()).copied() {
            self.meshes[cached.handle.0] = gpu_mesh;
            cached.handle
        } else if let Some(handle) = self.free_meshpart_meshes.pop() {
            self.meshes[handle.0] = gpu_mesh;
            handle
        } else {
            let handle = MeshHandle(self.meshes.len());
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
        for (index, material) in material_slots
            .slots
            .iter()
            .take(MATERIAL_SLOT_COUNT - 1)
            .enumerate()
            .filter_map(|(index, material)| material.as_ref().map(|material| (index, material)))
        {
            let slot = index + 1;
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

    fn material_filters(
        material: &Material,
        material_slots: &MeshMaterialSlots,
    ) -> [TextureFilter; MATERIAL_SLOT_COUNT] {
        let base_filter = material.filter;
        let mut filters = [base_filter; MATERIAL_SLOT_COUNT];
        for (index, material) in material_slots
            .slots
            .iter()
            .take(MATERIAL_SLOT_COUNT - 1)
            .enumerate()
        {
            if let Some(material) = material {
                filters[index + 1] = material.filter;
            } else {
                filters[index + 1] = base_filter;
            }
        }
        filters
    }

    fn material_set_index(
        &mut self,
        material: &Material,
        material_slots: &MeshMaterialSlots,
    ) -> u32 {
        // Fast path: no overrides means every face uses the base material, so
        // only the 9 base words need comparing.
        if material_slots.slots.is_empty() {
            let base = MaterialSetKey::base_bits(material);
            if let Some((last_base, index)) = self.material_factor_last_uniform
                && last_base == base
            {
                return index;
            }
            let key = MaterialSetKey::uniform(base);
            let index = self.material_set_index_uncached(key);
            self.material_factor_last_uniform = Some((base, index));
            return index;
        }
        let key = MaterialSetKey::from_materials(material, material_slots);
        if let Some((last_key, index)) = self.material_factor_last
            && last_key == key
        {
            return index;
        }
        let index = self.material_set_index_uncached(key);
        self.material_factor_last = Some((key, index));
        index
    }

    fn material_set_index_uncached(&mut self, key: MaterialSetKey) -> u32 {
        if let Some(&index) = self.material_factor_indices.get(&key) {
            return index;
        }
        let index = (self.material_factor_vec4s.len() / MATERIAL_VEC4S_PER_SET) as u32;
        self.material_factor_vec4s.extend(key.vec4s());
        self.material_factor_indices.insert(key, index);
        index
    }

    fn upload_material_factors(&mut self) {
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

    fn upload_workspace_texture(
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
            if self.gpu_texture_is_shared(gpu_handle, texture) {
                // This GPU copy is shared with another logical texture (a
                // dedup hit or a built-in default): allocate a fresh copy
                // instead of overwriting storage other owners still sample.
                let new_handle = GpuTextureHandle(self.textures.len());
                self.textures
                    .push(upload_texture(&self.device, &self.queue, texture)?);
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
    fn gpu_texture_is_shared(&self, gpu_handle: GpuTextureHandle, texture: &Texture) -> bool {
        if self
            .default_material_textures
            .base_color
            .contains(&gpu_handle)
            || self.default_material_textures.normal.contains(&gpu_handle)
            || self
                .default_material_textures
                .metallic_roughness
                .contains(&gpu_handle)
        {
            return true;
        }
        self.texture_dedup
            .iter()
            .any(|(known, &known_handle)| known_handle == gpu_handle && known != texture)
    }

    /// Re-uploads an edited workspace texture and refreshes every packed
    /// material texture built from it.
    ///
    /// The caller guarantees `gpu_handle` is exclusively owned by this
    /// texture (see [`gpu_texture_is_shared`](Self::gpu_texture_is_shared)).
    /// Textures validate on the way in, so edits made through
    /// [`Workspace::get_texture_mut`] that break the RGBA8 invariant surface
    /// here as an error instead of panicking inside surface packing.
    fn refresh_workspace_texture(
        &mut self,
        gpu_handle: GpuTextureHandle,
        texture: &Texture,
    ) -> Result<(), RendererError> {
        texture.validate()?;
        let mips = texture.mip_levels()?;
        let format = texture_gpu_format(texture.color_space);
        let gpu_texture = &self.textures[gpu_handle.0]._texture;
        if gpu_texture.size().width != texture.width
            || gpu_texture.size().height != texture.height
            || gpu_texture.format() != format
            || gpu_texture.mip_level_count() != mips.len() as u32
        {
            let replacement = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("material texture"),
                size: wgpu::Extent3d {
                    width: texture.width,
                    height: texture.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.textures[gpu_handle.0]._texture = replacement;
        }
        write_texture_mips(&self.queue, &self.textures[gpu_handle.0]._texture, &mips);
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
    fn refresh_packed_textures(&mut self, changed: GpuTextureHandle) -> Result<(), RendererError> {
        let affected: Vec<MaterialTextures> = self
            .packed_material_textures
            .keys()
            .filter(|textures| {
                textures.base_color.contains(&changed)
                    || textures.normal.contains(&changed)
                    || textures.metallic_roughness.contains(&changed)
            })
            .copied()
            .collect();
        let mut recreated = Vec::new();
        for textures in affected {
            let packed = self.packed_material_textures[&textures];
            for (slot, packed_handle) in packed.textures.into_iter().enumerate() {
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
                let base_mips = base_texture.mip_levels()?;
                let surface_mips = surface_texture.mip_levels()?;
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
                            depth_or_array_layers: 2,
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
                        base_texture.color_space,
                    );
                    let view = replacement.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2Array),
                        array_layer_count: Some(2),
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
        let format = texture_gpu_format(base_color.color_space);
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
        write_packed_mips(
            &self.queue,
            &gpu_texture,
            &base_mips,
            &surface_mips,
            base_color.color_space,
        );
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
        for (slot, packed_texture) in packed_textures.iter_mut().enumerate() {
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
            *packed_texture = self.upload_material_texture(&base_texture, &surface_texture)?;
        }
        let packed = PackedMaterialTextures {
            textures: packed_textures,
        };
        self.packed_material_textures.insert(textures, packed);
        Ok(packed)
    }

    fn material_bind_group(
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

    fn create_material_bind_group(
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
        let light_direction = Vec3::new(-0.45, 0.85, 0.35);
        let (light_vps, shadow_cascade_splits, shadow_texel_sizes) =
            light_view_projections(&workspace.current_camera, light_direction);
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
            light_direction: [-0.45, 0.85, 0.35, 0.0],
            light_color: [3.0, 2.8, 2.5, 0.0],
            ambient_color: [0.035, 0.045, 0.06, 0.0],
            shadow_cascade_splits: pack_shadow_values(shadow_cascade_splits),
            shadow_texel_sizes: pack_shadow_values(shadow_texel_sizes),
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
        let camera_planes = frustum_planes(camera_vp);
        let light_planes = light_vps.map(frustum_planes);
        let default_textures = self.default_material_textures;
        let default_filters = self.default_material_filters;

        let stale_meshparts: Vec<_> = self
            .meshpart_meshes
            .keys()
            .copied()
            .filter(|&id| workspace.get::<MeshPart>(id).is_none())
            .collect();
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
            batch.textures == default_textures && self.primitive_meshes.contains(&batch.mesh)
        });
        batch_indices.clear();
        for batch in &mut batches {
            batch.instances.clear();
        }
        // Fast path for the common untextured case: index directly by shape
        // and pass visibility instead of hashing a large material key per part.
        let mut default_batches = [[None; VISIBILITY_MASK_COUNT]; PartShape::COUNT];
        for shape in PartShape::ALL {
            let mesh = self.primitive_meshes[shape.index()];
            for (batch_index, batch) in batches.iter().enumerate() {
                if batch.mesh == mesh {
                    default_batches[shape.index()][batch.visibility_mask as usize] =
                        Some(batch_index);
                }
            }
        }
        let mut parts = workspace.get_all::<Part>();
        let mut group = Vec::with_capacity(CULL_GROUP_SIZE);
        loop {
            group.clear();
            let mut bounds_min = Vec3::splat(f32::INFINITY);
            let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);
            for part in parts.by_ref().take(CULL_GROUP_SIZE) {
                let pivot = part.pivot();
                let max_scale = part.size.max_element().max(0.0);
                let radius = part.shape.bounding_radius() * max_scale * 1.01;
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

            let mut classify = |planes: &[Vec4; 6], visibility_bit: u8| match aabb_frustum_relation(
                planes, bounds_min, bounds_max,
            ) {
                FrustumRelation::Inside => {
                    for candidate in &mut group {
                        candidate.visibility_mask |= visibility_bit;
                    }
                }
                FrustumRelation::Intersecting => {
                    for candidate in &mut group {
                        if sphere_visible(planes, candidate.center, candidate.radius) {
                            candidate.visibility_mask |= visibility_bit;
                        }
                    }
                }
                FrustumRelation::Outside => {}
            };
            classify(&camera_planes, 1);
            for (cascade, planes) in light_planes.iter().enumerate() {
                classify(planes, 1 << (cascade + 1));
            }

            for candidate in &group {
                let part = candidate.part;
                let pivot = candidate.pivot;
                let visibility_mask = candidate.visibility_mask;
                if visibility_mask == 0 {
                    continue;
                }
                let has_custom_textures = visibility_mask & 1 != 0
                    && (part.material.textures.base_color.is_some()
                        || part.material.textures.normal.is_some()
                        || part.material.textures.metallic_roughness.is_some()
                        || part.material_slots.slots.iter().flatten().any(|material| {
                            material.textures.base_color.is_some()
                                || material.textures.normal.is_some()
                                || material.textures.metallic_roughness.is_some()
                        }));
                let custom_textures = if has_custom_textures {
                    let textures =
                        self.material_textures(workspace, &part.material, &part.material_slots)?;
                    (textures != default_textures).then_some(textures)
                } else {
                    None
                };
                // Filters only matter when textures are bound: 1x1 default
                // textures sample identically under any filter, so untextured
                // parts keep sharing the fast-path default batch.
                let custom_filters = if custom_textures.is_some() {
                    let filters = Self::material_filters(&part.material, &part.material_slots);
                    (filters != default_filters).then_some(filters)
                } else {
                    None
                };
                let batch_index = if let Some(textures) = custom_textures {
                    let filters = custom_filters.unwrap_or(default_filters);
                    let mesh = self.primitive_meshes[part.shape.index()];
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
                            instances: Vec::new(),
                            instance_start: 0,
                        });
                        batch_index
                    }
                } else {
                    let slot = &mut default_batches[part.shape.index()][visibility_mask as usize];
                    if let Some(batch_index) = *slot {
                        batch_index
                    } else {
                        let batch_index = batches.len();
                        *slot = Some(batch_index);
                        batches.push(RenderBatch {
                            mesh: self.primitive_meshes[part.shape.index()],
                            textures: default_textures,
                            filters: default_filters,
                            visibility_mask,
                            instances: Vec::new(),
                            instance_start: 0,
                        });
                        batch_index
                    }
                };
                let model = Mat4::from_cols(
                    pivot.x_axis * part.size.x,
                    pivot.y_axis * part.size.y,
                    pivot.z_axis * part.size.z,
                    pivot.w_axis,
                );
                let (normal_scales, tint, material_set) = if visibility_mask & 1 != 0 {
                    (
                        normal_scales_from_model(&model),
                        part.color.rgba(),
                        self.material_set_index(&part.material, &part.material_slots),
                    )
                } else {
                    ([0.0; 3], [0.0; 4], 0)
                };
                batches[batch_index].instances.push(InstanceRaw {
                    model: [
                        model.x_axis.truncate().to_array(),
                        model.y_axis.truncate().to_array(),
                        model.z_axis.truncate().to_array(),
                        model.w_axis.truncate().to_array(),
                    ],
                    normal_scales,
                    tint,
                    material_set,
                });
            }
        }

        for meshpart in workspace.get_all::<MeshPart>() {
            let pivot = meshpart.pivot();
            let center = pivot.w_axis.truncate();
            let radius = meshpart.bounding_radius() * meshpart.size.abs().max_element() * 1.01;
            let mut visibility_mask = 0;
            if sphere_visible(&camera_planes, center, radius) {
                visibility_mask |= 1;
            }
            for (cascade, planes) in light_planes.iter().enumerate() {
                if sphere_visible(planes, center, radius) {
                    visibility_mask |= 1 << (cascade + 1);
                }
            }
            if visibility_mask == 0 {
                continue;
            }

            let mesh = self.meshpart_mesh(meshpart)?;
            let has_custom_textures = visibility_mask & 1 != 0
                && (meshpart.material.textures.base_color.is_some()
                    || meshpart.material.textures.normal.is_some()
                    || meshpart.material.textures.metallic_roughness.is_some()
                    || meshpart
                        .material_slots
                        .slots
                        .iter()
                        .flatten()
                        .any(|material| {
                            material.textures.base_color.is_some()
                                || material.textures.normal.is_some()
                                || material.textures.metallic_roughness.is_some()
                        }));
            let textures = if has_custom_textures {
                self.material_textures(workspace, &meshpart.material, &meshpart.material_slots)?
            } else {
                default_textures
            };
            let filters = if textures == default_textures {
                default_filters
            } else {
                Self::material_filters(&meshpart.material, &meshpart.material_slots)
            };
            let key = (mesh, textures, filters, visibility_mask);
            let batch_index = if let Some(&batch_index) = batch_indices.get(&key) {
                batch_index
            } else {
                let batch_index = batches.len();
                batch_indices.insert(key, batch_index);
                batches.push(RenderBatch {
                    mesh,
                    textures,
                    filters,
                    visibility_mask,
                    instances: Vec::new(),
                    instance_start: 0,
                });
                batch_index
            };
            let model = Mat4::from_cols(
                pivot.x_axis * meshpart.size.x,
                pivot.y_axis * meshpart.size.y,
                pivot.z_axis * meshpart.size.z,
                pivot.w_axis,
            );
            let (normal_scales, tint, material_set) = if visibility_mask & 1 != 0 {
                (
                    normal_scales_from_model(&model),
                    meshpart.color.rgba(),
                    self.material_set_index(&meshpart.material, &meshpart.material_slots),
                )
            } else {
                ([0.0; 3], [0.0; 4], 0)
            };
            batches[batch_index].instances.push(InstanceRaw {
                model: [
                    model.x_axis.truncate().to_array(),
                    model.y_axis.truncate().to_array(),
                    model.z_axis.truncate().to_array(),
                    model.w_axis.truncate().to_array(),
                ],
                normal_scales,
                tint,
                material_set,
            });
        }

        let total_instances: usize = batches.iter().map(|batch| batch.instances.len()).sum();
        self.upload_material_factors();
        batches.sort_unstable_by_key(|batch| (batch.mesh.0, batch.visibility_mask));
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
                instance_start: batch.instance_start,
                instance_count: batch.instances.len() as u32,
            });
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
        dynamic_offsets: &[wgpu::DynamicOffset],
        use_materials: bool,
        visibility_bit: u8,
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

    fn draw_scene<'a>(&self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_batches(pass, &self.pipeline, &self.camera_bind_group, &[], true, 1);
    }

    fn draw_shadow_scene<'a>(&self, pass: &mut wgpu::RenderPass<'a>, cascade: usize) {
        self.draw_batches(
            pass,
            &self.shadow_pipeline,
            &self.shadow_camera_bind_group,
            &[cascade as u32 * self.shadow_camera_stride],
            false,
            1 << (cascade + 1),
        );
    }

    fn encode_shadow_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        for (cascade, view) in self.shadow_layer_views.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("directional shadow cascade pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
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
            self.draw_shadow_scene(&mut pass, cascade);
        }
    }

    fn encode_scene_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        let color_attachment = wgpu::RenderPassColorAttachment {
            view: color_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(self.clear_color),
                store: wgpu::StoreOp::Store,
            },
        };
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        };
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

    fn submit_scene(&self) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("eframe scene command encoder"),
            });
        self.encode_shadow_pass(&mut encoder);
        self.encode_scene_pass(&mut encoder, &self.eframe_scene.view, &self.depth_view);
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
}

impl EframeSceneTarget {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
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

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let texture = create_eframe_scene_texture(device, self.format, width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_eframe_scene_bind_group(device, &self.bind_group_layout, &view, &self.sampler);
        self._texture = texture;
        self.view = view;
        self.bind_group = bind_group;
    }
}

fn create_eframe_scene_bind_group(
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

fn create_eframe_scene_texture(
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

fn create_material_sampler(device: &wgpu::Device, filter: TextureFilter) -> wgpu::Sampler {
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

fn texture_gpu_format(color_space: TextureColorSpace) -> wgpu::TextureFormat {
    match color_space {
        TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
    }
}

fn create_material_factor_texture(
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

fn create_material_factor_bind_group(
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

fn write_texture_mips(queue: &wgpu::Queue, texture: &wgpu::Texture, mips: &[Image]) {
    for (mip_level, image) in mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
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
}

fn write_packed_mips(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    base_mips: &[Image],
    surface_mips: &[Image],
    base_color_space: TextureColorSpace,
) {
    for (mip_level, (base_image, surface_image)) in base_mips.iter().zip(surface_mips).enumerate() {
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
    }
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &Texture,
) -> Result<GpuTexture, TextureError> {
    let format = texture_gpu_format(texture.color_space);
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
    write_texture_mips(queue, &gpu_texture, &mip_levels);
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

fn light_view_projections(
    camera: &Camera,
    light_direction: Vec3,
) -> (
    [Mat4; SHADOW_CASCADE_COUNT],
    [f32; SHADOW_CASCADE_COUNT],
    [f32; SHADOW_CASCADE_COUNT],
) {
    let light_direction = match light_direction.try_normalize() {
        Some(direction) => direction,
        None => Vec3::Y,
    };
    let up = if light_direction.dot(Vec3::Y).abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let light_forward = -light_direction;
    let light_right = light_forward.cross(up).normalize();
    let light_up = light_right.cross(light_forward).normalize();
    let near = camera.znear.max(0.001);
    let far = camera.zfar.min(SHADOW_DISTANCE).max(near + 0.001);
    let mut splits = [far; SHADOW_CASCADE_COUNT];
    let mut matrices = [Mat4::IDENTITY; SHADOW_CASCADE_COUNT];
    let mut texel_sizes = [0.0; SHADOW_CASCADE_COUNT];
    let mut cascade_near = near;

    for cascade in 0..SHADOW_CASCADE_COUNT {
        let fraction = (cascade + 1) as f32 / SHADOW_CASCADE_COUNT as f32;
        let cascade_far = near * (far / near).powf(fraction);
        splits[cascade] = cascade_far;

        let corners = camera_frustum_slice_corners(camera, cascade_near, cascade_far);
        let center = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
        let radius = corners
            .iter()
            .map(|corner| corner.distance(center))
            .fold(0.0, f32::max);
        // A fixed-size bounding sphere prevents projection scale from changing as
        // the camera rotates. Leave a little room for snapping at the map edge.
        let extent = ((radius + 1.0 / 16.0) * 16.0).ceil() / 16.0;
        let texel_size = 2.0 * extent / SHADOW_MAP_SIZE as f32;
        texel_sizes[cascade] = texel_size;

        // Quantizing the light-space center keeps stationary shadows from
        // shimmering as the camera moves by sub-texel amounts.
        let center_x = center.dot(light_right);
        let center_y = center.dot(light_up);
        let snapped_center = center
            + light_right * ((center_x / texel_size).round() * texel_size - center_x)
            + light_up * ((center_y / texel_size).round() * texel_size - center_y);
        let light_position = snapped_center + light_direction * (extent + SHADOW_CASTER_MARGIN);
        let view = crate::glam::camera::rh::view::look_at_mat4(light_position, snapped_center, up);
        let (mut min_depth, mut max_depth) = (f32::MAX, f32::MIN);
        for corner in corners {
            let depth = -view.transform_point3(corner).z;
            min_depth = min_depth.min(depth);
            max_depth = max_depth.max(depth);
        }
        let shadow_near = (min_depth - SHADOW_CASTER_MARGIN).max(0.001);
        let shadow_far = (max_depth + SHADOW_RECEIVER_MARGIN).max(shadow_near + 0.001);
        let projection = crate::glam::camera::rh::proj::directx::orthographic(
            -extent,
            extent,
            -extent,
            extent,
            shadow_near,
            shadow_far,
        );
        matrices[cascade] = projection * view;
        cascade_near = cascade_far;
    }

    (matrices, splits, texel_sizes)
}

fn pack_shadow_values(values: [f32; SHADOW_CASCADE_COUNT]) -> [[f32; 4]; 2] {
    let mut packed = [[values[SHADOW_CASCADE_COUNT - 1]; 4]; 2];
    for (index, value) in values.into_iter().enumerate() {
        packed[index / 4][index % 4] = value;
    }
    packed
}

fn camera_frustum_slice_corners(camera: &Camera, near: f32, far: f32) -> [Vec3; 8] {
    let tan_half_fovy = (camera.fovy * 0.5).tan();
    let near_height = near * tan_half_fovy;
    let near_width = near_height * camera.aspect;
    let far_height = far * tan_half_fovy;
    let far_width = far_height * camera.aspect;
    let corners = [
        Vec3::new(-near_width, -near_height, -near),
        Vec3::new(near_width, -near_height, -near),
        Vec3::new(-near_width, near_height, -near),
        Vec3::new(near_width, near_height, -near),
        Vec3::new(-far_width, -far_height, -far),
        Vec3::new(far_width, -far_height, -far),
        Vec3::new(-far_width, far_height, -far),
        Vec3::new(far_width, far_height, -far),
    ];
    corners.map(|corner| camera.pivot().transform_point3(corner))
}

fn align_to(value: u64, alignment: u64) -> u64 {
    value.div_ceil(alignment) * alignment
}

/// Factors used to reconstruct normal-matrix columns from a `pivot * scale` model.
///
/// `Part::transform` is always a rigid pivot multiplied by an axis-aligned
/// scale, so with `M3 = R * S` each column is a unit rotation axis scaled by
/// its axis scale. Multiplying by the reciprocal squared length recovers
/// `R * S^-1`, which is exactly the inverse-transpose for this TRS form.
fn normal_scales_from_model(model: &Mat4) -> [f32; 3] {
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

enum FrustumRelation {
    Outside,
    Intersecting,
    Inside,
}

fn aabb_frustum_relation(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> FrustumRelation {
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

fn sphere_visible(planes: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    let center = center.extend(1.0);
    for plane in planes {
        if plane.dot(center) < -radius {
            return false;
        }
    }
    true
}

fn create_shadow_texture(
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

fn create_depth_texture(
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
    fn eframe_composite_shader_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(include_str!("blit.wgsl"))
            .expect("eframe composite shader should parse");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        validator
            .validate(&module)
            .expect("eframe composite shader should validate");
    }

    #[test]
    fn material_set_keys_resolve_unset_slots_to_the_base_material() {
        use crate::{MaterialSlot, Part};

        let mut part = Part::new("part");
        part.material = Material {
            base_color: [0.76, 0.30, 0.14, 1.0],
            metallic: 0.82,
            roughness: 0.24,
            emissive: [0.1, 0.2, 0.3],
            ..Material::default()
        };
        let top = Material {
            base_color: [0.1, 0.4, 0.2, 1.0],
            metallic: 0.1,
            roughness: 0.9,
            emissive: [0.0, 0.0, 0.0],
            ..Material::default()
        };
        part.set_material_slot(MaterialSlot::Top, top);

        let vec4s = MaterialSetKey::from_materials(&part.material, &part.material_slots).vec4s();
        // Base slot carries the base factors.
        assert_eq!(vec4s[0], [0.76, 0.30, 0.14, 1.0]);
        assert_eq!(vec4s[1], [0.1, 0.2, 0.3, 0.24]);
        assert_eq!(vec4s[2][0], 0.82);
        // The Top override carries its own factors.
        assert_eq!(vec4s[3], [0.1, 0.4, 0.2, 1.0]);
        assert_eq!(vec4s[4], [0.0, 0.0, 0.0, 0.9]);
        assert_eq!(vec4s[5][0], 0.1);
        // Unset slots fall back to the base factors, not the defaults.
        assert_eq!(vec4s[6], [0.76, 0.30, 0.14, 1.0]);
        assert_eq!(vec4s[7], [0.1, 0.2, 0.3, 0.24]);
        assert_eq!(vec4s[8][0], 0.82);
    }

    #[test]
    fn material_set_keys_distinguish_per_face_factors() {
        use crate::{MaterialSlot, Part};

        let mut copper = Part::new("copper");
        copper.material.metallic = 0.82;
        let mut copper_top_metal = Part::new("copper-top-metal");
        copper_top_metal.material.metallic = 0.82;
        copper_top_metal.set_material_slot(
            MaterialSlot::Top,
            Material {
                metallic: 0.1,
                ..Material::default()
            },
        );
        let mut copper_clone = Part::new("copper-clone");
        copper_clone.material.metallic = 0.82;

        assert_eq!(
            MaterialSetKey::from_materials(&copper.material, &copper.material_slots),
            MaterialSetKey::from_materials(&copper_clone.material, &copper_clone.material_slots)
        );
        assert_ne!(
            MaterialSetKey::from_materials(&copper.material, &copper.material_slots),
            MaterialSetKey::from_materials(
                &copper_top_metal.material,
                &copper_top_metal.material_slots
            )
        );
    }

    #[test]
    fn compact_normal_scales_match_inverse_transpose_for_trs() {
        use crate::glam::{Quat, Vec3};
        let rotation = Quat::from_euler(crate::glam::EulerRot::XYZ, 0.4, -0.7, 0.2);
        let pivot = Mat4::from_rotation_translation(rotation, Vec3::new(1.0, -2.0, 3.0));
        let size = Vec3::new(0.82, 1.1, 0.6);
        let model = pivot * Mat4::from_scale(size);
        let scales = normal_scales_from_model(&model);
        let columns = [
            (model.x_axis.truncate() * scales[0]).to_array(),
            (model.y_axis.truncate() * scales[1]).to_array(),
            (model.z_axis.truncate() * scales[2]).to_array(),
        ];
        let reference = model.inverse().transpose().to_cols_array_2d();
        for (computed, expected) in columns.iter().zip([
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

    #[test]
    fn shadow_cascades_cover_their_camera_frustum_slices() {
        let camera = Camera::new(Vec3::new(3.0, 4.0, 8.0), Vec3::ZERO, 16.0 / 9.0);
        let (matrices, splits, texel_sizes) =
            light_view_projections(&camera, Vec3::new(-0.45, 0.85, 0.35));
        let mut near = camera.znear;

        for cascade in 0..SHADOW_CASCADE_COUNT {
            assert!(splits[cascade] > near);
            assert!(texel_sizes[cascade] > 0.0);
            for corner in camera_frustum_slice_corners(&camera, near, splits[cascade]) {
                let clip = matrices[cascade] * corner.extend(1.0);
                let projected = clip.truncate() / clip.w;
                assert!(projected.x.abs() <= 1.0, "cascade {cascade}: {projected:?}");
                assert!(projected.y.abs() <= 1.0, "cascade {cascade}: {projected:?}");
                assert!(
                    (0.0..=1.0).contains(&projected.z),
                    "cascade {cascade}: {projected:?}"
                );
            }
            near = splits[cascade];
        }
        assert!((splits[SHADOW_CASCADE_COUNT - 1] - SHADOW_DISTANCE).abs() < 1e-4);
    }
}
