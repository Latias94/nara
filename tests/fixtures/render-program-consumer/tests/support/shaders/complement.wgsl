@group(0) @binding(0) var source_image: texture_2d<f32>;
@group(0) @binding(1) var destination_image: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(8, 8)
fn complement(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(destination_image)) {
        return;
    }
    let p = textureLoad(source_image, vec2<i32>(id.xy), 0);
    textureStore(destination_image, vec2<i32>(id.xy), vec4<f32>(vec3<f32>(p.a) - p.rgb, p.a));
}
