// No storage or uniform bindings: frequencies arrive through the vertex fetch path.
@vertex
fn vertex(
    @location(0) frequency: f32,
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) bin: u32,
) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let x = -1.0 + (f32(bin) + corner.x) * 0.5;
    let y = -1.0 + corner.y * clamp(frequency, 0.0, 1.0) * 2.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}
