struct Camera {
    view_projection: mat4x4<f32>,
    light_view_projections: array<mat4x4<f32>, 7>,
    camera_position: vec4<f32>,
    camera_forward: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    ambient_color: vec4<f32>,
    shadow_cascade_splits: array<vec4<f32>, 2>,
    shadow_texel_sizes: array<vec4<f32>, 2>,
};

struct ShadowCamera {
    light_view_projection: mat4x4<f32>,
};

override FRAMEBUFFER_IS_SRGB: f32 = 1.0;
override SHADOW_MAP_SIZE: f32 = 3072.0;

const SHADOW_CASCADE_COUNT: u32 = 7u;
const SHADOW_FILTER_OFFSETS = array<f32, 3>(-1.0, 0.0, 1.0);
const SHADOW_FILTER_WEIGHTS = array<f32, 3>(1.0, 2.0, 1.0);

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var shadow_map: texture_depth_2d_array;

@group(0) @binding(2)
var shadow_sampler: sampler_comparison;

@group(0) @binding(3)
var<uniform> shadow_camera: ShadowCamera;

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
var material_texture_6: texture_2d_array<f32>;

@group(1) @binding(7)
var material_sampler_0: sampler;

@group(1) @binding(8)
var material_sampler_1: sampler;

@group(1) @binding(9)
var material_sampler_2: sampler;

@group(1) @binding(10)
var material_sampler_3: sampler;

@group(1) @binding(11)
var material_sampler_4: sampler;

@group(1) @binding(12)
var material_sampler_5: sampler;

@group(1) @binding(13)
var material_sampler_6: sampler;

// Per-face PBR factors for every deduplicated part material set. Each set
// packs seven slots (base + six directions, in MaterialSlot order) as three
// vec4s per slot: base color, then emissive RGB + roughness, then metallic.
//
// Stored in a 2D float texture (width = MATERIAL_VEC4S_PER_SET, height =
// set count) instead of a storage buffer: OpenGL ES / WebGL backends expose
// zero storage buffers in the fragment stage, while textureLoad works
// everywhere.
@group(2) @binding(0)
var material_factor_texture: texture_2d<f32>;

const MATERIAL_VEC4S_PER_SLOT: u32 = 3u;
const MATERIAL_VEC4S_PER_SET: u32 = 21u;

fn load_material_vec4(set_index: u32, vec4_index: u32) -> vec4<f32> {
    return textureLoad(material_factor_texture, vec2<u32>(vec4_index, set_index), 0);
}

fn slot_base_color(set_index: u32, slot: u32) -> vec4<f32> {
    return load_material_vec4(set_index, slot * MATERIAL_VEC4S_PER_SLOT);
}

fn slot_emissive_roughness(set_index: u32, slot: u32) -> vec4<f32> {
    return load_material_vec4(set_index, slot * MATERIAL_VEC4S_PER_SLOT + 1u);
}

fn slot_metallic(set_index: u32, slot: u32) -> f32 {
    return load_material_vec4(set_index, slot * MATERIAL_VEC4S_PER_SLOT + 2u).x;
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

fn distribution_ggx(normal_dot_half: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denominator = normal_dot_half * normal_dot_half * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denominator * denominator, 0.0001);
}

fn geometry_schlick_ggx(normal_dot_direction: f32, roughness: f32) -> f32 {
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    return normal_dot_direction / max(normal_dot_direction * (1.0 - k) + k, 0.0001);
}

fn geometry_smith(normal_dot_view: f32, normal_dot_light: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(normal_dot_view, roughness)
        * geometry_schlick_ggx(normal_dot_light, roughness);
}

fn fresnel_schlick(view_dot_half: f32, base_reflectance: vec3<f32>) -> vec3<f32> {
    return base_reflectance
        + (vec3<f32>(1.0) - base_reflectance) * pow(1.0 - view_dot_half, 5.0);
}

fn linear_to_srgb(linear_color: vec3<f32>) -> vec3<f32> {
    let color = max(linear_color, vec3<f32>(0.0));
    return select(
        12.92 * color,
        1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
        color > vec3<f32>(0.0031308),
    );
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
    } else if slot == 6u {
        return textureSampleGrad(material_texture_6, material_sampler_6, uv, 0, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler_0, uv, 0, uv_dx, uv_dy);
}

fn unpack_normal(surface_sample: vec4<f32>) -> vec3<f32> {
    let xy = surface_sample.xy * 2.0 - 1.0;
    let z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    return normalize(vec3<f32>(xy, z));
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
    } else if slot == 6u {
        return textureSampleGrad(material_texture_6, material_sampler_6, uv, 1, uv_dx, uv_dy);
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
    } else if slot == 6u {
        return textureSampleGrad(material_texture_6, material_sampler_6, uv, 2, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler_0, uv, 2, uv_dx, uv_dy);
}

fn sample_shadow(shadow_position: vec4<f32>, cascade: u32) -> f32 {
    let inverse_w = 1.0 / max(shadow_position.w, 0.0001);
    let projected = shadow_position.xyz * inverse_w;
    let shadow_uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if projected.z <= 0.0 || projected.z >= 1.0 {
        return 1.0;
    }

    if any(shadow_uv <= vec2<f32>(0.0)) || any(shadow_uv >= vec2<f32>(1.0)) {
        return 1.0;
    }

    let texel_size = 1.0 / SHADOW_MAP_SIZE;
    var visibility = 0.0;
    for (var x = 0u; x < 3u; x = x + 1u) {
        for (var y = 0u; y < 3u; y = y + 1u) {
            let weight = SHADOW_FILTER_WEIGHTS[x] * SHADOW_FILTER_WEIGHTS[y];
            let offset = vec2<f32>(SHADOW_FILTER_OFFSETS[x], SHADOW_FILTER_OFFSETS[y])
                * texel_size;
            visibility += textureSampleCompareLevel(
                shadow_map,
                shadow_sampler,
                shadow_uv + offset,
                i32(cascade),
                projected.z - 1e-6,
            ) * weight;
        }
    }
    return visibility / 16.0;
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
    let slot_factors = slot_base_color(vertex.material_set, vertex.material_slot);
    let slot_emissive_roughness =
        slot_emissive_roughness(vertex.material_set, vertex.material_slot);
    let slot_metallic = slot_metallic(vertex.material_set, vertex.material_slot);
    let base_color_sample = sample_base_color(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
    let base_color = base_color_sample * vertex.vertex_color * vertex.tint * slot_factors;
    // The normal (RG) and metallic-roughness (B=metallic, A=roughness) live in
    // the same surface layer, so one fetch serves both: previously this sampled
    // the identical texel twice with bitwise-identical results.
    let surface_sample = sample_surface(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
    let emissive_sample = sample_emissive(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
    let normal_sample = unpack_normal(surface_sample);
    let metallic_roughness_sample = vec4<f32>(0.0, surface_sample.a, surface_sample.b, 1.0);
    let world_normal = normalize(vertex.normal);
    let tangent = normalize(vertex.tangent.xyz - world_normal * dot(world_normal, vertex.tangent.xyz));
    let bitangent = normalize(cross(world_normal, tangent)) * vertex.tangent.w;
    let mapped_normal = normalize(
        tangent * normal_sample.x + bitangent * normal_sample.y + world_normal * normal_sample.z,
    );

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

    let base_reflectance = mix(vec3<f32>(0.04), base_color.rgb, metallic);
    let fresnel = fresnel_schlick(view_dot_half, base_reflectance);
    let normal_distribution = distribution_ggx(normal_dot_half, roughness);
    let geometry = geometry_smith(normal_dot_view, normal_dot_light, roughness);
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
    // Scale the normal offset to the selected cascade's texel footprint. It is
    // zero on light-facing planes and capped at two texels at grazing angles.
    let normal_slope = sqrt(max(1.0 - geometric_normal_dot_light * geometric_normal_dot_light, 0.0))
        / max(geometric_normal_dot_light, 0.2);
    let normal_bias = shadow_texel_size(cascade) * min(normal_slope, 2.0);
    let biased_world = vertex.world_position + world_normal * normal_bias;
    let light_view_projection = camera.light_view_projections[cascade];
    let biased_shadow_position = light_view_projection * vec4<f32>(biased_world, 1.0);
    let cascade_visibility = sample_shadow(biased_shadow_position, cascade);
    var shadow_visibility = cascade_visibility;
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
            let next_biased_shadow_position = next_light_view_projection
                * vec4<f32>(vertex.world_position + world_normal * next_normal_bias, 1.0);
            let next_visibility = sample_shadow(next_biased_shadow_position, next_cascade);
            let blend_amount = smoothstep(blend_start, split, view_depth);
            shadow_visibility = mix(cascade_visibility, next_visibility, blend_amount);
        }
    }
    let specular = normal_distribution * geometry * fresnel
        / max(4.0 * normal_dot_view * normal_dot_light, 0.0001);
    let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) / 3.14159265;
    let direct = (diffuse * base_color.rgb + specular)
        * camera.light_color.rgb
        * normal_dot_light
        * shadow_visibility;
    let ambient = base_color.rgb * camera.ambient_color.rgb * (1.0 - metallic);
    let color = ambient + direct + slot_emissive_roughness.rgb * emissive_sample.rgb;
    let tone_mapped = color / (color + vec3<f32>(1.0));
    var display_color = linear_to_srgb(tone_mapped);
    if FRAMEBUFFER_IS_SRGB > 0.5 {
        display_color = tone_mapped;
    }
    return vec4<f32>(display_color, base_color.a);
}
