fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let width = i32(textureDimensions(material_texture).x);
    let index = clamp(i32(round(sampled.r * f32(width - 1) + material_parameters.x)), 0, width - 1);
    let palette_color = textureLoad(material_texture, vec2<i32>(index, 0), 0);
    let color = mix(sampled.rgb, palette_color.rgb, clamp(material_parameters.y, 0.0, 1.0));
    return vec4<f32>(color * tint.rgb, sampled.a * palette_color.a * tint.a);
}
