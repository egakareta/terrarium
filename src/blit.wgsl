struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var scene_texture: texture_2d<f32>;

@group(0) @binding(1)
var scene_sampler: sampler;

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let inverse_size = 1.0 / vec2<f32>(textureDimensions(scene_texture));
    let center = textureSample(scene_texture, scene_sampler, vertex.uv);
    let northwest = textureSample(
        scene_texture,
        scene_sampler,
        vertex.uv + vec2<f32>(-1.0, -1.0) * inverse_size,
    ).rgb;
    let northeast = textureSample(
        scene_texture,
        scene_sampler,
        vertex.uv + vec2<f32>(1.0, -1.0) * inverse_size,
    ).rgb;
    let southwest = textureSample(
        scene_texture,
        scene_sampler,
        vertex.uv + vec2<f32>(-1.0, 1.0) * inverse_size,
    ).rgb;
    let southeast = textureSample(
        scene_texture,
        scene_sampler,
        vertex.uv + vec2<f32>(1.0, 1.0) * inverse_size,
    ).rgb;

    let luma_center = luminance(center.rgb);
    let luma_northwest = luminance(northwest);
    let luma_northeast = luminance(northeast);
    let luma_southwest = luminance(southwest);
    let luma_southeast = luminance(southeast);
    let luma_min = min(
        luma_center,
        min(min(luma_northwest, luma_northeast), min(luma_southwest, luma_southeast)),
    );
    let luma_max = max(
        luma_center,
        max(max(luma_northwest, luma_northeast), max(luma_southwest, luma_southeast)),
    );
    let luma_range = luma_max - luma_min;
    if luma_range < max(0.0312, luma_max * 0.125) {
        return center;
    }

    var direction = vec2<f32>(
        -((luma_northwest + luma_northeast) - (luma_southwest + luma_southeast)),
        (luma_northwest + luma_southwest) - (luma_northeast + luma_southeast),
    );
    let direction_reduce = max(
        (luma_northwest + luma_northeast + luma_southwest + luma_southeast)
            * (0.25 * 0.0312),
        0.0078,
    );
    let inverse_direction_min = 1.0 / (min(abs(direction.x), abs(direction.y)) + direction_reduce);
    direction = clamp(direction * inverse_direction_min, vec2<f32>(-8.0), vec2<f32>(8.0))
        * inverse_size;

    let color_a = 0.5 * (
        textureSampleLevel(
            scene_texture,
            scene_sampler,
            vertex.uv + direction * (1.0 / 3.0 - 0.5),
            0.0,
        ).rgb
        + textureSampleLevel(
            scene_texture,
            scene_sampler,
            vertex.uv + direction * (2.0 / 3.0 - 0.5),
            0.0,
        ).rgb
    );
    let color_b = color_a * 0.5 + 0.25 * (
        textureSampleLevel(scene_texture, scene_sampler, vertex.uv + direction * -0.5, 0.0).rgb
        + textureSampleLevel(scene_texture, scene_sampler, vertex.uv + direction * 0.5, 0.0).rgb
    );
    let luma_b = luminance(color_b);
    let filtered = select(color_b, color_a, luma_b < luma_min || luma_b > luma_max);
    return vec4<f32>(filtered, center.a);
}
