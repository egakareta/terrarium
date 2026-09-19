use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use thiserror::Error;

use crate::{
    BasePart, DEFAULT_MATERIAL, Face, MATERIAL_SLOT_COUNT, Material, Mesh, MeshMaterialSlots,
    Texture, TextureColorSpace, TextureError, TextureFilter, TextureHandle, Vertex, Workspace,
    glam::{Mat4, Vec2, Vec3},
};

/// Errors produced while importing a glTF asset as a [`MeshPart`].
#[derive(Debug, Error)]
pub enum GltfError {
    /// The glTF document, its buffers, or one of its images could not be decoded.
    #[error("could not import glTF data: {0}")]
    Import(#[from] gltf::Error),
    /// The document did not define a default or fallback scene.
    #[error("glTF document contains no scene")]
    NoScene,
    /// The selected scene did not contain any mesh geometry.
    #[error("glTF scene contains no mesh geometry")]
    NoMesh,
    /// A mesh primitive did not contain the required `POSITION` attribute.
    #[error("glTF mesh {mesh} primitive {primitive} has no POSITION attribute")]
    MissingPositions {
        /// Zero-based glTF mesh index.
        mesh: usize,
        /// Zero-based primitive index within the mesh.
        primitive: usize,
    },
    /// Flattened geometry exceeded the renderer's 16-bit index range.
    #[error("glTF scene has {count} vertices; MeshPart supports at most 65536")]
    TooManyVertices {
        /// Number of vertices required by the flattened scene.
        count: usize,
    },
    /// The asset used more materials than the built-in shader can bind at once.
    #[error("glTF scene uses {count} materials; MeshPart supports at most {MATERIAL_SLOT_COUNT}")]
    TooManyMaterials {
        /// Number of materials used by scene primitives.
        count: usize,
    },
    /// A primitive index referred past the end of its position attribute.
    #[error("glTF primitive index {index} is outside its {vertex_count}-vertex range")]
    InvalidIndex {
        /// Invalid primitive-local index.
        index: u32,
        /// Number of vertices in the primitive.
        vertex_count: usize,
    },
    /// A triangle-list primitive had an incomplete final triangle.
    #[error("glTF triangle primitive has {count} indices, which is not divisible by three")]
    InvalidTriangleCount {
        /// Number of source indices.
        count: usize,
    },
    /// A point or line primitive cannot be represented by the triangle renderer.
    #[error("glTF primitive mode {mode:?} is not supported")]
    UnsupportedPrimitiveMode {
        /// Unsupported glTF primitive mode.
        mode: gltf::mesh::Mode,
    },
    /// A material sampled a UV set other than the single set supported by [`Vertex`].
    #[error("glTF material uses TEXCOORD_{set}; MeshPart currently supports TEXCOORD_0")]
    UnsupportedTextureCoordinates {
        /// Unsupported texture-coordinate set index.
        set: u32,
    },
    /// An imported image could not be represented as a workspace texture.
    #[error("could not create glTF texture: {0}")]
    Texture(#[from] TextureError),
}

/// A visible scene object backed by caller-provided or imported mesh data.
///
/// `MeshPart` dereferences to [`BasePart`], so its hierarchy, transform, size,
/// tint, and collision metadata use the same API as [`crate::Part`].
#[derive(Clone, Debug)]
pub struct MeshPart {
    basepart: BasePart,
    mesh: MeshHandle,
    mesh_revision: u64,
    bounding_radius: f32,
    pub(crate) material_slots: MeshMaterialSlots,
}

/// A mesh source accepted by [`crate::Workspace::add_mesh`].
pub enum MeshSource<'a> {
    /// Caller-provided CPU-side geometry.
    Data(Mesh),
    /// A glTF 2.0 document, including embedded images and buffers.
    Gltf(&'a [u8]),
}

impl<'a> From<Mesh> for MeshSource<'a> {
    fn from(mesh: Mesh) -> Self {
        Self::Data(mesh)
    }
}

impl<'a> From<&'a [u8]> for MeshSource<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::Gltf(bytes)
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for MeshSource<'a> {
    fn from(bytes: &'a [u8; N]) -> Self {
        Self::Gltf(bytes)
    }
}

#[derive(Clone, Debug)]
struct MeshAsset {
    mesh: Mesh,
    material_slots: MeshMaterialSlots,
}

/// A handle to a mesh asset stored in a [`crate::Workspace`].
///
/// Cloning the handle is cheap and creates another independent [`MeshPart`]
/// variant when passed to [`MeshPart::new`].
#[derive(Clone, Debug)]
pub struct MeshHandle(Arc<MeshAsset>);

impl MeshHandle {
    fn from_parts(mesh: Mesh, material_slots: MeshMaterialSlots) -> Self {
        Self(Arc::new(MeshAsset {
            mesh,
            material_slots,
        }))
    }

    /// Returns the CPU-side geometry in this mesh asset.
    pub fn mesh(&self) -> &Mesh {
        &self.0.mesh
    }

    pub(crate) fn same_asset(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl From<Mesh> for MeshHandle {
    fn from(mesh: Mesh) -> Self {
        Self::from_parts(mesh, MeshMaterialSlots::default())
    }
}

impl MeshPart {
    /// Creates a visible mesh part from a registered mesh asset or raw geometry.
    ///
    /// Passing a cloned [`MeshHandle`] creates an independent part variant that
    /// shares the underlying mesh data while retaining its own transform and
    /// material overrides.
    pub fn new<M>(mesh: M) -> Self
    where
        M: Into<MeshHandle>,
    {
        let mesh = mesh.into();
        let bounding_radius = mesh_bounding_radius(mesh.mesh());
        let material_slots = mesh.0.material_slots.clone();
        Self {
            basepart: <BasePart as crate::Instance>::named(BasePart::new(), "MeshPart"),
            mesh,
            mesh_revision: 0,
            bounding_radius,
            material_slots,
        }
    }

    pub(crate) fn import_gltf(
        bytes: &[u8],
        workspace: &mut Workspace,
    ) -> Result<MeshHandle, GltfError> {
        let (document, buffers, images) = gltf::import_slice(bytes)?;
        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .ok_or(GltfError::NoScene)?;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut material_keys = Vec::new();

        for node in scene.nodes() {
            append_node(
                node,
                Mat4::IDENTITY,
                &buffers,
                &mut vertices,
                &mut indices,
                &mut material_keys,
            )?;
        }
        if vertices.is_empty() || indices.is_empty() {
            return Err(GltfError::NoMesh);
        }

        let mut texture_cache = HashMap::new();
        let mut materials = Vec::with_capacity(material_keys.len());
        for key in material_keys {
            let source = key.and_then(|index| document.materials().nth(index));
            materials.push(import_material(
                source,
                &images,
                workspace,
                &mut texture_cache,
            )?);
        }

        let mut material_slots = MeshMaterialSlots::default();
        for (slot, material) in Face::ALL.into_iter().zip(materials) {
            material_slots.set(slot, material);
        }
        Ok(MeshHandle::from_parts(
            Mesh::new(vertices, indices),
            material_slots,
        ))
    }

    /// Returns the CPU-side geometry rendered by this mesh part.
    pub fn mesh(&self) -> &Mesh {
        self.mesh.mesh()
    }

    /// Returns the shared mesh asset used by this part.
    pub fn mesh_handle(&self) -> &MeshHandle {
        &self.mesh
    }

    /// Replaces the geometry and schedules it for upload before the next frame.
    pub fn set_mesh(&mut self, mesh: Mesh) {
        self.bounding_radius = mesh_bounding_radius(&mesh);
        self.mesh = MeshHandle::from_parts(mesh, self.material_slots.clone());
        self.mesh_revision = self.mesh_revision.wrapping_add(1);
    }

    /// Assigns the same material to all six directional slots.
    pub fn set_material(&mut self, material: Material) {
        for slot in Face::ALL {
            self.set_material_slot(slot, material);
        }
    }

    /// Assigns a material to a mesh-selected directional slot.
    pub fn set_material_slot(&mut self, slot: Face, material: Material) {
        self.material_slots.set(slot, material);
    }

    /// Returns the material if all directional slots have the same effective material.
    ///
    /// Unassigned slots use [`Material::default()`] when compared.
    pub fn material(&self) -> Option<&Material> {
        let material = self.material_slot(Face::ALL[0]);
        if Face::ALL
            .into_iter()
            .skip(1)
            .all(|slot| self.material_slot(slot) == material)
        {
            Some(material)
        } else {
            None
        }
    }

    /// Returns the effective material for a mesh slot.
    pub fn material_slot(&self, slot: Face) -> &Material {
        self.material_slots.get(slot).unwrap_or(&DEFAULT_MATERIAL)
    }

    pub(crate) fn mesh_revision(&self) -> u64 {
        self.mesh_revision
    }

    pub(crate) fn bounding_radius(&self) -> f32 {
        self.bounding_radius
    }
}

crate::impl_instance!(MeshPart, class_name = "MeshPart", data = basepart.instance,);

impl Deref for MeshPart {
    type Target = BasePart;

    fn deref(&self) -> &Self::Target {
        &self.basepart
    }
}

impl DerefMut for MeshPart {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.basepart
    }
}

fn mesh_bounding_radius(mesh: &Mesh) -> f32 {
    mesh.vertices
        .iter()
        .map(|vertex| Vec3::from_array(vertex.position).length())
        .fold(0.0, f32::max)
}

fn append_node(
    node: gltf::Node<'_>,
    parent_transform: Mat4,
    buffers: &[gltf::buffer::Data],
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    material_keys: &mut Vec<Option<usize>>,
) -> Result<(), GltfError> {
    let local_transform = Mat4::from_cols_array_2d(&node.transform().matrix());
    let transform = parent_transform * local_transform;
    if let Some(mesh) = node.mesh() {
        let mesh_index = mesh.index();
        for primitive in mesh.primitives() {
            append_primitive(
                primitive,
                mesh_index,
                transform,
                buffers,
                vertices,
                indices,
                material_keys,
            )?;
        }
    }
    for child in node.children() {
        append_node(child, transform, buffers, vertices, indices, material_keys)?;
    }
    Ok(())
}

fn append_primitive(
    primitive: gltf::Primitive<'_>,
    mesh_index: usize,
    transform: Mat4,
    buffers: &[gltf::buffer::Data],
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
    material_keys: &mut Vec<Option<usize>>,
) -> Result<(), GltfError> {
    let reader = primitive.reader(|buffer| Some(buffers[buffer.index()].0.as_slice()));
    let positions: Vec<Vec3> = reader
        .read_positions()
        .ok_or(GltfError::MissingPositions {
            mesh: mesh_index,
            primitive: primitive.index(),
        })?
        .map(|position| transform.transform_point3(Vec3::from_array(position)))
        .collect();
    let vertex_count = positions.len();
    let total_vertices = vertices.len() + vertex_count;
    if total_vertices > u16::MAX as usize + 1 {
        return Err(GltfError::TooManyVertices {
            count: total_vertices,
        });
    }

    let source_indices: Vec<u32> = reader
        .read_indices()
        .map(|indices| indices.into_u32().collect())
        .unwrap_or_else(|| (0..vertex_count as u32).collect());
    let mut primitive_indices = triangulate(primitive.mode(), &source_indices)?;
    for &index in &primitive_indices {
        if index as usize >= vertex_count {
            return Err(GltfError::InvalidIndex {
                index,
                vertex_count,
            });
        }
    }
    let reflected = transform.determinant() < 0.0;
    if reflected {
        for triangle in primitive_indices.chunks_exact_mut(3) {
            triangle.swap(1, 2);
        }
    }

    let normal_matrix = transform.inverse().transpose();
    let normals = reader.read_normals().map_or_else(
        || generate_normals(&positions, &primitive_indices),
        |normals| {
            normals
                .map(|normal| {
                    normal_matrix
                        .transform_vector3(Vec3::from_array(normal))
                        .normalize_or(Vec3::Y)
                })
                .collect()
        },
    );
    let uvs: Vec<Vec2> = reader
        .read_tex_coords(0)
        .map(|coordinates| coordinates.into_f32().map(Vec2::from_array).collect())
        .unwrap_or_else(|| vec![Vec2::ZERO; vertex_count]);
    let colors: Vec<[f32; 4]> = reader
        .read_colors(0)
        .map(|colors| colors.into_rgba_f32().collect())
        .unwrap_or_else(|| vec![[1.0; 4]; vertex_count]);
    let tangents: Vec<[f32; 4]> = reader.read_tangents().map_or_else(
        || generate_tangents(&positions, &normals, &uvs, &primitive_indices),
        |tangents| {
            tangents
                .enumerate()
                .map(|(index, tangent)| {
                    let normal = normals[index];
                    let direction = transform
                        .transform_vector3(Vec3::from_array(tangent[..3].try_into().unwrap()));
                    let direction = (direction - normal * normal.dot(direction))
                        .normalize_or(fallback_tangent(normal));
                    [
                        direction.x,
                        direction.y,
                        direction.z,
                        if reflected { -tangent[3] } else { tangent[3] },
                    ]
                })
                .collect()
        },
    );

    let material_key = primitive.material().index();
    let material_slot =
        if let Some(index) = material_keys.iter().position(|key| *key == material_key) {
            index
        } else {
            if material_keys.len() == MATERIAL_SLOT_COUNT {
                return Err(GltfError::TooManyMaterials {
                    count: material_keys.len() + 1,
                });
            }
            material_keys.push(material_key);
            material_keys.len() - 1
        } as u32;

    let base_vertex = vertices.len() as u32;
    vertices.extend((0..vertex_count).map(|index| Vertex {
        position: positions[index].to_array(),
        normal: normals[index].to_array(),
        uv: uvs[index].to_array(),
        tangent: tangents[index],
        color: colors[index],
        material_slot,
    }));
    indices.extend(
        primitive_indices
            .into_iter()
            .map(|index| (base_vertex + index) as u16),
    );
    Ok(())
}

fn triangulate(mode: gltf::mesh::Mode, source: &[u32]) -> Result<Vec<u32>, GltfError> {
    match mode {
        gltf::mesh::Mode::Triangles => {
            if !source.len().is_multiple_of(3) {
                return Err(GltfError::InvalidTriangleCount {
                    count: source.len(),
                });
            }
            Ok(source.to_vec())
        }
        gltf::mesh::Mode::TriangleStrip => {
            let mut indices = Vec::with_capacity(source.len().saturating_sub(2) * 3);
            for index in 2..source.len() {
                let triangle = if index % 2 == 0 {
                    [source[index - 2], source[index - 1], source[index]]
                } else {
                    [source[index - 1], source[index - 2], source[index]]
                };
                if triangle[0] != triangle[1]
                    && triangle[1] != triangle[2]
                    && triangle[2] != triangle[0]
                {
                    indices.extend(triangle);
                }
            }
            Ok(indices)
        }
        gltf::mesh::Mode::TriangleFan => {
            let mut indices = Vec::with_capacity(source.len().saturating_sub(2) * 3);
            for index in 2..source.len() {
                let triangle = [source[0], source[index - 1], source[index]];
                if triangle[0] != triangle[1]
                    && triangle[1] != triangle[2]
                    && triangle[2] != triangle[0]
                {
                    indices.extend(triangle);
                }
            }
            Ok(indices)
        }
        mode => Err(GltfError::UnsupportedPrimitiveMode { mode }),
    }
}

fn generate_normals(positions: &[Vec3], indices: &[u32]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let normal = (positions[triangle[1] as usize] - positions[triangle[0] as usize])
            .cross(positions[triangle[2] as usize] - positions[triangle[0] as usize]);
        for &index in triangle {
            normals[index as usize] += normal;
        }
    }
    for normal in &mut normals {
        *normal = normal.normalize_or(Vec3::Y);
    }
    normals
}

fn generate_tangents(
    positions: &[Vec3],
    normals: &[Vec3],
    uvs: &[Vec2],
    indices: &[u32],
) -> Vec<[f32; 4]> {
    let mut tangent_sums = vec![Vec3::ZERO; positions.len()];
    let mut bitangent_sums = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let edge_a = positions[b] - positions[a];
        let edge_b = positions[c] - positions[a];
        let uv_a = uvs[b] - uvs[a];
        let uv_b = uvs[c] - uvs[a];
        let determinant = uv_a.x * uv_b.y - uv_a.y * uv_b.x;
        let (tangent, bitangent) = if determinant.abs() > f32::EPSILON {
            let inverse = determinant.recip();
            (
                (edge_a * uv_b.y - edge_b * uv_a.y) * inverse,
                (edge_b * uv_a.x - edge_a * uv_b.x) * inverse,
            )
        } else {
            (edge_a, Vec3::ZERO)
        };
        for index in [a, b, c] {
            tangent_sums[index] += tangent;
            bitangent_sums[index] += bitangent;
        }
    }

    normals
        .iter()
        .zip(tangent_sums)
        .zip(bitangent_sums)
        .map(|((&normal, tangent), bitangent)| {
            let tangent =
                (tangent - normal * normal.dot(tangent)).normalize_or(fallback_tangent(normal));
            let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [tangent.x, tangent.y, tangent.z, handedness]
        })
        .collect()
}

fn fallback_tangent(normal: Vec3) -> Vec3 {
    if normal.y.abs() < 0.999 {
        normal.cross(Vec3::Y).normalize()
    } else {
        normal.cross(Vec3::X).normalize()
    }
}

fn import_material(
    source: Option<gltf::Material<'_>>,
    images: &[gltf::image::Data],
    workspace: &mut Workspace,
    texture_cache: &mut HashMap<(usize, TextureColorSpace), TextureHandle>,
) -> Result<Material, GltfError> {
    let Some(source) = source else {
        return Ok(Material::default());
    };
    let pbr = source.pbr_metallic_roughness();
    let mut material = Material {
        base_color: pbr.base_color_factor(),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        emissive: source.emissive_factor(),
        ..Material::default()
    };

    if let Some(info) = pbr.base_color_texture() {
        require_tex_coord_zero(info.tex_coord())?;
        material.textures.base_color = Some(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Srgb,
            images,
            workspace,
            texture_cache,
        )?);
        material.filter = import_filter(info.texture().sampler());
    }
    if let Some(info) = source.normal_texture() {
        require_tex_coord_zero(info.tex_coord())?;
        material.textures.normal = Some(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Linear,
            images,
            workspace,
            texture_cache,
        )?);
        if pbr.base_color_texture().is_none() {
            material.filter = import_filter(info.texture().sampler());
        }
    }
    if let Some(info) = pbr.metallic_roughness_texture() {
        require_tex_coord_zero(info.tex_coord())?;
        material.textures.metallic_roughness = Some(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Linear,
            images,
            workspace,
            texture_cache,
        )?);
        if pbr.base_color_texture().is_none() && source.normal_texture().is_none() {
            material.filter = import_filter(info.texture().sampler());
        }
    }
    if let Some(info) = source.emissive_texture() {
        require_tex_coord_zero(info.tex_coord())?;
        material.textures.emissive = Some(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Srgb,
            images,
            workspace,
            texture_cache,
        )?);
        if pbr.base_color_texture().is_none()
            && source.normal_texture().is_none()
            && pbr.metallic_roughness_texture().is_none()
        {
            material.filter = import_filter(info.texture().sampler());
        }
    }
    Ok(material)
}

fn require_tex_coord_zero(set: u32) -> Result<(), GltfError> {
    if set == 0 {
        Ok(())
    } else {
        Err(GltfError::UnsupportedTextureCoordinates { set })
    }
}

fn import_filter(sampler: gltf::texture::Sampler<'_>) -> TextureFilter {
    use gltf::texture::{MagFilter, MinFilter};

    match sampler.min_filter() {
        Some(MinFilter::Nearest | MinFilter::NearestMipmapNearest) => TextureFilter::Nearest,
        Some(
            MinFilter::Linear | MinFilter::LinearMipmapNearest | MinFilter::NearestMipmapLinear,
        ) => TextureFilter::Bilinear,
        Some(MinFilter::LinearMipmapLinear) => TextureFilter::Trilinear,
        None if sampler.mag_filter() == Some(MagFilter::Nearest) => TextureFilter::Nearest,
        None => TextureFilter::Trilinear,
    }
}

fn import_texture(
    image_index: usize,
    color_space: TextureColorSpace,
    images: &[gltf::image::Data],
    workspace: &mut Workspace,
    texture_cache: &mut HashMap<(usize, TextureColorSpace), TextureHandle>,
) -> Result<TextureHandle, GltfError> {
    let key = (image_index, color_space);
    if let Some(&handle) = texture_cache.get(&key) {
        return Ok(handle);
    }
    let image = &images[image_index];
    let texture = Texture::with_color_space(
        image.width,
        image.height,
        image_to_rgba8(image),
        color_space,
    )?;
    let handle = workspace.add_texture(texture)?;
    texture_cache.insert(key, handle);
    Ok(handle)
}

fn image_to_rgba8(image: &gltf::image::Data) -> Vec<u8> {
    use gltf::image::Format;

    let pixel_count = image.width as usize * image.height as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    match image.format {
        Format::R8 => {
            for &red in &image.pixels {
                rgba.extend([red, red, red, 255]);
            }
        }
        Format::R8G8 => {
            for pixel in image.pixels.chunks_exact(2) {
                rgba.extend([pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        Format::R8G8B8 => {
            for pixel in image.pixels.chunks_exact(3) {
                rgba.extend([pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        Format::R8G8B8A8 => rgba.extend_from_slice(&image.pixels),
        Format::R16 => append_u16_pixels(&mut rgba, &image.pixels, 1),
        Format::R16G16 => append_u16_pixels(&mut rgba, &image.pixels, 2),
        Format::R16G16B16 => append_u16_pixels(&mut rgba, &image.pixels, 3),
        Format::R16G16B16A16 => append_u16_pixels(&mut rgba, &image.pixels, 4),
        Format::R32G32B32FLOAT => append_f32_pixels(&mut rgba, &image.pixels, 3),
        Format::R32G32B32A32FLOAT => append_f32_pixels(&mut rgba, &image.pixels, 4),
    }
    rgba
}

fn append_u16_pixels(rgba: &mut Vec<u8>, pixels: &[u8], channels: usize) {
    for pixel in pixels.chunks_exact(channels * 2) {
        let component = |index: usize| {
            let offset = index * 2;
            let value = u16::from_ne_bytes([pixel[offset], pixel[offset + 1]]);
            ((u32::from(value) * 255 + 32767) / 65535) as u8
        };
        let red = component(0);
        match channels {
            1 => rgba.extend([red, red, red, 255]),
            2 => rgba.extend([red, red, red, component(1)]),
            3 => rgba.extend([red, component(1), component(2), 255]),
            4 => rgba.extend([red, component(1), component(2), component(3)]),
            _ => unreachable!(),
        }
    }
}

fn append_f32_pixels(rgba: &mut Vec<u8>, pixels: &[u8], channels: usize) {
    for pixel in pixels.chunks_exact(channels * 4) {
        let component = |index: usize| {
            let offset = index * 4;
            let value = f32::from_ne_bytes(pixel[offset..offset + 4].try_into().unwrap());
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        let red = component(0);
        match channels {
            3 => rgba.extend([red, component(1), component(2), 255]),
            4 => rgba.extend([red, component(1), component(2), component(3)]),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instance;

    #[test]
    fn material_returns_only_uniform_effective_slots() {
        let mut meshpart = MeshPart::new(Mesh::block(1.0, [1.0; 4])).named("part");
        assert_eq!(meshpart.material(), Some(&Material::default()));

        let material = Material::from_color(crate::Color3::new(1.0, 0.0, 0.0));
        meshpart.set_material(material);
        assert_eq!(meshpart.material(), Some(&material));

        meshpart.set_material_slot(Face::Top, Material::default());
        assert_eq!(meshpart.material(), None);
    }

    #[test]
    fn imports_supplied_glb_assets_as_renderable_meshparts() {
        let assets: [(&str, &[u8]); 2] = [
            (
                "DamagedHelmet",
                include_bytes!("../../assets/DamagedHelmet.glb"),
            ),
            ("Duck", include_bytes!("../../assets/Duck.glb")),
        ];

        for (name, bytes) in assets {
            let mut workspace = Workspace::new();
            let mesh_handle = workspace
                .add_mesh(bytes)
                .unwrap_or_else(|error| panic!("failed to import {name}: {error}"));
            let meshpart = MeshPart::new(mesh_handle.clone()).named(name);
            let mesh = meshpart.mesh();

            assert!(!mesh.vertices.is_empty(), "{name} has no vertices");
            assert!(!mesh.indices.is_empty(), "{name} has no indices");
            assert!(mesh.indices.len().is_multiple_of(3));
            assert!(
                mesh.indices
                    .iter()
                    .all(|&index| usize::from(index) < mesh.vertices.len())
            );
            for vertex in &mesh.vertices {
                let normal = Vec3::from_array(vertex.normal);
                let tangent = Vec3::from_array(vertex.tangent[..3].try_into().unwrap());
                assert!(vertex.position.iter().all(|value| value.is_finite()));
                assert!((normal.length() - 1.0).abs() < 0.001);
                assert!((tangent.length() - 1.0).abs() < 0.001);
                assert!(normal.dot(tangent).abs() < 0.001);
                assert!(vertex.uv.iter().all(|value| value.is_finite()));
                assert!(vertex.tangent[3] == 1.0 || vertex.tangent[3] == -1.0);
                assert!((vertex.material_slot as usize) < MATERIAL_SLOT_COUNT);
            }

            let texture_handles = Face::ALL
                .into_iter()
                .flat_map(|slot| {
                    let textures = meshpart.material_slot(slot).textures;
                    [
                        textures.base_color,
                        textures.normal,
                        textures.metallic_roughness,
                        textures.emissive,
                    ]
                })
                .flatten()
                .collect::<Vec<_>>();
            assert!(
                !texture_handles.is_empty(),
                "{name} has no imported textures"
            );
            assert!(
                texture_handles
                    .iter()
                    .all(|&handle| workspace.get_texture(handle).is_some())
            );

            let id = workspace.add_child(meshpart);
            let loaded = workspace.get::<MeshPart>(id).unwrap();
            assert_eq!(loaded.class_name(), "MeshPart");
            assert_eq!(loaded.name(), name);
            let vertex_count = loaded.mesh().vertices.len();
            let variant_id = workspace.add_child(MeshPart::new(mesh_handle.clone()).named(name));
            let variant = workspace.get::<MeshPart>(variant_id).unwrap();
            assert_eq!(variant.mesh().vertices.len(), vertex_count);
            assert!(workspace.get_mesh(&mesh_handle).is_some());
        }
    }
}
