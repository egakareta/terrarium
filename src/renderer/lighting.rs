use super::*;

impl Renderer {
    pub(super) fn prepare_local_lights(
        &mut self,
        workspace: &Workspace,
        camera_planes: &[Vec4; 6],
    ) -> PreparedLocalShadows {
        let camera = &workspace.current_camera;
        let mut candidates = std::mem::take(&mut self.local_light_scratch);
        candidates.clear();

        for instance in workspace.instances() {
            let candidate = if let Some(light) = instance.downcast_ref::<PointLight>() {
                point_light_candidate(workspace, light, camera, camera_planes)
            } else if let Some(light) = instance.downcast_ref::<SpotLight>() {
                spot_light_candidate(workspace, light, camera, camera_planes)
            } else if let Some(light) = instance.downcast_ref::<SurfaceLight>() {
                surface_light_candidate(workspace, light, camera, camera_planes)
            } else {
                None
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

pub(super) fn light_parent(workspace: &Workspace, light: &dyn Instance) -> Option<LightParent> {
    let mut parent_id = light.parent();
    while let Some(id) = parent_id {
        let parent = workspace.instance(id)?;
        if let Some(part) = parent.downcast_ref::<Part>() {
            return Some(LightParent {
                id,
                pivot: part.pivot(),
                size: part.size().abs(),
            });
        }
        #[cfg(feature = "meshpart")]
        if let Some(part) = parent.downcast_ref::<MeshPart>() {
            return Some(LightParent {
                id,
                pivot: part.pivot(),
                size: part.size().abs(),
            });
        }
        if let Some(part) = parent.downcast_ref::<BasePart>() {
            return Some(LightParent {
                id,
                pivot: part.pivot(),
                size: part.size().abs(),
            });
        }
        parent_id = parent.parent();
    }
    None
}

pub(super) fn light_face_frame(parent: LightParent, face: Face) -> (Vec3, Vec3, Vec4, Vec4) {
    let (normal, normal_extent, axis_u, half_width, axis_v, half_height) = match face {
        Face::Top => (
            Vec3::Y,
            parent.size.y * 0.5,
            Vec3::X,
            parent.size.x * 0.5,
            Vec3::Z,
            parent.size.z * 0.5,
        ),
        Face::Bottom => (
            Vec3::NEG_Y,
            parent.size.y * 0.5,
            Vec3::X,
            parent.size.x * 0.5,
            Vec3::Z,
            parent.size.z * 0.5,
        ),
        Face::Front => (
            Vec3::Z,
            parent.size.z * 0.5,
            Vec3::X,
            parent.size.x * 0.5,
            Vec3::Y,
            parent.size.y * 0.5,
        ),
        Face::Back => (
            Vec3::NEG_Z,
            parent.size.z * 0.5,
            Vec3::X,
            parent.size.x * 0.5,
            Vec3::Y,
            parent.size.y * 0.5,
        ),
        Face::Left => (
            Vec3::NEG_X,
            parent.size.x * 0.5,
            Vec3::Z,
            parent.size.z * 0.5,
            Vec3::Y,
            parent.size.y * 0.5,
        ),
        Face::Right => (
            Vec3::X,
            parent.size.x * 0.5,
            Vec3::Z,
            parent.size.z * 0.5,
            Vec3::Y,
            parent.size.y * 0.5,
        ),
    };
    let direction = parent.pivot.transform_vector3(normal).normalize_or_zero();
    let position = parent.pivot.w_axis.truncate() + direction * normal_extent;
    let axis_u = parent
        .pivot
        .transform_vector3(axis_u)
        .normalize_or_zero()
        .extend(half_width);
    let axis_v = parent
        .pivot
        .transform_vector3(axis_v)
        .normalize_or_zero()
        .extend(half_height);
    (position, direction, axis_u, axis_v)
}

pub(super) fn local_light_color(color: Color3, brightness: f32) -> Option<([f32; 4], f32)> {
    let brightness = finite_nonnegative(brightness);
    let color = lighting_color(color);
    let color_strength = color[0].max(color[1]).max(color[2]);
    (brightness > 0.0 && color_strength > 0.0).then_some((color, brightness))
}

pub(super) fn local_light_score(
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

pub(super) fn directional_light_relevance(
    camera: &Camera,
    position: Vec3,
    direction: Vec3,
    angle: f32,
    range: f32,
) -> f32 {
    let view = camera.pivot().inverse();
    let view_position = view.transform_point3(position);
    let view_direction = view.transform_vector3(direction);
    let source_depth = -view_position.z;
    let tan_half_fovy = (camera.fovy * 0.5).tan();
    let outer_cosine = (angle * 0.5).to_radians().cos();
    let inner_cosine = outer_cosine + (1.0 - outer_cosine) * 0.15;
    let mut relevance: f32 = 0.0;

    let mut evaluate_probe = |probe: Vec3| {
        let probe = camera.pivot().transform_point3(probe);
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

pub(super) fn point_light_candidate(
    workspace: &Workspace,
    light: &PointLight,
    camera: &Camera,
    camera_planes: &[Vec4; 6],
) -> Option<LocalLightCandidate> {
    let parent = light_parent(workspace, light)?;
    let (color, brightness) = local_light_color(light.color, light.brightness)?;
    let range = finite_nonnegative(light.range).min(10_000.0);
    if range <= 0.0 {
        return None;
    }
    let position = parent.pivot.w_axis.truncate();
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
        shadow_excluded_caster: parent.id,
        score: local_light_score(position, range, 0.0, color, brightness, camera.position()),
    })
}

pub(super) fn spot_light_candidate(
    workspace: &Workspace,
    light: &SpotLight,
    camera: &Camera,
    camera_planes: &[Vec4; 6],
) -> Option<LocalLightCandidate> {
    directional_light_candidate(
        workspace,
        light,
        light.face,
        light.angle,
        light.color,
        light.brightness,
        light.shadows,
        false,
        camera,
        camera_planes,
    )
}

pub(super) fn surface_light_candidate(
    workspace: &Workspace,
    light: &SurfaceLight,
    camera: &Camera,
    camera_planes: &[Vec4; 6],
) -> Option<LocalLightCandidate> {
    directional_light_candidate(
        workspace,
        light,
        light.face,
        light.angle,
        light.color,
        light.brightness,
        light.shadows,
        true,
        camera,
        camera_planes,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn directional_light_candidate(
    workspace: &Workspace,
    light: &dyn Instance,
    face: Face,
    angle: f32,
    color: Color3,
    brightness: f32,
    shadows: bool,
    surface: bool,
    camera: &Camera,
    camera_planes: &[Vec4; 6],
) -> Option<LocalLightCandidate> {
    let parent = light_parent(workspace, light)?;
    let (color, brightness) = local_light_color(color, brightness)?;
    let angle = finite_or(angle, 0.0).clamp(0.0, 180.0);
    if angle <= 0.0 {
        return None;
    }
    let range = (brightness.sqrt() * LOCAL_LIGHT_DERIVED_RANGE).clamp(0.03, LOCAL_LIGHT_MAX_RANGE);
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
    let shadow_view_projections = if shadows {
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
        shadow_layer_count: usize::from(shadows),
        shadow_excluded_caster: parent.id,
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
    let view = crate::glam::camera::rh::view::look_at_mat4(position, position + direction, up);
    let projection = crate::glam::camera::rh::proj::directx::perspective(
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
    let projection = crate::glam::camera::rh::proj::directx::perspective(
        std::f32::consts::FRAC_PI_2,
        1.0,
        near,
        far.max(near + 0.001),
    );
    directions.map(|(direction, up)| {
        projection * crate::glam::camera::rh::view::look_at_mat4(position, position + direction, up)
    })
}

pub(super) fn light_view_projections(
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

pub(super) fn camera_frustum_slice_corners(camera: &Camera, near: f32, far: f32) -> [Vec3; 8] {
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
