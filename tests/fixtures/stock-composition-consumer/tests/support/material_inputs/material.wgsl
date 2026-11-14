fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette = textureSample(palette_texture, sprite_sampler, uv);
    let accent = textureLoad(accent_texture, vec2<i32>(0, 0), 0);
    let straight = select(vec3<f32>(0.0), palette.rgb / max(palette.a, 0.000001), palette.a > 0.0);
    return vec4<f32>(
        straight * accent.rgb * material_parameters.scale.rgb,
        palette.a * (1.0 + material_parameters.scale.w),
    ) * sampled * tint;
}
