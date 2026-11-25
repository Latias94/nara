use super::*;
use crate::image::ImageColorSpace;
use crate::material::Material2dBinding;
use crate::tilemap::{TileSetAssets, TileSetDescriptor, TileSetPublicationInput};
use nara_app::{FixedTime, FixedUpdateSet, RealTime};
use nara_asset::{AssetProductRef, AssetServer, ImportedProductId};
use nara_ecs::{Query, Resource, With};

fn capture_frames(runtime: &mut RuntimeInstance, count: usize) -> Vec<ExtractedSprites> {
    (0..count)
        .map(|_| {
            runtime.drive(Duration::ZERO).unwrap();
            runtime.world().resource::<ExtractedSprites>().clone()
        })
        .collect()
}

#[test]
fn unchanged_frames_preserve_exact_tile_geometry_and_current_source_order() {
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let prefix = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    let mut map = Tilemap::new(Vec2::new(8.0, 4.0))
        .with_layer(3)
        .with_sort_key(-2)
        .with_y_sort_anchor(Vec2::new(1.0, -2.0));
    map.set_cell(
        TileCoord::new(-2, 1),
        TileCell::new(TileIndex::new(5)).with_color(Color::rgb(1.0, 0.0, 0.0)),
    );
    map.set_cell(TileCoord::new(1, -1), TileCell::new(TileIndex::new(2)));
    map.clear_dirty_chunks();
    let entity = world
        .spawn((map, Transform2d::from_translation(Vec2::new(10.0, -6.0))))
        .id();
    let parent = world.spawn_empty().add_children(&[entity, prefix]).id();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        match world.resource::<RealTime>().frame {
            4 => {
                world.entity_mut(parent).replace_children(&[prefix, entity]);
            }
            7 => {
                let front = world
                    .spawn((
                        Sprite::from_color(Vec2::ONE, Color::BLACK),
                        Transform2d::default(),
                    ))
                    .id();
                world.entity_mut(parent).insert_child(0, front);
            }
            _ => {}
        }
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = capture_frames(&mut runtime, 9);
    runtime.finish().unwrap();

    let expected = [
        ExtractedSprite {
            entity,
            source_order: 0,
            kind: ExtractedSpriteKind::TilemapCell {
                coord: TileCoord::new(-2, 1),
                tile: TileIndex::new(5),
            },
            material: ExtractedSpriteMaterial::from_color(Color::WHITE),
            image_content: None,
            material_extension: None,
            texture_region: TextureUvRect::FULL,
            world_center: Vec2::new(-2.0, 0.0),
            world_x_axis: Vec2::new(4.0, 0.0),
            world_y_axis: Vec2::new(0.0, 2.0),
            color: Color::rgb(1.0, 0.0, 0.0),
            layer: 3,
            world_z: 0.0,
            sort_key: -2,
            world_sort_y: -2.0,
        },
        ExtractedSprite {
            entity,
            source_order: 1,
            kind: ExtractedSpriteKind::TilemapCell {
                coord: TileCoord::new(1, -1),
                tile: TileIndex::new(2),
            },
            material: ExtractedSpriteMaterial::from_color(Color::WHITE),
            image_content: None,
            material_extension: None,
            texture_region: TextureUvRect::FULL,
            world_center: Vec2::new(22.0, -8.0),
            world_x_axis: Vec2::new(4.0, 0.0),
            world_y_axis: Vec2::new(0.0, 2.0),
            color: Color::WHITE,
            layer: 3,
            world_z: 0.0,
            sort_key: -2,
            world_sort_y: -10.0,
        },
    ];
    for (frames, front_count) in frames.chunks_exact(3).zip(0..3) {
        for frame in frames {
            assert_eq!(frame, &frames[0]);
            assert_eq!(frame.len(), 3 + usize::from(front_count == 2));
            for (index, expected) in expected.iter().enumerate() {
                let mut expected = expected.clone();
                expected.source_order += front_count as u64;
                assert_eq!(frame.as_slice()[front_count + index], expected);
            }
            let prefix_index = if front_count == 0 { 2 } else { front_count - 1 };
            assert_eq!(frame.as_slice()[prefix_index].entity, prefix);
        }
    }
}

#[test]
fn cell_edits_survive_cleared_dirty_hints_and_retained_extractions() {
    let mut app = sprite_app();
    let mut map = Tilemap::new(Vec2::new(8.0, 4.0));
    let coord = TileCoord::new(1, -1);
    map.set_cell(coord, TileCell::new(TileIndex::new(1)));
    map.clear_dirty_chunks();
    let world = app.world_mut().unwrap();
    let entity = world.spawn((map, Transform2d::default())).id();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        let frame = world.resource::<RealTime>().frame;
        if !matches!(frame, 4 | 7 | 10) {
            return;
        }
        let mut map = world.get_mut::<Tilemap>(entity).unwrap();
        match frame {
            4 => {
                map.set_cell(
                    coord,
                    TileCell::new(TileIndex::new(7)).with_color(Color::rgba(0.5, 0.25, 1.0, 0.5)),
                );
            }
            7 => {
                map.remove_cell(coord);
            }
            10 => {
                map.set_cell(
                    TileCoord::new(-1, 2),
                    TileCell::new(TileIndex::new(3)).with_color(Color::rgb(0.0, 1.0, 0.0)),
                );
            }
            _ => unreachable!(),
        }
        map.clear_dirty_chunks();
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = capture_frames(&mut runtime, 12);
    let dirty_hints_empty = runtime
        .world()
        .get::<Tilemap>(entity)
        .unwrap()
        .dirty_chunks()
        .next()
        .is_none();
    runtime.finish().unwrap();

    assert!(dirty_hints_empty);
    for (stage, (coord, tile, color, center)) in [
        (coord, 1, Color::WHITE, Vec2::new(12.0, -2.0)),
        (
            coord,
            7,
            Color::rgba(0.5, 0.25, 1.0, 0.5),
            Vec2::new(12.0, -2.0),
        ),
        (
            TileCoord::new(-1, 2),
            3,
            Color::rgb(0.0, 1.0, 0.0),
            Vec2::new(-4.0, 10.0),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let start = if stage == 2 { 9 } else { stage * 3 };
        for frame in &frames[start..start + 3] {
            assert_eq!(frame.len(), 1);
            let extracted = &frame.as_slice()[0];
            assert_eq!(extracted.entity, entity);
            assert_eq!(extracted.source_order, 0);
            assert_eq!(
                extracted.kind,
                ExtractedSpriteKind::TilemapCell {
                    coord,
                    tile: TileIndex::new(tile),
                }
            );
            assert_eq!(extracted.color, color);
            assert_eq!(extracted.world_center, center);
            assert_eq!(extracted.world_x_axis, Vec2::new(4.0, 0.0));
            assert_eq!(extracted.world_y_axis, Vec2::new(0.0, 2.0));
        }
    }
    assert!(frames[6..9].iter().all(ExtractedSprites::is_empty));
}

#[test]
fn cloned_maps_with_coincident_dirty_revisions_preserve_distinct_cell_values() {
    let coord = TileCoord::new(0, 0);
    let mut seed = Tilemap::new(Vec2::new(6.0, 2.0));
    seed.set_cell(coord, TileCell::new(TileIndex::new(0)));
    let mut first = seed.clone();
    let mut second = seed.clone();
    first.set_cell(
        coord,
        TileCell::new(TileIndex::new(3)).with_color(Color::rgb(1.0, 0.0, 0.0)),
    );
    second.set_cell(
        coord,
        TileCell::new(TileIndex::new(8)).with_color(Color::rgb(0.0, 0.0, 1.0)),
    );
    assert_eq!(
        first.dirty_chunks().collect::<Vec<_>>(),
        second.dirty_chunks().collect::<Vec<_>>()
    );
    first.clear_dirty_chunks();
    second.clear_dirty_chunks();
    let mut replacement = Some(second);
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let entity = world.spawn((first, Transform2d::default())).id();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        if world.resource::<RealTime>().frame == 4 {
            world.entity_mut(entity).insert(replacement.take().unwrap());
        }
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = capture_frames(&mut runtime, 6);
    runtime.finish().unwrap();

    for (frames, (tile, color)) in frames.chunks_exact(3).zip([
        (3, Color::rgb(1.0, 0.0, 0.0)),
        (8, Color::rgb(0.0, 0.0, 1.0)),
    ]) {
        for frame in frames {
            assert_eq!(frame.len(), 1);
            let extracted = &frame.as_slice()[0];
            assert_eq!(extracted.entity, entity);
            assert_eq!(
                extracted.kind,
                ExtractedSpriteKind::TilemapCell {
                    coord,
                    tile: TileIndex::new(tile),
                }
            );
            assert_eq!(extracted.color, color);
            assert_eq!(extracted.world_center, Vec2::new(3.0, 1.0));
        }
    }
}

#[test]
fn public_map_fields_and_parented_shear_are_reobserved_independently() {
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Transform2d::default()).id();
    let mut map = Tilemap::new(Vec2::new(4.0, 2.0));
    map.set_cell(TileCoord::new(1, -1), TileCell::new(TileIndex::new(2)));
    let entity = world.spawn((map, Transform2d::default())).id();
    place_parented(world, entity, parent);
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        match world.resource::<RealTime>().frame {
            4 => world.get_mut::<Tilemap>(entity).unwrap().tile_size = Vec2::new(8.0, 6.0),
            7 => world.get_mut::<Tilemap>(entity).unwrap().layer.index = 5,
            10 => world.get_mut::<Tilemap>(entity).unwrap().sort_key = -7,
            13 => {
                world.get_mut::<Tilemap>(entity).unwrap().y_sort_anchor =
                    Some(Vec2::new(2.0, -1.0));
            }
            16 => {
                world.get_mut::<Transform2d>(parent).unwrap().translation = Vec2::new(10.0, 20.0);
            }
            19 => world.get_mut::<Transform2d>(parent).unwrap().scale = Vec2::new(2.0, 1.0),
            22 => {
                world.get_mut::<Transform2d>(entity).unwrap().rotation =
                    std::f32::consts::FRAC_PI_4;
            }
            _ => {}
        }
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = capture_frames(&mut runtime, 24);
    runtime.finish().unwrap();

    let q = std::f32::consts::FRAC_1_SQRT_2;
    let expected = [
        (
            Vec2::new(6.0, -1.0),
            Vec2::new(2.0, 0.0),
            Vec2::Y,
            0,
            0,
            0.0,
        ),
        (
            Vec2::new(12.0, -3.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
            0,
            0,
            0.0,
        ),
        (
            Vec2::new(12.0, -3.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
            5,
            0,
            0.0,
        ),
        (
            Vec2::new(12.0, -3.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
            5,
            -7,
            0.0,
        ),
        (
            Vec2::new(12.0, -3.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
            5,
            -7,
            -4.0,
        ),
        (
            Vec2::new(22.0, 17.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
            5,
            -7,
            16.0,
        ),
        (
            Vec2::new(34.0, 17.0),
            Vec2::new(8.0, 0.0),
            Vec2::new(0.0, 3.0),
            5,
            -7,
            16.0,
        ),
        (
            Vec2::new(10.0 + 30.0 * q, 20.0 + 9.0 * q),
            Vec2::new(8.0 * q, 4.0 * q),
            Vec2::new(-6.0 * q, 3.0 * q),
            5,
            -7,
            20.0 + 10.0 * q,
        ),
    ];
    for (frames, (center, x_axis, y_axis, layer, sort_key, sort_y)) in
        frames.chunks_exact(3).zip(expected)
    {
        for frame in frames {
            assert_eq!(frame.len(), 1);
            let extracted = &frame.as_slice()[0];
            assert_eq!(extracted.entity, entity);
            assert!(extracted.world_center.abs_diff_eq(center, 0.000_02));
            assert!(extracted.world_x_axis.abs_diff_eq(x_axis, 0.000_02));
            assert!(extracted.world_y_axis.abs_diff_eq(y_axis, 0.000_02));
            assert_eq!(extracted.layer, layer);
            assert_eq!(extracted.sort_key, sort_key);
            assert!((extracted.world_sort_y - sort_y).abs() < 0.000_02);
        }
    }
    let sheared = &frames[23].as_slice()[0];
    assert!((sheared.world_x_axis.dot(sheared.world_y_axis) + 18.0).abs() < 0.000_02);
}

#[test]
fn tileset_mutations_aba_and_foreign_stores_refresh_current_inputs() {
    let tileset_product = AssetProductRef::new(
        StableAssetId::parse_str("4e1a5700-0000-4000-8000-000000000001").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    let image_product = AssetProductRef::new(
        StableAssetId::parse_str("4e1a5700-0000-4000-8000-000000000002").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    let mut server = AssetServer::new();
    let handle = server.reserve_product::<TileSet>(tileset_product).unwrap();
    let image = server.reserve_product::<ImageAsset>(image_product).unwrap();
    let alternate = nara_asset::allocate_anonymous_handle::<TileSet>().unwrap();
    let other_image = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mut pixels = ImageAsset::from_product(
        image_product,
        ImageExtent::new(8, 4),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        [63, 127, 255, 255].repeat(32),
    )
    .unwrap();
    assert!(pixels.try_attach_retention_owner(Arc::new(())));
    let descriptor = TileSetDescriptor::new(
        tileset_product,
        image_product,
        TileAtlasLayout::grid(Vec2::splat(2.0), 4, 2),
    )
    .unwrap();
    let input = TileSetPublicationInput::from_product(
        handle,
        descriptor,
        image,
        pixels.share_retained().unwrap(),
    );
    let mut owner = TileSetAssets::default();
    let mut images = Assets::<ImageAsset>::default();
    let mut tilesets = Assets::<TileSet>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let prepared = owner
        .prepare_publications(vec![input], &[], &server, None, &tilesets, &states)
        .unwrap();
    images
        .commit_loaded(image, pixels, &mut states, &mut events, None, None)
        .unwrap();
    prepared.commit(&server, &images, &mut tilesets, &mut states, &mut events);
    let managed_value = tilesets.get(handle).unwrap().clone();
    drop(owner);
    let mut app = sprite_app();
    let world = app.world_mut().unwrap();
    world.insert_resource(server);
    world.insert_resource(images);
    world.insert_resource(states);
    world.insert_resource(tilesets);
    let mut map = Tilemap::new(Vec2::new(8.0, 4.0)).with_tileset(handle);
    map.set_cell(
        TileCoord::new(0, 0),
        TileCell::new(TileIndex::new(5)).with_color(Color::rgba(0.5, 0.25, 1.0, 0.5)),
    );
    let entity = world.spawn((map, Transform2d::default())).id();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        match world.resource::<RealTime>().frame {
            4 => {
                let mut tilesets = world.resource_mut::<Assets<TileSet>>();
                let tileset = tilesets.get_mut(handle).unwrap();
                *tileset = tileset
                    .clone()
                    .with_image(image, TileAtlasLayout::grid(Vec2::splat(2.0), 2, 4));
            }
            7 => {
                world.resource_mut::<Assets<TileSet>>().insert(
                    handle,
                    TileSet::from_image(other_image, TileAtlasLayout::grid(Vec2::splat(2.0), 8, 1))
                        .with_tint(Color::rgb(0.25, 0.5, 0.75))
                        .with_alpha_mode(AlphaMode2d::Opaque),
                );
            }
            10 => {
                let mut tilesets = world.resource_mut::<Assets<TileSet>>();
                tilesets.remove(handle).unwrap();
                tilesets.insert(
                    handle,
                    TileSet::from_image(image, TileAtlasLayout::grid(Vec2::splat(2.0), 1, 8)),
                );
            }
            13 | 16 => {
                let (image, columns, rows) = if world.resource::<RealTime>().frame == 13 {
                    (image, 4, 2)
                } else {
                    (other_image, 2, 4)
                };
                let mut foreign = Assets::<TileSet>::default();
                foreign.insert(
                    handle,
                    TileSet::from_image(
                        image,
                        TileAtlasLayout::grid(Vec2::splat(2.0), columns, rows),
                    ),
                );
                foreign.insert(
                    alternate,
                    TileSet::from_image(image, TileAtlasLayout::grid(Vec2::splat(2.0), 8, 1)),
                );
                world.insert_resource(foreign);
            }
            19 => world.get_mut::<Tilemap>(entity).unwrap().tileset = Some(alternate),
            22 => {
                world
                    .resource_mut::<Assets<TileSet>>()
                    .insert(handle, managed_value.clone());
                world.get_mut::<Tilemap>(entity).unwrap().tileset = Some(handle);
            }
            _ => {}
        }
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = capture_frames(&mut runtime, 24);
    let world = runtime.world();
    let states = world.resource::<AssetStates>();
    let usable = states
        .state(handle.id())
        .and_then(nara_asset::AssetState::usable_version);
    let published_slot = states.published_slot_revision(handle.id()).cloned();
    let actual_slot = world.resource::<Assets<TileSet>>().slot_revision(handle);
    runtime.finish().unwrap();
    assert!(usable.is_some());
    assert_ne!(published_slot, Some(actual_slot));

    for frame in &frames[..3] {
        assert_eq!(frame.len(), 1);
        let captured = frame.as_slice()[0].image_content.as_ref().unwrap();
        assert_eq!(captured.handle(), image);
        assert_eq!(captured.product(), image_product);
        assert_eq!(captured.extent(), ImageExtent::new(8, 4));
        assert_eq!(captured.pixels(), [63, 127, 255, 255].repeat(32));
        assert!(captured.same_allocation(frames[0].as_slice()[0].image_content.as_ref().unwrap()));
    }
    for frame in &frames[3..21] {
        assert!(frame.as_slice()[0].image_content.is_none());
    }
    assert!(frames[21..].iter().all(ExtractedSprites::is_empty));
    let plain = Color::rgba(0.5, 0.25, 1.0, 0.5);
    let expected = [
        (
            image,
            Vec2::new(0.25, 0.5),
            Vec2::new(0.25, 0.5),
            plain,
            AlphaMode2d::Blend,
        ),
        (
            image,
            Vec2::new(0.5, 0.5),
            Vec2::new(0.5, 0.25),
            plain,
            AlphaMode2d::Blend,
        ),
        (
            other_image,
            Vec2::new(0.625, 0.0),
            Vec2::new(0.125, 1.0),
            Color::rgba(0.125, 0.125, 0.75, 0.5),
            AlphaMode2d::Opaque,
        ),
        (
            image,
            Vec2::new(0.0, 0.625),
            Vec2::new(1.0, 0.125),
            plain,
            AlphaMode2d::Blend,
        ),
        (
            image,
            Vec2::new(0.25, 0.5),
            Vec2::new(0.25, 0.5),
            plain,
            AlphaMode2d::Blend,
        ),
        (
            other_image,
            Vec2::new(0.5, 0.5),
            Vec2::new(0.5, 0.25),
            plain,
            AlphaMode2d::Blend,
        ),
        (
            other_image,
            Vec2::new(0.625, 0.0),
            Vec2::new(0.125, 1.0),
            plain,
            AlphaMode2d::Blend,
        ),
    ];
    for (frames, (image, min, size, color, alpha_mode)) in
        frames[..21].chunks_exact(3).zip(expected)
    {
        for frame in frames {
            assert_eq!(frame.len(), 1);
            let extracted = &frame.as_slice()[0];
            assert_eq!(extracted.entity, entity);
            assert_eq!(extracted.material.image, Some(image));
            assert_eq!(extracted.material.alpha_mode, alpha_mode);
            assert_eq!(extracted.texture_region, TextureUvRect::new(min, size));
            assert_eq!(extracted.color, color);
            assert_eq!(extracted.world_center, Vec2::new(4.0, 2.0));
            assert_eq!(extracted.world_x_axis, Vec2::new(4.0, 0.0));
            assert_eq!(extracted.world_y_axis, Vec2::new(0.0, 2.0));
        }
    }
}

#[test]
fn removing_readding_and_reusing_entity_slots_preserves_current_tiles() {
    #[derive(Resource)]
    struct ReplacementEntity(Entity);

    let mut app = sprite_app();
    let mut map = Tilemap::new(Vec2::new(4.0, 2.0));
    map.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(1)));
    let mut replacement = map.clone();
    replacement.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(9)));
    let world = app.world_mut().unwrap();
    let entity = world.spawn((map, Transform2d::default())).id();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        match world.resource::<RealTime>().frame {
            4 => {
                world.entity_mut(entity).remove::<Tilemap>();
            }
            7 => {
                world.entity_mut(entity).insert(replacement.clone());
            }
            10 => {
                let next = world.despawn_no_free(entity).unwrap();
                world.insert_resource(ReplacementEntity(next));
            }
            13 => {
                let mut map = Tilemap::new(Vec2::new(4.0, 2.0));
                map.set_cell(TileCoord::new(-1, 0), TileCell::new(TileIndex::new(5)));
                let successor = world.resource::<ReplacementEntity>().0;
                let new_entity = world
                    .spawn_at(successor, (map, Transform2d::default()))
                    .unwrap()
                    .id();
                world.insert_resource(ReplacementEntity(new_entity));
            }
            _ => {}
        }
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let frames = capture_frames(&mut runtime, 15);
    let new_entity = runtime.world().resource::<ReplacementEntity>().0;
    runtime.finish().unwrap();

    assert_ne!(new_entity, entity);
    assert_eq!(new_entity.index(), entity.index());
    assert!(frames[3..6].iter().all(ExtractedSprites::is_empty));
    assert!(frames[9..12].iter().all(ExtractedSprites::is_empty));
    for (start, entity, coord, tile, center) in [
        (0, entity, TileCoord::new(0, 0), 1, Vec2::new(2.0, 1.0)),
        (6, entity, TileCoord::new(0, 0), 9, Vec2::new(2.0, 1.0)),
        (
            12,
            new_entity,
            TileCoord::new(-1, 0),
            5,
            Vec2::new(-2.0, 1.0),
        ),
    ] {
        for frame in &frames[start..start + 3] {
            assert_eq!(frame.len(), 1);
            let extracted = &frame.as_slice()[0];
            assert_eq!(extracted.entity, entity);
            assert_eq!(extracted.source_order, 0);
            assert_eq!(extracted.world_center, center);
            assert_eq!(
                extracted.kind,
                ExtractedSpriteKind::TilemapCell {
                    coord,
                    tile: TileIndex::new(tile),
                }
            );
        }
    }
}

#[test]
fn tile_geometry_uses_the_latest_fixed_transform_and_stays_stable_between_ticks() {
    let mut app = sprite_app();
    app.add_systems(
        CoreStage::FixedUpdate,
        (|mut transforms: Query<&mut Transform2d, With<Tilemap>>| {
            for mut transform in &mut transforms {
                transform.translation.x += 8.0;
            }
        })
        .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let mut map = Tilemap::new(Vec2::new(2.0, 4.0));
    map.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(1)));
    let world = app.world_mut().unwrap();
    let entity = world.spawn((map, Transform2d::default())).id();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let unchanged = capture_frames(&mut runtime, 3);
    let step = FixedTime::DEFAULT_TIMESTEP;
    runtime.drive(step).unwrap();
    let completed = runtime.world().resource::<ExtractedSprites>().clone();
    let mut presented = Vec::new();
    for _ in 0..3 {
        runtime.drive(step / 4).unwrap();
        presented.push((
            runtime.world().resource::<ExtractedSprites>().clone(),
            *runtime.world().get::<Transform2d>(entity).unwrap(),
            runtime.world().resource::<FixedTime>().tick(),
        ));
    }
    let repeated = capture_frames(&mut runtime, 1);
    runtime.finish().unwrap();

    assert!(unchanged.iter().all(|frame| frame == &unchanged[0]));
    assert_eq!(unchanged[0].as_slice()[0].world_center, Vec2::new(1.0, 2.0));
    for (frame, simulation, tick) in &presented {
        assert_eq!(*tick, 1);
        assert_eq!(simulation.translation, Vec2::new(8.0, 0.0));
        assert_eq!(frame, &completed);
        assert_eq!(frame.len(), 1);
        let extracted = &frame.as_slice()[0];
        assert_eq!(extracted.entity, entity);
        assert_eq!(extracted.world_center, Vec2::new(9.0, 2.0));
        assert_eq!(extracted.world_x_axis, Vec2::X);
        assert_eq!(extracted.world_y_axis, Vec2::new(0.0, 2.0));
    }
    assert_eq!(repeated[0], presented[2].0);
}

#[test]
fn equal_material_bindings_retain_the_current_owner_and_outlive_retirement() {
    let materials = palette_materials("test.tile-reuse-owner", AlphaMode2d::Opaque);
    let generation = materials.current("test.tile-reuse-owner").unwrap();
    let texture = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
    let parameters = PaletteParameters {
        x: 0.25,
        y: 0.75,
        z: 0.0,
        w: 1.0,
    };
    let first_owner = Arc::new(());
    let first_weak = Arc::downgrade(&first_owner);
    let first = generation
        .with_lifetime_owner(first_owner)
        .bind(
            &parameters,
            BTreeMap::from([("palette".to_owned(), texture)]),
        )
        .unwrap();
    let current_owner = Arc::new(());
    let current_weak = Arc::downgrade(&current_owner);
    let current = generation
        .with_lifetime_owner(current_owner)
        .bind(
            &parameters,
            BTreeMap::from([("palette".to_owned(), texture)]),
        )
        .unwrap();
    assert_eq!(first, current);
    let current_values = current.uniform_bytes().as_ptr() as usize;
    let mut replacement = Some(current);
    let mut app = sprite_app();
    insert_loaded_image(&mut app, texture);
    let mut map = Tilemap::new(Vec2::new(4.0, 2.0));
    map.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(1)));
    let world = app.world_mut().unwrap();
    let entity = world.spawn((map, first, Transform2d::default())).id();
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        match world.resource::<RealTime>().frame {
            4 => {
                world.entity_mut(entity).insert(replacement.take().unwrap());
            }
            7 => {
                world.entity_mut(entity).remove::<Material2dBinding>();
            }
            _ => {}
        }
    })
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let old_frames = capture_frames(&mut runtime, 3);
    let bound_frames = capture_frames(&mut runtime, 3);
    let current_values_per_frame = bound_frames
        .iter()
        .map(|frame| {
            frame.as_slice()[0]
                .material_extension
                .as_ref()
                .unwrap()
                .binding
                .uniform_bytes()
                .as_ptr() as usize
        })
        .collect::<Vec<_>>();
    let retained = bound_frames[2].clone();
    drop(bound_frames);
    let unbound = capture_frames(&mut runtime, 3);
    let first_alive_with_capture = first_weak.upgrade().is_some();
    drop(old_frames);
    let first_released_while_world_alive = first_weak.upgrade().is_none();
    runtime.finish().unwrap();
    drop(materials);

    assert!(first_alive_with_capture);
    assert!(first_released_while_world_alive);
    assert_eq!(current_values_per_frame, vec![current_values; 3]);
    for frame in unbound {
        assert_eq!(frame.len(), 1);
        assert!(frame.as_slice()[0].material_extension.is_none());
    }
    assert!(current_weak.upgrade().is_some());
    assert_eq!(retained.len(), 1);
    let extracted = &retained.as_slice()[0];
    assert_eq!(extracted.world_center, Vec2::new(2.0, 1.0));
    assert_eq!(
        extracted
            .material_extension
            .as_ref()
            .unwrap()
            .binding
            .uniform_bytes(),
        parameters.encode_uniform().unwrap()
    );
    assert_eq!(
        extracted
            .material_extension
            .as_ref()
            .unwrap()
            .binding
            .textures(),
        &BTreeMap::from([("palette".to_owned(), texture)])
    );
    drop(retained);
    assert!(current_weak.upgrade().is_none());
}
