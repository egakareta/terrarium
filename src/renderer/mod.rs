use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use bytemuck::{Pod, Zeroable};
use thiserror::Error;
use web_time::Instant;

#[cfg(feature = "meshpart")]
use crate::MeshPart;
use crate::{
    BasePart, Camera, Color3, Face, Instance, InstanceId, Mesh, OutlineMode, Part, PartShape,
    PointLight, Skybox, SkyboxError, SpotLight, SurfaceLight, Vertex, Workspace,
    glam::{Mat4, Vec3, Vec4},
    wgpu::util::DeviceExt,
};
mod frame;
mod helpers;
mod init;
mod lighting;
mod material;
mod mesh;
mod scene;
mod skybox;
mod texture;
use helpers::*;
use lighting::*;
pub use material::*;
#[cfg(test)]
mod tests;

const SHADOW_MAP_SIZE: u32 = 3072;
const SHADOW_CASCADE_COUNT: usize = 7;
/// Maximum number of camera-relevant local lights rendered in one frame.
///
/// When a scene contains more lights, the renderer keeps the lights with the
/// strongest estimated contribution to the current camera.
pub const MAX_LOCAL_LIGHTS: usize = 64;
/// Maximum local-light shadow-map layers rendered in one frame.
///
/// Spot and surface lights consume one layer each. Omnidirectional point-light
/// shadows consume six. Lights outside this budget still illuminate the scene
/// without shadows.
pub const MAX_LOCAL_SHADOW_LAYERS: usize = 8;
const LOCAL_SHADOW_MAP_SIZE: u32 = 1024;
const LOCAL_SHADOW_VISIBILITY_OFFSET: usize = SHADOW_CASCADE_COUNT + 1;
const LOCAL_LIGHT_DERIVED_RANGE: f32 = 24.0;
const LOCAL_LIGHT_MAX_RANGE: f32 = 100.0;
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
    /// A skybox failed validation.
    #[error("invalid skybox: {0}")]
    InvalidSkybox(#[from] SkyboxError),
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
pub struct GpuMeshHandle(usize);
struct RenderBatch {
    mesh: GpuMeshHandle,
    textures: MaterialTextures,
    filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    visibility_mask: u16,
    transparent: bool,
    sort_depth: f32,
    instances: Vec<InstanceRaw>,
    instance_start: usize,
}

struct DrawBatchOptions<'a> {
    pipeline: &'a wgpu::RenderPipeline,
    camera_bind_group: &'a wgpu::BindGroup,
    dynamic_offsets: &'a [wgpu::DynamicOffset],
    use_materials: bool,
    visibility_bit: u16,
    transparent: bool,
}

struct PreparedRenderBatch {
    mesh: GpuMeshHandle,
    packed_textures: PackedMaterialTextures,
    filters: [TextureFilter; MATERIAL_SLOT_COUNT],
    visibility_mask: u16,
    transparent: bool,
    instance_start: usize,
    instance_count: u32,
}

#[derive(Clone, Copy)]
struct OutlineSpec {
    color: Color3,
    width: f32,
    threshold: f32,
}

struct OutlineRenderBatch {
    mesh: GpuMeshHandle,
    instances: Vec<InstanceRaw>,
    instance_start: usize,
}

struct PartCandidate<'a> {
    part: &'a Part,
    pivot: Mat4,
    center: Vec3,
    radius: f32,
    visibility_mask: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LocalLightRaw {
    position_range: [f32; 4],
    color_brightness: [f32; 4],
    direction_type: [f32; 4],
    axis_u_half_width: [f32; 4],
    axis_v_half_height: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LocalLightsUniform {
    params: [u32; 4],
    lights: [LocalLightRaw; MAX_LOCAL_LIGHTS],
    shadow_view_projections: [[[f32; 4]; 4]; MAX_LOCAL_SHADOW_LAYERS],
}

struct LocalLightCandidate {
    raw: LocalLightRaw,
    shadow_view_projections: [Mat4; 6],
    shadow_layer_count: usize,
    shadow_excluded_caster: InstanceId,
    score: f32,
}

#[derive(Clone, Copy)]
struct LightParent {
    id: InstanceId,
    pivot: Mat4,
    size: Vec3,
}

struct PreparedLocalShadows {
    planes: [[Vec4; 6]; MAX_LOCAL_SHADOW_LAYERS],
    excluded_casters: [Option<InstanceId>; MAX_LOCAL_SHADOW_LAYERS],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GpuTextureHandle(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialTextures {
    base_color: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    normal: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    metallic_roughness: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    emissive: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PackedMaterialTextures {
    textures: [PackedTextureHandle; MATERIAL_SLOT_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PackedTextureHandle(usize);

/// Packed `vec4` count for one deduplicated per-face PBR factor set: six
/// directional slots times three `vec4`s per slot (base color, emissive RGB +
/// roughness, metallic).
const MATERIAL_VEC4S_PER_SET: usize = MATERIAL_SLOT_COUNT * 3;

/// Width of the material-factor data texture: one texel per packed `vec4`,
/// so each deduplicated set occupies exactly one row.
const MATERIAL_FACTOR_TEXTURE_WIDTH: u32 = MATERIAL_VEC4S_PER_SET as u32;

/// GPU format of the material-factor data texture: exact `f32` storage
/// sampled with `textureLoad` (no filtering, so no float-filterable feature
/// is required).
const MATERIAL_FACTOR_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// Hashable per-face PBR factors for one part: bit patterns of base color
/// RGBA, metallic, roughness, and emissive RGB for each directional slot, with
/// unset slots resolved to the default material.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialSetKey([[u32; 9]; MATERIAL_SLOT_COUNT]);

impl MaterialSetKey {
    fn base_bits(material: &Material) -> [u32; 9] {
        let base_color = material.base_color();
        let emissive = material.emissive();
        [
            base_color[0].to_bits(),
            base_color[1].to_bits(),
            base_color[2].to_bits(),
            base_color[3].to_bits(),
            material.metallic().to_bits(),
            material.roughness().to_bits(),
            emissive[0].to_bits(),
            emissive[1].to_bits(),
            emissive[2].to_bits(),
        ]
    }

    /// A uniform set where every face uses the default material: the common
    /// case for parts without slot overrides.
    fn uniform(material: [u32; 9]) -> Self {
        Self([material; MATERIAL_SLOT_COUNT])
    }

    fn from_materials(material_slots: &MeshMaterialSlots) -> Self {
        if material_slots.slots.is_empty() {
            return Self::uniform(Self::base_bits(&DEFAULT_MATERIAL));
        }
        let mut slots = [[0u32; 9]; MATERIAL_SLOT_COUNT];
        for (index, slot) in Face::ALL.into_iter().enumerate() {
            let material = material_slots.get(slot).unwrap_or(&DEFAULT_MATERIAL);
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
    source: Texture,
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

#[cfg(feature = "meshpart")]
#[derive(Clone, Copy)]
struct CachedMeshPart {
    revision: u64,
    handle: GpuMeshHandle,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShadowCameraUniform {
    light_view_projection: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyboxCameraUniform {
    view_projection: [[f32; 4]; 4],
    exposure: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct OutlineUniform {
    toon_params: [f32; 4],
    toon_color: [f32; 4],
    silhouette_params: [f32; 4],
    silhouette_color: [f32; 4],
    stencil_params: [f32; 4],
    stencil_color: [f32; 4],
}

/// Unit-cube corners shared by the skybox vertex buffer.
const SKYBOX_VERTICES: [[f32; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];

/// Cube triangles covering all six skybox faces (winding is irrelevant: the
/// skybox pipeline disables face culling).
const SKYBOX_INDICES: [u16; 36] = [
    0, 1, 2, 0, 2, 3, // back (-Z)
    4, 6, 5, 4, 7, 6, // front (+Z)
    0, 3, 7, 0, 7, 4, // left (-X)
    1, 5, 6, 1, 6, 2, // right (+X)
    3, 2, 6, 3, 6, 7, // top (+Y)
    0, 4, 5, 0, 5, 1, // bottom (-Y)
];

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
    scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    outline_mask_texture: wgpu::Texture,
    outline_mask_view: wgpu::TextureView,
    _shadow_texture: wgpu::Texture,
    shadow_layer_views: [wgpu::TextureView; SHADOW_CASCADE_COUNT],
    _local_shadow_texture: wgpu::Texture,
    local_shadow_view: wgpu::TextureView,
    local_shadow_layer_views: [wgpu::TextureView; MAX_LOCAL_SHADOW_LAYERS],
    _shadow_sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    outline_composite_pipeline: wgpu::RenderPipeline,
    toon_mask_pipeline: wgpu::RenderPipeline,
    silhouette_mask_pipeline: wgpu::RenderPipeline,
    stencil_mask_pipeline: wgpu::RenderPipeline,
    stencil_outline_pipeline: wgpu::RenderPipeline,
    eframe_scene: EframeSceneTarget,
    skybox_pipeline: wgpu::RenderPipeline,
    skybox_bind_group_layout: wgpu::BindGroupLayout,
    skybox_uniform_buffer: wgpu::Buffer,
    skybox_sampler: wgpu::Sampler,
    skybox_vertex_buffer: wgpu::Buffer,
    skybox_index_buffer: wgpu::Buffer,
    skybox_texture: Option<wgpu::Texture>,
    skybox_view: Option<wgpu::TextureView>,
    skybox_bind_group: Option<wgpu::BindGroup>,
    skybox_revision: Option<(InstanceId, u64)>,
    camera_buffer: wgpu::Buffer,
    local_lights_buffer: wgpu::Buffer,
    outline_uniform_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    outline_bind_group_layout: wgpu::BindGroupLayout,
    outline_bind_group: wgpu::BindGroup,
    outline_geometry_bind_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    _fallback_environment_texture: wgpu::Texture,
    fallback_environment_view: wgpu::TextureView,
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
    outline_instance_buffers: [wgpu::Buffer; 3],
    meshes: Vec<GpuMesh>,
    primitive_meshes: [GpuMeshHandle; PartShape::COUNT],
    #[cfg(feature = "meshpart")]
    meshpart_meshes: HashMap<InstanceId, CachedMeshPart>,
    #[cfg(feature = "meshpart")]
    free_meshpart_meshes: Vec<GpuMeshHandle>,
    clear_color: wgpu::Color,
    shadow_pass_enabled: bool,
    local_shadow_layer_count: usize,
    last_frame: Instant,
    fps_timer: Instant,
    frame_count: u32,
    fps: f32,
    prepared_batches: Vec<PreparedRenderBatch>,
    outline_batches: [Vec<OutlineRenderBatch>; 3],
    local_light_scratch: Vec<LocalLightCandidate>,
    batch_scratch: Vec<RenderBatch>,
    batch_indices_scratch: HashMap<
        (
            GpuMeshHandle,
            MaterialTextures,
            [TextureFilter; MATERIAL_SLOT_COUNT],
            u16,
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
    /// RGB outdoor ambient contribution.
    pub outdoor_ambient_color: [f32; 4],
    /// RGB tint applied to upward-facing surfaces.
    pub color_shift_top: [f32; 4],
    /// RGB tint applied to downward-facing surfaces.
    pub color_shift_bottom: [f32; 4],
    /// RGB tint used by fully shadowed direct light.
    pub shadow_color: [f32; 4],
    /// x: daylight factor, y: shadow enable, z: shadow softness, w: exposure.
    pub lighting_params: [f32; 4],
    /// RGB fog color.
    pub fog_color: [f32; 4],
    /// x: fog start distance, y: fog end distance.
    pub fog_params: [f32; 4],
    /// View-space far distance of each directional shadow cascade.
    pub shadow_cascade_splits: [[f32; 4]; 2],
    /// World-space width of one texel in each directional shadow cascade.
    pub shadow_texel_sizes: [[f32; 4]; 2],
    /// Image-based lighting parameters: cubemap mip count, diffuse scale,
    /// specular scale, and 1.0/0.0 environment presence.
    pub environment_params: [f32; 4],
}
