@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> mode: vec4<u32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var source_pixel = vec2<i32>(position.xy);
    if mode.x == 2u {
        source_pixel.x = i32(textureDimensions(source_image).x) - 1 - source_pixel.x;
    }
    let color = textureLoad(source_image, source_pixel, 0);
    if mode.x == 1u {
        return vec4<f32>(color.b, color.g, color.r, color.a);
    }
    return color;
}
