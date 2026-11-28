#![cfg(any(feature = "sprite-render", feature = "sprite-source-hydration"))]

use std::sync::Arc;

use nara_app::App;
use nara_asset::{
    AssetAppExt, AssetPlugin, Assets, NpakEntryKind, RuntimeAssetSource,
    cooked::{NpakArchive, NpakCompression, NpakInput, encode_npak},
};
use nara_core::{AssetId, Vec4};
use nara_render::{
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
    material::{Material2dBinding, SpriteMaterialAsset, SpriteMaterialBindingAsset},
};

const SHADER: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.color;
}
"#;

fn id(suffix: u128) -> AssetId {
    AssetId::try_from_uuid(uuid::Uuid::from_u128(
        0x0193_a100_1111_7000_8000_0000_0000_0000 | suffix,
    ))
    .unwrap()
}

fn image(red: u8) -> ImageAsset {
    ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![red, 0, 0, 255],
    )
    .unwrap()
}

fn material(red: f32) -> SpriteMaterialAsset {
    SpriteMaterialAsset::new(id(5), Vec4::new(red, 1.0, 1.0, 1.0)).unwrap()
}

fn material_red(binding: &Material2dBinding) -> f32 {
    f32::from_le_bytes(binding.uniform_bytes()[..4].try_into().unwrap())
}

fn archive(shader_kind: NpakEntryKind) -> RuntimeAssetSource {
    let first_image = image(17).encode_cooked().unwrap();
    let second_image = image(42).encode_cooked().unwrap();
    let first_material = material(0.25).to_cooked();
    let second_material = material(0.5).to_cooked();
    let entries = [
        (id(1), NpakEntryKind::Image, first_image.as_slice()),
        (id(2), NpakEntryKind::Image, second_image.as_slice()),
        (id(3), NpakEntryKind::Material, first_material.as_slice()),
        (id(4), NpakEntryKind::Material, second_material.as_slice()),
        (id(5), shader_kind, SHADER.as_bytes()),
    ]
    .map(|(id, kind, bytes)| NpakInput::new(id, kind, bytes, NpakCompression::None));
    let archive = NpakArchive::from_bytes(encode_npak(&entries).unwrap()).unwrap();
    RuntimeAssetSource::from_npak_archive(Arc::new(archive))
}

#[cfg(feature = "sprite-render")]
mod loader {
    use nara_asset::{AssetLoadError, AssetStates, cooked::NpakError, load_asset_sync};
    use nara_render::{
        RenderPlugin, image::ImagePreparePlugin, material::SpriteMaterialLoaderError,
        sprite_render::SpriteRenderPlugin,
    };
    use nara_scene::SpatialPlugin;

    use super::*;

    fn render_app(source: RuntimeAssetSource) -> App {
        let mut app = App::new();
        app.add_plugins((
            AssetPlugin,
            SpatialPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
            SpriteRenderPlugin,
        ))
        .unwrap();
        app.set_asset_source(source).unwrap();
        app
    }

    fn assert_binding(mut app: App) {
        let world = app.world_mut().unwrap();
        let handle = load_asset_sync::<SpriteMaterialBindingAsset>(world, id(3)).unwrap();
        let binding = &world
            .resource::<Assets<SpriteMaterialBindingAsset>>()
            .get(handle)
            .unwrap()
            .0;
        assert_eq!(material_red(binding), 0.25);
        assert_eq!(
            world
                .resource::<AssetStates>()
                .state(handle.id())
                .unwrap()
                .dependencies(),
            &[id(5)]
        );
    }

    #[test]
    fn sprite_render_loads_cooked_materials_and_checks_the_shader_kind() {
        assert_binding(render_app(archive(NpakEntryKind::Shader)));

        let mut app = render_app(archive(NpakEntryKind::Raw));
        let error = load_asset_sync::<SpriteMaterialBindingAsset>(app.world_mut().unwrap(), id(3))
            .unwrap_err();
        let AssetLoadError::Decode { source, .. } = error else {
            panic!("expected a material load error, got {error}");
        };
        assert!(matches!(
            source.downcast_ref::<SpriteMaterialLoaderError>(),
            Some(SpriteMaterialLoaderError::Dependency(
                AssetLoadError::Archive(NpakError::WrongKind {
                    expected: NpakEntryKind::Shader,
                    actual: NpakEntryKind::Raw,
                    ..
                })
            ))
        ));
    }

    #[cfg(feature = "material-authoring")]
    #[test]
    fn sprite_render_loads_source_materials_and_tracks_the_same_shader_dependency() {
        use nara_asset::{RuntimeAssetMode, source::asset_id_path};
        use nara_core::vfs::MemoryAssetSource;

        let mut files = MemoryAssetSource::new();
        let descriptor = format!(
            r#"SpriteMaterial(shader: "{}", color: (0.25, 1.0, 1.0, 1.0))"#,
            id(5)
        );
        files
            .insert(&asset_id_path(id(3)), descriptor.into_bytes())
            .unwrap();
        files
            .insert(&asset_id_path(id(5)), SHADER.as_bytes())
            .unwrap();
        assert_binding(render_app(RuntimeAssetSource::from_memory(
            RuntimeAssetMode::Source,
            files,
        )));
    }
}

#[cfg(feature = "sprite-source-hydration")]
mod hydration {
    use std::time::{Duration, Instant};

    use nara_asset::{
        Asset, AssetProductRef, AssetServer, AssetStates, ImportedProductId, LoadState, add_asset,
    };
    use nara_core::{Color, Vec2};
    use nara_ecs::{Entity, World};
    use nara_render::{
        image::ImagePlugin,
        material::{MaterialParameterValue, SamplerDescriptor},
        sprite::{Sprite, SpriteSourceHydrationPlugin, TextureRegion},
    };
    use nara_scene::components::SpriteSource;

    use super::*;

    const REGION: TextureRegion = TextureRegion::new(Vec2::ZERO, Vec2::new(0.5, 1.0));
    const RUNTIME_TINT: Color = Color::rgb(0.1, 0.2, 0.3);

    fn headless_app() -> App {
        let mut app = App::new();
        app.add_plugins((AssetPlugin, ImagePlugin, SpriteSourceHydrationPlugin))
            .unwrap();
        app.set_asset_source(archive(NpakEntryKind::Shader))
            .unwrap();
        app
    }

    fn source() -> SpriteSource {
        SpriteSource {
            size: Vec2::new(20.0, 10.0),
            tint: Color::BLACK,
            layer: 5,
            image: Some(id(1)),
            material: Some(id(3)),
        }
    }

    fn tick(app: &mut App) {
        app.update_with_delta(Duration::ZERO).unwrap();
    }

    fn drive_until(app: &mut App, ready: impl Fn(&World) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready(app.world()) {
            assert!(Instant::now() < deadline, "sprite assets never settled");
            tick(app);
            std::thread::yield_now();
        }
    }

    fn image_red(world: &World, entity: Entity) -> Option<u8> {
        let handle = world.get::<Sprite>(entity)?.material.image?;
        let image = world.resource::<Assets<ImageAsset>>().get(handle)?;
        Some(image.pixels()[0])
    }

    fn bound_red(world: &World, entity: Entity) -> Option<f32> {
        world.get::<Material2dBinding>(entity).map(material_red)
    }

    fn load_failed<T: Asset>(world: &World, id: AssetId) -> bool {
        let reference = AssetProductRef::new(id.into(), ImportedProductId::PRIMARY).unwrap();
        world
            .resource::<AssetServer>()
            .get_product_handle::<T>(reference)
            .and_then(|handle| world.resource::<AssetStates>().state(handle.id()))
            .is_some_and(|state| matches!(state.load_state(), LoadState::Failed { .. }))
    }

    #[test]
    fn headless_hydration_follows_authored_changes_and_preserves_runtime_effects() {
        let mut app = headless_app();
        let entity = app.world_mut().unwrap().spawn(source()).id();
        tick(&mut app);
        let sprite = app.world().get::<Sprite>(entity).unwrap();
        assert_eq!(sprite.size, source().size);
        assert_eq!(sprite.material.tint, Color::BLACK);
        assert_eq!(sprite.material.image, None);
        drive_until(&mut app, |world| {
            image_red(world, entity) == Some(17) && bound_red(world, entity) == Some(0.25)
        });

        let world = app.world_mut().unwrap();
        {
            let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
            sprite.texture_region = Some(REGION);
            sprite.material.sampler = SamplerDescriptor::NEAREST_CLAMP;
            sprite.material.tint = RUNTIME_TINT;
        }
        *world.get_mut::<SpriteSource>(entity).unwrap() = SpriteSource {
            size: Vec2::splat(40.0),
            layer: 7,
            tint: Color::WHITE,
            image: Some(id(2)),
            material: Some(id(4)),
        };
        tick(&mut app);
        assert_eq!(image_red(app.world(), entity), Some(17));
        assert_eq!(bound_red(app.world(), entity), Some(0.25));
        drive_until(&mut app, |world| {
            image_red(world, entity) == Some(42) && bound_red(world, entity) == Some(0.5)
        });
        let sprite = app.world().get::<Sprite>(entity).unwrap();
        assert_eq!(sprite.size, Vec2::splat(40.0));
        assert_eq!(sprite.layer, 7);
        assert_eq!(sprite.texture_region, Some(REGION));
        assert_eq!(sprite.material.tint, RUNTIME_TINT);
        assert_eq!(sprite.material.sampler, SamplerDescriptor::NEAREST_CLAMP);

        let world = app.world_mut().unwrap();
        let handle = world.resource_mut::<AssetServer>().load_id(id(4)).unwrap();
        let updated = SpriteMaterialBindingAsset(material(0.75).bind(SHADER).unwrap());
        world
            .resource_mut::<Assets<SpriteMaterialBindingAsset>>()
            .insert(handle, updated);
        tick(&mut app);
        assert_eq!(bound_red(app.world(), entity), Some(0.75));

        {
            let mut source = app
                .world_mut()
                .unwrap()
                .get_mut::<SpriteSource>(entity)
                .unwrap();
            source.image = Some(id(6));
            source.material = Some(id(7));
        }
        drive_until(&mut app, |world| {
            load_failed::<ImageAsset>(world, id(6))
                && load_failed::<SpriteMaterialBindingAsset>(world, id(7))
        });
        assert_eq!(image_red(app.world(), entity), Some(42));
        assert_eq!(bound_red(app.world(), entity), Some(0.75));

        {
            let mut source = app
                .world_mut()
                .unwrap()
                .get_mut::<SpriteSource>(entity)
                .unwrap();
            source.image = None;
            source.material = None;
        }
        tick(&mut app);
        assert_eq!(image_red(app.world(), entity), None);
        assert_eq!(bound_red(app.world(), entity), None);
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().texture_region,
            Some(REGION)
        );
    }

    #[test]
    fn headless_hydration_leaves_manual_sprites_and_bindings_under_game_control() {
        let mut app = headless_app();
        let world = app.world_mut().unwrap();
        let handle = add_asset(world, image(80)).unwrap();
        let sprite = Sprite::from_texture(handle, Vec2::splat(8.0)).with_texture_region(REGION);
        let binding = material(1.0).bind(SHADER).unwrap();
        let manual = world
            .spawn((source(), sprite.clone(), binding.clone()))
            .id();
        let generated = world.spawn(source()).id();
        let overridden = world.spawn(source()).id();
        drive_until(&mut app, |world| {
            image_red(world, generated) == Some(17) && bound_red(world, generated) == Some(0.25)
        });
        assert_eq!(app.world().get::<Sprite>(manual), Some(&sprite));
        assert_eq!(app.world().get::<Material2dBinding>(manual), Some(&binding));

        let world = app.world_mut().unwrap();
        let authored_binding = world.get::<Material2dBinding>(overridden).unwrap();
        let mut parameters = authored_binding.parameters().clone();
        parameters.insert(
            "color".into(),
            MaterialParameterValue::Vec4([0.75, 1.0, 1.0, 1.0]),
        );
        let override_binding = authored_binding
            .generation()
            .bind_values(parameters, authored_binding.textures().clone())
            .unwrap();
        world
            .entity_mut(overridden)
            .insert(override_binding.clone());
        world.get_mut::<Sprite>(overridden).unwrap().material.image = Some(handle);
        *world.get_mut::<SpriteSource>(overridden).unwrap() = SpriteSource::default();
        world.entity_mut(manual).remove::<SpriteSource>();
        world.entity_mut(generated).remove::<SpriteSource>();
        tick(&mut app);
        assert_eq!(app.world().get::<Sprite>(manual), Some(&sprite));
        assert_eq!(app.world().get::<Material2dBinding>(manual), Some(&binding));
        assert!(app.world().get::<Sprite>(generated).is_none());
        assert!(app.world().get::<Material2dBinding>(generated).is_none());
        assert_eq!(image_red(app.world(), overridden), Some(80));
        assert_eq!(
            app.world().get::<Material2dBinding>(overridden),
            Some(&override_binding)
        );
    }
}
