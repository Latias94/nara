// Independent linear temporal accumulation; no optional float filtering is required.
@group(0) @binding(0) var first: texture_2d<f32>;
@group(0) @binding(1) var<uniform> parameters: vec4<u32>;
@group(0) @binding(2) var second: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(corners[index], 0.0, 1.0);
}

@fragment
fn accumulate(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<u32>(position.xy);
    let phase = parameters.x;
    let tag = parameters.y;
    let current = vec4<f32>(
        f32((pixel.x + phase + 3u * tag) % 8u) * 0.125,
        f32((pixel.y + 2u * phase + tag) % 8u) * 0.125,
        f32((pixel.x + pixel.y + 3u * phase + 2u * tag) % 8u) * 0.125,
        1.0,
    );
    let previous = textureLoad(first, vec2<i32>(pixel), 0);
    return 0.25 * current + 0.75 * previous;
}

@fragment
fn composite(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let dimensions = textureDimensions(first);
    let pixel = vec2<u32>(position.xy);
    let local = vec2<i32>(i32(pixel.x % dimensions.x), i32(pixel.y));
    var color: vec4<f32>;
    if pixel.y < dimensions.y / 2u {
        color = textureLoad(first, local, 0);
    } else {
        color = textureLoad(second, local, 0);
    }
    return vec4<f32>(color.rgb, 1.0);
}
