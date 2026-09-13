use glam::Vec3;

use crate::{
    MaterialSlot, Vertex, push_quad, push_quad_with_material_slot, push_quad_with_uv,
    push_triangle, push_triangle_with_uv,
};

/// The primitive geometry available to a [`crate::Part`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PartShape {
    #[default]
    /// A six-sided box.
    Block,
    /// A UV sphere.
    Ball,
    /// A cylinder aligned to the Y axis.
    Cylinder,
    /// A triangular prism with a sloped top.
    Wedge,
    /// A four-sided wedge with one high corner.
    CornerWedge,
}

impl PartShape {
    pub(crate) const ALL: [Self; 5] = [
        Self::Block,
        Self::Ball,
        Self::Cylinder,
        Self::Wedge,
        Self::CornerWedge,
    ];

    pub(crate) const COUNT: usize = Self::ALL.len();

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Block => 0,
            Self::Ball => 1,
            Self::Cylinder => 2,
            Self::Wedge => 3,
            Self::CornerWedge => 4,
        }
    }

    pub(crate) fn mesh(self, color: [f32; 4]) -> Mesh {
        match self {
            Self::Block => Mesh::block(1.0, color),
            Self::Ball => Mesh::ball(0.5, 16, 24, color),
            Self::Cylinder => Mesh::cylinder(0.5, 1.0, 24, color),
            Self::Wedge => Mesh::wedge(color),
            Self::CornerWedge => Mesh::corner_wedge(color),
        }
    }
}

/// CPU-side mesh data ready to be uploaded to a [`crate::Renderer`].
///
/// Indices are `u16`, so a custom mesh must address fewer than `u16::MAX + 1`
/// vertices. The renderer validates that the mesh is non-empty and that every
/// index refers to an existing vertex before uploading it.
#[derive(Clone, Debug)]
pub struct Mesh {
    /// Vertex attributes consumed by the built-in PBR pipeline.
    pub vertices: Vec<Vertex>,
    /// Triangle-list indices into [`Self::vertices`].
    pub indices: Vec<u16>,
}

impl Mesh {
    /// Creates mesh data from caller-owned vertices and triangle-list indices.
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u16>) -> Self {
        Self { vertices, indices }
    }

    /// Creates a block centered at the origin.
    ///
    /// This is an alias for [`Mesh::block`].
    pub fn cube(size: f32, color: [f32; 4]) -> Self {
        Self::block(size, color)
    }

    /// Creates a box centered at the origin.
    ///
    /// The generated mesh labels its faces as [`MaterialSlot::Top`],
    /// [`MaterialSlot::Bottom`], or [`MaterialSlot::Side`].
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
    ///
    /// `latitude_segments` and `longitude_segments` are clamped to at least
    /// 2 and 3 respectively. Higher values produce smoother geometry and more
    /// vertices. The sphere's radius is applied directly to its positions.
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
    /// `segments` is clamped to at least 3.
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
    ///
    /// The geometry occupies a unit cube centered at the origin and is meant
    /// to be scaled through [`crate::BasePart::size`].
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
    ///
    /// The geometry occupies a unit cube centered at the origin and is meant
    /// to be scaled through [`crate::BasePart::size`].
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
    ///
    /// The plane's normal points toward `+Y`.
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

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::MaterialSlot;

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
}
