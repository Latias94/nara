use super::*;
use nara_ecs::bevy_ecs::system::RunSystemOnce;

#[test]
fn all_declared_textures_are_captured_in_binding_order_and_missing_draws_are_skipped() {
    let materials = multitexture_materials("test.multitexture");
    let generation = materials.current("test.multitexture").unwrap();
    let palette = nara_asset::allocate_anonymous_handle().unwrap();
    let accent = nara_asset::allocate_anonymous_handle().unwrap();
    let missing = nara_asset::allocate_anonymous_handle().unwrap();
    let parameters = PaletteParameters {
        x: 0.25,
        y: 0.75,
        z: 0.0,
        w: 0.0,
    };
    let binding = generation
        .bind(
            &parameters,
            BTreeMap::from([
                ("accent".to_owned(), accent),
                ("palette".to_owned(), palette),
            ]),
        )
        .unwrap();
    let missing_binding = generation
        .bind(
            &parameters,
            BTreeMap::from([
                ("accent".to_owned(), accent),
                ("palette".to_owned(), missing),
            ]),
        )
        .unwrap();
    let mut extracted = ExtractedSprites::default();
    let stock = placeholder_extracted_sprite();
    let mut custom = stock.clone();
    custom.source_order += 1;
    custom.material_extension = Some(binding.clone());
    let mut missing_sprite = stock.clone();
    missing_sprite.source_order += 2;
    missing_sprite.material_extension = Some(missing_binding);
    extracted.push(stock);
    extracted.push(custom);
    extracted.push(missing_sprite);
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let palette_snapshot = publish_image_snapshot(
        palette,
        vec![255; 16],
        &mut images,
        &mut states,
        &mut events,
    );
    let accent_snapshot = publish_image_snapshot(
        accent,
        [255, 0, 0, 255].repeat(4),
        &mut images,
        &mut states,
        &mut events,
    );
    let mut prepared_images = PreparedImages::default();
    prepared_images.insert(accent_snapshot.clone());
    prepared_images.insert(palette_snapshot.clone());
    let mut views = nara_render::ExtractedViews::default();
    views.push(test_view());
    let mut world = World::new();
    world.insert_resource(extracted);
    world.insert_resource(views);
    world.insert_resource(prepared_images);
    world.init_resource::<QueuedSpriteItems>();
    world.init_resource::<SpriteRenderStats>();

    world.run_system_once(crate::queue::queue_sprites).unwrap();

    let queued = world.resource::<QueuedSpriteItems>().as_slice();
    assert_eq!(queued.len(), 2);
    assert!(queued[0].material_extension.is_none());
    assert_eq!(queued[1].material.alpha_mode, AlphaMode2d::Opaque);
    let captured = queued[1].material_extension.as_ref().unwrap();
    assert_eq!(captured.binding, binding);
    assert_eq!(
        captured.textures,
        vec![(1, palette_snapshot), (3, accent_snapshot)]
    );
    assert_eq!(world.resource::<SpriteRenderStats>().unprepared_textures, 1);

    world.remove_resource::<PreparedImages>();
    world.run_system_once(crate::queue::queue_sprites).unwrap();

    let queued = world.resource::<QueuedSpriteItems>().as_slice();
    assert_eq!(queued.len(), 1);
    assert!(queued[0].material_extension.is_none());
    assert_eq!(world.resource::<SpriteRenderStats>().unprepared_textures, 2);
}

#[test]
fn offscreen_multitexture_validation_preserves_error_priority_with_two_or_zero_views() {
    let materials = multitexture_materials("test.offscreen-multitexture");
    let generation = materials.current("test.offscreen-multitexture").unwrap();
    let palette = nara_asset::allocate_anonymous_handle().unwrap();
    let accent = nara_asset::allocate_anonymous_handle().unwrap();
    let missing = nara_asset::allocate_anonymous_handle().unwrap();
    let parameters = PaletteParameters {
        x: 0.25,
        y: 0.75,
        z: 0.0,
        w: 0.0,
    };
    let binding = generation
        .bind(
            &parameters,
            BTreeMap::from([
                ("accent".to_owned(), accent),
                ("palette".to_owned(), palette),
            ]),
        )
        .unwrap();
    let mut visible = placeholder_extracted_sprite();
    visible.material_extension = Some(binding.clone());
    let mut later_missing = visible.clone();
    later_missing.source_order += 1;
    later_missing.world_center = Vec2::splat(10_000.0);
    later_missing.material_extension = Some(
        generation
            .bind(
                &parameters,
                BTreeMap::from([
                    ("accent".to_owned(), accent),
                    ("palette".to_owned(), missing),
                ]),
            )
            .unwrap(),
    );
    let mut both_missing = later_missing.clone();
    both_missing.source_order += 1;
    both_missing.material_extension = Some(
        generation
            .bind(
                &parameters,
                BTreeMap::from([
                    ("accent".to_owned(), missing),
                    ("palette".to_owned(), missing),
                ]),
            )
            .unwrap(),
    );
    let mut base_missing = both_missing.clone();
    base_missing.source_order += 1;
    base_missing.material.image = Some(missing);
    let mut invalid_uv = base_missing.clone();
    invalid_uv.source_order += 1;
    invalid_uv.texture_region = TextureUvRect::new(Vec2::ZERO, Vec2::splat(-1.0));
    let mut extracted = ExtractedSprites::default();
    for sprite in [
        later_missing,
        both_missing,
        base_missing,
        invalid_uv,
        visible,
    ] {
        extracted.push(sprite);
    }
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let palette_snapshot = publish_image_snapshot(
        palette,
        vec![255; 16],
        &mut images,
        &mut states,
        &mut events,
    );
    let accent_snapshot = publish_image_snapshot(
        accent,
        [255, 0, 0, 255].repeat(4),
        &mut images,
        &mut states,
        &mut events,
    );
    let mut prepared_images = PreparedImages::default();
    prepared_images.insert(palette_snapshot.clone());
    prepared_images.insert(accent_snapshot.clone());
    let mut views = nara_render::ExtractedViews::default();
    views.push(test_view());
    views.push(test_view());
    let mut world = World::new();
    world.insert_resource(extracted);
    world.insert_resource(views);
    world.insert_resource(prepared_images);
    world.init_resource::<QueuedSpriteItems>();
    world.init_resource::<SpriteRenderStats>();

    world.run_system_once(crate::queue::queue_sprites).unwrap();

    let queued = world.resource::<QueuedSpriteItems>().as_slice();
    assert_eq!(queued.len(), 2);
    for (view_index, item) in queued.iter().enumerate() {
        assert_eq!(item.view_index, view_index);
        assert_eq!(
            item.source_order,
            placeholder_extracted_sprite().source_order
        );
        let captured = item.material_extension.as_ref().unwrap();
        assert_eq!(captured.binding, binding);
        assert_eq!(
            captured.textures,
            vec![(1, palette_snapshot.clone()), (3, accent_snapshot.clone())]
        );
    }
    let stats = world.resource::<SpriteRenderStats>();
    assert_eq!(stats.queued_items, 2);
    assert_eq!(stats.unprepared_textures, 3);
    assert_eq!(stats.invalid_texture_regions, 1);

    world.resource_mut::<nara_render::ExtractedViews>().clear();
    world.run_system_once(crate::queue::queue_sprites).unwrap();

    assert!(world.resource::<QueuedSpriteItems>().is_empty());
    let stats = world.resource::<SpriteRenderStats>();
    assert_eq!(stats.queued_items, 0);
    assert_eq!(stats.unprepared_textures, 3);
    assert_eq!(stats.invalid_texture_regions, 1);
}

#[test]
fn two_visible_views_retain_exact_multitexture_captures_after_successor_and_owner_removal() {
    let id = "test.retained-multitexture";
    let mut materials = multitexture_materials(id);
    let declaration = materials.current(id).unwrap().declaration().clone();
    let successor_source = Arc::<str>::from(
        materials
            .current(id)
            .unwrap()
            .shader()
            .user_source()
            .replace("color * tint.rgb", "color.bgr * tint.rgb"),
    );
    let palette = nara_asset::allocate_anonymous_handle().unwrap();
    let accent = nara_asset::allocate_anonymous_handle().unwrap();
    let textures = BTreeMap::from([
        ("accent".to_owned(), accent),
        ("palette".to_owned(), palette),
    ]);
    let mut views = nara_render::ExtractedViews::default();
    let mut excluded = test_view();
    excluded.mapping = nara_render::ViewMapping2d::orthographic(
        nara_core::Mat3::from_translation(Vec2::splat(1_000.0)),
        100.0,
        excluded.viewport,
    )
    .unwrap();
    views.push(excluded);
    views.push(test_view());
    let mut shifted = test_view();
    shifted.mapping = nara_render::ViewMapping2d::orthographic(
        nara_core::Mat3::from_translation(Vec2::new(5.0, -5.0)),
        100.0,
        shifted.viewport,
    )
    .unwrap();
    views.push(shifted);
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut world = World::new();
    world.insert_resource(views);
    world.init_resource::<PreparedImages>();
    world.init_resource::<SpriteRenderStats>();
    let mut retained = Vec::new();
    let mut expected = Vec::new();

    for (round, (parameters, palette_pixels, accent_pixels)) in [
        (
            PaletteParameters {
                x: 0.25,
                y: 0.75,
                z: 0.125,
                w: 0.5,
            },
            [255, 0, 0, 255].repeat(4),
            [0, 255, 0, 255].repeat(4),
        ),
        (
            PaletteParameters {
                x: 0.75,
                y: 0.25,
                z: -0.5,
                w: 1.0,
            },
            [0, 0, 255, 255].repeat(4),
            [255, 255, 0, 255].repeat(4),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if round == 1 {
            let path = AssetPath::new("multitexture.wgsl").unwrap();
            let read_shader = |_: &AssetPath| Ok(Arc::clone(&successor_source));
            let prepared = materials.prepare_update(id, &[path], read_shader).unwrap();
            materials.publish(&prepared, read_shader).unwrap();
        }
        let generation = materials.current(id).unwrap();
        let binding = generation.bind(&parameters, textures.clone()).unwrap();
        let palette_snapshot = publish_image_snapshot(
            palette,
            palette_pixels.clone(),
            &mut images,
            &mut states,
            &mut events,
        );
        let accent_snapshot = publish_image_snapshot(
            accent,
            accent_pixels.clone(),
            &mut images,
            &mut states,
            &mut events,
        );
        world
            .resource_mut::<PreparedImages>()
            .insert(palette_snapshot.clone());
        world
            .resource_mut::<PreparedImages>()
            .insert(accent_snapshot.clone());
        let mut extracted = ExtractedSprites::default();
        let mut sprite = placeholder_extracted_sprite();
        sprite.material_extension = Some(binding.clone());
        extracted.push(sprite);
        world.insert_resource(extracted);
        world.init_resource::<QueuedSpriteItems>();
        world.init_resource::<SpriteBatches>();

        world.run_system_once(crate::queue::queue_sprites).unwrap();
        world
            .run_system_once(crate::queue::sort_and_batch_sprites)
            .unwrap();

        let queued = world.remove_resource::<QueuedSpriteItems>().unwrap();
        let batches = world.remove_resource::<SpriteBatches>().unwrap();
        assert_eq!(
            queued
                .as_slice()
                .iter()
                .map(|item| item.view_index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            batches
                .as_slice()
                .iter()
                .map(|batch| batch.view_index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        for (item, batch) in queued.as_slice().iter().zip(batches.as_slice()) {
            let captured = item.material_extension.as_ref().unwrap();
            assert_eq!(captured.binding, binding);
            assert_eq!(
                captured.textures,
                vec![(1, palette_snapshot.clone()), (3, accent_snapshot.clone())]
            );
            assert_eq!(batch.material_extension.as_ref().unwrap(), captured);
            assert_eq!(batch.instances, vec![item.instance]);
            assert_eq!(item.material.alpha_mode, AlphaMode2d::Opaque);
            assert_eq!(batch.material, item.material);
        }
        let stats = world.resource::<SpriteRenderStats>();
        assert_eq!(stats.queued_items, 2);
        assert_eq!(stats.batches, 2);
        assert_eq!(stats.unprepared_textures, 0);
        assert_eq!(stats.invalid_texture_regions, 0);
        expected.push((
            generation.sequence(),
            generation.shader().source().to_owned(),
            generation.shader().digest(),
            parameters.parameter_values().unwrap(),
            parameters.encode_uniform().unwrap(),
            [
                (1, palette_snapshot.identity().clone(), palette_pixels),
                (3, accent_snapshot.identity().clone(), accent_pixels),
            ],
        ));
        retained.push((queued, batches));
    }

    world.remove_resource::<ExtractedSprites>();
    world.remove_resource::<PreparedImages>();
    drop(materials);
    drop(images);
    drop(states);
    drop(events);

    assert_ne!(expected[0].0, expected[1].0);
    assert_ne!(expected[0].2, expected[1].2);
    assert_ne!(expected[0].4, expected[1].4);
    for (old_texture, new_texture) in expected[0].5.iter().zip(&expected[1].5) {
        assert_ne!(old_texture.1.asset_version(), new_texture.1.asset_version());
        assert_ne!(old_texture.1.slot_revision(), new_texture.1.slot_revision());
    }
    for ((queued, batches), (sequence, shader, digest, values, uniform, snapshots)) in
        retained.iter().zip(&expected)
    {
        for (item, batch) in queued.as_slice().iter().zip(batches.as_slice()) {
            for captured in [
                item.material_extension.as_ref().unwrap(),
                batch.material_extension.as_ref().unwrap(),
            ] {
                assert_eq!(captured.binding.generation().sequence(), *sequence);
                assert_eq!(captured.binding.generation().declaration(), &declaration);
                assert_eq!(captured.binding.generation().shader().source(), shader);
                assert_eq!(captured.binding.generation().shader().digest(), *digest);
                assert_eq!(captured.binding.parameters(), values);
                assert_eq!(captured.binding.uniform_bytes(), uniform);
                assert_eq!(captured.binding.textures(), &textures);
                assert_eq!(captured.textures.len(), snapshots.len());
                for ((binding, snapshot), (expected_binding, identity, pixels)) in
                    captured.textures.iter().zip(snapshots)
                {
                    assert_eq!(binding, expected_binding);
                    assert_eq!(snapshot.identity(), identity);
                    assert_eq!(snapshot.resource().pixels(), pixels);
                }
            }
        }
    }
}

#[test]
fn retained_generations_values_and_image_versions_remain_distinct_batch_inputs() {
    let mut materials = palette_materials("test.palette", AlphaMode2d::Opaque);
    let original_generation = materials.current("test.palette").unwrap().clone();
    let texture = nara_asset::allocate_anonymous_handle().unwrap();
    let parameters = PaletteParameters {
        x: 0.0,
        y: 1.0,
        z: 0.0,
        w: 0.0,
    };
    let textures = BTreeMap::from([("palette".to_owned(), texture)]);
    let original = original_generation
        .bind(&parameters, textures.clone())
        .unwrap();
    let mut extracted = ExtractedSprites::default();
    let mut sprite = placeholder_extracted_sprite();
    sprite.material_extension = Some(original.clone());
    extracted.push(sprite.clone());
    let mut views = nara_render::ExtractedViews::default();
    views.push(test_view());
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let old_snapshot = publish_image_snapshot(
        texture,
        vec![255; 16],
        &mut images,
        &mut states,
        &mut events,
    );
    let mut prepared_images = PreparedImages::default();
    prepared_images.insert(old_snapshot.clone());
    let mut world = World::new();
    world.insert_resource(extracted);
    world.insert_resource(views);
    world.insert_resource(prepared_images);
    world.init_resource::<QueuedSpriteItems>();
    world.init_resource::<SpriteBatches>();
    world.init_resource::<SpriteRenderStats>();
    world.run_system_once(crate::queue::queue_sprites).unwrap();
    world
        .run_system_once(crate::queue::sort_and_batch_sprites)
        .unwrap();
    let old_item = world.resource::<QueuedSpriteItems>().as_slice()[0].clone();
    let retained = world.resource::<SpriteBatches>().clone();

    let path = AssetPath::new("palette.wgsl").unwrap();
    let successor_source = Arc::<str>::from(
        include_str!("../../../../examples/support/palette_material.wgsl")
            .replace("color * tint.rgb", "color.bgr * tint.rgb"),
    );
    let read_shader = |_: &AssetPath| Ok(Arc::clone(&successor_source));
    let prepared = materials
        .prepare_update("test.palette", &[path], read_shader)
        .unwrap();
    let successor_generation = materials.publish(&prepared, read_shader).unwrap();
    let successor = successor_generation
        .bind(&parameters, textures.clone())
        .unwrap();
    let changed_values = original_generation
        .bind(
            &PaletteParameters {
                x: 1.0,
                ..parameters
            },
            textures,
        )
        .unwrap();
    assert_ne!(original_generation, successor_generation);
    assert_eq!(original.uniform_bytes(), successor.uniform_bytes());
    assert_ne!(original.uniform_bytes(), changed_values.uniform_bytes());
    drop(materials);
    let new_snapshot = publish_image_snapshot(
        texture,
        [0, 255, 0, 255].repeat(4),
        &mut images,
        &mut states,
        &mut events,
    );
    assert_ne!(
        old_snapshot.identity().asset_version(),
        new_snapshot.identity().asset_version()
    );
    assert_ne!(
        old_snapshot.identity().slot_revision(),
        new_snapshot.identity().slot_revision()
    );
    world
        .resource_mut::<PreparedImages>()
        .insert(new_snapshot.clone());
    let mut extracted = ExtractedSprites::default();
    for (order, binding) in [
        original.clone(),
        original.clone(),
        changed_values.clone(),
        successor.clone(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut current = sprite.clone();
        current.source_order = order as u64;
        current.material_extension = Some(binding);
        extracted.push(current);
    }
    world.insert_resource(extracted);
    drop(images);
    drop(states);

    world.run_system_once(crate::queue::queue_sprites).unwrap();
    world
        .run_system_once(crate::queue::sort_and_batch_sprites)
        .unwrap();

    let queued = world.resource::<QueuedSpriteItems>().as_slice();
    assert_eq!(queued.len(), 4);
    assert_eq!(
        build_sprite_batches(&[old_item, queued[0].clone()]).len(),
        2
    );
    let batches = world.resource::<SpriteBatches>();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches.as_slice()[0].instances.len(), 2);
    for (batch, binding) in batches
        .as_slice()
        .iter()
        .zip([&original, &changed_values, &successor])
    {
        let captured = batch.material_extension.as_ref().unwrap();
        assert_eq!(&captured.binding, binding);
        assert_eq!(captured.textures, vec![(1, new_snapshot.clone())]);
        assert_eq!(batch.material.alpha_mode, AlphaMode2d::Opaque);
    }
    let captured = retained.as_slice()[0].material_extension.as_ref().unwrap();
    assert_eq!(captured.binding, original);
    assert_eq!(captured.binding.generation(), &original_generation);
    assert_eq!(
        captured.binding.uniform_bytes(),
        parameters.encode_uniform().unwrap()
    );
    assert_eq!(captured.textures, vec![(1, old_snapshot)]);
    assert_eq!(captured.textures[0].1.resource().pixels(), &[255; 16]);
    assert_eq!(world.resource::<SpriteRenderStats>().unprepared_textures, 0);
}

fn multitexture_materials(id: &str) -> Material2dLibrary {
    let mut materials = Material2dLibrary::default();
    materials
        .declare(
            Material2dDeclaration::from_parameters::<PaletteParameters>(
                id,
                1,
                AlphaMode2d::Opaque,
                vec![
                    MaterialTextureDeclaration::new("accent", "accent_texture", 3).unwrap(),
                    MaterialTextureDeclaration::new("palette", "material_texture", 1).unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let path = AssetPath::new("multitexture.wgsl").unwrap();
    let source = Arc::<str>::from(
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
            let palette = textureLoad(material_texture, vec2<i32>(0, 0), 0);
            let accent = textureLoad(accent_texture, vec2<i32>(0, 0), 0);
            let color = palette.rgb * material_parameters.y + accent.rgb * material_parameters.x;
            return vec4<f32>(color * tint.rgb, sampled.a * palette.a * accent.a * tint.a);
        }",
    );
    let read_shader = |_: &AssetPath| Ok(Arc::clone(&source));
    let prepared = materials.prepare_update(id, &[path], read_shader).unwrap();
    materials.publish(&prepared, read_shader).unwrap();
    materials
}

fn publish_image_snapshot(
    handle: Handle<ImageAsset>,
    pixels: Vec<u8>,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
) -> PreparedImageSnapshot {
    let image = test_image_with_pixels(pixels);
    let source_hash = image.source().unwrap().source_hash();
    let import_recipe_digest = image.source().unwrap().import_recipe().digest();
    images
        .commit_loaded(
            handle,
            image,
            states,
            events,
            Some(source_hash),
            Some(import_recipe_digest),
        )
        .unwrap();
    PreparedImageSnapshot::capture(handle, images, states).unwrap()
}
