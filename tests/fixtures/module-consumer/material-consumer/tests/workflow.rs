use std::{collections::BTreeMap, sync::Arc};

use nara_asset::{AssetPath, allocate_anonymous_handle};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, MaterialParameterValue, MaterialUpdateError};
use nara_material_module_consumer::{palette_sprite, startup_materials};

#[test]
fn native_carrier_has_one_default_or_custom_owner_and_supports_explicit_disable() {
    use nara::{
        MinimalPlugins, ProductRecipe, Runtime2dPlugins, SchemaContribution,
        app::{App, PluginGroup, PluginPlan, PluginPlanError},
        asset::Assets,
        material::{
            MATERIAL_PLUGIN_ID, MATERIAL_SCHEMA_OWNER_ID, MATERIAL_SCHEMA_PROVIDER,
            Material2dAsset, Material2dPlugin,
        },
        reflect::{ComponentTypeId, component_registry},
    };

    let recipe = || {
        ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<Material2dPlugin>::for_default([MATERIAL_SCHEMA_PROVIDER])
                    .unwrap(),
            )
            .unwrap()
    };
    let mut defaults = App::new();
    defaults.add_plugins(Runtime2dPlugins).unwrap();
    let mut custom = App::new();
    custom
        .add_plugins(recipe().with_defaults(MinimalPlugins))
        .unwrap();
    let mut disabled = App::new();
    disabled
        .add_plugins(Runtime2dPlugins.edit().disable::<Material2dPlugin>())
        .unwrap();
    for (app, selected) in [(defaults, true), (custom, true), (disabled, false)] {
        assert_eq!(
            app.installed_plugins()
                .filter(|plugin| plugin.id == MATERIAL_PLUGIN_ID)
                .count(),
            usize::from(selected),
        );
        let sealed = app.seal().unwrap();
        let world = sealed.world();
        let registry = component_registry(world).unwrap();
        assert_eq!(
            registry
                .schema(&ComponentTypeId::new("nara.material2d"))
                .is_some(),
            selected,
        );
        assert_eq!(
            registry
                .contribution_receipts()
                .filter(|receipt| receipt.owner().owner() == MATERIAL_SCHEMA_OWNER_ID)
                .count(),
            usize::from(selected),
        );
        assert_eq!(
            world.contains_resource::<Assets<Material2dAsset>>(),
            selected
        );
    }
    assert!(matches!(
        PluginPlan::resolve(recipe().with_defaults(Runtime2dPlugins)),
        Err(PluginPlanError::DuplicatePlugin { plugin }) if plugin == MATERIAL_PLUGIN_ID,
    ));
}

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
