struct SurfaceInput {
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    view_direction: vec3<f32>,
    uv: vec2<f32>,
    parameters: vec4<f32>,
};

struct Surface {
    base_color: vec4<f32>,
    normal: vec3<f32>,
    emissive: vec3<f32>,
    metallic: f32,
    roughness: f32,
    reflectance: f32,
    occlusion: f32,
    unlit: u32,
};
