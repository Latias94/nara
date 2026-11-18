fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette = textureLoad(region_palette, vec2<i32>(0, 0), 0);
    let graded = sampled.rgb * material_parameters.grade.rgb + palette.rgb * material_parameters.grade.a;
    return vec4<f32>(graded * tint.rgb, sampled.a * tint.a);
}
