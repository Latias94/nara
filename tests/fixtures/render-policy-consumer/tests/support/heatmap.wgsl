struct Parameters {
    extent_and_palette: vec4<u32>,
    samples: array<vec4<u32>, 4>,
};

@group(0) @binding(0) var<uniform> parameters: Parameters;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let x = min(u32(position.x) * 4u / parameters.extent_and_palette.x, 3u);
    let y = min(u32(position.y) * 4u / parameters.extent_and_palette.y, 3u);
    let sample = parameters.samples[y][x];
    let band = (sample + parameters.extent_and_palette.z) % 4u;
    switch band {
        case 0u: { return vec4<f32>(0.0, 0.0, 1.0, 1.0); }
        case 1u: { return vec4<f32>(0.0, 1.0, 1.0, 1.0); }
        case 2u: { return vec4<f32>(1.0, 1.0, 0.0, 1.0); }
        default: { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }
    }
}
