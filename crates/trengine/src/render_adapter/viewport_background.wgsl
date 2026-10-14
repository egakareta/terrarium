@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}
@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(19.0 / 255.0, 29.0 / 255.0, 42.0 / 255.0, 1.0);
}
