@group(0) @binding(0)
var sprite_texture: texture_2d<f32>;

@group(0) @binding(1)
var sprite_sampler: sampler;

const NARA_TIME_PERIOD_SECONDS: f32 = 3600.0;

struct NaraView {
    world_to_clip: mat3x3<f32>,
    clip_to_world: mat3x3<f32>,
    viewport: vec4<f32>,
    real_elapsed: f32,
    real_delta: f32,
    virtual_elapsed: f32,
    virtual_delta: f32,
    fixed_elapsed: f32,
    fixed_delta: f32,
    fixed_timestep: f32,
    interpolation_alpha: f32,
    time_scale: f32,
    clock_available: u32,
    paused: u32,
    spatial_interpolated: u32,
    frame: vec2<u32>,
    fixed_tick: vec2<u32>,
    real_period: vec2<u32>,
    virtual_period: vec2<u32>,
    fixed_period: vec2<u32>,
    _reserved0: vec2<u32>,
    virtual_frame: vec2<u32>,
    fixed_steps: u32,
    _reserved1: u32,
};

@group(2) @binding(0)
var<uniform> nara_view: NaraView;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
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
    return output;
}

fn fragment_color(input: VertexOutput) -> vec4<f32> {
    let sampled = textureSample(sprite_texture, sprite_sampler, input.uv);
    return material_fragment(sampled, input.color, input.uv);
}

@fragment
fn fs_opaque(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(fragment_color(input).rgb, 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return fragment_color(input);
}
