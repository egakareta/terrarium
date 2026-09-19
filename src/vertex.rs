use bytemuck::{Pod, Zeroable};

use crate::{MaterialSlot, glam::Vec3};

/// A vertex consumed by the built-in PBR pipeline.
///
/// Tangents use the fourth component as the bitangent handedness, as in glTF:
/// the bitangent is `cross(normal, tangent) * tangent.w`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    /// Object-space position.
    pub position: [f32; 3],
    /// Object-space unit normal.
    pub normal: [f32; 3],
    /// UV coordinates used to sample material textures.
    pub uv: [f32; 2],
    /// Object-space tangent; `.w` stores bitangent handedness.
    pub tangent: [f32; 4],
    /// A per-vertex color multiplier retained for custom mesh tinting.
    pub color: [f32; 4],
    /// GPU representation of the material slot selected by this vertex.
    pub material_slot: u32,
}

impl Vertex {
    /// Creates a vertex using the directional slot matching its normal.
    pub fn with_attributes(
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
        tangent: [f32; 4],
        color: [f32; 4],
    ) -> Self {
        Self::with_material_slot(
            position,
            normal,
            uv,
            tangent,
            color,
            MaterialSlot::from_normal(normal),
        )
    }

    /// Creates a vertex with an explicit material slot.
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

    pub(crate) fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
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

/// Appends one triangle using default UVs and the directional slot matching its
/// normal.
///
/// The triangle normal and tangent are derived from the supplied positions and
/// UVs. Indices are appended relative to the existing vertex count.
pub fn push_triangle(
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

/// Appends one triangle with explicit UVs and the directional slot matching its
/// normal.
pub fn push_triangle_with_uv(
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
        MaterialSlot::from_normal(triangle_normal(positions)),
    );
}

/// Appends one triangle with explicit UVs and material slot.
pub fn push_triangle_with_uv_and_material_slot(
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

/// Appends one quad as two triangles using default UVs and the directional slot
/// matching its normal.
pub fn push_quad(
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

/// Appends one quad as two triangles with explicit UVs and the directional slot
/// matching its normal.
pub fn push_quad_with_uv(
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
        MaterialSlot::from_normal(triangle_normal([positions[0], positions[1], positions[2]])),
    );
}

/// Appends one quad as two triangles using default UVs and an explicit material slot.
pub fn push_quad_with_material_slot(
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

/// Appends one quad as two triangles with explicit UVs and material slot.
pub fn push_quad_with_uv_and_material_slot(
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

/// Computes a normalized counter-clockwise triangle normal.
///
/// Degenerate triangles return the zero vector.
pub fn triangle_normal(positions: [[f32; 3]; 3]) -> [f32; 3] {
    let edge_a = Vec3::from_array(positions[1]) - Vec3::from_array(positions[0]);
    let edge_b = Vec3::from_array(positions[2]) - Vec3::from_array(positions[0]);
    edge_a.cross(edge_b).normalize_or_zero().to_array()
}

/// Computes a tangent and its glTF-compatible bitangent handedness.
///
/// The returned value is `[x, y, z, w]`, where the bitangent is
/// `cross(normal, tangent) * w`. If the UV triangle is degenerate, the first
/// position edge is used as a fallback before orthogonalization.
pub fn tangent_from_uv(positions: [[f32; 3]; 3], uvs: [[f32; 2]; 3], normal: [f32; 3]) -> [f32; 4] {
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
    let bitangent = if determinant.abs() > f32::EPSILON {
        (position_b * uv_a.x - position_a * uv_b.x) / determinant
    } else {
        normal.cross(tangent)
    };
    let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
        -1.0
    } else {
        1.0
    };
    [tangent.x, tangent.y, tangent.z, handedness]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tangent_frame_follows_uv_directions() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        let normal = triangle_normal(positions);
        let tangent = tangent_from_uv(positions, uvs, normal);
        let normal = Vec3::from_array(normal);
        let tangent_direction = Vec3::from_array(tangent[..3].try_into().unwrap());
        let bitangent_direction = normal.cross(tangent_direction) * tangent[3];

        assert!(tangent_direction.dot(Vec3::X) > 0.9999);
        assert!(bitangent_direction.dot(Vec3::Y) > 0.9999);
    }
}
