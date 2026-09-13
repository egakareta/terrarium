mod basepart;
mod camera;
mod color3;
mod instance;

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub use basepart::*;
use bytemuck::{Pod, Zeroable};
pub use camera::*;
pub use color3::*;
use glam::Vec3;
pub use instance::*;
use thiserror::Error;
use web_time::Instant;
use wgpu::util::DeviceExt;
use winit::window::Window;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MATERIAL_SLOT_COUNT: usize = 4;

/// Errors returned while creating or using a renderer.
#[derive(Debug, Error)]
pub enum RendererError {
    #[error("could not create the rendering surface: {0}")]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),
    #[error("could not find a compatible GPU adapter: {0}")]
    AdapterRequest(#[from] wgpu::RequestAdapterError),
    #[error("could not create the GPU device: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),
    #[error("the window surface did not expose any texture formats")]
    NoSurfaceFormat,
    #[error("mesh must contain at least one vertex and one index")]
    EmptyMesh,
    #[error("mesh index {index} is outside the vertex range")]
    InvalidMeshIndex { index: u16 },
    #[error("invalid texture: {0}")]
    InvalidTexture(#[from] TextureError),
    #[error("the surface reported a validation error while acquiring a frame")]
    SurfaceValidation,
    #[error("could not wait for submitted GPU work: {0}")]
    DevicePoll(#[from] wgpu::PollError),
}

/// A vertex consumed by the built-in PBR pipeline.
///
/// Tangents use the fourth component as the bitangent handedness, as in glTF:
/// the bitangent is `cross(normal, tangent) * tangent.w`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
    /// A per-vertex color multiplier retained for custom mesh tinting.
    pub color: [f32; 4],
    /// GPU representation of the material slot selected by this vertex.
    pub material_slot: u32,
}

impl Vertex {
    /// Creates a legacy vertex with default tangent-space attributes.
    ///
    /// New meshes should prefer [`Vertex::with_attributes`] so their normals,
    /// UVs, and tangents participate in PBR lighting and normal mapping.
    pub fn new(position: [f32; 3], color: [f32; 4]) -> Self {
        Self::with_attributes(
            position,
            [0.0, 1.0, 0.0],
            [0.0, 0.0],
            [1.0, 0.0, 0.0, 1.0],
            color,
        )
    }

    pub fn with_attributes(
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        tangent: [f32; 4],
        color: [f32; 4],
    ) -> Self {
        Self::with_material_slot(position, normal, uv, tangent, color, MaterialSlot::Base)
    }

    pub fn with_material_slot(
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        tangent: [f32; 4],
        color: [f32; 4],
        material_slot: MaterialSlot,
    ) -> Self {
        Self {
            position,
            normal,
            uv,
            tangent,
            color,
            material_slot: material_slot as u32,
        }
    }

    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
            2 => Float32x2,
            3 => Float32x4,
            4 => Float32x4,
            5 => Uint32,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRIBUTES,
        }
    }
}

/// The primitive geometry available to a [`Part`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PartShape {
    #[default]
    Block,
    Ball,
    Cylinder,
    Wedge,
    CornerWedge,
}

impl PartShape {
    const ALL: [Self; 5] = [
        Self::Block,
        Self::Ball,
        Self::Cylinder,
        Self::Wedge,
        Self::CornerWedge,
    ];

    const COUNT: usize = Self::ALL.len();

    const fn index(self) -> usize {
        match self {
            Self::Block => 0,
            Self::Ball => 1,
            Self::Cylinder => 2,
            Self::Wedge => 3,
            Self::CornerWedge => 4,
        }
    }

    fn mesh(self, color: [f32; 4]) -> Mesh {
        match self {
            Self::Block => Mesh::block(1.0, color),
            Self::Ball => Mesh::ball(0.5, 16, 24, color),
            Self::Cylinder => Mesh::cylinder(0.5, 1.0, 24, color),
            Self::Wedge => Mesh::wedge(color),
            Self::CornerWedge => Mesh::corner_wedge(color),
        }
    }
}

/// CPU-side mesh data ready to be uploaded to a [`Renderer`].
#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

impl Mesh {
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u16>) -> Self {
        Self { vertices, indices }
    }

    /// Creates a block centered at the origin.
    pub fn cube(size: f32, color: [f32; 4]) -> Self {
        Self::block(size, color)
    }

    /// Creates a box centered at the origin.
    pub fn block(size: f32, color: [f32; 4]) -> Self {
        let h = size * 0.5;
        let faces = [
            ([-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h]),
            ([h, -h, -h], [-h, -h, -h], [-h, h, -h], [h, h, -h]),
            ([-h, h, h], [h, h, h], [h, h, -h], [-h, h, -h]),
            ([-h, -h, -h], [h, -h, -h], [h, -h, h], [-h, -h, h]),
            ([h, -h, h], [h, -h, -h], [h, h, -h], [h, h, h]),
            ([-h, -h, -h], [-h, -h, h], [-h, h, h], [-h, h, -h]),
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (face_index, (a, b, c, d)) in faces.into_iter().enumerate() {
            let material_slot = match face_index {
                2 => MaterialSlot::Top,
                3 => MaterialSlot::Bottom,
                _ => MaterialSlot::Side,
            };
            push_quad_with_material_slot(
                &mut vertices,
                &mut indices,
                [a, b, c, d],
                color,
                material_slot,
            );
        }

        Self { vertices, indices }
    }

    /// Creates a UV sphere centered at the origin.
    pub fn ball(
        radius: f32,
        latitude_segments: usize,
        longitude_segments: usize,
        color: [f32; 4],
    ) -> Self {
        let latitude_segments = latitude_segments.max(2);
        let longitude_segments = longitude_segments.max(3);
        let mut vertices = Vec::with_capacity((latitude_segments + 1) * (longitude_segments + 1));
        let mut indices = Vec::with_capacity(latitude_segments * longitude_segments * 6);
        for latitude in 0..=latitude_segments {
            let v = latitude as f32 / latitude_segments as f32;
            let phi = v * std::f32::consts::PI;
            let y = phi.cos();
            let ring = phi.sin();
            for longitude in 0..=longitude_segments {
                let u = longitude as f32 / longitude_segments as f32;
                let theta = u * std::f32::consts::TAU;
                let normal = Vec3::new(theta.cos() * ring, y, theta.sin() * ring);
                let tangent = Vec3::new(-theta.sin(), 0.0, theta.cos());
                vertices.push(Vertex::with_attributes(
                    [normal.x * radius, normal.y * radius, normal.z * radius],
                    normal.to_array(),
                    [u, v],
                    [tangent.x, tangent.y, tangent.z, 1.0],
                    color,
                ));
            }
        }

        for latitude in 0..latitude_segments {
            for longitude in 0..longitude_segments {
                let row = longitude_segments + 1;
                let top_left = (latitude * row + longitude) as u16;
                let top_right = top_left + 1;
                let bottom_left = ((latitude + 1) * row + longitude) as u16;
                let bottom_right = bottom_left + 1;
                indices.extend([
                    top_left,
                    bottom_left,
                    top_right,
                    top_right,
                    bottom_left,
                    bottom_right,
                ]);
            }
        }

        Self { vertices, indices }
    }

    /// Creates a cylinder aligned to the Y axis and centered at the origin.
    ///
    /// i.e. The top face points toward `+Y` and the bottom face toward `-Y`.
    pub fn cylinder(radius: f32, height: f32, segments: usize, color: [f32; 4]) -> Self {
        let segments = segments.max(3);
        let half_height = height * 0.5;
        let mut vertices = Vec::with_capacity(segments * 12);
        let mut indices = Vec::with_capacity(segments * 12);

        for segment in 0..segments {
            let next = (segment + 1) % segments;
            let angle = segment as f32 / segments as f32 * std::f32::consts::TAU;
            let next_angle = next as f32 / segments as f32 * std::f32::consts::TAU;
            let u = segment as f32 / segments as f32;
            let next_u = (segment + 1) as f32 / segments as f32;
            push_quad_with_uv(
                &mut vertices,
                &mut indices,
                [
                    [radius * angle.cos(), -half_height, radius * angle.sin()],
                    [
                        radius * next_angle.cos(),
                        -half_height,
                        radius * next_angle.sin(),
                    ],
                    [
                        radius * next_angle.cos(),
                        half_height,
                        radius * next_angle.sin(),
                    ],
                    [radius * angle.cos(), half_height, radius * angle.sin()],
                ],
                [[u, 0.0], [next_u, 0.0], [next_u, 1.0], [u, 1.0]],
                color,
            );

            push_triangle_with_uv(
                &mut vertices,
                &mut indices,
                [
                    [0.0, half_height, 0.0],
                    [
                        radius * next_angle.cos(),
                        half_height,
                        radius * next_angle.sin(),
                    ],
                    [radius * angle.cos(), half_height, radius * angle.sin()],
                ],
                [
                    [0.5, 0.5],
                    [0.5 + next_angle.cos() * 0.5, 0.5 + next_angle.sin() * 0.5],
                    [0.5 + angle.cos() * 0.5, 0.5 + angle.sin() * 0.5],
                ],
                color,
            );
            push_triangle_with_uv(
                &mut vertices,
                &mut indices,
                [
                    [0.0, -half_height, 0.0],
                    [radius * angle.cos(), -half_height, radius * angle.sin()],
                    [
                        radius * next_angle.cos(),
                        -half_height,
                        radius * next_angle.sin(),
                    ],
                ],
                [
                    [0.5, 0.5],
                    [0.5 + angle.cos() * 0.5, 0.5 + angle.sin() * 0.5],
                    [0.5 + next_angle.cos() * 0.5, 0.5 + next_angle.sin() * 0.5],
                ],
                color,
            );
        }

        Self { vertices, indices }
    }

    /// Creates a triangular prism with a sloped top surface.
    ///
    /// The tall end is `+X` centered vertically toward `+Y`,
    /// meaning the wedge "points" or slopes toward `+X`.
    pub fn wedge(color: [f32; 4]) -> Self {
        let h = 0.5;
        let front_bottom_left = [-h, -h, -h];
        let front_bottom_right = [h, -h, -h];
        let front_top_right = [h, h, -h];
        let back_bottom_left = [-h, -h, h];
        let back_bottom_right = [h, -h, h];
        let back_top_right = [h, h, h];
        let mut vertices = Vec::with_capacity(18);
        let mut indices = Vec::with_capacity(24);

        push_triangle(
            &mut vertices,
            &mut indices,
            [front_bottom_left, front_bottom_right, front_top_right],
            color,
        );
        push_triangle(
            &mut vertices,
            &mut indices,
            [back_bottom_left, back_top_right, back_bottom_right],
            color,
        );
        push_quad(
            &mut vertices,
            &mut indices,
            [
                front_bottom_left,
                back_bottom_left,
                back_bottom_right,
                front_bottom_right,
            ],
            color,
        );
        push_quad(
            &mut vertices,
            &mut indices,
            [
                front_bottom_right,
                back_bottom_right,
                back_top_right,
                front_top_right,
            ],
            color,
        );
        push_quad(
            &mut vertices,
            &mut indices,
            [
                front_bottom_left,
                front_top_right,
                back_top_right,
                back_bottom_left,
            ],
            color,
        );

        Self { vertices, indices }
    }

    /// Creates a pyramid-like corner wedge with one high corner and four sloped sides.
    ///
    /// The apex direction from the center is `(-X, +Y, -Z)`.
    pub fn corner_wedge(color: [f32; 4]) -> Self {
        let h = 0.5;
        let corners = [[-h, -h, -h], [h, -h, -h], [h, -h, h], [-h, -h, h]];
        let apex = [-h, h, -h];
        let mut vertices = Vec::with_capacity(16);
        let mut indices = Vec::with_capacity(18);

        push_quad(
            &mut vertices,
            &mut indices,
            [corners[0], corners[3], corners[2], corners[1]],
            color,
        );
        for (index, next) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
            push_triangle(
                &mut vertices,
                &mut indices,
                [corners[index], corners[next], apex],
                color,
            );
        }

        Self { vertices, indices }
    }

    /// Creates a square on the XZ plane, centered at the origin.
    pub fn plane(size: f32, color: [f32; 4]) -> Self {
        let h = size * 0.5;
        Self {
            vertices: vec![
                Vertex::with_attributes(
                    [-h, 0.0, -h],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                ),
                Vertex::with_attributes(
                    [h, 0.0, -h],
                    [0.0, 1.0, 0.0],
                    [1.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                ),
                Vertex::with_attributes(
                    [h, 0.0, h],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                ),
                Vertex::with_attributes(
                    [-h, 0.0, h],
                    [0.0, 1.0, 0.0],
                    [0.0, 1.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                ),
            ],
            indices: vec![0, 1, 2, 2, 3, 0],
        }
    }
}

fn push_triangle(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 3],
    color: [f32; 4],
) {
    push_triangle_with_uv(
        vertices,
        indices,
        positions,
        [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        color,
    );
}

fn push_triangle_with_uv(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 3],
    uvs: [[f32; 2]; 3],
    color: [f32; 4],
) {
    push_triangle_with_uv_and_material_slot(
        vertices,
        indices,
        positions,
        uvs,
        color,
        MaterialSlot::Base,
    );
}

fn push_triangle_with_uv_and_material_slot(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 3],
    uvs: [[f32; 2]; 3],
    color: [f32; 4],
    material_slot: MaterialSlot,
) {
    let normal = triangle_normal(positions);
    let tangent = tangent_from_uv(positions, uvs, normal);
    let start = vertices.len() as u16;
    vertices.extend(positions.into_iter().zip(uvs).map(|(position, uv)| {
        Vertex::with_material_slot(position, normal, uv, tangent, color, material_slot)
    }));
    indices.extend([start, start + 1, start + 2]);
}

fn push_quad(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 4],
    color: [f32; 4],
) {
    push_quad_with_uv(
        vertices,
        indices,
        positions,
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color,
    );
}

fn push_quad_with_uv(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
    color: [f32; 4],
) {
    push_quad_with_uv_and_material_slot(
        vertices,
        indices,
        positions,
        uvs,
        color,
        MaterialSlot::Base,
    );
}

fn push_quad_with_material_slot(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 4],
    color: [f32; 4],
    material_slot: MaterialSlot,
) {
    push_quad_with_uv_and_material_slot(
        vertices,
        indices,
        positions,
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        color,
        material_slot,
    );
}

fn push_quad_with_uv_and_material_slot(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    positions: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
    color: [f32; 4],
    material_slot: MaterialSlot,
) {
    let normal = triangle_normal([positions[0], positions[1], positions[2]]);
    let tangent = tangent_from_uv(
        [positions[0], positions[1], positions[2]],
        [uvs[0], uvs[1], uvs[2]],
        normal,
    );
    let start = vertices.len() as u16;
    vertices.extend(positions.into_iter().zip(uvs).map(|(position, uv)| {
        Vertex::with_material_slot(position, normal, uv, tangent, color, material_slot)
    }));
    indices.extend([start, start + 1, start + 2, start + 2, start + 3, start]);
}

fn triangle_normal(positions: [[f32; 3]; 3]) -> [f32; 3] {
    let edge_a = Vec3::from_array(positions[1]) - Vec3::from_array(positions[0]);
    let edge_b = Vec3::from_array(positions[2]) - Vec3::from_array(positions[0]);
    edge_a.cross(edge_b).normalize_or_zero().to_array()
}

fn tangent_from_uv(positions: [[f32; 3]; 3], uvs: [[f32; 2]; 3], normal: [f32; 3]) -> [f32; 4] {
    let position_a = Vec3::from_array(positions[1]) - Vec3::from_array(positions[0]);
    let position_b = Vec3::from_array(positions[2]) - Vec3::from_array(positions[0]);
    let uv_a = Vec3::new(uvs[1][0] - uvs[0][0], uvs[1][1] - uvs[0][1], 0.0);
    let uv_b = Vec3::new(uvs[2][0] - uvs[0][0], uvs[2][1] - uvs[0][1], 0.0);
    let determinant = uv_a.x * uv_b.y - uv_a.y * uv_b.x;
    let tangent = if determinant.abs() > f32::EPSILON {
        (position_a * uv_b.y - position_b * uv_a.y) / determinant
    } else {
        position_a
    };
    let normal = Vec3::from_array(normal);
    let tangent = (tangent - normal * normal.dot(tangent)).normalize_or_zero();
    let bitangent = position_a * uv_b.x - position_b * uv_a.x;
    let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
        -1.0
    } else {
        1.0
    };
    [tangent.x, tangent.y, tangent.z, handedness]
}

/// A handle to mesh data stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshHandle(usize);

/// A handle to a texture stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureHandle(usize);

/// The color space used when sampling an RGBA8 texture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextureColorSpace {
    /// Color data such as a base-color map. The GPU converts it to linear space.
    Srgb,
    /// Data maps such as normals and metallic-roughness.
    Linear,
}

/// CPU-side image data independent of GPU texture resources.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// Decodes any image format supported by the configured image codecs.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        let image = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = image.dimensions();
        Self::from_rgba8(width, height, image.into_raw())
    }

    /// Creates an image from RGBA8 pixels.
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        let image = Self {
            width,
            height,
            pixels,
        };
        image.validate()?;
        Ok(image)
    }

    fn validate(&self) -> Result<(), TextureError> {
        if self.width == 0 || self.height == 0 {
            return Err(TextureError::ZeroDimensions);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(TextureError::DimensionsTooLarge)?;
        if self.pixels.len() != expected {
            return Err(TextureError::InvalidData {
                actual: self.pixels.len(),
                expected,
            });
        }
        Ok(())
    }
}

/// CPU-side RGBA8 texture data ready to be uploaded to a [`Renderer`].
///
/// Textures are intentionally single-mip resources for now. The renderer does
/// not generate or upload additional mip levels.
#[derive(Clone, Debug)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub color_space: TextureColorSpace,
}

#[derive(Debug, Error)]
pub enum TextureError {
    #[error("could not decode image data: {0}")]
    ImageDecode(#[from] image::ImageError),
    #[error("texture dimensions must be greater than zero")]
    ZeroDimensions,
    #[error("texture dimensions are too large")]
    DimensionsTooLarge,
    #[error("RGBA8 texture data has {actual} bytes, expected {expected}")]
    InvalidData { actual: usize, expected: usize },
}

impl Texture {
    /// Creates an sRGB RGBA8 texture, suitable for a base-color map.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        Self::with_color_space(width, height, pixels, TextureColorSpace::Srgb)
    }

    /// Creates an RGBA8 texture with an explicit color space.
    pub fn with_color_space(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        color_space: TextureColorSpace,
    ) -> Result<Self, TextureError> {
        let texture = Self {
            width,
            height,
            pixels,
            color_space,
        };
        texture.validate()?;
        Ok(texture)
    }

    /// Creates a texture from decoded image data.
    pub fn from_image(image: Image, color_space: TextureColorSpace) -> Result<Self, TextureError> {
        Self::with_color_space(image.width, image.height, image.pixels, color_space)
    }

    /// Creates a linear RGBA8 texture, suitable for normal or metallic-roughness data.
    pub fn linear(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        Self::with_color_space(width, height, pixels, TextureColorSpace::Linear)
    }

    /// Decodes an image from memory with an explicit texture color space.
    pub fn from_bytes(bytes: &[u8], color_space: TextureColorSpace) -> Result<Self, TextureError> {
        Self::from_image(Image::from_bytes(bytes)?, color_space)
    }

    fn validate(&self) -> Result<(), TextureError> {
        if self.width == 0 || self.height == 0 {
            return Err(TextureError::ZeroDimensions);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(TextureError::DimensionsTooLarge)?;
        if self.pixels.len() != expected {
            return Err(TextureError::InvalidData {
                actual: self.pixels.len(),
                expected,
            });
        }
        Ok(())
    }
}

/// The material slot selected by a mesh vertex.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterialSlot {
    Base = 0,
    Top = 1,
    Bottom = 2,
    Side = 3,
}

impl MaterialSlot {
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Texture maps used by a PBR material.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureSet {
    pub base_color: Option<TextureHandle>,
    pub normal: Option<TextureHandle>,
    pub metallic_roughness: Option<TextureHandle>,
}

/// PBR factors and optional texture maps used by a [`Part`].
///
/// The metallic-roughness map follows the glTF convention: metallic is read
/// from the blue channel and roughness from the green channel. All scalar
/// factors are supplied per instance, so parts can remain instanced efficiently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub textures: TextureSet,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
            textures: TextureSet::default(),
        }
    }
}

impl Material {
    /// Creates a material using a texture as its base-color map.
    pub fn textured(texture: TextureHandle) -> Self {
        Self::default().with_base_color_texture(texture)
    }

    pub fn with_base_color_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.base_color = Some(texture);
        self
    }

    pub fn with_normal_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.normal = Some(texture);
        self
    }

    pub fn with_metallic_roughness_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.metallic_roughness = Some(texture);
        self
    }

    pub fn from_color(color: Color3) -> Self {
        Self {
            base_color: color.rgba(),
            ..Self::default()
        }
    }
}

/// Materials assigned to the non-default slots of a mesh.
///
/// Slot zero is the [`Part::material`] field. The first element in this list
/// is slot one, the second is slot two, and so on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshMaterialSlots {
    pub slots: Vec<Material>,
}

impl MeshMaterialSlots {
    pub fn set(&mut self, slot: MaterialSlot, material: Material) {
        assert!(slot != MaterialSlot::Base, "slot zero is Part::material");
        let index = slot.index() - 1;
        self.slots.resize(index + 1, Material::default());
        self.slots[index] = material;
    }
}

#[derive(Clone, Debug)]
pub struct Part {
    basepart: BasePart,
    pub shape: PartShape,
    pub material: Material,
    pub material_slots: MeshMaterialSlots,
}

impl Part {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            basepart: BasePart::new(name),
            shape: PartShape::Block,
            material: Material::default(),
            material_slots: MeshMaterialSlots::default(),
        }
    }

    /// Assigns a material to a mesh-selected slot.
    pub fn set_material_slot(&mut self, slot: MaterialSlot, material: Material) {
        if slot == MaterialSlot::Base {
            self.material = material;
        } else {
            self.material_slots.set(slot, material);
        }
    }
}

impl Deref for Part {
    type Target = BasePart;
    fn deref(&self) -> &Self::Target {
        &self.basepart
    }
}

impl DerefMut for Part {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.basepart
    }
}

/// Stable identifier for an [`Instance`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstanceId(usize);

/// The 3D container that owns all renderable [`Part`] instances and its active camera.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    children: Vec<Part>,
    /// The camera used when this workspace is rendered.
    pub current_camera: Camera,
    camera_controller: CameraController,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates and parents a new Part to this workspace.
    pub fn create_part(&mut self, name: impl Into<String>) -> InstanceId {
        self.add_part(Part::new(name))
    }

    pub fn create_part_with(
        &mut self,
        name: impl Into<String>,
        configure: impl FnOnce(&mut Part),
    ) -> InstanceId {
        let mut part = Part::new(name);
        configure(&mut part);
        self.add_part(part)
    }

    pub fn add_part(&mut self, part: Part) -> InstanceId {
        let id = InstanceId(self.children.len());
        self.children.push(part);
        id
    }

    pub fn add_parts<I>(&mut self, parts: I) -> Vec<InstanceId>
    where
        I: IntoIterator<Item = Part>,
    {
        parts.into_iter().map(|part| self.add_part(part)).collect()
    }

    pub fn part(&self, id: InstanceId) -> Option<&Part> {
        self.children.get(id.0)
    }

    pub fn part_mut(&mut self, id: InstanceId) -> Option<&mut Part> {
        self.children.get_mut(id.0)
    }

    pub fn find_first_child(&mut self, name: &str) -> Option<(InstanceId, &mut Part)> {
        self.children
            .iter_mut()
            .position(|part| part.name == name)
            .map(|index| (InstanceId(index), &mut self.children[index]))
    }

    pub fn parts(&self) -> &[Part] {
        &self.children
    }

    pub fn update_camera(&mut self, delta: f32) {
        self.camera_controller
            .update_camera(&mut self.current_camera, delta);
    }

    pub fn process_device_event(&mut self, event: &winit::event::DeviceEvent) {
        self.camera_controller.process_device_event(event);
    }

    pub fn process_window_event(&mut self, event: &winit::event::WindowEvent) {
        self.camera_controller.process_window_event(event);
    }
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialTextures {
    base_color: [TextureHandle; MATERIAL_SLOT_COUNT],
    normal: [TextureHandle; MATERIAL_SLOT_COUNT],
    metallic_roughness: [TextureHandle; MATERIAL_SLOT_COUNT],
}

struct RenderBatch {
    shape: PartShape,
    textures: MaterialTextures,
    instances: Vec<InstanceRaw>,
    instance_start: usize,
}

struct GpuTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// The wgpu state and built-in PBR mesh pipeline.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
    material_sampler: wgpu::Sampler,
    textures: Vec<GpuTexture>,
    default_material_textures: MaterialTextures,
    instance_buffer: wgpu::Buffer,
    instance_data: Vec<InstanceRaw>,
    meshes: Vec<GpuMesh>,
    primitive_meshes: [MeshHandle; PartShape::COUNT],
    clear_color: wgpu::Color,
    last_frame: Instant,
    fps_timer: Instant,
    frame_count: u32,
    fps: f32,
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
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        log::info!("selected wgpu adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("terrarium device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
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

        let (depth_texture, depth_view) = create_depth_texture(&device, &config);
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
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let mut material_bind_group_entries = Vec::with_capacity(MATERIAL_SLOT_COUNT * 3 + 1);
        for binding in 0..(MATERIAL_SLOT_COUNT * 3) {
            material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
            binding: (MATERIAL_SLOT_COUNT * 3) as u32,
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
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let default_base_color = Texture::new(1, 1, vec![255, 255, 255, 255])?;
        let default_normal = Texture::linear(1, 1, vec![128, 128, 255, 255])?;
        let default_metallic_roughness = Texture::linear(1, 1, vec![0, 255, 0, 255])?;
        let textures = vec![
            upload_texture(&device, &queue, &default_base_color),
            upload_texture(&device, &queue, &default_normal),
            upload_texture(&device, &queue, &default_metallic_roughness),
        ];
        let default_material_textures = MaterialTextures {
            base_color: [TextureHandle(0); MATERIAL_SLOT_COUNT],
            normal: [TextureHandle(1); MATERIAL_SLOT_COUNT],
            metallic_roughness: [TextureHandle(2); MATERIAL_SLOT_COUNT],
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PBR mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
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
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout()), Some(InstanceRaw::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
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

        let mut renderer = Self {
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            pipeline,
            camera_buffer,
            camera_bind_group,
            material_bind_group_layout,
            material_sampler,
            textures,
            default_material_textures,
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
        };
        for shape in PartShape::ALL {
            let mesh = renderer.add_mesh(&shape.mesh([1.0; 4]))?;
            renderer.primitive_meshes[shape.index()] = mesh;
        }
        Ok(renderer)
    }

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

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth_texture, self.depth_view) = create_depth_texture(&self.device, &self.config);
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

    /// Uploads a single-mip RGBA8 texture and returns its GPU handle.
    ///
    /// Mipmap generation is intentionally not performed yet. Use sRGB data for
    /// base-color textures and linear data for normal or metallic-roughness maps.
    pub fn add_texture(&mut self, texture: &Texture) -> Result<TextureHandle, RendererError> {
        texture.validate()?;
        let handle = TextureHandle(self.textures.len());
        self.textures
            .push(upload_texture(&self.device, &self.queue, texture));
        Ok(handle)
    }

    fn material_textures(
        &self,
        material: &Material,
        material_slots: &MeshMaterialSlots,
    ) -> MaterialTextures {
        let resolve = |handle: Option<TextureHandle>, fallback: TextureHandle| {
            handle
                .filter(|handle| self.textures.get(handle.0).is_some())
                .unwrap_or(fallback)
        };
        let base_color = resolve(
            material.textures.base_color,
            self.default_material_textures.base_color[0],
        );
        let normal = resolve(
            material.textures.normal,
            self.default_material_textures.normal[0],
        );
        let metallic_roughness = resolve(
            material.textures.metallic_roughness,
            self.default_material_textures.metallic_roughness[0],
        );
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
            textures.base_color[slot] = resolve(material.textures.base_color, base_color);
            textures.normal[slot] = resolve(material.textures.normal, normal);
            textures.metallic_roughness[slot] =
                resolve(material.textures.metallic_roughness, metallic_roughness);
        }
        textures
    }

    fn create_material_bind_group(&self, textures: MaterialTextures) -> wgpu::BindGroup {
        let mut entries = Vec::with_capacity(MATERIAL_SLOT_COUNT * 3 + 1);
        for (slot, texture) in textures.base_color.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: slot as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[texture.0].view),
            });
        }
        for (slot, texture) in textures.normal.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: (MATERIAL_SLOT_COUNT + slot) as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[texture.0].view),
            });
        }
        for (slot, texture) in textures.metallic_roughness.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: (MATERIAL_SLOT_COUNT * 2 + slot) as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[texture.0].view),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: (MATERIAL_SLOT_COUNT * 3) as u32,
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

    /// Renders a workspace using its current camera. A lost or outdated surface is reconfigured
    /// and retried on the next frame; minimized and occluded windows simply skip their frame.
    pub fn render(&mut self, workspace: &Workspace) -> Result<(), RendererError> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RendererError::SurfaceValidation);
            }
        };

        let camera_uniform = CameraUniform {
            view_projection: workspace
                .current_camera
                .view_projection_matrix()
                .to_cols_array_2d(),
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
        let parts = workspace.parts();
        let mut batches = Vec::<RenderBatch>::new();
        let mut batch_indices = HashMap::new();
        for part in parts {
            let textures = self.material_textures(&part.material, &part.material_slots);
            let key = (part.shape, textures);
            let batch_index = if let Some(&batch_index) = batch_indices.get(&key) {
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
            };
            let model = part.transform();
            let normal_matrix = model.inverse().transpose().to_cols_array_2d();
            let material = part.material;
            let tint = part.color.rgba();
            batches[batch_index].instances.push(InstanceRaw {
                model: model.to_cols_array_2d(),
                normal_0: [
                    normal_matrix[0][0],
                    normal_matrix[0][1],
                    normal_matrix[0][2],
                    0.0,
                ],
                normal_1: [
                    normal_matrix[1][0],
                    normal_matrix[1][1],
                    normal_matrix[1][2],
                    0.0,
                ],
                normal_2: [
                    normal_matrix[2][0],
                    normal_matrix[2][1],
                    normal_matrix[2][2],
                    0.0,
                ],
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

        let material_bind_groups = batches
            .iter()
            .map(|batch| self.create_material_bind_group(batch.textures))
            .collect::<Vec<_>>();

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
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: &self.depth_view,
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
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene render pass"),
                color_attachments: &[Some(color_attachment)],
                depth_stencil_attachment: Some(depth_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for (batch, material_bind_group) in batches.iter().zip(&material_bind_groups) {
                if batch.instances.is_empty() {
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
                    + batch.instances.len() as u64 * std::mem::size_of::<InstanceRaw>() as u64;
                pass.set_vertex_buffer(1, self.instance_buffer.slice(instance_start..instance_end));
                pass.set_bind_group(1, material_bind_group, &[]);
                pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed;
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
        Ok(())
    }
}

fn upload_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &Texture) -> GpuTexture {
    let format = match texture.color_space {
        TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
    };
    let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        // Mipmaps are deliberately deferred; every material texture has one level for now.
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texture.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width * 4),
            rows_per_image: Some(texture.height),
        },
        wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
    );
    let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
    GpuTexture {
        _texture: gpu_texture,
        view,
    }
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
    fn part_position_and_orientation_accessors_use_the_pivot() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let orientation = Vec3::new(10.0, 20.0, 30.0);
        let mut part = Part::new("part");

        part.set_position(position);
        part.set_orientation(orientation);

        assert_eq!(part.position(), position);
        assert!((part.orientation() - orientation).abs().max_element() < 0.0001);
    }

    #[test]
    fn camera_mouse_look_rotates_in_place_with_the_expected_horizontal_sign() {
        let mut camera = Camera::default();
        let original_position = camera.pivot().w_axis.truncate();
        let mut controller = CameraController::new(6.0, 0.1);
        controller.mouse_delta = (1.0, 0.0);

        controller.update_camera(&mut camera, 1.0 / 60.0);

        assert_eq!(camera.pivot().w_axis.truncate(), original_position);
        assert!(camera.forward().x > 0.0);
    }

    #[test]
    fn primitive_meshes_have_valid_tangent_space_attributes() {
        let meshes = [
            Mesh::block(1.0, [1.0; 4]),
            Mesh::ball(1.0, 4, 8, [1.0; 4]),
            Mesh::cylinder(1.0, 1.0, 8, [1.0; 4]),
            Mesh::wedge([1.0; 4]),
            Mesh::corner_wedge([1.0; 4]),
            Mesh::plane(1.0, [1.0; 4]),
        ];

        for mesh in meshes {
            assert!(!mesh.vertices.is_empty());
            for vertex in mesh.vertices {
                let normal = Vec3::from_array(vertex.normal);
                let tangent = Vec3::from_array(vertex.tangent[..3].try_into().unwrap());
                assert!((normal.length() - 1.0).abs() < 0.0001);
                assert!((tangent.length() - 1.0).abs() < 0.0001);
                assert!(normal.dot(tangent).abs() < 0.0001);
                assert!(vertex.uv.iter().all(|coordinate| coordinate.is_finite()));
                assert!(vertex.tangent[3] == 1.0 || vertex.tangent[3] == -1.0);
            }
        }
    }

    #[test]
    fn textures_validate_rgba8_data_and_material_defaults_are_rough_dielectrics() {
        assert!(matches!(
            Texture::new(2, 2, vec![0; 3]),
            Err(TextureError::InvalidData {
                actual: 3,
                expected: 16
            })
        ));
        let texture = Texture::linear(1, 1, vec![128, 128, 255, 255]).unwrap();
        assert_eq!(texture.color_space, TextureColorSpace::Linear);

        let material = Material::default();
        assert_eq!(material.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(material.metallic, 0.0);
        assert_eq!(material.roughness, 0.5);
        assert!(material.textures.base_color.is_none());
        assert!(material.textures.normal.is_none());
        assert!(material.textures.metallic_roughness.is_none());
    }

    #[test]
    fn block_mesh_uses_named_material_slots() {
        let mesh = Mesh::block(1.0, [1.0; 4]);

        assert!(
            mesh.vertices[0..4]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Side as u32)
        );
        assert!(
            mesh.vertices[8..12]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Top as u32)
        );
        assert!(
            mesh.vertices[12..16]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Bottom as u32)
        );
    }

    #[test]
    fn material_helpers_configure_a_texture_set() {
        let texture = TextureHandle(7);
        let material = Material::textured(texture)
            .with_normal_texture(TextureHandle(8))
            .with_metallic_roughness_texture(TextureHandle(9));

        assert_eq!(
            material.textures,
            TextureSet {
                base_color: Some(texture),
                normal: Some(TextureHandle(8)),
                metallic_roughness: Some(TextureHandle(9)),
            }
        );
    }
}
