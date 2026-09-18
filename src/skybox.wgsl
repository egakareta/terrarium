struct SkyboxCamera {
    // Projection multiplied by the camera's rotation-only view matrix, so the
    // skybox follows camera rotation but not translation.
    view_projection: mat4x4<f32>,
};

override FRAMEBUFFER_IS_SRGB: f32 = 1.0;

@group(0) @binding(0)
var<uniform> sky_camera: SkyboxCamera;

@group(0) @binding(1)
var skybox_texture: texture_cube<f32>;

@group(0) @binding(2)
var skybox_sampler: sampler;

struct SkyboxVertexInput {
    @location(0) position: vec3<f32>,
};

struct SkyboxVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
};

@vertex
fn vs_main(vertex: SkyboxVertexInput) -> SkyboxVertexOutput {
    var output: SkyboxVertexOutput;
    // The cube corner doubles as the sampling direction.
    output.direction = vertex.position;
    let clip = sky_camera.view_projection * vec4<f32>(vertex.position, 1.0);
    // Force the far plane so the skybox sits behind every part.
    output.position = clip.xyww;
    return output;
}

fn linear_to_srgb(linear_color: vec3<f32>) -> vec3<f32> {
    let color = max(linear_color, vec3<f32>(0.0));
    return select(
        12.92 * color,
        1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
        color > vec3<f32>(0.0031308),
    );
}

@fragment
fn fs_main(vertex: SkyboxVertexOutput) -> @location(0) vec4<f32> {
    let direction = normalize(vertex.direction);
    let color = textureSample(skybox_texture, skybox_sampler, direction);
    // Match the mesh pipeline's filmic curve so the background and the lit
    // parts share one display mapping.
    let tone_mapped = color.rgb / (color.rgb + vec3<f32>(1.0));
    var display_color = linear_to_srgb(tone_mapped);
    if FRAMEBUFFER_IS_SRGB > 0.5 {
        display_color = tone_mapped;
    }
    return vec4<f32>(display_color, 1.0);
}
