// The caller pins the exact initialized inputs and provides distinct output storage.
@group(0) @binding(0) var current_image: texture_2d<f32>;
@group(0) @binding(1) var history_image: texture_2d<f32>;
@group(0) @binding(2) var destination_image: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(8, 8, 1)
fn accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(current_image, 0))
        || any(id.xy >= textureDimensions(history_image, 0))
        || any(id.xy >= textureDimensions(destination_image)) {
        return;
    }
    let pixel = vec2<i32>(id.xy);
    let current = textureLoad(current_image, pixel, 0);
    let history = textureLoad(history_image, pixel, 0);
    textureStore(destination_image, pixel, 0.5 * (current + history));
}
