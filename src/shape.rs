use glam::Vec3;

use crate::{
    MaterialSlot, Vertex, push_quad, push_quad_with_material_slot, push_quad_with_uv,
    push_triangle, push_triangle_with_uv,
};

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
