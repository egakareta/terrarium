use crate::{
    MaterialSlot, Vertex, glam::Vec3, push_quad_with_material_slot,
    push_quad_with_uv_and_material_slot, push_triangle_with_uv_and_material_slot, triangle_normal,
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

    /// Conservative bounding-sphere radius of the unit mesh centered at the
    /// origin, used for exact frustum culling.
    pub(crate) const fn bounding_radius(self) -> f32 {
        match self {
            // Unit cube corners at (±0.5, ±0.5, ±0.5): sqrt(3)/2.
            Self::Block | Self::Wedge | Self::CornerWedge => 0.866_025_4,
            // Sphere radius 0.5.
            Self::Ball => 0.5,
            // Rim at (0.5, ±0.5): sqrt(0.5).
            Self::Cylinder => std::f32::consts::FRAC_1_SQRT_2,
        }
    }

    pub(crate) fn mesh(self, color: [f32; 4]) -> Mesh {
        match self {
            Self::Block => Mesh::block(1.0, color),
            // 12x18 sphere: shading uses analytic normals so lighting is
            // smooth; only the silhouette differs from 16x24 by ~1% of radius
            // (subpixel at typical multi-object distances) for 44% fewer tris.
            Self::Ball => Mesh::ball(0.5, 12, 18, color),
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

    /// Tags every vertex with the directional [`MaterialSlot`] matching its
    /// normal, so per-face materials work on any arbitrary mesh.
    ///
    /// This is how the built-in primitives support [`MaterialSlot::Top`] and
    /// friends on every shape: run it on a custom mesh after setting normals,
    /// then assign per-face materials through
    /// [`crate::Part::set_material_slot`]. Vertices with a zero normal keep
    /// [`MaterialSlot::Base`].
    pub fn assign_directional_slots(&mut self) {
        for vertex in &mut self.vertices {
            vertex.material_slot = MaterialSlot::from_normal(vertex.normal) as u32;
        }
    }

    /// Creates a block centered at the origin.
    ///
    /// This is an alias for [`Mesh::block`].
    pub fn cube(size: f32, color: [f32; 4]) -> Self {
        Self::block(size, color)
    }

    /// Creates a box centered at the origin.
    ///
    /// The generated mesh labels each face with its corresponding directional
    /// [`MaterialSlot`].
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
        let material_slots = [
            MaterialSlot::Front,
            MaterialSlot::Back,
            MaterialSlot::Top,
            MaterialSlot::Bottom,
            MaterialSlot::Right,
            MaterialSlot::Left,
        ];
        for ((a, b, c, d), material_slot) in faces.into_iter().zip(material_slots) {
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
    ///
    /// Each vertex is tagged with the directional [`MaterialSlot`] matching
    /// its normal, so polar caps respond to [`MaterialSlot::Top`] and
    /// [`MaterialSlot::Bottom`] while equatorial bands respond to the side
    /// slots.
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
                vertices.push(Vertex::with_material_slot(
                    [normal.x * radius, normal.y * radius, normal.z * radius],
                    normal.to_array(),
                    [u, v],
                    [tangent.x, tangent.y, tangent.z, 1.0],
                    color,
                    MaterialSlot::from_normal(normal.to_array()),
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
                    top_right,
                    bottom_left,
                    top_right,
                    bottom_right,
                    bottom_left,
                ]);
            }
        }

        Self { vertices, indices }
    }

    /// Creates a cylinder aligned to the Y axis and centered at the origin.
    ///
    /// i.e. The top face points toward `+Y` and the bottom face toward `-Y`.
    /// `segments` is clamped to at least 3.
    ///
    /// Side quads are tagged with the directional [`MaterialSlot`] matching
    /// their outward normal while the caps use [`MaterialSlot::Top`] and
    /// [`MaterialSlot::Bottom`].
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
            let side_positions = [
                [radius * angle.cos(), -half_height, radius * angle.sin()],
                [radius * angle.cos(), half_height, radius * angle.sin()],
                [
                    radius * next_angle.cos(),
                    half_height,
                    radius * next_angle.sin(),
                ],
                [
                    radius * next_angle.cos(),
                    -half_height,
                    radius * next_angle.sin(),
                ],
            ];
            push_quad_with_uv_and_material_slot(
                &mut vertices,
                &mut indices,
                side_positions,
                [[u, 0.0], [u, 1.0], [next_u, 1.0], [next_u, 0.0]],
                color,
                MaterialSlot::from_normal(triangle_normal([
                    side_positions[0],
                    side_positions[1],
                    side_positions[2],
                ])),
            );

            push_triangle_with_uv_and_material_slot(
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
                MaterialSlot::Top,
            );
            push_triangle_with_uv_and_material_slot(
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
                MaterialSlot::Bottom,
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
    ///
    /// Each face is tagged with the directional [`MaterialSlot`] matching its
    /// outward normal, so per-face materials work just like on a block.
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

        let back_face = [front_bottom_left, front_top_right, front_bottom_right];
        push_triangle_with_uv_and_material_slot(
            &mut vertices,
            &mut indices,
            back_face,
            [[0.0, 0.0], [1.0, 1.0], [1.0, 0.0]],
            color,
            MaterialSlot::from_normal(triangle_normal(back_face)),
        );
        let front_face = [back_bottom_left, back_bottom_right, back_top_right];
        push_triangle_with_uv_and_material_slot(
            &mut vertices,
            &mut indices,
            front_face,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            color,
            MaterialSlot::from_normal(triangle_normal(front_face)),
        );
        let bottom_face = [
            front_bottom_left,
            front_bottom_right,
            back_bottom_right,
            back_bottom_left,
        ];
        push_quad_with_uv_and_material_slot(
            &mut vertices,
            &mut indices,
            bottom_face,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color,
            MaterialSlot::from_normal(triangle_normal([
                bottom_face[0],
                bottom_face[1],
                bottom_face[2],
            ])),
        );
        let right_face = [
            back_bottom_right,
            front_bottom_right,
            front_top_right,
            back_top_right,
        ];
        push_quad_with_uv_and_material_slot(
            &mut vertices,
            &mut indices,
            right_face,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color,
            MaterialSlot::from_normal(triangle_normal([
                right_face[0],
                right_face[1],
                right_face[2],
            ])),
        );
        let slope_face = [
            front_bottom_left,
            back_bottom_left,
            back_top_right,
            front_top_right,
        ];
        push_quad_with_uv_and_material_slot(
            &mut vertices,
            &mut indices,
            slope_face,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color,
            MaterialSlot::from_normal(triangle_normal([
                slope_face[0],
                slope_face[1],
                slope_face[2],
            ])),
        );

        Self { vertices, indices }
    }

    /// Creates a pyramid-like corner wedge with one high corner and four sloped sides.
    ///
    /// The apex direction from the center is `(-X, +Y, -Z)`.
    ///
    /// The geometry occupies a unit cube centered at the origin and is meant
    /// to be scaled through [`crate::BasePart::size`].
    ///
    /// Each face is tagged with the directional [`MaterialSlot`] matching its
    /// outward normal, so per-face materials work just like on a block.
    pub fn corner_wedge(color: [f32; 4]) -> Self {
        let h = 0.5;
        let corners = [[-h, -h, -h], [h, -h, -h], [h, -h, h], [-h, -h, h]];
        let apex = [-h, h, -h];
        let mut vertices = Vec::with_capacity(16);
        let mut indices = Vec::with_capacity(18);

        push_quad_with_uv_and_material_slot(
            &mut vertices,
            &mut indices,
            [corners[0], corners[1], corners[2], corners[3]],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color,
            MaterialSlot::Bottom,
        );
        for (index, next) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
            let face = [corners[index], apex, corners[next]];
            push_triangle_with_uv_and_material_slot(
                &mut vertices,
                &mut indices,
                face,
                [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]],
                color,
                MaterialSlot::from_normal(triangle_normal(face)),
            );
        }

        Self { vertices, indices }
    }

    /// Creates a square on the XZ plane, centered at the origin.
    ///
    /// The plane's normal points toward `+Y`, so its vertices use
    /// [`MaterialSlot::Top`].
    pub fn plane(size: f32, color: [f32; 4]) -> Self {
        let h = size * 0.5;
        Self {
            vertices: vec![
                Vertex::with_material_slot(
                    [-h, 0.0, -h],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                    MaterialSlot::Top,
                ),
                Vertex::with_material_slot(
                    [h, 0.0, -h],
                    [0.0, 1.0, 0.0],
                    [1.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                    MaterialSlot::Top,
                ),
                Vertex::with_material_slot(
                    [h, 0.0, h],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                    MaterialSlot::Top,
                ),
                Vertex::with_material_slot(
                    [-h, 0.0, h],
                    [0.0, 1.0, 0.0],
                    [0.0, 1.0],
                    [1.0, 0.0, 0.0, 1.0],
                    color,
                    MaterialSlot::Top,
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

    fn assert_face_normal(vertices: &[Vertex], expected: Vec3) {
        let expected = expected.normalize();
        for vertex in vertices {
            let normal = Vec3::from_array(vertex.normal);
            assert!(
                normal.dot(expected) > 0.9999,
                "expected normal {expected:?}, got {normal:?}"
            );
        }
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
    fn block_mesh_uses_named_material_slots() {
        let mesh = Mesh::block(1.0, [1.0; 4]);
        let expected_slots = [
            MaterialSlot::Front,
            MaterialSlot::Back,
            MaterialSlot::Top,
            MaterialSlot::Bottom,
            MaterialSlot::Right,
            MaterialSlot::Left,
        ];

        for (vertices, slot) in mesh.vertices.chunks_exact(4).zip(expected_slots) {
            assert!(
                vertices
                    .iter()
                    .all(|vertex| vertex.material_slot == slot as u32)
            );
        }
    }

    #[test]
    fn wedge_face_normals_point_outward() {
        let mesh = Mesh::wedge([1.0; 4]);
        let faces = [
            (0..3, Vec3::NEG_Z),
            (3..6, Vec3::Z),
            (6..10, Vec3::NEG_Y),
            (10..14, Vec3::X),
            (14..18, Vec3::new(-1.0, 1.0, 0.0)),
        ];

        for (range, expected) in faces {
            assert_face_normal(&mesh.vertices[range], expected);
        }
    }

    #[test]
    fn cylinder_face_normals_point_outward() {
        let mesh = Mesh::cylinder(1.0, 1.0, 8, [1.0; 4]);

        for segment in mesh.vertices.chunks_exact(10) {
            let side_direction = (Vec3::from_array(segment[0].position)
                + Vec3::from_array(segment[2].position))
            .with_y(0.0)
            .normalize();
            assert_face_normal(&segment[0..4], side_direction);
            assert_face_normal(&segment[4..7], Vec3::Y);
            assert_face_normal(&segment[7..10], Vec3::NEG_Y);
        }
    }

    #[test]
    fn corner_wedge_face_normals_point_outward() {
        let mesh = Mesh::corner_wedge([1.0; 4]);
        let faces = [
            (0..4, Vec3::NEG_Y),
            (4..7, Vec3::NEG_Z),
            (7..10, Vec3::new(1.0, 1.0, 0.0)),
            (10..13, Vec3::new(0.0, 1.0, 1.0)),
            (13..16, Vec3::NEG_X),
        ];

        for (range, expected) in faces {
            assert_face_normal(&mesh.vertices[range], expected);
        }
    }

    #[test]
    fn ball_winding_matches_outward_normals_for_backface_culling() {
        let mesh = Mesh::ball(0.5, 8, 12, [1.0; 4]);
        assert!(!mesh.indices.is_empty());
        for triangle in mesh.indices.chunks_exact(3) {
            let positions = [
                Vec3::from_array(mesh.vertices[triangle[0] as usize].position),
                Vec3::from_array(mesh.vertices[triangle[1] as usize].position),
                Vec3::from_array(mesh.vertices[triangle[2] as usize].position),
            ];
            // Pole fans duplicate the pole vertex per longitude, so one
            // triangle per polar quad is degenerate (zero area, no pixels).
            // Skip by unnormalized area instead of the normalized normal.
            let cross = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
            if cross.length_squared() < 1e-12 {
                continue;
            }
            let centroid = (positions[0] + positions[1] + positions[2]) / 3.0;
            // Outward winding => derived CCW normal points along the radius.
            assert!(
                cross.normalize().dot(centroid.normalize()) > 0.99,
                "ball triangle {triangle:?} winds inward"
            );
        }
    }

    #[test]
    fn every_primitive_tags_faces_with_directional_material_slots() {
        let meshes = [
            Mesh::block(1.0, [1.0; 4]),
            Mesh::ball(0.5, 6, 12, [1.0; 4]),
            Mesh::cylinder(0.5, 1.0, 12, [1.0; 4]),
            Mesh::wedge([1.0; 4]),
            Mesh::corner_wedge([1.0; 4]),
            Mesh::plane(1.0, [1.0; 4]),
        ];

        for mesh in &meshes {
            assert!(!mesh.vertices.is_empty());
            for vertex in &mesh.vertices {
                let slot = vertex.material_slot;
                assert!(
                    (MaterialSlot::Top as u32..=MaterialSlot::Right as u32).contains(&slot),
                    "expected a directional slot, got {slot}"
                );
            }
        }
    }

    #[test]
    fn cylinder_caps_use_top_and_bottom_slots() {
        let mesh = Mesh::cylinder(1.0, 1.0, 8, [1.0; 4]);

        for segment in mesh.vertices.chunks_exact(10) {
            assert!(
                segment[0..4]
                    .iter()
                    .all(|vertex| vertex.material_slot != MaterialSlot::Base as u32)
            );
            assert!(
                segment[4..7]
                    .iter()
                    .all(|vertex| vertex.material_slot == MaterialSlot::Top as u32)
            );
            assert!(
                segment[7..10]
                    .iter()
                    .all(|vertex| vertex.material_slot == MaterialSlot::Bottom as u32)
            );
        }
    }

    #[test]
    fn cylinder_side_slots_cover_horizontal_quarters() {
        let mesh = Mesh::cylinder(1.0, 1.0, 24, [1.0; 4]);
        let mut counts = [0usize; MaterialSlot::ALL_DIRECTIONS.len()];

        for segment in mesh.vertices.chunks_exact(10) {
            let slot = segment[0].material_slot as usize;
            if slot >= MaterialSlot::Top as usize {
                counts[slot - 1] += 1;
            }
        }

        assert_eq!(counts[MaterialSlot::Front as usize - 1], 6);
        assert_eq!(counts[MaterialSlot::Back as usize - 1], 6);
        assert_eq!(counts[MaterialSlot::Left as usize - 1], 6);
        assert_eq!(counts[MaterialSlot::Right as usize - 1], 6);
    }

    #[test]
    fn wedge_faces_use_directional_slots_matching_their_normals() {
        let mesh = Mesh::wedge([1.0; 4]);
        let expected_slots = [
            MaterialSlot::Back,
            MaterialSlot::Front,
            MaterialSlot::Bottom,
            MaterialSlot::Right,
            MaterialSlot::Top,
        ];
        let face_ranges = [0..3, 3..6, 6..10, 10..14, 14..18];

        for (range, slot) in face_ranges.into_iter().zip(expected_slots) {
            assert!(
                mesh.vertices[range]
                    .iter()
                    .all(|vertex| vertex.material_slot == slot as u32),
                "expected {slot:?} for face"
            );
        }
    }

    #[test]
    fn ball_poles_use_top_and_bottom_slots() {
        let mesh = Mesh::ball(0.5, 6, 12, [1.0; 4]);
        let row = 12 + 1;
        assert!(
            mesh.vertices[..row]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Top as u32)
        );
        assert!(
            mesh.vertices[mesh.vertices.len() - row..]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Bottom as u32)
        );
    }

    #[test]
    fn assign_directional_slots_tags_a_custom_mesh_by_normal() {
        let mut mesh = Mesh::block(1.0, [1.0; 4]);
        for vertex in &mut mesh.vertices {
            vertex.material_slot = MaterialSlot::Base as u32;
        }
        mesh.assign_directional_slots();

        let expected_slots = [
            MaterialSlot::Front,
            MaterialSlot::Back,
            MaterialSlot::Top,
            MaterialSlot::Bottom,
            MaterialSlot::Right,
            MaterialSlot::Left,
        ];
        for (vertices, slot) in mesh.vertices.chunks_exact(4).zip(expected_slots) {
            assert!(
                vertices
                    .iter()
                    .all(|vertex| vertex.material_slot == slot as u32)
            );
        }
    }
}
