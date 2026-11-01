struct Parameters {
    radius: vec4<u32>,
};

@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> parameters: Parameters;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn blur(position: vec2<i32>, axis: vec2<i32>) -> vec4<f32> {
    // These are the actual bound input's texels, independent of window/Canvas coordinates.
    let last = vec2<i32>(textureDimensions(source_image, 0)) - vec2<i32>(1);
    let radius = i32(parameters.radius.x);
    var sum = vec4<f32>(0.0);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let texel = clamp(position + axis * offset, vec2<i32>(0), last);
        // sRGB views decode RGB before averaging; alpha and the scratch view stay linear.
        sum += textureLoad(source_image, texel, 0);
    }
    return sum / f32(2 * radius + 1);
}

@fragment
fn horizontal(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(vec2<i32>(position.xy), vec2<i32>(1, 0));
}

@fragment
fn vertical(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return blur(vec2<i32>(position.xy), vec2<i32>(0, 1));
}
