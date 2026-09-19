struct Camera {
    view_projection: mat4x4<f32>,
};

struct Outline {
    toon_params: vec4<f32>,
    toon_color: vec4<f32>,
    silhouette_params: vec4<f32>,
    silhouette_color: vec4<f32>,
    stencil_params: vec4<f32>,
    stencil_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0)
var<uniform> outline: Outline;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(6) model_0: vec3<f32>,
    @location(7) model_1: vec3<f32>,
    @location(8) model_2: vec3<f32>,
    @location(9) model_3: vec3<f32>,
    @location(10) normal_scales: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
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
    let clip_position = camera.view_projection * world_position;
    let expansion = outline.stencil_params.x * 0.0015 * max(clip_position.w, 1.0);
    var output: VertexOutput;
    output.position = camera.view_projection
        * vec4<f32>(world_position.xyz + world_normal * expansion, 1.0);
    return output;
}

@vertex
fn vs_mask(vertex: VertexInput) -> VertexOutput {
    let model = mat4x4<f32>(
        vec4<f32>(vertex.model_0, 0.0),
        vec4<f32>(vertex.model_1, 0.0),
        vec4<f32>(vertex.model_2, 0.0),
        vec4<f32>(vertex.model_3, 1.0),
    );
    var output: VertexOutput;
    output.position = camera.view_projection * model * vec4<f32>(vertex.position, 1.0);
    return output;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return outline.stencil_color;
}

@fragment
fn fs_mask() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}
