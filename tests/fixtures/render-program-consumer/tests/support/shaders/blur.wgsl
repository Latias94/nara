@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var destination_image: texture_storage_2d<rgba32float, write>;

fn apply_blur(pixel: vec2<u32>, axis: vec2<i32>) {
    if any(pixel >= textureDimensions(destination_image)) {
        return;
    }
    let p = vec2<i32>(pixel);
    let last = vec2<i32>(textureDimensions(source_image, 0)) - vec2<i32>(1);
    let before = clamp(p - axis, vec2<i32>(0), last);
    let after = clamp(p + axis, vec2<i32>(0), last);
    let color = (textureLoad(source_image, before, 0)
        + textureLoad(source_image, p, 0)
        + textureLoad(source_image, after, 0)) / 3.0;
    textureStore(destination_image, p, color);
}

@compute @workgroup_size(8, 8)
fn horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    apply_blur(id.xy, vec2<i32>(1, 0));
}

@compute @workgroup_size(8, 8)
fn vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    apply_blur(id.xy, vec2<i32>(0, 1));
}
