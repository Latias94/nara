fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let width = i32(textureDimensions(material_texture).x);
    let index = clamp(i32(round(sampled.r * f32(width - 1) + material_parameters.x)), 0, width - 1);
    let palette = textureLoad(material_texture, vec2<i32>(index, 0), 0);
    return vec4<f32>(mix(sampled.rgb, palette.rgb, material_parameters.y) * tint.rgb, sampled.a * palette.a * tint.a);
}
