use glam::Vec2;

use super::*;

const SHADOW_SPLIT_UNIFORM_WEIGHT: f32 = 0.5;

impl Renderer {
    pub(super) fn prepare_local_lights(
        &mut self,
        scene: &RenderScene<'_>,
        camera_planes: &[Vec4; 6],
    ) -> PreparedLocalShadows {
        let camera = &scene.camera;
        let mut candidates = std::mem::take(&mut self.local_light_scratch);
        candidates.clear();

        for (node, parent_transform) in scene.nodes() {
            let Some(light) = &node.light else {
                continue;
            };
            let candidate = match light.kind {
                RenderLightKind::Point { range } => {
                    point_light_candidate(light, parent_transform, range, camera, camera_planes)
                }
                RenderLightKind::Spot { face, angle } => directional_light_candidate(
                    light,
                    parent_transform,
                    face,
                    angle,
                    false,
                    camera,
                    camera_planes,
                ),
                RenderLightKind::Surface { face, angle } => directional_light_candidate(
                    light,
                    parent_transform,
                    face,
                    angle,
                    true,
                    camera,
                    camera_planes,
                ),
            };
            if let Some(candidate) = candidate {
                candidates.push(candidate);
            }
        }

        if candidates.len() > MAX_LOCAL_LIGHTS {
            candidates.select_nth_unstable_by(MAX_LOCAL_LIGHTS, |a, b| b.score.total_cmp(&a.score));
            candidates.truncate(MAX_LOCAL_LIGHTS);
        }
        candidates.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));

        let mut uniform = LocalLightsUniform::zeroed();
        let mut shadow_planes = [[Vec4::ZERO; 6]; MAX_LOCAL_SHADOW_LAYERS];
        let mut shadow_excluded_casters = [None; MAX_LOCAL_SHADOW_LAYERS];
        let mut shadow_layer_count = 0;
        for (light_index, candidate) in candidates.iter_mut().enumerate() {
            let requested_layers = candidate.shadow_layer_count;
            if requested_layers != 0
                && shadow_layer_count + requested_layers <= MAX_LOCAL_SHADOW_LAYERS
            {
                candidate.raw.params[1] = shadow_layer_count as f32;
                candidate.raw.params[2] = requested_layers as f32;
                for matrix in candidate
                    .shadow_view_projections
                    .iter()
                    .take(requested_layers)
                {
                    let layer = shadow_layer_count;
                    uniform.shadow_view_projections[layer] = matrix.to_cols_array_2d();
                    shadow_planes[layer] = frustum_planes(*matrix);
                    shadow_excluded_casters[layer] = Some(candidate.shadow_excluded_caster);
                    self.queue.write_buffer(
                        &self.shadow_camera_buffer,
                        (SHADOW_CASCADE_COUNT + layer) as u64
                            * u64::from(self.shadow_camera_stride),
                        bytemuck::bytes_of(&ShadowCameraUniform {
                            light_view_projection: matrix.to_cols_array_2d(),
                        }),
                    );
                    shadow_layer_count += 1;
                }
            }
            uniform.lights[light_index] = candidate.raw;
        }
        uniform.params[0] = candidates.len() as u32;
        self.queue
            .write_buffer(&self.local_lights_buffer, 0, bytemuck::bytes_of(&uniform));
        if shadow_layer_count != 0 && self._local_shadow_texture.width() != LOCAL_SHADOW_MAP_SIZE {
            let (texture, view, layer_views) =
                create_local_shadow_texture(&self.device, LOCAL_SHADOW_MAP_SIZE);
            self._local_shadow_texture = texture;
            self.local_shadow_view = view;
            self.local_shadow_layer_views = layer_views;
            self.refresh_environment_binding();
        }
        self.local_shadow_layer_count = shadow_layer_count;
        self.local_light_scratch = candidates;
        PreparedLocalShadows {
            planes: shadow_planes,
            excluded_casters: shadow_excluded_casters,
        }
    }
}

fn light_face_frame(parent: LightParent, face: Face) -> (Vec3, Vec3, Vec4, Vec4) {
    let (_, rotation, translation) = parent.transform.to_scale_rotation_translation();
    let pose = Mat4::from_rotation_translation(rotation, translation);
    let size = parent.transform.to_scale_rotation_translation().0.abs();
    let (normal, normal_extent, axis_u, half_width, axis_v, half_height) = match face {
        Face::Top => (
            Vec3::Y,
            size.y * 0.5,
            Vec3::X,
            size.x * 0.5,
            Vec3::Z,
            size.z * 0.5,
        ),
        Face::Bottom => (
            Vec3::NEG_Y,
            size.y * 0.5,
            Vec3::X,
            size.x * 0.5,
            Vec3::Z,
            size.z * 0.5,
        ),
        Face::Front => (
            Vec3::Z,
            size.z * 0.5,
            Vec3::X,
            size.x * 0.5,
            Vec3::Y,
            size.y * 0.5,
        ),
        Face::Back => (
            Vec3::NEG_Z,
            size.z * 0.5,
            Vec3::X,
            size.x * 0.5,
            Vec3::Y,
            size.y * 0.5,
        ),
        Face::Left => (
            Vec3::NEG_X,
            size.x * 0.5,
            Vec3::Z,
            size.z * 0.5,
            Vec3::Y,
            size.y * 0.5,
        ),
        Face::Right => (
            Vec3::X,
            size.x * 0.5,
            Vec3::Z,
            size.z * 0.5,
            Vec3::Y,
            size.y * 0.5,
        ),
    };
    let direction = pose.transform_vector3(normal).normalize_or_zero();
    let position = translation + direction * normal_extent;
    let axis_u = pose
        .transform_vector3(axis_u)
        .normalize_or_zero()
        .extend(half_width);
    let axis_v = pose
        .transform_vector3(axis_v)
        .normalize_or_zero()
        .extend(half_height);
    (position, direction, axis_u, axis_v)
}

fn local_light_color(color: Color3, brightness: f32) -> Option<([f32; 4], f32)> {
    let brightness = finite_nonnegative(brightness);
    let color = lighting_color(color);
    let color_strength = color[0].max(color[1]).max(color[2]);
    (brightness > 0.0 && color_strength > 0.0).then_some((color, brightness))
}

fn local_light_score(
    position: Vec3,
    range: f32,
    source_radius: f32,
    color: [f32; 4],
    brightness: f32,
    camera_position: Vec3,
) -> f32 {
    let distance = (position.distance(camera_position) - source_radius).max(0.0);
    let attenuation = if distance < range {
        let normalized_distance = distance / range;
        let range_falloff = (1.0 - normalized_distance.powi(4)).max(0.0);
        range_falloff.powi(2) / distance.powi(2).max(1.0)
    } else {
        0.25 / (distance - range + 1.0).powi(2)
    };
    color[0].max(color[1]).max(color[2]) * brightness * attenuation
}

fn directional_light_relevance(
    camera: &RenderCamera,
    position: Vec3,
    direction: Vec3,
    angle: f32,
    range: f32,
) -> f32 {
    let view = camera.pose.inverse();
    let view_position = view.transform_point3(position);
    let view_direction = view.transform_vector3(direction);
    let source_depth = -view_position.z;
    let tan_half_fovy = (camera.fovy * 0.5).tan();
    let outer_cosine = (angle * 0.5).to_radians().cos();
    let inner_cosine = outer_cosine + (1.0 - outer_cosine) * 0.15;
    let mut relevance: f32 = 0.0;

    let mut evaluate_probe = |probe: Vec3| {
        let probe = camera.pose.transform_point3(probe);
        let Some(to_probe) = (probe - position).try_normalize() else {
            return;
        };
        let cosine = direction.dot(to_probe);
        let amount =
            ((cosine - outer_cosine) / (inner_cosine - outer_cosine).max(1e-5)).clamp(0.0, 1.0);
        let smooth_amount = amount * amount * (3.0 - 2.0 * amount);
        relevance = relevance.max(smooth_amount);
    };

    for depth in [
        camera.znear,
        camera.zfar,
        source_depth,
        source_depth + range * 0.5,
        source_depth + range,
    ] {
        let depth = depth.clamp(camera.znear, camera.zfar);
        let half_height = depth * tan_half_fovy;
        let half_width = half_height * camera.aspect;
        let closest_x = view_position.x.clamp(-half_width, half_width);
        let closest_y = view_position.y.clamp(-half_height, half_height);
        for [x, y] in [
            [closest_x, closest_y],
            [0.0, 0.0],
            [-half_width, -half_height],
            [-half_width, half_height],
            [half_width, -half_height],
            [half_width, half_height],
            [closest_x, -half_height],
            [closest_x, half_height],
            [-half_width, closest_y],
            [half_width, closest_y],
        ] {
            evaluate_probe(Vec3::new(x, y, -depth));
        }

        if view_direction.z.abs() > 1e-6 {
            let distance = (-depth - view_position.z) / view_direction.z;
            if distance >= 0.0 {
                let axis_point = view_position + view_direction * distance;
                if axis_point.x.abs() <= half_width && axis_point.y.abs() <= half_height {
                    evaluate_probe(axis_point);
                }
            }
        } else if (depth - source_depth).abs() <= 1e-4 {
            if view_direction.x.abs() > 1e-6 {
                for x in [-half_width, half_width] {
                    let distance = (x - view_position.x) / view_direction.x;
                    let y = view_position.y + view_direction.y * distance;
                    if distance >= 0.0 && y.abs() <= half_height {
                        evaluate_probe(Vec3::new(x, y, -depth));
                    }
                }
            }
            if view_direction.y.abs() > 1e-6 {
                for y in [-half_height, half_height] {
                    let distance = (y - view_position.y) / view_direction.y;
                    let x = view_position.x + view_direction.x * distance;
                    if distance >= 0.0 && x.abs() <= half_width {
                        evaluate_probe(Vec3::new(x, y, -depth));
                    }
                }
            }
        }
    }

    relevance
}

fn point_light_candidate(
    light: &RenderLight,
    parent_transform: Mat4,
    range: f32,
    camera: &RenderCamera,
    camera_planes: &[Vec4; 6],
) -> Option<LocalLightCandidate> {
    let (color, brightness) = local_light_color(light.color, light.brightness)?;
    let range = finite_nonnegative(range).min(10_000.0);
    if range <= 0.0 {
        return None;
    }
    let position = parent_transform.w_axis.truncate();
    if !sphere_visible(camera_planes, position, range) {
        return None;
    }
    let shadow_view_projections = if light.shadows {
        point_shadow_view_projections(position, 0.03, range)
    } else {
        [Mat4::IDENTITY; 6]
    };
    Some(LocalLightCandidate {
        raw: LocalLightRaw {
            position_range: position.extend(range).to_array(),
            color_brightness: [color[0], color[1], color[2], brightness],
            direction_type: [0.0, 0.0, 0.0, 0.0],
            axis_u_half_width: [0.0; 4],
            axis_v_half_height: [0.0; 4],
            params: [0.0, -1.0, 0.0, 0.0],
        },
        shadow_view_projections,
        shadow_layer_count: if light.shadows { 6 } else { 0 },
        shadow_excluded_caster: light.parent_id,
        score: local_light_score(position, range, 0.0, color, brightness, camera.position()),
    })
}

fn directional_light_candidate(
    light: &RenderLight,
    parent_transform: Mat4,
    face: Face,
    angle: f32,
    surface: bool,
    camera: &RenderCamera,
    camera_planes: &[Vec4; 6],
) -> Option<LocalLightCandidate> {
    let (color, brightness) = local_light_color(light.color, light.brightness)?;
    let angle = finite_or(angle, 0.0).clamp(0.0, 180.0);
    if angle <= 0.0 {
        return None;
    }
    let range = (brightness.sqrt() * LOCAL_LIGHT_DERIVED_RANGE).clamp(0.03, LOCAL_LIGHT_MAX_RANGE);
    let parent = LightParent {
        transform: parent_transform,
    };
    let (face_position, direction, axis_u, axis_v) = light_face_frame(parent, face);
    let position = face_position + direction * 0.01;
    let source_radius = if surface {
        axis_u.w.hypot(axis_v.w)
    } else {
        0.0
    };
    if direction == Vec3::ZERO || !sphere_visible(camera_planes, position, range + source_radius) {
        return None;
    }
    let shadow_view_projections = if light.shadows {
        let mut matrices = [Mat4::IDENTITY; 6];
        matrices[0] = cone_shadow_view_projection(position, direction, angle, range);
        matrices
    } else {
        [Mat4::IDENTITY; 6]
    };
    Some(LocalLightCandidate {
        raw: LocalLightRaw {
            position_range: position.extend(range).to_array(),
            color_brightness: [color[0], color[1], color[2], brightness],
            direction_type: direction.extend(if surface { 2.0 } else { 1.0 }).to_array(),
            axis_u_half_width: if surface { axis_u.to_array() } else { [0.0; 4] },
            axis_v_half_height: if surface { axis_v.to_array() } else { [0.0; 4] },
            params: [(angle * 0.5).to_radians().cos(), -1.0, 0.0, 0.0],
        },
        shadow_view_projections,
        shadow_layer_count: usize::from(light.shadows),
        shadow_excluded_caster: light.parent_id,
        score: local_light_score(
            position,
            range,
            source_radius,
            color,
            brightness,
            camera.position(),
        ) * directional_light_relevance(camera, position, direction, angle, range),
    })
}

pub(super) fn cone_shadow_view_projection(
    position: Vec3,
    direction: Vec3,
    angle: f32,
    range: f32,
) -> Mat4 {
    let up = if direction.dot(Vec3::Y).abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = glam::camera::rh::view::look_at_mat4(position, position + direction, up);
    let projection = glam::camera::rh::proj::directx::perspective(
        angle.clamp(1.0, 179.0).to_radians(),
        1.0,
        0.03,
        range.max(0.031),
    );
    projection * view
}

pub(super) fn point_shadow_view_projections(position: Vec3, near: f32, far: f32) -> [Mat4; 6] {
    let directions = [
        (Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::NEG_Y),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::NEG_Z),
        (Vec3::Z, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_Y),
    ];
    let projection = glam::camera::rh::proj::directx::perspective(
        std::f32::consts::FRAC_PI_2,
        1.0,
        near,
        far.max(near + 0.001),
    );
    directions.map(|(direction, up)| {
        projection * glam::camera::rh::view::look_at_mat4(position, position + direction, up)
    })
}

pub(super) fn light_view_projections(
    camera: &RenderCamera,
    light_direction: Vec3,
    shadow_map_size: u32,
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
        let logarithmic = near * (far / near).powf(fraction);
        let uniform = near + (far - near) * fraction;
        let cascade_far = logarithmic * (1.0 - SHADOW_SPLIT_UNIFORM_WEIGHT)
            + uniform * SHADOW_SPLIT_UNIFORM_WEIGHT;
        splits[cascade] = cascade_far;

        let corners = camera_frustum_slice_corners(camera, cascade_near, cascade_far);
        let center = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
        let (mut extent_x, mut extent_y): (f32, f32) = (0.0, 0.0);
        for corner in corners {
            let offset = corner - center;
            extent_x = extent_x.max(offset.dot(light_right).abs());
            extent_y = extent_y.max(offset.dot(light_up).abs());
        }
        extent_x = ((extent_x + 1.0 / 16.0) * 16.0).ceil() / 16.0;
        extent_y = ((extent_y + 1.0 / 16.0) * 16.0).ceil() / 16.0;
        let texel_size_x = 2.0 * extent_x / shadow_map_size as f32;
        let texel_size_y = 2.0 * extent_y / shadow_map_size as f32;
        let texel_size = texel_size_x.max(texel_size_y);
        texel_sizes[cascade] = texel_size;
        let center_x = center.dot(light_right);
        let center_y = center.dot(light_up);
        let snapped_center = center
            + light_right * ((center_x / texel_size_x).round() * texel_size_x - center_x)
            + light_up * ((center_y / texel_size_y).round() * texel_size_y - center_y);
        let light_position =
            snapped_center + light_direction * (extent_x.max(extent_y) + SHADOW_CASTER_MARGIN);
        let view = glam::camera::rh::view::look_at_mat4(light_position, snapped_center, up);
        let (mut min_depth, mut max_depth) = (f32::MAX, f32::MIN);
        for corner in corners {
            let depth = -view.transform_point3(corner).z;
            min_depth = min_depth.min(depth);
            max_depth = max_depth.max(depth);
        }
        let shadow_near = (min_depth - SHADOW_CASTER_MARGIN).max(0.001);
        let shadow_far = (max_depth + SHADOW_RECEIVER_MARGIN).max(shadow_near + 0.001);
        let projection = glam::camera::rh::proj::directx::orthographic(
            -extent_x,
            extent_x,
            -extent_y,
            extent_y,
            shadow_near,
            shadow_far,
        );
        matrices[cascade] = projection * view;
        cascade_near = cascade_far;
    }

    (matrices, splits, texel_sizes)
}

struct ShadowReceiverBounds {
    transform: Mat4,
    inverse: Mat4,
    half_extent: f32,
    corners: [Vec3; 8],
}

fn clip_shadow_receiver_edge(mut edge: [Vec3; 2], planes: &[Vec4; 6]) -> Option<[Vec3; 2]> {
    for plane in planes {
        let a = plane.dot(edge[0].extend(1.0)) + 1e-5;
        let b = plane.dot(edge[1].extend(1.0)) + 1e-5;
        if a < 0.0 && b < 0.0 {
            return None;
        }
        if (a < 0.0) != (b < 0.0) {
            let intersection = edge[0].lerp(edge[1], a / (a - b));
            edge[usize::from(a >= 0.0)] = intersection;
        }
    }
    Some(edge)
}

fn shadow_receiver_clip_bounds(
    receiver: &ShadowReceiverBounds,
    frustum_corners: [Vec3; 8],
    frustum_planes: &[Vec4; 6],
    light_view_projection: Mat4,
) -> Option<(Vec2, Vec2)> {
    let mut enclosed = true;
    for plane in frustum_planes {
        let (minimum, maximum) = receiver.corners.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), point| {
                let distance = plane.dot(point.extend(1.0));
                (minimum.min(distance), maximum.max(distance))
            },
        );
        if maximum < -1e-5 {
            return None;
        }
        enclosed &= minimum >= -1e-5;
    }
    let mut minimum = Vec2::splat(f32::INFINITY);
    let mut maximum = Vec2::splat(f32::NEG_INFINITY);
    let mut include = |point: Vec3| {
        let point = light_view_projection.transform_point3(point).truncate();
        minimum = minimum.min(point);
        maximum = maximum.max(point);
    };
    if enclosed {
        for point in receiver.corners {
            include(point);
        }
        return Some((minimum, maximum));
    }
    let local_frustum = frustum_corners.map(|point| receiver.inverse.transform_point3(point));
    let box_planes = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ]
    .map(|normal| normal.extend(receiver.half_extent));
    for corner in 0..8 {
        for axis in 0..3 {
            let next = corner ^ (1 << axis);
            if corner >= next {
                continue;
            }
            if let Some(edge) = clip_shadow_receiver_edge(
                [receiver.corners[corner], receiver.corners[next]],
                frustum_planes,
            ) {
                for point in edge {
                    include(point);
                }
            }
            if let Some(edge) =
                clip_shadow_receiver_edge([local_frustum[corner], local_frustum[next]], &box_planes)
            {
                for point in edge {
                    include(receiver.transform.transform_point3(point));
                }
            }
        }
    }
    minimum.is_finite().then_some((minimum, maximum))
}

/// Fits directional cascade XY coverage to conservative visible receiver bounds.
pub(super) fn scene_light_view_projections(
    scene: &RenderScene<'_>,
    light_direction: Vec3,
    shadow_map_size: u32,
) -> (
    [Mat4; SHADOW_CASCADE_COUNT],
    [f32; SHADOW_CASCADE_COUNT],
    [f32; SHADOW_CASCADE_COUNT],
) {
    let camera = &scene.camera;
    let result = light_view_projections(camera, light_direction, shadow_map_size);
    if !scene.lighting.shadows_enabled() {
        return result;
    }
    let camera_planes = frustum_planes(camera.view_projection);
    let mut receivers = Vec::new();
    for (node, transform) in scene.nodes() {
        let Some(mesh) = &node.mesh else {
            continue;
        };
        if finite_or(mesh.transparency, 0.0) >= 1.0
            || !sphere_visible(
                &camera_planes,
                transform.w_axis.truncate(),
                scaled_bounding_radius(mesh.bounding_radius, transform),
            )
        {
            continue;
        }
        let inverse = transform.inverse();
        if !inverse.is_finite() {
            return result;
        }
        let half_extent = match mesh.geometry {
            RenderGeometry::Primitive(_) => 0.5,
            RenderGeometry::Custom { .. } => mesh.bounding_radius,
        };
        receivers.push(ShadowReceiverBounds {
            transform,
            inverse,
            half_extent,
            corners: std::array::from_fn(|index| {
                let point = Vec3::new(
                    if index & 1 == 0 {
                        -half_extent
                    } else {
                        half_extent
                    },
                    if index & 2 == 0 {
                        -half_extent
                    } else {
                        half_extent
                    },
                    if index & 4 == 0 {
                        -half_extent
                    } else {
                        half_extent
                    },
                );
                transform.transform_point3(point)
            }),
        });
    }
    let (mut matrices, splits, mut texel_sizes) = result;
    let forward = camera.forward;
    let camera_depth = forward.dot(camera.position());
    let mut cascade_near = camera.znear.max(0.001);
    for cascade in 0..SHADOW_CASCADE_COUNT {
        let fit_near = if cascade == 0 {
            cascade_near
        } else {
            (cascade_near - (cascade_near * 0.1).max(0.05)).max(camera.znear)
        };
        let corners = camera_frustum_slice_corners(camera, fit_near, splits[cascade]);
        let mut planes = camera_planes;
        planes[4] = forward.extend(-camera_depth - fit_near);
        planes[5] = (-forward).extend(camera_depth + splits[cascade]);
        let matrix = matrices[cascade];
        let mut minimum = Vec2::splat(f32::INFINITY);
        let mut maximum = Vec2::splat(f32::NEG_INFINITY);
        for receiver in &receivers {
            if let Some((low, high)) =
                shadow_receiver_clip_bounds(receiver, corners, &planes, matrix)
            {
                minimum = minimum.min(low);
                maximum = maximum.max(high);
            }
        }
        if minimum.is_finite() {
            let original_scale = Vec2::new(
                Vec3::new(matrix.x_axis.x, matrix.y_axis.x, matrix.z_axis.x).length(),
                Vec3::new(matrix.x_axis.y, matrix.y_axis.y, matrix.z_axis.y).length(),
            );
            let original_offset = matrix.w_axis.truncate().truncate();
            let center = ((minimum + maximum) * 0.5 - original_offset) / original_scale;
            // Four guard texels cover the filter support and the half-texel center snap.
            let guard_scale = shadow_map_size as f32 / (shadow_map_size as f32 - 8.0);
            let extent = ((maximum - minimum) * 0.5 / original_scale * guard_scale * 16.0)
                .ceil()
                .max(Vec2::ONE)
                / 16.0;
            let texel_step = extent * (2.0 / shadow_map_size as f32);
            let snapped_center = (center / texel_step).round() * texel_step;
            let crop_center = snapped_center * original_scale + original_offset;
            let crop_scale = (extent * original_scale).recip();
            // Preserve light-depth coverage for off-screen casters whose projections reach a receiver.
            matrices[cascade] = Mat4::from_scale(Vec3::new(crop_scale.x, crop_scale.y, 1.0))
                * Mat4::from_translation(Vec3::new(-crop_center.x, -crop_center.y, 0.0))
                * matrix;
            texel_sizes[cascade] = texel_step.max_element();
        }
        cascade_near = splits[cascade];
    }
    (matrices, splits, texel_sizes)
}

pub(super) fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

pub(super) fn finite_nonnegative(value: f32) -> f32 {
    finite_or(value, 0.0).max(0.0)
}

pub(super) fn color_channel(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, 1.0)
}

pub(super) fn lighting_color(color: Color3) -> [f32; 4] {
    [
        color_channel(color.r),
        color_channel(color.g),
        color_channel(color.b),
        0.0,
    ]
}

pub(super) fn scaled_color(color: Color3, scale: f32) -> [f32; 4] {
    let scale = finite_nonnegative(scale);
    [
        color_channel(color.r) * scale,
        color_channel(color.g) * scale,
        color_channel(color.b) * scale,
        0.0,
    ]
}

pub(super) fn pack_shadow_values(values: [f32; SHADOW_CASCADE_COUNT]) -> [[f32; 4]; 2] {
    let mut packed = [[values[SHADOW_CASCADE_COUNT - 1]; 4]; 2];
    for (index, value) in values.into_iter().enumerate() {
        packed[index / 4][index % 4] = value;
    }
    packed
}

pub(super) fn camera_frustum_slice_corners(
    camera: &RenderCamera,
    near: f32,
    far: f32,
) -> [Vec3; 8] {
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
    corners.map(|corner| camera.pose.transform_point3(corner))
}
