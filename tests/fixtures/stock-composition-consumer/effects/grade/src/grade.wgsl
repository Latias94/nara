struct Parameters {
    // Exposure multiplier, saturation, contrast, padding.
    values: vec4<f32>,
};

@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> parameters: Parameters;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // The equal-extent attachment maps pixel centers directly to the actual source texels.
    let pixel = textureLoad(source_image, vec2<i32>(position.xy), 0);
    if pixel.a == 0.0 {
        return vec4<f32>(0.0);
    }

    // Hardware sRGB decoding already happened; grade straight linear RGB, then premultiply.
    let exposed = (pixel.rgb / pixel.a) * parameters.values.x;
    let luminance = dot(exposed, vec3<f32>(0.2126, 0.7152, 0.0722));
    let saturated = mix(vec3<f32>(luminance), exposed, parameters.values.y);
    let contrasted = (saturated - vec3<f32>(0.5)) * parameters.values.z + vec3<f32>(0.5);
    let color = clamp(contrasted, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color * pixel.a, pixel.a);
}
