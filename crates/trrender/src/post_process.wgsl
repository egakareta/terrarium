struct PostProcessInput {
    uv: vec2<f32>,
    resolution: vec2<f32>,
    parameters: vec4<f32>,
};

struct FullscreenVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

override FRAMEBUFFER_IS_SRGB: f32 = 1.0;

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> effect_parameters: vec4<f32>;

fn sample_scene(uv: vec2<f32>) -> vec4<f32> {
    let sampled = textureSampleLevel(scene_texture, scene_sampler, uv, 0.0);
    if FRAMEBUFFER_IS_SRGB > 0.5 {
        return sampled;
    }
    let linear = select(
        sampled.rgb / 12.92,
        pow((sampled.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        sampled.rgb > vec3<f32>(0.04045),
    );
    return vec4<f32>(linear, sampled.a);
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> FullscreenVertex {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let uvs = array<vec2<f32>, 3>(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    return FullscreenVertex(vec4<f32>(positions[index], 0.0, 1.0), uvs[index]);
}

@fragment
fn fs_main(vertex: FullscreenVertex) -> @location(0) vec4<f32> {
    let input = PostProcessInput(vertex.uv, vec2<f32>(textureDimensions(scene_texture)), effect_parameters);
    let result = post_process(input);
    if FRAMEBUFFER_IS_SRGB > 0.5 {
        return result;
    }
    let color = max(result.rgb, vec3<f32>(0.0));
    let encoded = select(12.92 * color, 1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), color > vec3<f32>(0.0031308));
    return vec4<f32>(encoded, result.a);
}
