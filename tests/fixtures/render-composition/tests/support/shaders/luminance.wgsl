@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> counts: array<atomic<u32>, 4>;

@compute @workgroup_size(8, 8, 1)
fn luminance(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(source_image, 0)) {
        return;
    }
    // Measure linear premultiplied rgb, including alpha-zero pixels, without unpremultiplying.
    let pixel = textureLoad(source_image, vec2<i32>(id.xy), 0);
    let value = clamp(dot(pixel.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    let bin = min(u32(value * 4.0), 3u);
    atomicAdd(&counts[bin], 1u);
}
