use super::*;
use crate::image::ImageContentSnapshot;

const PALETTE_B: [u8; 4] = [17, 34, 51, 255];

fn replacement_palette(product: AssetProductRef) -> ImageAsset {
    ImageAsset::from_product(
        product,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        PALETTE_B.to_vec(),
    )
    .unwrap()
}

#[test]
fn authored_textures_freeze_before_prepare_share_across_cells_and_views_and_recover_after_publish()
{
    let palette = image_product(1);
    let accent = image_product(2);
    let asset = import_material(
        product(1),
        declaration("test.authored-frozen-textures", 1),
        AssetRef::Product(palette),
        AssetRef::Product(accent),
        SHADER,
    );
    let mut assets = TestAssets::default();
    let palette_handle = assets.image(palette);
    let accent_handle = assets.image(accent);
    let palette_a = ImageContentSnapshot::capture(
        palette_handle,
        &assets.server,
        &assets.states,
        &assets.images,
    )
    .unwrap()
    .prepared_snapshot();
    let accent_a = ImageContentSnapshot::capture(
        accent_handle,
        &assets.server,
        &assets.states,
        &assets.images,
    )
    .unwrap()
    .prepared_snapshot();
    let handle = assets.material(asset.clone());
    let authored = Material2d::from_parameters(
        handle,
        asset.generation().declaration(),
        &parameters(0.25),
        BTreeMap::new(),
    )
    .unwrap();
    let expected_binding = authored
        .bind(&asset, |reference| assets.resolve(reference))
        .unwrap();
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let cameras = [0, 1].map(|order| {
        world
            .spawn((
                Camera2d {
                    order,
                    target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                    viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                    viewport_height: 100.0,
                    ..Camera2d::default()
                },
                Transform2d::default(),
            ))
            .id()
    });
    let entity = spawn_authored(world, authored);
    let mut tilemap = Tilemap::new(Vec2::ONE);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));
    tilemap.set_cell(TileCoord::new(1, 0), TileCell::new(TileIndex::new(1)));
    world.entity_mut(entity).insert(tilemap);
    let stock = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    place_roots(world, &[cameras[0], cameras[1], entity, stock]);
    assets.install(&mut app);
    app.add_systems(
        CoreStage::Extract,
        (move |time: Res<nara_app::RealTime>, mut images: ResMut<Assets<ImageAsset>>| {
            if time.frame == 1 {
                assert!(
                    images
                        .insert(palette_handle, replacement_palette(palette))
                        .is_some()
                );
            }
        })
        .after(crate::sprite_render::extract::extract_sprites),
    )
    .unwrap();
    app.add_systems(
        CoreStage::Update,
        move |time: Res<nara_app::RealTime>,
              mut images: ResMut<Assets<ImageAsset>>,
              mut states: ResMut<AssetStates>,
              mut events: ResMut<AssetEvents>| {
            if time.frame == 3 {
                images
                    .commit_loaded(
                        palette_handle,
                        replacement_palette(palette),
                        &mut states,
                        &mut events,
                        None,
                        None,
                    )
                    .unwrap();
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let mut retained = Vec::new();

    for frame in 1..=3 {
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        let captures = prepared_captures(world, entity);
        let extracted = world.resource::<ExtractedSprites>().as_slice();
        let queued = world.resource::<QueuedSpriteItems>();
        let batches = world.resource::<SpriteBatches>();
        let stats = world.resource::<SpriteRenderStats>();
        let prepared_palette = world
            .resource::<PreparedImages>()
            .get(image_resource_key(palette_handle))
            .unwrap();
        assert_eq!(prepared_palette.resource().pixels(), &PALETTE_B);
        assert_eq!(stats.unprepared_textures, 0);
        assert_eq!(stats.invalid_texture_regions, 0);

        if frame == 2 {
            assert!(captures.is_empty());
            assert_eq!(extracted.len(), 1);
            assert_eq!(extracted[0].entity, stock);
            assert_eq!(extracted[0].source_order, 0);
            assert!(extracted[0].material_extension.is_none());
            assert_eq!(queued.len(), 2);
            assert!(queued.as_slice().iter().all(|item| item.entity == stock));
            assert_eq!(batches.len(), 2);
            assert!(
                batches
                    .as_slice()
                    .iter()
                    .all(|batch| batch.material_extension.is_none())
            );
            assert_eq!(stats.missing_materials, 1);
            assert_eq!(stats.extracted_sprites, 1);
            assert_eq!(stats.extracted_tile_cells, 0);
            assert_eq!(stats.queued_items, 2);
            assert_eq!(stats.batches, 2);
            continue;
        }

        let expected_palette = if frame == 1 {
            assert_eq!(
                prepared_palette.identity().asset_version(),
                palette_a.identity().asset_version()
            );
            assert_ne!(
                prepared_palette.identity().slot_revision(),
                palette_a.identity().slot_revision()
            );
            assert_ne!(
                world
                    .resource::<AssetStates>()
                    .published_slot_revision(palette_handle.id()),
                Some(
                    &world
                        .resource::<Assets<ImageAsset>>()
                        .slot_revision(palette_handle)
                )
            );
            palette_a.clone()
        } else {
            let published = ImageContentSnapshot::capture(
                palette_handle,
                world.resource::<AssetServer>(),
                world.resource::<AssetStates>(),
                world.resource::<Assets<ImageAsset>>(),
            )
            .unwrap()
            .prepared_snapshot();
            assert_eq!(&published, prepared_palette);
            assert_ne!(
                published.identity().asset_version(),
                palette_a.identity().asset_version()
            );
            assert_ne!(
                published.identity().slot_revision(),
                palette_a.identity().slot_revision()
            );
            published
        };
        assert_eq!(captures.len(), 3);
        let capture = captures[0];
        assert_eq!(capture.binding, expected_binding);
        assert_eq!(
            capture.binding.generation().declaration(),
            asset.generation().declaration()
        );
        assert_eq!(
            capture.binding.uniform_bytes(),
            parameters(0.25).encode_uniform().unwrap()
        );
        assert_eq!(
            capture.textures,
            vec![(1, expected_palette.clone()), (3, accent_a.clone())]
        );
        assert!(std::ptr::eq(
            capture.textures[0].1.resource().pixels(),
            expected_palette.resource().pixels()
        ));
        assert!(captures.iter().all(|other| Arc::ptr_eq(capture, other)));
        assert_eq!(extracted.len(), 4);
        assert_eq!(queued.len(), 8);
        assert_eq!(batches.len(), 6);
        for view_index in 0..2 {
            let items = queued
                .as_slice()
                .iter()
                .filter(|item| item.view_index == view_index && item.entity == entity)
                .collect::<Vec<_>>();
            assert_eq!(items.len(), 3);
            for item in items {
                assert!(Arc::ptr_eq(
                    item.material_extension.as_ref().unwrap(),
                    capture
                ));
                assert_eq!(item.material.alpha_mode, AlphaMode2d::Opaque);
            }
            let material_batches = batches
                .for_view(view_index)
                .filter(|batch| batch.material_extension.is_some())
                .collect::<Vec<_>>();
            assert_eq!(material_batches.len(), 2);
            for batch in material_batches {
                assert!(Arc::ptr_eq(
                    batch.material_extension.as_ref().unwrap(),
                    capture
                ));
                assert_eq!(
                    batch.instances.len(),
                    match batch.kind {
                        SpriteBatchKind::Sprite => 1,
                        SpriteBatchKind::Tilemap => 2,
                    }
                );
            }
        }
        assert_eq!(stats.missing_materials, 0);
        assert_eq!(stats.extracted_sprites, 2);
        assert_eq!(stats.extracted_tile_cells, 2);
        assert_eq!(stats.queued_items, 8);
        assert_eq!(stats.batches, 6);
        retained.push((Arc::clone(capture), queued.clone(), batches.clone()));
    }
    runtime.finish().unwrap();

    assert_eq!(retained.len(), 2);
    let (capture_a, queued_a, batches_a) = &retained[0];
    let capture_b = &retained[1].0;
    assert!(!Arc::ptr_eq(capture_a, capture_b));
    assert_eq!(capture_a.binding, capture_b.binding);
    assert_eq!(capture_a.textures[0].1, palette_a);
    assert_eq!(capture_a.textures[0].1.resource().pixels(), &[255; 4]);
    assert_eq!(capture_b.textures[0].1.resource().pixels(), &PALETTE_B);
    for capture in queued_a
        .as_slice()
        .iter()
        .filter_map(|item| item.material_extension.as_ref())
        .chain(
            batches_a
                .as_slice()
                .iter()
                .filter_map(|batch| batch.material_extension.as_ref()),
        )
    {
        assert!(Arc::ptr_eq(capture, capture_a));
        assert_eq!(capture.textures[0].1, palette_a);
        assert_eq!(capture.textures[0].1.resource().pixels(), &[255; 4]);
    }
}
