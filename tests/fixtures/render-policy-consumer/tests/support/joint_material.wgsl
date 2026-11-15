fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let r = (f32(u32(round(nara_view.virtual_elapsed * 250.0)) % 8u) * 24.0
        + material_parameters.level * 256.0) / 255.0;
    let g = (f32(u32(round(nara_view.real_elapsed * 250.0)) % 4u) * 32.0
        + f32(nara_view.paused) * 16.0
        + f32(nara_view.spatial_interpolated) * 16.0) / 255.0;
    let b = 64.0 / 255.0;
    return vec4<f32>(r, g, b, 1.0);
}
