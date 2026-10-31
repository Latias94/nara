// Shared unfiltered RGBA32Float input and distinct write-only storage output.
@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var destination_image: texture_storage_2d<rgba32float, write>;

fn in_bounds(pixel: vec2<u32>) -> bool {
    return all(pixel < textureDimensions(source_image, 0))
        && all(pixel < textureDimensions(destination_image));
}

fn blur(pixel: vec2<u32>, axis: vec2<i32>) -> vec4<f32> {
    let position = vec2<i32>(pixel);
    let last = vec2<i32>(textureDimensions(source_image, 0)) - vec2<i32>(1);
    let before = clamp(position - axis, vec2<i32>(0), last);
    let after = clamp(position + axis, vec2<i32>(0), last);
    return (textureLoad(source_image, before, 0)
        + textureLoad(source_image, position, 0)
        + textureLoad(source_image, after, 0)) / 3.0;
}

@compute @workgroup_size(8, 8, 1)
fn blur_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id.xy) {
        return;
    }
    textureStore(destination_image, vec2<i32>(id.xy), blur(id.xy, vec2<i32>(1, 0)));
}

@compute @workgroup_size(8, 8, 1)
fn blur_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id.xy) {
        return;
    }
    textureStore(destination_image, vec2<i32>(id.xy), blur(id.xy, vec2<i32>(0, 1)));
}

@compute @workgroup_size(8, 8, 1)
fn complement(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id.xy) {
        return;
    }
    let pixel = textureLoad(source_image, vec2<i32>(id.xy), 0);
    textureStore(destination_image, vec2<i32>(id.xy), vec4<f32>(vec3<f32>(pixel.a) - pixel.rgb, pixel.a));
}

@compute @workgroup_size(8, 8, 1)
fn hud(@builtin(global_invocation_id) id: vec3<u32>) {
    if !in_bounds(id.xy) {
        return;
    }
    // Keep these constants aligned with HUD_RECT and HUD_COLOR in effects.rs.
    if id.x >= 2u && id.x < 10u && id.y >= 2u && id.y < 6u {
        textureStore(destination_image, vec2<i32>(id.xy), vec4<f32>(1.0, 1.0, 0.0, 1.0));
    } else {
        textureStore(destination_image, vec2<i32>(id.xy), textureLoad(source_image, vec2<i32>(id.xy), 0));
    }
}
