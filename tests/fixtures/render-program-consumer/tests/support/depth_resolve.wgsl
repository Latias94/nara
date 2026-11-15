@group(0) @binding(0) var source_depth: texture_depth_2d;
@group(0) @binding(1) var resolved: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(8, 8)
fn resolve(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let size = textureDimensions(source_depth);
    if invocation.x >= size.x || invocation.y >= size.y {
        return;
    }
    let pixel = vec2<i32>(invocation.xy);
    let depth = textureLoad(source_depth, pixel, 0);
    textureStore(resolved, pixel, vec4<f32>(depth, depth, depth, 1.0));
}
