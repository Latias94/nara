struct VertexInput {
    @location(0) clip_origin: vec2f,
    @location(1) clip_axes: vec4f,
    @location(2) points_ab: vec4f,
    @location(3) points_cd: vec4f,
    @location(4) premultiplied: vec4f,
    @location(5) half_width: f32,
    @location(6) kind: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) points_ab: vec4f,
    @location(1) @interpolate(flat) points_cd: vec4f,
    @location(2) @interpolate(flat) premultiplied: vec4f,
    @location(3) @interpolate(flat) half_width: f32,
    @location(4) @interpolate(flat) kind: u32,
};

@vertex
fn vs_main(input: VertexInput, @builtin(vertex_index) index: u32) -> VertexOutput {
    let clip = input.clip_origin
        + f32(index & 1u) * input.clip_axes.xy
        + f32(index >> 1u) * input.clip_axes.zw;
    return VertexOutput(
        vec4f(clip, 0.0, 1.0), input.points_ab, input.points_cd,
        input.premultiplied, input.half_width, input.kind,
    );
}

fn segment_distance(point: vec2f, start: vec2f, end: vec2f) -> f32 {
    let edge = end - start;
    let along = clamp(dot(point - start, edge) / max(dot(edge, edge), 1e-12), 0.0, 1.0);
    return length((point - start) - along * edge);
}

fn rect_distance(point: vec2f, ab: vec4f, cd: vec4f) -> f32 {
    let first = min(segment_distance(point, ab.xy, ab.zw), segment_distance(point, ab.zw, cd.xy));
    let second = min(segment_distance(point, cd.xy, cd.zw), segment_distance(point, cd.zw, ab.xy));
    return min(first, second);
}

fn ellipse_distance(point: vec2f, center: vec2f, inverse_x: vec2f, inverse_y: vec2f) -> f32 {
    let delta = point - center;
    let local = vec2f(dot(delta, inverse_x), dot(delta, inverse_y));
    let radius = length(local);
    let normal = local / max(radius, 1e-6);
    let gradient = normal.x * inverse_x + normal.y * inverse_y;
    let inverse_scale = select(
        max(length(inverse_x), length(inverse_y)), length(gradient), radius > 1e-6,
    );
    return abs(radius - 1.0) / max(inverse_scale, 1e-20);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let point = input.position.xy;
    var distance: f32;
    switch input.kind {
        case 0u: { distance = segment_distance(point, input.points_ab.xy, input.points_ab.zw); }
        case 1u: { distance = rect_distance(point, input.points_ab, input.points_cd); }
        default: {
            distance = ellipse_distance(point, input.points_ab.xy, input.points_ab.zw, input.points_cd.xy);
        }
    }
    let coverage = clamp(input.half_width + 0.5 - distance, 0.0, min(1.0, 2.0 * input.half_width));
    return input.premultiplied * coverage;
}
