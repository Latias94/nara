struct Pattern { phase: vec4<u32>, };
@group(0) @binding(0) var<uniform> pattern: Pattern;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let x = u32(position.x);
    let y = u32(position.y);
    let phase = pattern.phase.x;
    let alphas = array<f32, 4>(0.0, 0.25, 0.5, 1.0);
    let alpha = alphas[(x + 2u * y + phase) % 4u];
    return vec4<f32>(
        alpha * f32((x + phase) % 8u) * 0.125,
        alpha * f32((2u * y + phase) % 8u) * 0.125,
        alpha * f32((x + 3u * y + phase) % 8u) * 0.125,
        alpha);
}
