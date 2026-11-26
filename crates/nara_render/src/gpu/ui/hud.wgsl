struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local_pos: vec2<f32>,
    @location(3) @interpolate(flat) half_size: vec2<f32>,
    @location(4) @interpolate(flat) radii: vec4<f32>,
    @location(5) @interpolate(flat) border: f32,
    @location(6) @interpolate(flat) mode: u32,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) local_pos: vec2<f32>,
    @location(4) half_size: vec2<f32>,
    @location(5) radii: vec4<f32>,
    @location(6) border: f32,
    @location(7) mode: u32,
) -> VertexOutput {
    return VertexOutput(vec4<f32>(position, 0.0, 1.0), uv, color, local_pos, half_size, radii, border, mode);
}

fn coverage(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let edge = abs(point) - half_size + vec2<f32>(radius);
    let distance = length(max(edge, vec2<f32>(0.0))) + min(max(edge.x, edge.y), 0.0) - radius;
    return clamp(0.5 - distance, 0.0, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(source, source_sampler, input.uv);
    if input.mode == 1u {
        return vec4<f32>(input.color.rgb, input.color.a * texel.r);
    }
    let shaded = texel * input.color;
    if input.mode != 2u {
        return shaded;
    }
    let point = input.local_pos;
    let left = select(input.radii.w, input.radii.x, point.y < 0.0);
    let right = select(input.radii.z, input.radii.y, point.y < 0.0);
    let radius = select(right, left, point.x < 0.0);
    let outer = coverage(point, input.half_size, radius);
    let inner_size = input.half_size - vec2<f32>(input.border);
    var inner = 0.0;
    if input.border > 0.0 && all(inner_size > vec2<f32>(0.0)) {
        inner = coverage(point, inner_size, max(radius - input.border, 0.0));
    }
    return vec4<f32>(shaded.rgb, shaded.a * outer * (1.0 - inner));
}
