use std::{collections::HashMap, sync::Arc};

use thiserror::Error;

use crate::{
    BasePart, Face, HasBasePart, HasMaterials, HasPVInstance, Instance, MATERIAL_SLOT_COUNT,
    Material, Mesh, MeshMaterialSlots, PVInstance, Texture, TextureColorSpace, TextureError,
    TextureFilter, TextureHandle, Vertex, Workspace,
    glam::{Mat4, Vec2, Vec3},
};

/// Errors produced while decoding a glTF asset or deriving its render view.
#[derive(Debug, Error)]
pub enum GltfError {
    /// The glTF document, its buffers, or one of its images could not be decoded.
    #[error("could not import glTF data: {0}")]
    Import(#[from] gltf::Error),
    /// The document did not define a default or fallback scene for its render view.
    #[error("glTF document contains no scene")]
    NoScene,
    /// The selected scene did not contain any mesh geometry for its render view.
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
/// `MeshPart` exposes [`BasePart`] behavior through [`HasBasePart`] and
/// [`PVInstance`] behavior through [`HasPVInstance`], so its hierarchy,
/// transform, size, tint, and collision metadata use the same API as
/// [`crate::Part`].
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

/// The source stored by a [`MeshHandle`].
#[derive(Clone, Copy, Debug)]
pub enum MeshSourceRef<'a> {
    /// Caller-provided.
    Data(&'a Mesh),
    /// The parsed glTF asset.
    Gltf(&'a GltfAsset),
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

/// A glTF document and its decoded resources.
#[derive(Debug)]
pub struct GltfAsset {
    source: Arc<[u8]>,
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
}

impl GltfAsset {
    fn import(bytes: &[u8]) -> Result<Self, GltfError> {
        let source = Arc::<[u8]>::from(bytes);
        let (document, buffers, images) = gltf::import_slice(bytes)?;
        Ok(Self {
            source,
            document,
            buffers,
            images,
        })
    }

    /// Returns the exact bytes passed to [`crate::Workspace::add_mesh`].
    pub fn source_bytes(&self) -> &[u8] {
        &self.source
    }

    /// Returns the complete parsed glTF document.
    pub fn document(&self) -> &gltf::Document {
        &self.document
    }

    /// Returns decoded buffer payloads in document order.
    pub fn buffers(&self) -> &[gltf::buffer::Data] {
        &self.buffers
    }

    /// Returns decoded image payloads in document order.
    pub fn images(&self) -> &[gltf::image::Data] {
        &self.images
    }
}

#[derive(Clone, Debug)]
enum MeshAssetSource {
    Data(Mesh),
    Gltf(Arc<GltfAsset>),
}

#[derive(Clone, Debug)]
struct MeshAsset {
    source: MeshAssetSource,
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
            source: MeshAssetSource::Data(mesh.clone()),
            mesh,
            material_slots,
        }))
    }

    fn from_gltf(asset: Arc<GltfAsset>, mesh: Mesh, material_slots: MeshMaterialSlots) -> Self {
        Self(Arc::new(MeshAsset {
            source: MeshAssetSource::Gltf(asset),
            mesh,
            material_slots,
        }))
    }

    /// Returns the renderer-ready geometry derived from this mesh asset.
    ///
    /// For glTF sources this is a compatibility view. The lossless source is
    /// available through [`Self::source`] and [`Self::gltf`].
    pub fn mesh(&self) -> &Mesh {
        &self.0.mesh
    }

    /// Returns the lossless source represented by this handle.
    pub fn source(&self) -> MeshSourceRef<'_> {
        match &self.0.source {
            MeshAssetSource::Data(mesh) => MeshSourceRef::Data(mesh),
            MeshAssetSource::Gltf(asset) => MeshSourceRef::Gltf(asset),
        }
    }

    /// Returns the lossless glTF source, if this handle was created from glTF.
    pub fn gltf(&self) -> Option<&GltfAsset> {
        match &self.0.source {
            MeshAssetSource::Data(_) => None,
            MeshAssetSource::Gltf(asset) => Some(asset),
        }
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
            basepart: BasePart::new().with_name("MeshPart"),
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
        let asset = Arc::new(GltfAsset::import(bytes)?);
        let document = asset.document();
        let buffers = asset.buffers();
        let images = asset.images();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut material_keys = Vec::new();

        if let Some(scene) = document
            .default_scene()
            .or_else(|| document.scenes().next())
        {
            for node in scene.nodes() {
                // The source asset is retained even when a primitive cannot be
                // represented by the built-in compatibility renderer.
                let _ = append_node(
                    node,
                    Mat4::IDENTITY,
                    buffers,
                    &mut vertices,
                    &mut indices,
                    &mut material_keys,
                );
            }
        }

        let mut texture_cache = HashMap::new();
        let mut materials = Vec::with_capacity(material_keys.len());
        for key in material_keys {
            let source = key.and_then(|index| document.materials().nth(index));
            materials.push(
                import_material(source, images, workspace, &mut texture_cache).unwrap_or_default(),
            );
        }

        let mut material_slots = MeshMaterialSlots::default();
        for (slot, material) in Face::ALL.into_iter().zip(materials) {
            material_slots.set(slot, material);
        }
        Ok(MeshHandle::from_gltf(
            asset,
            Mesh::new(vertices, indices),
            material_slots,
        ))
    }

    pub(crate) fn mesh_revision(&self) -> u64 {
        self.mesh_revision
    }

    pub(crate) fn bounding_radius(&self) -> f32 {
        self.bounding_radius
    }
}

crate::impl_instance!(MeshPart, class_name = "MeshPart", data = basepart.instance,);

/// Access to the underlying [`MeshPart`].
pub trait HasMeshPart {
    /// Returns shared access to the underlying [`MeshPart`].
    fn mesh_part(&self) -> &MeshPart;

    /// Returns mutable access to the underlying [`MeshPart`].
    fn mesh_part_mut(&mut self) -> &mut MeshPart;

    /// Returns the CPU-side geometry rendered by this mesh part.
    fn mesh(&self) -> &Mesh {
        self.mesh_part().mesh.mesh()
    }

    /// Returns the shared mesh asset used by this part.
    fn mesh_handle(&self) -> &MeshHandle {
        &self.mesh_part().mesh
    }

    /// Replaces the geometry and schedules it for upload before the next frame.
    fn with_mesh(mut self, mesh: Mesh) -> Self
    where
        Self: Sized,
    {
        let mesh_part = self.mesh_part_mut();
        mesh_part.bounding_radius = mesh_bounding_radius(&mesh);
        mesh_part.mesh = MeshHandle::from_parts(mesh, mesh_part.material_slots.clone());
        mesh_part.mesh_revision = mesh_part.mesh_revision.wrapping_add(1);
        self
    }
}

impl HasMeshPart for MeshPart {
    fn mesh_part(&self) -> &MeshPart {
        self
    }

    fn mesh_part_mut(&mut self) -> &mut MeshPart {
        self
    }
}

impl<T: HasMeshPart + ?Sized> HasMeshPart for &mut T {
    fn mesh_part(&self) -> &MeshPart {
        (**self).mesh_part()
    }

    fn mesh_part_mut(&mut self) -> &mut MeshPart {
        (**self).mesh_part_mut()
    }
}

impl HasPVInstance for MeshPart {
    fn pv(&self) -> &PVInstance {
        self.basepart.pv()
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        self.basepart.pv_mut()
    }
}

impl HasBasePart for MeshPart {
    fn base_part(&self) -> &BasePart {
        &self.basepart
    }

    fn base_part_mut(&mut self) -> &mut BasePart {
        &mut self.basepart
    }
}

impl HasMaterials for MeshPart {
    fn material_slots(&self) -> &MeshMaterialSlots {
        &self.material_slots
    }

    fn material_slots_mut(&mut self) -> &mut MeshMaterialSlots {
        &mut self.material_slots
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
            let _ = append_primitive(
                primitive,
                mesh_index,
                transform,
                buffers,
                vertices,
                indices,
                material_keys,
            );
        }
    }
    for child in node.children() {
        let _ = append_node(child, transform, buffers, vertices, indices, material_keys);
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
        } else if material_keys.len() < MATERIAL_SLOT_COUNT {
            material_keys.push(material_key);
            material_keys.len() - 1
        } else {
            // The six-slot renderer cannot represent every glTF material
            material_key.unwrap_or_default() % MATERIAL_SLOT_COUNT
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
    let mut material = Material::default()
        .with_base_color(pbr.base_color_factor())
        .with_metallic(pbr.metallic_factor())
        .with_roughness(pbr.roughness_factor())
        .with_emissive(source.emissive_factor());

    if let Some(info) = pbr
        .base_color_texture()
        .filter(|info| info.tex_coord() == 0)
    {
        material = material.with_base_color_texture(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Srgb,
            images,
            workspace,
            texture_cache,
        )?);
        material = material.with_filter(import_filter(info.texture().sampler()));
    }
    if let Some(info) = source.normal_texture().filter(|info| info.tex_coord() == 0) {
        material = material.with_normal_texture(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Linear,
            images,
            workspace,
            texture_cache,
        )?);
        if pbr.base_color_texture().is_none() {
            material = material.with_filter(import_filter(info.texture().sampler()));
        }
    }
    if let Some(info) = pbr
        .metallic_roughness_texture()
        .filter(|info| info.tex_coord() == 0)
    {
        material = material.with_metallic_roughness_texture(import_texture(
            info.texture().source().index(),
            TextureColorSpace::Linear,
            images,
            workspace,
            texture_cache,
        )?);
        if pbr.base_color_texture().is_none() && source.normal_texture().is_none() {
            material = material.with_filter(import_filter(info.texture().sampler()));
        }
    }
    if let Some(info) = source
        .emissive_texture()
        .filter(|info| info.tex_coord() == 0)
    {
        material = material.with_emissive_texture(import_texture(
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
            material = material.with_filter(import_filter(info.texture().sampler()));
        }
    }
    Ok(material)
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
        let meshpart = MeshPart::new(Mesh::block(1.0, [1.0; 4])).with_name("part");
        assert_eq!(meshpart.material(), Some(&Material::default()));

        let material = Material::from_color(crate::Color3::new(1.0, 0.0, 0.0));
        let meshpart = meshpart.with_material(material);
        assert_eq!(meshpart.material(), Some(&material));

        let meshpart = meshpart.with_material_slot(Face::Top, Material::default());
        assert_eq!(meshpart.material(), None);
    }

    #[test]
    fn mesh_part_builder_replaces_geometry_through_has_mesh_part() {
        let replacement = Mesh::new(
            vec![
                Vertex::with_attributes(
                    [2.0, 3.0, 4.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                    [1.0; 4],
                ),
                Vertex::with_attributes(
                    [2.0, 3.0, 5.0],
                    [0.0, 1.0, 0.0],
                    [1.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                    [1.0; 4],
                ),
                Vertex::with_attributes(
                    [3.0, 3.0, 4.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 1.0],
                    [1.0, 0.0, 0.0, 1.0],
                    [1.0; 4],
                ),
            ],
            vec![0, 1, 2],
        );
        let meshpart = MeshPart::new(Mesh::block(1.0, [1.0; 4])).with_mesh(replacement);

        assert_eq!(meshpart.mesh().vertices[0].position, [2.0, 3.0, 4.0]);
        assert_eq!(
            meshpart.mesh_handle().mesh().vertices[2].position,
            [3.0, 3.0, 4.0]
        );
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
            let meshpart = MeshPart::new(mesh_handle.clone()).with_name(name);
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
                    let textures = meshpart.material_slot(slot).textures();
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
            let variant_id =
                workspace.add_child(MeshPart::new(mesh_handle.clone()).with_name(name));
            let variant = workspace.get::<MeshPart>(variant_id).unwrap();
            assert_eq!(variant.mesh().vertices.len(), vertex_count);
            assert!(workspace.get_mesh(&mesh_handle).is_some());
        }
    }

    #[test]
    fn add_mesh_retains_the_lossless_gltf_source() {
        let bytes = include_bytes!("../../assets/DamagedHelmet.glb");
        let mut workspace = Workspace::new();
        let handle = workspace.add_mesh(bytes).unwrap();
        let source = handle.gltf().expect("glTF source should be retained");

        assert_eq!(source.source_bytes(), bytes);
        assert_eq!(source.buffers().len(), source.document().buffers().count());
        assert_eq!(source.images().len(), source.document().images().count());
        assert!(source.document().scenes().next().is_some());

        match handle.source() {
            MeshSourceRef::Gltf(source) => assert_eq!(source.source_bytes(), bytes),
            MeshSourceRef::Data(_) => panic!("expected a glTF source"),
        }
    }

    #[test]
    fn add_mesh_accepts_a_valid_gltf_without_a_renderable_scene() {
        let bytes = br#"{"asset":{"version":"2.0"},"scenes":[]}"#;
        let mut workspace = Workspace::new();
        let handle = workspace.add_mesh(bytes).unwrap();

        assert_eq!(handle.gltf().unwrap().source_bytes(), bytes);
        assert!(handle.mesh().vertices.is_empty());
        assert!(handle.mesh().indices.is_empty());
    }
}
