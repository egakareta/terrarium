struct Camera {
    view_projection: mat4x4<f32>,
    light_view_projections: array<mat4x4<f32>, 7>,
    camera_position: vec4<f32>,
    camera_forward: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    ambient_color: vec4<f32>,
    outdoor_ambient_color: vec4<f32>,
    color_shift_top: vec4<f32>,
    color_shift_bottom: vec4<f32>,
    shadow_color: vec4<f32>,
    // x: daylight factor, y: shadow enable, z: shadow softness, w: exposure.
    lighting_params: vec4<f32>,
    fog_color: vec4<f32>,
    // x: fog start distance, y: fog end distance.
    fog_params: vec4<f32>,
    shadow_cascade_splits: array<vec4<f32>, 2>,
    shadow_texel_sizes: array<vec4<f32>, 2>,
    // x: environment cubemap mip level count,
    // y: diffuse environment scale,
    // z: specular environment scale,
    // w: 1.0 when an environment is bound, 0.0 for the ambient fallback.
    environment_params: vec4<f32>,
};

struct ShadowCamera {
    light_view_projection: mat4x4<f32>,
};

struct LocalLight {
    // xyz: source position, w: finite influence range.
    position_range: vec4<f32>,
    // rgb: color, w: brightness.
    color_brightness: vec4<f32>,
    // xyz: emission direction, w: 0 point, 1 spot, 2 surface.
    direction_type: vec4<f32>,
    // xyz: surface axis, w: half width.
    axis_u_half_width: vec4<f32>,
    // xyz: surface axis, w: half height.
    axis_v_half_height: vec4<f32>,
    // x: outer cone cosine, y: first shadow layer (-1 if disabled),
    // z: shadow layer count.
    params: vec4<f32>,
};

struct LocalLights {
    // x: active local light count.
    params: vec4<u32>,
    lights: array<LocalLight, 64>,
    shadow_view_projections: array<mat4x4<f32>, 8>,
};

override FRAMEBUFFER_IS_SRGB: f32 = 1.0;

const SHADOW_CASCADE_COUNT: u32 = 7u;
// Keep receiver comparisons a few quantization steps in front of the
// Depth16Unorm shadow map. The world-space normal offset contributes almost no
// depth at a low sun angle, so the tiny epsilon alone allows ground receivers
// to self-shadow into a jagged second silhouette.
const DIRECTIONAL_SHADOW_DEPTH_BIAS: f32 = 0.0001;

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var shadow_map: texture_depth_2d_array;

@group(0) @binding(2)
var shadow_sampler: sampler_comparison;

@group(0) @binding(3)
var<uniform> shadow_camera: ShadowCamera;

@group(0) @binding(4)
var environment_map: texture_cube<f32>;

@group(0) @binding(5)
var environment_sampler: sampler;

@group(0) @binding(6)
var<uniform> local_lights: LocalLights;

@group(0) @binding(7)
var local_shadow_map: texture_depth_2d_array;

@group(1) @binding(0)
var material_texture_0: texture_2d_array<f32>;

@group(1) @binding(1)
var material_texture_1: texture_2d_array<f32>;

@group(1) @binding(2)
var material_texture_2: texture_2d_array<f32>;

@group(1) @binding(3)
var material_texture_3: texture_2d_array<f32>;

@group(1) @binding(4)
var material_texture_4: texture_2d_array<f32>;

@group(1) @binding(5)
var material_texture_5: texture_2d_array<f32>;

@group(1) @binding(6)
var material_sampler_0: sampler;

@group(1) @binding(7)
var material_sampler_1: sampler;

@group(1) @binding(8)
var material_sampler_2: sampler;

@group(1) @binding(9)
var material_sampler_3: sampler;

@group(1) @binding(10)
var material_sampler_4: sampler;

@group(1) @binding(11)
var material_sampler_5: sampler;

// Per-face PBR factors for every deduplicated part material set. Each set
// packs six directional slots in MaterialSlot order as three vec4s per slot:
// base color, emissive RGB + roughness, then metallic, projection, occlusion
// strength, and dielectric reflectance.
//
// Stored in a 2D float texture (width = MATERIAL_VEC4S_PER_SET, height =
// set count) instead of a storage buffer: OpenGL ES / WebGL backends expose
// zero storage buffers in the fragment stage, while textureLoad works
// everywhere.
@group(2) @binding(0)
var material_factor_texture: texture_2d<f32>;

const MATERIAL_VEC4S_PER_SLOT: u32 = 3u;
const MATERIAL_VEC4S_PER_SET: u32 = 18u;

fn load_material_vec4(set_index: u32, vec4_index: u32) -> vec4<f32> {
    return textureLoad(material_factor_texture, vec2<u32>(vec4_index, set_index), 0);
}

fn slot_base_color(set_index: u32, slot: u32) -> vec4<f32> {
    return load_material_vec4(set_index, slot * MATERIAL_VEC4S_PER_SLOT);
}

fn slot_emissive_roughness(set_index: u32, slot: u32) -> vec4<f32> {
    return load_material_vec4(set_index, slot * MATERIAL_VEC4S_PER_SLOT + 1u);
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) vertex_color: vec4<f32>,
    @location(5) material_slot: u32,
    @location(6) model_0: vec3<f32>,
    @location(7) model_1: vec3<f32>,
    @location(8) model_2: vec3<f32>,
    @location(9) model_3: vec3<f32>,
    @location(10) normal_scales: vec3<f32>,
    @location(11) tint: vec4<f32>,
    @location(12) material_set: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat) material_slot: u32,
    @location(5) vertex_color: vec4<f32>,
    @location(6) tint: vec4<f32>,
    @location(7) @interpolate(flat) material_set: u32,
};

struct ShadowVertexInput {
    @location(0) position: vec3<f32>,
    @location(6) model_0: vec3<f32>,
    @location(7) model_1: vec3<f32>,
    @location(8) model_2: vec3<f32>,
    @location(9) model_3: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        vec4<f32>(vertex.model_0, 0.0),
        vec4<f32>(vertex.model_1, 0.0),
        vec4<f32>(vertex.model_2, 0.0),
        vec4<f32>(vertex.model_3, 1.0),
    );
    let world_position = model * vec4<f32>(vertex.position, 1.0);
    let normal_matrix = mat3x3<f32>(
        vertex.model_0 * vertex.normal_scales.x,
        vertex.model_1 * vertex.normal_scales.y,
        vertex.model_2 * vertex.normal_scales.z,
    );
    let world_normal = normalize(normal_matrix * vertex.normal);
    let world_tangent = normalize((model * vec4<f32>(vertex.tangent.xyz, 0.0)).xyz);
    output.position = camera.view_projection * world_position;
    output.world_position = world_position.xyz;
    output.normal = world_normal;
    output.tangent = vec4<f32>(world_tangent, vertex.tangent.w);
    output.uv = vertex.uv;
    output.material_slot = vertex.material_slot;
    output.vertex_color = vertex.vertex_color;
    output.tint = vertex.tint;
    output.material_set = vertex.material_set;
    return output;
}

@vertex
fn vs_shadow(vertex: ShadowVertexInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(
        vec4<f32>(vertex.model_0, 0.0),
        vec4<f32>(vertex.model_1, 0.0),
        vec4<f32>(vertex.model_2, 0.0),
        vec4<f32>(vertex.model_3, 1.0),
    );
    return shadow_camera.light_view_projection * model * vec4<f32>(vertex.position, 1.0);
}

fn alpha_roughness(perceptual_roughness: f32) -> f32 {
    let clamped_roughness = clamp(perceptual_roughness, 0.045, 1.0);
    return clamped_roughness * clamped_roughness;
}

fn distribution_ggx(normal_dot_half: f32, alpha: f32) -> f32 {
    let alpha_squared = alpha * alpha;
    let one_minus_normal_dot_half_squared = 1.0 - normal_dot_half * normal_dot_half;
    let scaled_half = normal_dot_half * alpha;
    let denominator = one_minus_normal_dot_half_squared + scaled_half * scaled_half;
    return alpha_squared / max(3.14159265 * denominator * denominator, 0.000001);
}

fn geometry_smith(normal_dot_view: f32, normal_dot_light: f32, alpha: f32) -> f32 {
    let alpha_squared = alpha * alpha;
    let view_lambda = normal_dot_light * sqrt(max(
        normal_dot_view * normal_dot_view * (1.0 - alpha_squared) + alpha_squared,
        0.0,
    ));
    let light_lambda = normal_dot_view * sqrt(max(
        normal_dot_light * normal_dot_light * (1.0 - alpha_squared) + alpha_squared,
        0.0,
    ));
    return 0.5 / max(view_lambda + light_lambda, 0.000001);
}

fn fresnel_schlick(view_dot_half: f32, base_reflectance: vec3<f32>) -> vec3<f32> {
    let reflectance_at_grazing = clamp(dot(base_reflectance, vec3<f32>(16.5)), 0.0, 1.0);
    return base_reflectance
        + (vec3<f32>(reflectance_at_grazing) - base_reflectance)
            * pow5(1.0 - view_dot_half);
}

fn pow5(value: f32) -> f32 {
    let squared = value * value;
    return squared * squared * value;
}

fn linear_to_srgb(linear_color: vec3<f32>) -> vec3<f32> {
    let color = max(linear_color, vec3<f32>(0.0));
    return select(
        12.92 * color,
        1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
        color > vec3<f32>(0.0031308),
    );
}

// Narkowicz ACES filmic approximation: the same family of curve Three.js
// uses by default. It keeps specular highlights punchy instead of washing
// them out to gray the way Reinhard does.
fn aces_tone_map(color: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp(
        (color * (a * color + b)) / (color * (c * color + d) + e),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

// Bevy 0.20.0's raster PBR path uses this Unreal DFG polynomial for both
// environment response and direct-light multiple-scattering compensation.
fn environment_brdf_f_ab(perceptual_roughness: f32, normal_dot_view: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = perceptual_roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * normal_dot_view)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return max(ab, vec2<f32>(0.00005));
}

fn environment_brdf_approx(base_reflectance: vec3<f32>, f_ab: vec2<f32>) -> vec3<f32> {
    return base_reflectance * f_ab.x + f_ab.y;
}

fn specular_multiscatter(
    distribution: f32,
    visibility: f32,
    fresnel: vec3<f32>,
    base_reflectance: vec3<f32>,
    f_ab: vec2<f32>,
) -> vec3<f32> {
    let energy_compensation = vec3<f32>(1.0)
        + base_reflectance * (1.0 / max(f_ab.x + f_ab.y, 0.00005) - 1.0);
    return distribution * visibility * fresnel * energy_compensation;
}

fn diffuse_burley(
    perceptual_roughness: f32,
    normal_dot_view: f32,
    normal_dot_light: f32,
    light_dot_half: f32,
) -> f32 {
    let grazing_reflectance = 0.5 + 2.0 * perceptual_roughness * light_dot_half * light_dot_half;
    let light_scatter = 1.0 + (grazing_reflectance - 1.0) * pow5(1.0 - normal_dot_light);
    let view_scatter = 1.0 + (grazing_reflectance - 1.0) * pow5(1.0 - normal_dot_view);
    return light_scatter * view_scatter / 3.14159265;
}

fn sample_base_color(slot: u32, uv: vec2<f32>, uv_dx: vec2<f32>, uv_dy: vec2<f32>) -> vec4<f32> {
    if slot == 1u {
        return textureSampleGrad(material_texture_1, material_sampler_1, uv, 0, uv_dx, uv_dy);
    } else if slot == 2u {
        return textureSampleGrad(material_texture_2, material_sampler_2, uv, 0, uv_dx, uv_dy);
    } else if slot == 3u {
        return textureSampleGrad(material_texture_3, material_sampler_3, uv, 0, uv_dx, uv_dy);
    } else if slot == 4u {
        return textureSampleGrad(material_texture_4, material_sampler_4, uv, 0, uv_dx, uv_dy);
    } else if slot == 5u {
        return textureSampleGrad(material_texture_5, material_sampler_5, uv, 0, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler_0, uv, 0, uv_dx, uv_dy);
}

fn unpack_normal(surface_sample: vec4<f32>) -> vec3<f32> {
    let xy = surface_sample.xy * 2.0 - 1.0;
    let z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    return normalize(vec3<f32>(xy, z));
}

fn unpack_triplanar_normal(surface_sample: vec4<f32>) -> vec3<f32> {
    // Byte value 128 is the neutral normal. The tolerance covers quantization
    // when linear map channels are carried by the packed sRGB texture format.
    let encoded_xy = clamp(
        (surface_sample.xy * 255.0 - vec2<f32>(128.0)) / 127.0,
        vec2<f32>(-1.0),
        vec2<f32>(1.0),
    );
    let xy = select(encoded_xy, vec2<f32>(0.0), abs(encoded_xy) < vec2<f32>(0.003));
    let z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    return normalize(vec3<f32>(xy, z));
}

struct TriplanarCoordinates {
    x: vec2<f32>,
    x_dx: vec2<f32>,
    x_dy: vec2<f32>,
    y: vec2<f32>,
    y_dx: vec2<f32>,
    y_dy: vec2<f32>,
    z: vec2<f32>,
    z_dx: vec2<f32>,
    z_dy: vec2<f32>,
    weights: vec3<f32>,
};

fn triplanar_coordinates(
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    position_dx: vec3<f32>,
    position_dy: vec3<f32>,
) -> TriplanarCoordinates {
    let sign_x = select(-1.0, 1.0, world_normal.x >= 0.0);
    let sign_y = select(-1.0, 1.0, world_normal.y >= 0.0);
    let sign_z = select(-1.0, 1.0, world_normal.z >= 0.0);
    let absolute_normal = abs(world_normal);
    let squared_normal = absolute_normal * absolute_normal;
    let weights = squared_normal * squared_normal;
    let normalized_weights = weights / max(dot(weights, vec3<f32>(1.0)), 0.000001);
    return TriplanarCoordinates(
        vec2<f32>(-sign_x * world_position.z, world_position.y),
        vec2<f32>(-sign_x * position_dx.z, position_dx.y),
        vec2<f32>(-sign_x * position_dy.z, position_dy.y),
        vec2<f32>(world_position.x, -sign_y * world_position.z),
        vec2<f32>(position_dx.x, -sign_y * position_dx.z),
        vec2<f32>(position_dy.x, -sign_y * position_dy.z),
        vec2<f32>(sign_z * world_position.x, world_position.y),
        vec2<f32>(sign_z * position_dx.x, position_dx.y),
        vec2<f32>(sign_z * position_dy.x, position_dy.y),
        normalized_weights,
    );
}

fn sample_base_color_triplanar(
    slot: u32,
    coordinates: TriplanarCoordinates,
) -> vec4<f32> {
    let x = sample_base_color(slot, coordinates.x, coordinates.x_dx, coordinates.x_dy);
    let y = sample_base_color(slot, coordinates.y, coordinates.y_dx, coordinates.y_dy);
    let z = sample_base_color(slot, coordinates.z, coordinates.z_dx, coordinates.z_dy);
    return x * coordinates.weights.x + y * coordinates.weights.y + z * coordinates.weights.z;
}

fn sample_emissive_triplanar(
    slot: u32,
    coordinates: TriplanarCoordinates,
) -> vec4<f32> {
    let x = sample_emissive(slot, coordinates.x, coordinates.x_dx, coordinates.x_dy);
    let y = sample_emissive(slot, coordinates.y, coordinates.y_dx, coordinates.y_dy);
    let z = sample_emissive(slot, coordinates.z, coordinates.z_dx, coordinates.z_dy);
    return x * coordinates.weights.x + y * coordinates.weights.y + z * coordinates.weights.z;
}

fn tangent_from_projection_axis(axis: vec3<f32>, world_normal: vec3<f32>) -> vec3<f32> {
    let projected = axis - world_normal * dot(axis, world_normal);
    if dot(projected, projected) > 0.000001 {
        return normalize(projected);
    }
    let fallback_axis = select(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        abs(world_normal.x) > 0.9,
    );
    return normalize(fallback_axis - world_normal * dot(fallback_axis, world_normal));
}

fn normal_from_projection(
    tangent_normal: vec3<f32>,
    world_normal: vec3<f32>,
    projection_tangent: vec3<f32>,
    projection_bitangent: vec3<f32>,
) -> vec3<f32> {
    let tangent = tangent_from_projection_axis(projection_tangent, world_normal);
    let projected_bitangent = projection_bitangent
        - world_normal * dot(projection_bitangent, world_normal)
        - tangent * dot(projection_bitangent, tangent);
    var bitangent = normalize(cross(world_normal, tangent));
    if dot(projected_bitangent, projected_bitangent) > 0.000001 {
        bitangent = normalize(projected_bitangent);
    }
    return normalize(
        tangent * tangent_normal.x + bitangent * tangent_normal.y + world_normal * tangent_normal.z,
    );
}

fn triplanar_mapped_normal(
    x_sample: vec4<f32>,
    y_sample: vec4<f32>,
    z_sample: vec4<f32>,
    world_normal: vec3<f32>,
    coordinates: TriplanarCoordinates,
) -> vec3<f32> {
    let sign_x = select(-1.0, 1.0, world_normal.x >= 0.0);
    let sign_y = select(-1.0, 1.0, world_normal.y >= 0.0);
    let sign_z = select(-1.0, 1.0, world_normal.z >= 0.0);
    let x_normal = normal_from_projection(
        unpack_triplanar_normal(x_sample),
        world_normal,
        vec3<f32>(0.0, 0.0, -sign_x),
        vec3<f32>(0.0, 1.0, 0.0),
    );
    let y_normal = normal_from_projection(
        unpack_triplanar_normal(y_sample),
        world_normal,
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 0.0, -sign_y),
    );
    let z_normal = normal_from_projection(
        unpack_triplanar_normal(z_sample),
        world_normal,
        vec3<f32>(sign_z, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
    );
    return normalize(
        x_normal * coordinates.weights.x
            + y_normal * coordinates.weights.y
            + z_normal * coordinates.weights.z,
    );
}

fn sample_surface(slot: u32, uv: vec2<f32>, uv_dx: vec2<f32>, uv_dy: vec2<f32>) -> vec4<f32> {
    if slot == 1u {
        return textureSampleGrad(material_texture_1, material_sampler_1, uv, 1, uv_dx, uv_dy);
    } else if slot == 2u {
        return textureSampleGrad(material_texture_2, material_sampler_2, uv, 1, uv_dx, uv_dy);
    } else if slot == 3u {
        return textureSampleGrad(material_texture_3, material_sampler_3, uv, 1, uv_dx, uv_dy);
    } else if slot == 4u {
        return textureSampleGrad(material_texture_4, material_sampler_4, uv, 1, uv_dx, uv_dy);
    } else if slot == 5u {
        return textureSampleGrad(material_texture_5, material_sampler_5, uv, 1, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler_0, uv, 1, uv_dx, uv_dy);
}

fn sample_emissive(slot: u32, uv: vec2<f32>, uv_dx: vec2<f32>, uv_dy: vec2<f32>) -> vec4<f32> {
    if slot == 1u {
        return textureSampleGrad(material_texture_1, material_sampler_1, uv, 2, uv_dx, uv_dy);
    } else if slot == 2u {
        return textureSampleGrad(material_texture_2, material_sampler_2, uv, 2, uv_dx, uv_dy);
    } else if slot == 3u {
        return textureSampleGrad(material_texture_3, material_sampler_3, uv, 2, uv_dx, uv_dy);
    } else if slot == 4u {
        return textureSampleGrad(material_texture_4, material_sampler_4, uv, 2, uv_dx, uv_dy);
    } else if slot == 5u {
        return textureSampleGrad(material_texture_5, material_sampler_5, uv, 2, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler_0, uv, 2, uv_dx, uv_dy);
}

fn sample_shadow_compare(
    shadow_uv: vec2<f32>,
    depth: f32,
    layer: u32,
    local_map: bool,
) -> f32 {
    if local_map {
        return textureSampleCompareLevel(
            local_shadow_map,
            shadow_sampler,
            shadow_uv,
            i32(layer),
            depth,
        );
    }
    return textureSampleCompareLevel(shadow_map, shadow_sampler, shadow_uv, i32(layer), depth);
}

// Separable bilinear comparison samples reconstruct a 5x5 Gaussian PCF
// footprint with nine hardware comparisons (Castaño's optimized PCF method).
fn sample_shadow_map_castano_thirteen(
    shadow_uv: vec2<f32>,
    depth: f32,
    layer: u32,
    filter_radius: vec2<f32>,
    local_map: bool,
) -> f32 {
    var shadow_map_size = vec2<f32>(textureDimensions(shadow_map));
    if local_map {
        shadow_map_size = vec2<f32>(textureDimensions(local_shadow_map));
    }
    let inverse_shadow_map_size = 1.0 / shadow_map_size;
    let uv = shadow_uv * shadow_map_size;
    let base_texel = floor(uv + vec2<f32>(0.5));
    let s = uv.x + 0.5 - base_texel.x;
    let t = uv.y + 0.5 - base_texel.y;

    let uw0 = 4.0 - 3.0 * s;
    let uw1 = 7.0;
    let uw2 = 1.0 + 3.0 * s;
    let u0 = (3.0 - 2.0 * s) / uw0 - 2.0;
    let u1 = (3.0 + s) / uw1;
    let u2 = s / uw2 + 2.0;

    let vw0 = 4.0 - 3.0 * t;
    let vw1 = 7.0;
    let vw2 = 1.0 + 3.0 * t;
    let v0 = (3.0 - 2.0 * t) / vw0 - 2.0;
    let v1 = (3.0 + t) / vw1;
    let v2 = t / vw2 + 2.0;

    let base_uv = (base_texel - vec2<f32>(0.5)) * inverse_shadow_map_size;
    let texel_filter = inverse_shadow_map_size * filter_radius;
    var sum = 0.0;
    sum += uw0 * vw0 * sample_shadow_compare(
        base_uv + vec2<f32>(u0, v0) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw1 * vw0 * sample_shadow_compare(
        base_uv + vec2<f32>(u1, v0) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw2 * vw0 * sample_shadow_compare(
        base_uv + vec2<f32>(u2, v0) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw0 * vw1 * sample_shadow_compare(
        base_uv + vec2<f32>(u0, v1) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw1 * vw1 * sample_shadow_compare(
        base_uv + vec2<f32>(u1, v1) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw2 * vw1 * sample_shadow_compare(
        base_uv + vec2<f32>(u2, v1) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw0 * vw2 * sample_shadow_compare(
        base_uv + vec2<f32>(u0, v2) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw1 * vw2 * sample_shadow_compare(
        base_uv + vec2<f32>(u1, v2) * texel_filter,
        depth,
        layer,
        local_map,
    );
    sum += uw2 * vw2 * sample_shadow_compare(
        base_uv + vec2<f32>(u2, v2) * texel_filter,
        depth,
        layer,
        local_map,
    );
    return sum / 144.0;
}

fn shadow_filter_axis(
    texel_position: f32,
    inverse_variance: f32,
    cutoff: f32,
    ratio_step: f32,
) -> array<vec2<f32>, 5> {
    let first_texel = floor(texel_position) - 2.0;
    let distance = first_texel + 0.5 - texel_position;
    var gaussian = exp(-distance * distance * inverse_variance);
    // g(d + 1) / g(d) = exp(-(2d + 1) / (2 sigma^2)); adjacent ratios advance geometrically.
    var ratio = exp(-(2.0 * distance + 1.0) * inverse_variance);
    var samples: array<vec2<f32>, 5>;
    for (var index = 0u; index < 5u; index += 1u) {
        let center = first_texel + f32(index) + 0.5;
        let weight = max(gaussian - cutoff, 0.0);
        samples[index] = vec2<f32>(center, weight);
        gaussian *= ratio;
        ratio *= ratio_step;
    }
    return samples;
}

fn sample_shadow(
    shadow_position: vec4<f32>,
    cascade: u32,
    filter_scale: f32,
) -> f32 {
    let inverse_w = 1.0 / max(shadow_position.w, 0.0001);
    let projected = shadow_position.xyz * inverse_w;
    let shadow_uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if projected.z <= 0.0 || projected.z >= 1.0 {
        return 1.0;
    }

    if any(shadow_uv <= vec2<f32>(0.0)) || any(shadow_uv >= vec2<f32>(1.0)) {
        return 1.0;
    }

    let softness = clamp(camera.lighting_params.z, 0.0, 1.0);
    let sigma = (0.5 + softness * 2.0) * filter_scale;
    let inverse_variance = 0.5 / (sigma * sigma);
    // Zero weight at the support boundary keeps the kernel continuous when its texel window moves.
    let cutoff = exp(-2.5 * 2.5 * inverse_variance);
    let ratio_step = exp(-2.0 * inverse_variance);
    let map_size = vec2<f32>(textureDimensions(shadow_map));
    let u_samples = shadow_filter_axis(shadow_uv.x * map_size.x, inverse_variance, cutoff, ratio_step);
    let v_samples = shadow_filter_axis(shadow_uv.y * map_size.y, inverse_variance, cutoff, ratio_step);
    var sum = 0.0;
    var total_weight = 0.0;
    // Texel-center comparisons do not depend on backend-specific comparison filtering.
    for (var y = 0u; y < 5u; y += 1u) {
        for (var x = 0u; x < 5u; x += 1u) {
            let weight = u_samples[x].y * v_samples[y].y;
            sum += weight * sample_shadow_compare(
                vec2<f32>(u_samples[x].x, v_samples[y].x) / map_size,
                projected.z - DIRECTIONAL_SHADOW_DEPTH_BIAS,
                cascade,
                false,
            );
            total_weight += weight;
        }
    }
    return sum / total_weight;
}

fn point_shadow_face(direction: vec3<f32>) -> u32 {
    let absolute = abs(direction);
    if absolute.x >= absolute.y && absolute.x >= absolute.z {
        return select(1u, 0u, direction.x >= 0.0);
    }
    if absolute.y >= absolute.z {
        return select(3u, 2u, direction.y >= 0.0);
    }
    return select(5u, 4u, direction.z >= 0.0);
}

fn sample_local_shadow(
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    light: LocalLight,
) -> f32 {
    if light.params.y < 0.0 || light.params.z < 0.5 {
        return 1.0;
    }
    var layer = u32(light.params.y + 0.5);
    if light.direction_type.w < 0.5 {
        layer += point_shadow_face(world_position - light.position_range.xyz);
    }
    let light_distance = distance(world_position, light.position_range.xyz);
    let normal_bias = max(light_distance * 0.001, 0.002);
    let shadow_position = local_lights.shadow_view_projections[layer]
        * vec4<f32>(world_position + world_normal * normal_bias, 1.0);
    let inverse_w = 1.0 / max(shadow_position.w, 0.0001);
    let projected = shadow_position.xyz * inverse_w;
    let shadow_uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if projected.z <= 0.0 || projected.z >= 1.0
        || any(shadow_uv <= vec2<f32>(0.0))
        || any(shadow_uv >= vec2<f32>(1.0)) {
        return 1.0;
    }

    return sample_shadow_map_castano_thirteen(
        shadow_uv,
        projected.z - 0.0001,
        layer,
        vec2<f32>(1.0),
        true,
    );
}

fn evaluate_local_light(
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    mapped_normal: vec3<f32>,
    view_direction: vec3<f32>,
    normal_dot_view: f32,
    base_color: vec3<f32>,
    base_reflectance: vec3<f32>,
    metallic: f32,
    roughness: f32,
    light: LocalLight,
) -> vec3<f32> {
    var source_position = light.position_range.xyz;
    if light.direction_type.w > 1.5 {
        let from_center = world_position - source_position;
        let axis_u = light.axis_u_half_width.xyz;
        let axis_v = light.axis_v_half_height.xyz;
        source_position += axis_u * clamp(
            dot(from_center, axis_u),
            -light.axis_u_half_width.w,
            light.axis_u_half_width.w,
        );
        source_position += axis_v * clamp(
            dot(from_center, axis_v),
            -light.axis_v_half_height.w,
            light.axis_v_half_height.w,
        );
    }

    let to_light = source_position - world_position;
    let distance_squared = max(dot(to_light, to_light), 0.0001);
    let light_distance = sqrt(distance_squared);
    let range = light.position_range.w;
    if light_distance >= range {
        return vec3<f32>(0.0);
    }
    let light_direction = to_light / light_distance;
    let normal_dot_light = max(dot(mapped_normal, light_direction), 0.0);
    if normal_dot_light <= 0.0 {
        return vec3<f32>(0.0);
    }

    var cone = 1.0;
    if light.direction_type.w > 0.5 {
        let from_light = normalize(world_position - light.position_range.xyz);
        let direction_cosine = dot(light.direction_type.xyz, from_light);
        let inner_cosine = mix(light.params.x, 1.0, 0.15);
        cone = smoothstep(light.params.x, inner_cosine, direction_cosine);
        if cone <= 0.0 {
            return vec3<f32>(0.0);
        }
    }

    let normalized_distance = light_distance / range;
    let range_falloff = max(1.0 - pow(normalized_distance, 4.0), 0.0);
    let attenuation = range_falloff * range_falloff / max(distance_squared, 1.0);
    let half_direction = normalize(view_direction + light_direction);
    let normal_dot_half = max(dot(mapped_normal, half_direction), 0.0);
    let view_dot_half = max(dot(view_direction, half_direction), 0.0);
    let fresnel = fresnel_schlick(view_dot_half, base_reflectance);
    let alpha = alpha_roughness(roughness);
    let f_ab = environment_brdf_f_ab(roughness, normal_dot_view);
    let normal_distribution = distribution_ggx(normal_dot_half, alpha);
    let visibility = geometry_smith(normal_dot_view, normal_dot_light, alpha);
    let specular = specular_multiscatter(
        normal_distribution,
        visibility,
        fresnel,
        base_reflectance,
        f_ab,
    );
    let diffuse = clamp(vec3<f32>(1.0) - fresnel, vec3<f32>(0.0), vec3<f32>(1.0))
        * (1.0 - metallic)
        * diffuse_burley(roughness, normal_dot_view, normal_dot_light, view_dot_half);
    let shadow_visibility = sample_local_shadow(world_position, world_normal, light);
    let radiance = light.color_brightness.rgb
        * light.color_brightness.w
        * attenuation
        * cone
        * shadow_visibility;
    return (diffuse * base_color + specular) * radiance * normal_dot_light;
}

fn shadow_cascade_split(cascade: u32) -> f32 {
    if cascade < 4u {
        return camera.shadow_cascade_splits[0][cascade];
    }
    return camera.shadow_cascade_splits[1][cascade - 4u];
}

fn shadow_texel_size(cascade: u32) -> f32 {
    if cascade < 4u {
        return camera.shadow_texel_sizes[0][cascade];
    }
    return camera.shadow_texel_sizes[1][cascade - 4u];
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let uv_dx = dpdx(vertex.uv);
    let uv_dy = dpdy(vertex.uv);
    let position_dx = dpdx(vertex.world_position);
    let position_dy = dpdy(vertex.world_position);
    let slot_factors = slot_base_color(vertex.material_set, vertex.material_slot);
    let slot_emissive_roughness =
        slot_emissive_roughness(vertex.material_set, vertex.material_slot);
    let slot_material = load_material_vec4(
        vertex.material_set,
        vertex.material_slot * MATERIAL_VEC4S_PER_SLOT + 2u,
    );
    let slot_metallic = slot_material.x;
    let uses_triplanar = slot_material.y > 0.5;
    let material_occlusion_strength = slot_material.z;
    let slot_reflectance = slot_material.w;
    let world_normal = normalize(vertex.normal);
    var base_color_sample = vec4<f32>(0.0);
    var surface_sample = vec4<f32>(0.0);
    var emissive_sample = vec4<f32>(0.0);
    var mapped_normal = world_normal;
    if uses_triplanar {
        let coordinates = triplanar_coordinates(
            vertex.world_position,
            world_normal,
            position_dx,
            position_dy,
        );
        base_color_sample = sample_base_color_triplanar(vertex.material_slot, coordinates);
        let surface_x = sample_surface(
            vertex.material_slot,
            coordinates.x,
            coordinates.x_dx,
            coordinates.x_dy,
        );
        let surface_y = sample_surface(
            vertex.material_slot,
            coordinates.y,
            coordinates.y_dx,
            coordinates.y_dy,
        );
        let surface_z = sample_surface(
            vertex.material_slot,
            coordinates.z,
            coordinates.z_dx,
            coordinates.z_dy,
        );
        surface_sample = surface_x * coordinates.weights.x
            + surface_y * coordinates.weights.y
            + surface_z * coordinates.weights.z;
        emissive_sample = sample_emissive_triplanar(vertex.material_slot, coordinates);
        mapped_normal = triplanar_mapped_normal(
            surface_x,
            surface_y,
            surface_z,
            world_normal,
            coordinates,
        );
    } else {
        base_color_sample = sample_base_color(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
        surface_sample = sample_surface(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
        emissive_sample = sample_emissive(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
        let normal_sample = unpack_normal(surface_sample);
        let tangent = normalize(vertex.tangent.xyz - world_normal * dot(world_normal, vertex.tangent.xyz));
        let bitangent = normalize(cross(world_normal, tangent)) * vertex.tangent.w;
        mapped_normal = normalize(
            tangent * normal_sample.x + bitangent * normal_sample.y + world_normal * normal_sample.z,
        );
    }
    let base_color = base_color_sample * vertex.vertex_color * vertex.tint * slot_factors;
    let ambient_occlusion = mix(
        1.0,
        clamp(emissive_sample.a, 0.0, 1.0),
        clamp(material_occlusion_strength, 0.0, 1.0),
    );
    // The normal (RG) and metallic-roughness (B=metallic, A=roughness) share
    // the surface layer, so each projection sample supplies both.
    let metallic_roughness_sample = vec4<f32>(0.0, surface_sample.a, surface_sample.b, 1.0);

    let metallic = clamp(
        slot_metallic * metallic_roughness_sample.b,
        0.0,
        1.0,
    );
    let roughness = clamp(
        slot_emissive_roughness.w * metallic_roughness_sample.g,
        0.04,
        1.0,
    );

    let view_direction = normalize(camera.camera_position.xyz - vertex.world_position);
    let light_direction = normalize(camera.light_direction.xyz);
    let half_direction = normalize(view_direction + light_direction);
    let geometric_normal_dot_light = max(dot(world_normal, light_direction), 0.0);
    let normal_dot_view = max(dot(mapped_normal, view_direction), 0.0);
    let normal_dot_light = max(dot(mapped_normal, light_direction), 0.0);
    let normal_dot_half = max(dot(mapped_normal, half_direction), 0.0);
    let view_dot_half = max(dot(view_direction, half_direction), 0.0);

    let dielectric_reflectance = clamp(slot_reflectance, 0.0, 1.0);
    let base_reflectance = mix(
        vec3<f32>(0.16 * dielectric_reflectance * dielectric_reflectance),
        base_color.rgb,
        metallic,
    );
    let fresnel = fresnel_schlick(view_dot_half, base_reflectance);
    let alpha = alpha_roughness(roughness);
    let f_ab = environment_brdf_f_ab(roughness, normal_dot_view);
    let normal_distribution = distribution_ggx(normal_dot_half, alpha);
    let visibility = geometry_smith(normal_dot_view, normal_dot_light, alpha);
    let view_depth = max(
        dot(vertex.world_position - camera.camera_position.xyz, camera.camera_forward.xyz),
        0.0,
    );
    var cascade = SHADOW_CASCADE_COUNT - 1u;
    for (var candidate = 0u; candidate < SHADOW_CASCADE_COUNT - 1u; candidate = candidate + 1u) {
        if view_depth <= shadow_cascade_split(candidate) {
            cascade = candidate;
            break;
        }
    }
    // Preserve the normal-bias depth correction without shifting the receiver's
    // shadow-map UV by moving it along the light ray.
    let normal_slope = sqrt(max(1.0 - geometric_normal_dot_light * geometric_normal_dot_light, 0.0))
        / max(geometric_normal_dot_light, 0.2);
    let normal_bias = shadow_texel_size(cascade) * min(normal_slope, 2.0);
    let biased_world =
        vertex.world_position + light_direction * (normal_bias * geometric_normal_dot_light);
    let light_view_projection = camera.light_view_projections[cascade];
    let biased_shadow_position = light_view_projection * vec4<f32>(biased_world, 1.0);
    var shadow_visibility = 1.0;
    if camera.lighting_params.y > 0.5 {
        let filter_scale = 1.0 + 0.25 * min(normal_slope, 2.0);
        let cascade_visibility = sample_shadow(biased_shadow_position, cascade, filter_scale);
        shadow_visibility = cascade_visibility;
        if view_depth > shadow_cascade_split(SHADOW_CASCADE_COUNT - 1u) {
            shadow_visibility = 1.0;
        } else if cascade < SHADOW_CASCADE_COUNT - 1u {
            // Fade between cascades while their projected texel footprints overlap.
            // Without this, the different resolutions produce a visible band at
            // every split even when both cascades are stable and well filtered.
            let split = shadow_cascade_split(cascade);
            let blend_width = max(split * 0.1, 0.05);
            let blend_start = split - blend_width;
            if view_depth > blend_start {
                let next_cascade = cascade + 1u;
                let next_normal_bias = shadow_texel_size(next_cascade) * min(normal_slope, 2.0);
                let next_light_view_projection = camera.light_view_projections[next_cascade];
                let next_biased_world = vertex.world_position
                    + light_direction * (next_normal_bias * geometric_normal_dot_light);
                let next_biased_shadow_position =
                    next_light_view_projection * vec4<f32>(next_biased_world, 1.0);
                let next_visibility =
                    sample_shadow(
                        next_biased_shadow_position,
                        next_cascade,
                        filter_scale,
                    );
                let blend_amount = smoothstep(blend_start, split, view_depth);
                shadow_visibility = mix(cascade_visibility, next_visibility, blend_amount);
            }
        }
    }
    let specular = specular_multiscatter(
        normal_distribution,
        visibility,
        fresnel,
        base_reflectance,
        f_ab,
    );
    let diffuse = clamp(vec3<f32>(1.0) - fresnel, vec3<f32>(0.0), vec3<f32>(1.0))
        * (1.0 - metallic)
        * diffuse_burley(roughness, normal_dot_view, normal_dot_light, view_dot_half);
    let direct = (diffuse * base_color.rgb + specular)
        * camera.light_color.rgb
        * normal_dot_light
        * mix(camera.shadow_color.rgb, vec3<f32>(1.0), shadow_visibility);
    // Image-based lighting from the workspace skybox: metals get their
    // reflections here. Diffuse irradiance is a heavily blurred normal sample,
    // specular is a roughness-driven prefiltered reflection modulated by an
    // analytic environment BRDF (no LUT texture required).
    let environment_mip_count = camera.environment_params.x;
    let environment_diffuse_scale = camera.environment_params.y;
    let environment_specular_scale = camera.environment_params.z;
    let exposure = camera.lighting_params.w;
    let has_environment = camera.environment_params.w;
    let max_environment_lod = max(environment_mip_count - 1.0, 0.0);
    let reflection = reflect(-view_direction, mapped_normal);
    let specular_lod = roughness * max_environment_lod;
    let prefiltered = textureSampleLevel(
        environment_map,
        environment_sampler,
        reflection,
        specular_lod,
    ).rgb;
    let diffuse_lod = clamp(
        environment_mip_count - 3.0,
        0.0,
        max_environment_lod,
    );
    let irradiance = textureSampleLevel(
        environment_map,
        environment_sampler,
        mapped_normal,
        diffuse_lod,
    ).rgb;
    let diffuse_ibl = irradiance * base_color.rgb * (1.0 - metallic) * environment_diffuse_scale;
    let specular_ibl = prefiltered
        * environment_brdf_approx(base_reflectance, f_ab);
    let environment_day_factor = 0.08 + camera.lighting_params.x * 0.92;
    let image_based = (diffuse_ibl + specular_ibl * environment_specular_scale)
        * environment_day_factor
        * has_environment
        * ambient_occlusion;
    let outdoor_ambient = camera.outdoor_ambient_color.rgb * camera.lighting_params.x;
    let ambient = base_color.rgb * (camera.ambient_color.rgb + outdoor_ambient)
        * (1.0 - metallic)
        * ambient_occlusion;
    let top_shift = max(mapped_normal.y, 0.0) * camera.color_shift_top.rgb * ambient_occlusion;
    let bottom_shift = max(-mapped_normal.y, 0.0) * camera.color_shift_bottom.rgb
        * ambient_occlusion;
    var local_direct = vec3<f32>(0.0);
    for (var light_index = 0u; light_index < local_lights.params.x; light_index += 1u) {
        local_direct += evaluate_local_light(
            vertex.world_position,
            world_normal,
            mapped_normal,
            view_direction,
            normal_dot_view,
            base_color.rgb,
            base_reflectance,
            metallic,
            roughness,
            local_lights.lights[light_index],
        );
    }
    var color = ambient + direct + local_direct
        + slot_emissive_roughness.rgb * emissive_sample.rgb
        + image_based + base_color.rgb * (top_shift + bottom_shift);
    let fog_amount = smoothstep(
        camera.fog_params.x,
        camera.fog_params.y,
        view_depth,
    );
    color = mix(color, camera.fog_color.rgb, fog_amount);
    let tone_mapped = aces_tone_map(color * exposure);
    var display_color = linear_to_srgb(tone_mapped);
    if FRAMEBUFFER_IS_SRGB > 0.5 {
        display_color = tone_mapped;
    }
    return vec4<f32>(display_color, base_color.a);
}
