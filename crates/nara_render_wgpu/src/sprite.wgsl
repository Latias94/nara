@group(0) @binding(0)
var sprite_texture: texture_2d<f32>;

@group(0) @binding(1)
var sprite_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) texture_mode: u32,
    @location(3) local_position: vec2<f32>,
    @location(4) @interpolate(flat) half_extent: vec2<f32>,
    @location(5) @interpolate(flat) corner_radius: f32,
    @location(6) @interpolate(flat) border_width: f32,
    @location(7) @interpolate(flat) shape_mode: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) center: vec2<f32>,
    @location(1) x_axis: vec2<f32>,
    @location(2) y_axis: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) uv_min: vec2<f32>,
    @location(5) uv_size: vec2<f32>,
    @location(6) texture_mode: u32,
    @location(7) shape_mode: u32,
    @location(8) rect_size: vec2<f32>,
    @location(9) corner_radius: f32,
    @location(10) border_width: f32,
) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
    );

    var output: VertexOutput;
    let corner = corners[vertex_index];
    let local_uv = vec2<f32>((corner.x + 1.0) * 0.5, (1.0 - corner.y) * 0.5);
    output.position = vec4<f32>(center + x_axis * corner.x + y_axis * corner.y, 0.0, 1.0);
    output.color = color;
    output.uv = uv_min + local_uv * uv_size;
    output.texture_mode = texture_mode;
    output.local_position = corner * rect_size * 0.5;
    output.half_extent = rect_size * 0.5;
    output.corner_radius = corner_radius;
    output.border_width = border_width;
    output.shape_mode = shape_mode;
    return output;
}

fn rounded_rect_distance(
    point: vec2<f32>,
    half_extent: vec2<f32>,
    requested_radius: f32,
) -> f32 {
    let radius = clamp(requested_radius, 0.0, min(half_extent.x, half_extent.y));
    let offset = abs(point) - (half_extent - vec2<f32>(radius));
    return length(max(offset, vec2<f32>(0.0)))
        + min(max(offset.x, offset.y), 0.0)
        - radius;
}

fn coverage(distance: f32) -> f32 {
    return 1.0 - smoothstep(-0.5, 0.5, distance);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(sprite_texture, sprite_sampler, input.uv);
    var output_color = input.color * sampled;
    if input.texture_mode == 1u {
        output_color = vec4<f32>(input.color.rgb, input.color.a * sampled.r);
    }
    if input.shape_mode == 0u {
        return output_color;
    }

    let outer_coverage = coverage(rounded_rect_distance(
        input.local_position,
        input.half_extent,
        input.corner_radius,
    ));
    if input.shape_mode == 1u {
        return vec4<f32>(output_color.rgb, output_color.a * outer_coverage);
    }

    let width = clamp(
        input.border_width,
        0.0,
        min(input.half_extent.x, input.half_extent.y),
    );
    let inner_extent = input.half_extent - vec2<f32>(width);
    var inner_coverage = 0.0;
    if all(inner_extent > vec2<f32>(0.0)) {
        inner_coverage = coverage(rounded_rect_distance(
            input.local_position,
            inner_extent,
            max(input.corner_radius - width, 0.0),
        ));
    }
    let ring_coverage = outer_coverage * (1.0 - inner_coverage);
    return vec4<f32>(output_color.rgb, output_color.a * ring_coverage);
}
