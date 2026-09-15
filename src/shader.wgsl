struct Camera {
    view_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    light_direction: vec4<f32>,
    light_color: vec4<f32>,
    ambient_color: vec4<f32>,
};

override FRAMEBUFFER_IS_SRGB: f32 = 1.0;
override SHADOW_MAP_SIZE: f32 = 2048.0;

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var shadow_map: texture_depth_2d;

@group(0) @binding(2)
var shadow_sampler: sampler_comparison;

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
var material_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) vertex_color: vec4<f32>,
    @location(5) material_slot: u32,
    @location(6) model_0: vec4<f32>,
    @location(7) model_1: vec4<f32>,
    @location(8) model_2: vec4<f32>,
    @location(9) model_3: vec4<f32>,
    @location(10) normal_scales: vec3<f32>,
    @location(11) base_color: vec4<f32>,
    @location(12) metallic_roughness: vec2<f32>,
    @location(13) emissive: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat) material_slot: u32,
    @location(5) vertex_color: vec4<f32>,
    @location(6) base_color: vec4<f32>,
    @location(7) metallic_roughness: vec2<f32>,
    @location(8) emissive: vec3<f32>,
};

struct ShadowVertexInput {
    @location(0) position: vec3<f32>,
    @location(6) model_0: vec4<f32>,
    @location(7) model_1: vec4<f32>,
    @location(8) model_2: vec4<f32>,
    @location(9) model_3: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let model = mat4x4<f32>(
        vertex.model_0,
        vertex.model_1,
        vertex.model_2,
        vertex.model_3,
    );
    let world_position = model * vec4<f32>(vertex.position, 1.0);
    let normal_matrix = mat3x3<f32>(
        vertex.model_0.xyz * vertex.normal_scales.x,
        vertex.model_1.xyz * vertex.normal_scales.y,
        vertex.model_2.xyz * vertex.normal_scales.z,
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
    output.base_color = vertex.base_color;
    output.metallic_roughness = vertex.metallic_roughness;
    output.emissive = vertex.emissive;
    return output;
}

@vertex
fn vs_shadow(vertex: ShadowVertexInput) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(
        vertex.model_0,
        vertex.model_1,
        vertex.model_2,
        vertex.model_3,
    );
    return camera.light_view_projection * model * vec4<f32>(vertex.position, 1.0);
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
        return textureSampleGrad(material_texture_1, material_sampler, uv, 0, uv_dx, uv_dy);
    } else if slot == 2u {
        return textureSampleGrad(material_texture_2, material_sampler, uv, 0, uv_dx, uv_dy);
    } else if slot == 3u {
        return textureSampleGrad(material_texture_3, material_sampler, uv, 0, uv_dx, uv_dy);
    } else if slot == 4u {
        return textureSampleGrad(material_texture_4, material_sampler, uv, 0, uv_dx, uv_dy);
    } else if slot == 5u {
        return textureSampleGrad(material_texture_5, material_sampler, uv, 0, uv_dx, uv_dy);
    } else if slot == 6u {
        return textureSampleGrad(material_texture_6, material_sampler, uv, 0, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler, uv, 0, uv_dx, uv_dy);
}

fn unpack_normal(surface_sample: vec4<f32>) -> vec3<f32> {
    let xy = surface_sample.xy * 2.0 - 1.0;
    let z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    return normalize(vec3<f32>(xy, z));
}

fn sample_surface(slot: u32, uv: vec2<f32>, uv_dx: vec2<f32>, uv_dy: vec2<f32>) -> vec4<f32> {
    if slot == 1u {
        return textureSampleGrad(material_texture_1, material_sampler, uv, 1, uv_dx, uv_dy);
    } else if slot == 2u {
        return textureSampleGrad(material_texture_2, material_sampler, uv, 1, uv_dx, uv_dy);
    } else if slot == 3u {
        return textureSampleGrad(material_texture_3, material_sampler, uv, 1, uv_dx, uv_dy);
    } else if slot == 4u {
        return textureSampleGrad(material_texture_4, material_sampler, uv, 1, uv_dx, uv_dy);
    } else if slot == 5u {
        return textureSampleGrad(material_texture_5, material_sampler, uv, 1, uv_dx, uv_dy);
    } else if slot == 6u {
        return textureSampleGrad(material_texture_6, material_sampler, uv, 1, uv_dx, uv_dy);
    }
    return textureSampleGrad(material_texture_0, material_sampler, uv, 1, uv_dx, uv_dy);
}

fn sample_shadow(shadow_position: vec4<f32>, normal_dot_light: f32) -> f32 {
    let projected = shadow_position.xyz / max(shadow_position.w, 0.0001);
    if projected.z <= 0.0 || projected.z >= 1.0 {
        return 1.0;
    }

    let shadow_uv = projected.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if any(shadow_uv <= vec2<f32>(0.0)) || any(shadow_uv >= vec2<f32>(1.0)) {
        return 1.0;
    }

    let texel_size = 1.0 / SHADOW_MAP_SIZE;
    // Base bias covers ~1.5 texels of depth quantization: slope term grows
    // toward grazing angles where depth changes fastest across a texel.
    let depth_bias = 0.0003 + 0.0006 * (1.0 - normal_dot_light);
    let depth = projected.z - depth_bias;
    var visibility = 0.0;
    for (var x: i32 = -2; x <= 2; x = x + 1) {
        let weight_x = select(1.0, 4.0, abs(x) == 1) * select(1.0, 6.0, x == 0);
        for (var y: i32 = -2; y <= 2; y = y + 1) {
            let weight_y = select(1.0, 4.0, abs(y) == 1) * select(1.0, 6.0, y == 0);
            let weight = weight_x * weight_y;
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            visibility += textureSampleCompareLevel(
                shadow_map,
                shadow_sampler,
                shadow_uv + offset,
                depth,
            ) * weight;
        }
    }
    return visibility / 256.0;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let uv_dx = dpdx(vertex.uv);
    let uv_dy = dpdy(vertex.uv);
    let base_color_sample = sample_base_color(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
    let base_color = base_color_sample * vertex.vertex_color * vertex.base_color;
    // The normal (RG) and metallic-roughness (B=metallic, A=roughness) live in
    // the same surface layer, so one fetch serves both: previously this sampled
    // the identical texel twice with bitwise-identical results.
    let surface_sample = sample_surface(vertex.material_slot, vertex.uv, uv_dx, uv_dy);
    let normal_sample = unpack_normal(surface_sample);
    let metallic_roughness_sample = vec4<f32>(0.0, surface_sample.a, surface_sample.b, 1.0);
    let world_normal = normalize(vertex.normal);
    let tangent = normalize(vertex.tangent.xyz - world_normal * dot(world_normal, vertex.tangent.xyz));
    let bitangent = normalize(cross(world_normal, tangent)) * vertex.tangent.w;
    let mapped_normal = normalize(
        tangent * normal_sample.x + bitangent * normal_sample.y + world_normal * normal_sample.z,
    );

    let metallic = clamp(
        vertex.metallic_roughness.x * metallic_roughness_sample.b,
        0.0,
        1.0,
    );
    let roughness = clamp(
        vertex.metallic_roughness.y * metallic_roughness_sample.g,
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
    // Normal-offset bias: move the receiver off its own surface along the
    // geometric normal before the shadow lookup. Depth-only bias always trades
    // acne against peter-panning. The normal offset removes self-intersection
    // at grazing angles without pushing contact shadows away along light dir.
    let normal_bias = mix(0.06, 0.02, geometric_normal_dot_light);
    let biased_world = vertex.world_position + world_normal * normal_bias;
    let biased_shadow_position = camera.light_view_projection * vec4<f32>(biased_world, 1.0);
    let shadow_visibility = sample_shadow(biased_shadow_position, geometric_normal_dot_light);
    let specular = normal_distribution * geometry * fresnel
        / max(4.0 * normal_dot_view * normal_dot_light, 0.0001);
    let diffuse = (vec3<f32>(1.0) - fresnel) * (1.0 - metallic) / 3.14159265;
    let direct = (diffuse * base_color.rgb + specular)
        * camera.light_color.rgb
        * normal_dot_light
        * shadow_visibility;
    let ambient = base_color.rgb * camera.ambient_color.rgb * (1.0 - metallic);
    let color = ambient + direct + vertex.emissive.rgb;
    let tone_mapped = color / (color + vec3<f32>(1.0));
    var display_color = linear_to_srgb(tone_mapped);
    if FRAMEBUFFER_IS_SRGB > 0.5 {
        display_color = tone_mapped;
    }
    return vec4<f32>(display_color, base_color.a);
}
