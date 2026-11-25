@group(0) @binding(0) var world_canvas: texture_2d<f32>;
@group(0) @binding(1) var<uniform> physical_rect: vec4<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let position = positions[index];
    var output: VertexOutput;
    output.position = vec4(position, 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = textureDimensions(world_canvas);
    let uv = (input.position.xy - physical_rect.xy) / (physical_rect.zw - physical_rect.xy);
    let pixel = clamp(vec2<i32>(floor(uv * vec2<f32>(size))), vec2(0), vec2<i32>(size) - vec2(1));
    return textureLoad(world_canvas, pixel, 0);
}
