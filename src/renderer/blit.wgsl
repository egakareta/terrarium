struct Outline {
    toon_params: vec4<f32>,
    toon_color: vec4<f32>,
    silhouette_params: vec4<f32>,
    silhouette_color: vec4<f32>,
    stencil_params: vec4<f32>,
    stencil_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var scene_texture: texture_2d<f32>;

@group(0) @binding(1)
var scene_sampler: sampler;

@group(0) @binding(2)
var outline_mask: texture_2d<f32>;

@group(0) @binding(3)
var<uniform> outline: Outline;

const EDGE_OFFSETS = array<vec2<i32>, 8>(
    vec2<i32>(-1, -1),
    vec2<i32>(0, -1),
    vec2<i32>(1, -1),
    vec2<i32>(-1, 0),
    vec2<i32>(1, 0),
    vec2<i32>(-1, 1),
    vec2<i32>(0, 1),
    vec2<i32>(1, 1),
);

fn luminance(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

fn mask_at(pixel: vec2<i32>, channel: u32) -> f32 {
    let dimensions = vec2<i32>(textureDimensions(outline_mask));
    let clamped = clamp(pixel, vec2<i32>(0), dimensions - vec2<i32>(1));
    return textureLoad(outline_mask, clamped, 0)[channel];
}

fn mask_edge(pixel: vec2<i32>, step: i32, channel: u32) -> f32 {
    let center = mask_at(pixel, channel);
    var edge = 0.0;
    for (var index = 0u; index < 8u; index = index + 1u) {
        let neighbor = mask_at(pixel + EDGE_OFFSETS[index] * step, channel);
        edge = max(edge, abs(center - neighbor));
    }
    return edge;
}

fn color_edge(uv: vec2<f32>, inverse_size: vec2<f32>, step: i32) -> f32 {
    let offset = inverse_size * f32(step);
    let center = luminance(textureSampleLevel(scene_texture, scene_sampler, uv, 0.0).rgb);
    let horizontal = abs(
        luminance(textureSampleLevel(scene_texture, scene_sampler, uv + vec2<f32>(offset.x, 0.0), 0.0).rgb)
            - luminance(textureSampleLevel(scene_texture, scene_sampler, uv - vec2<f32>(offset.x, 0.0), 0.0).rgb)
    );
    let vertical = abs(
        luminance(textureSampleLevel(scene_texture, scene_sampler, uv + vec2<f32>(0.0, offset.y), 0.0).rgb)
            - luminance(textureSampleLevel(scene_texture, scene_sampler, uv - vec2<f32>(0.0, offset.y), 0.0).rgb)
    );
    return max(max(horizontal, vertical), abs(center - luminance(
        textureSampleLevel(scene_texture, scene_sampler, uv + offset, 0.0).rgb,
    )));
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
    var filtered = center.rgb;
    if luma_range >= max(0.0312, luma_max * 0.125) {
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
        filtered = select(color_b, color_a, luma_b < luma_min || luma_b > luma_max);
    }

    let dimensions = vec2<i32>(textureDimensions(outline_mask));
    let pixel = vec2<i32>(vertex.uv * vec2<f32>(dimensions));
    let toon_step = i32(clamp(outline.toon_params.x + 0.5, 1.0, 4.0));
    let silhouette_step = i32(clamp(outline.silhouette_params.x + 0.5, 1.0, 4.0));
    let toon_mask = mask_at(pixel, 0u);
    let silhouette_mask = mask_at(pixel, 1u);
    let toon_boundary = mask_edge(pixel, toon_step, 0u);
    let silhouette_boundary = mask_edge(pixel, silhouette_step, 1u);
    let toon_color = color_edge(vertex.uv, inverse_size, toon_step) * toon_mask;
    let toon_threshold = outline.toon_params.y;
    let toon_factor = max(
        toon_boundary,
        smoothstep(toon_threshold, toon_threshold + 0.04, toon_color),
    );
    let silhouette_threshold = outline.silhouette_params.y;
    let silhouette_factor = smoothstep(
        silhouette_threshold,
        silhouette_threshold + 0.04,
        silhouette_boundary,
    ) * max(silhouette_mask, silhouette_boundary);

    var output = filtered;
    output = mix(output, outline.toon_color.rgb, toon_factor);
    output = mix(output, outline.silhouette_color.rgb, silhouette_factor);
    return vec4<f32>(output, center.a);
}
