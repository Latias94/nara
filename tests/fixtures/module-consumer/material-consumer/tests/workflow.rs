use std::{collections::BTreeMap, sync::Arc};

use nara_asset::{AssetPath, allocate_anonymous_handle};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, MaterialParameterValue, MaterialUpdateError};
use nara_material_module_consumer::{palette_sprite, startup_materials};

#[test]
fn public_palette_retains_original_values_after_shader_update_and_library_retirement() {
    let mut library = startup_materials().unwrap();
    let indices = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let palette = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let (sprite, original) = palette_sprite(&library, indices, palette, -0.0).unwrap();
    let id = original.generation().declaration().id().to_owned();
    assert_eq!(
        original.generation().declaration().alpha_mode(),
        AlphaMode2d::Blend
    );
    assert_eq!(sprite.material.image, Some(indices));
    assert_eq!(
        original.textures(),
        &BTreeMap::from([("palette".to_owned(), palette)])
    );
    assert_eq!(&original.uniform_bytes()[..4], &(-0.0_f32).to_le_bytes());
    assert_eq!(
        original.parameters().get("palette.strength"),
        Some(&MaterialParameterValue::F32(1.0))
    );
    let path = AssetPath::new("palette.wgsl").unwrap();
    let edited: Arc<str> = Arc::from(
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(tint.r, sampled.g, uv.x, tint.a); }",
    );
    let prepared = library
        .prepare_update(&id, std::slice::from_ref(&path), |_| {
            Ok(Arc::clone(&edited))
        })
        .unwrap();
    let newer = library
        .publish(&prepared, |_| Ok(Arc::clone(&edited)))
        .unwrap();
    let (_, current) = palette_sprite(&library, indices, palette, 2.0).unwrap();
    assert_eq!(current.generation(), &newer);
    assert_eq!(&current.uniform_bytes()[..4], &2.0_f32.to_le_bytes());
    assert_ne!(
        original.generation().shader().digest(),
        newer.shader().digest()
    );
    assert_eq!(
        original.generation().shader().interface_digest(),
        newer.shader().interface_digest()
    );
    let failure = library
        .prepare_update(&id, &[path], |_| Ok(Arc::from("fn invalid(")))
        .unwrap_err();
    assert!(matches!(failure, MaterialUpdateError::Shader(_)));
    assert_eq!(library.current(&id), Some(&newer));
    assert_eq!(library.last_failure(&id), Some(&failure));
    drop(library);
    assert_ne!(original.generation(), current.generation());
    assert_eq!(
        original.generation().inputs()[0].source(),
        include_str!("../src/palette.wgsl")
    );
    assert!(
        original
            .generation()
            .shader()
            .source()
            .contains(include_str!("../src/palette.wgsl"))
    );
    assert_eq!(&original.uniform_bytes()[..4], &(-0.0_f32).to_le_bytes());
    assert_eq!(original.textures().get("palette"), Some(&palette));
}
