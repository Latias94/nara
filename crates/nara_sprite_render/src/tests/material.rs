use super::*;
use nara_ecs::bevy_ecs::system::RunSystemOnce;

#[test]
fn all_declared_textures_are_captured_in_binding_order_and_missing_draws_are_skipped() {
    let mut materials = Material2dLibrary::default();
    materials
        .declare(
            Material2dDeclaration::from_parameters::<PaletteParameters>(
                "test.multitexture",
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
    let prepared = materials
        .prepare_update("test.multitexture", &[path], read_shader)
        .unwrap();
    let generation = materials.publish(&prepared, read_shader).unwrap();
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
