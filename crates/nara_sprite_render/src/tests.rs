use super::*;
use std::time::Duration;

use nara_app::App;
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetId, AssetPath, AssetStates, Assets,
    Handle, ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest, ImportProfile,
    ImportSettingsHash, ImportedAssetType, ImporterId, ImporterVersion, SourceHash, StableAssetId,
};
use nara_core::{Color, Vec2};
use nara_ecs::Entity;
use nara_image::{
    ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageSourceMetadata, image_resource_key,
};
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{
    Camera2d, ExtractedView, RenderImage2d, RenderPhaseLabel, RenderTarget, ViewportRect,
};
use nara_sprite::{Sprite, TextureRegion};
use nara_tilemap::{TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, Tilemap};
use nara_transform::Transform2d;

fn test_view() -> ExtractedView {
    ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport: ViewportRect::new(0, 0, 200, 100).unwrap(),
        world_position: Vec2::ZERO,
        viewport_height: 100.0,
        order: 0,
        clear_color: Color::BLACK,
    }
}

fn instance() -> SpriteInstance {
    SpriteInstance::axis_aligned(Vec2::ZERO, Vec2::splat(0.5), Color::WHITE)
}

#[test]
fn extraction_clears_stale_sprites_and_uses_identity_transform() {
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .resource_mut::<ExtractedSprites>()
        .push(ExtractedSprite {
            entity: Entity::PLACEHOLDER,
            source_order: 99,
            kind: ExtractedSpriteKind::Sprite,
            material: ExtractedSpriteMaterial::from_color(Color::WHITE),
            texture_region: TextureUvRect::FULL,
            world_center: Vec2::new(9.0, 9.0),
            world_x_axis: Vec2::X,
            world_y_axis: Vec2::Y,
            color: Color::WHITE,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
        });
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Sprite::from_color(Vec2::new(16.0, 8.0), Color::WHITE));

    app.run_once(Duration::ZERO).unwrap();

    let extracted = app.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted.as_slice()[0].world_center, Vec2::ZERO);
    assert_eq!(extracted.as_slice()[0].world_x_axis, Vec2::new(8.0, 0.0));
    assert_eq!(extracted.as_slice()[0].world_y_axis, Vec2::new(0.0, 4.0));
}

#[test]
fn sprite_extraction_preserves_rotation_axes() {
    let sprite = Sprite::from_color(Vec2::new(4.0, 2.0), Color::WHITE);
    let transform = Transform2d {
        rotation: std::f32::consts::FRAC_PI_2,
        ..Transform2d::default()
    };

    let extracted = extract_sprite(Entity::PLACEHOLDER, &sprite, Some(&transform), 0);

    assert!(
        extracted
            .world_x_axis
            .abs_diff_eq(Vec2::new(0.0, 2.0), 0.000_001)
    );
    assert!(
        extracted
            .world_y_axis
            .abs_diff_eq(Vec2::new(-1.0, 0.0), 0.000_001)
    );
}

#[test]
fn tilemap_cells_lower_to_world_positions() {
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    let mut tilemap = Tilemap::new(Vec2::new(10.0, 20.0)).with_layer(2);
    tilemap.set_cell(
        TileCoord::new(1, -1),
        TileCell::new(TileIndex::new(3)).with_color(Color::rgb(0.5, 0.25, 1.0)),
    );
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn((
            tilemap,
            Transform2d {
                translation: Vec2::new(5.0, 5.0),
                ..Transform2d::default()
            },
        ));

    app.run_once(Duration::ZERO).unwrap();

    let extracted = app.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(
        extracted.as_slice()[0].kind,
        ExtractedSpriteKind::TilemapCell {
            coord: TileCoord::new(1, -1),
            tile: TileIndex::new(3),
        }
    );
    assert_eq!(extracted.as_slice()[0].world_center, Vec2::new(20.0, -5.0));
    assert_eq!(extracted.as_slice()[0].world_x_axis, Vec2::new(5.0, 0.0));
    assert_eq!(extracted.as_slice()[0].world_y_axis, Vec2::new(0.0, 10.0));
    assert_eq!(extracted.as_slice()[0].layer, 2);
}

#[test]
fn queueing_records_missing_texture_assets() {
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            ..Camera2d::default()
        });
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Sprite::from_texture(
            Handle::new(AssetId::from_raw(7)),
            Vec2::new(10.0, 10.0),
        ));

    app.run_once(Duration::ZERO).unwrap();

    let queued = app.world().resource::<QueuedSpriteItems>();
    let stats = app.world().resource::<SpriteRenderStats>();
    assert!(queued.is_empty());
    assert_eq!(stats.missing_textures, 1);
    assert_eq!(stats.unprepared_textures, 0);
}

#[test]
fn queueing_uses_prepared_image_resource_keys_and_uvs() {
    let image = Handle::new(AssetId::from_raw(7));
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    insert_loaded_image(&mut app, image);
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            ..Camera2d::default()
        });
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(
            Sprite::from_texture(image, Vec2::new(10.0, 10.0))
                .with_texture_region(TextureRegion::new(Vec2::new(0.25, 0.0), Vec2::splat(0.5))),
        );

    app.run_once(Duration::ZERO).unwrap();

    let queued = app.world().resource::<QueuedSpriteItems>();
    let batches = app.world().resource::<SpriteBatches>();
    let stats = app.world().resource::<SpriteRenderStats>();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        queued.as_slice()[0].material.image,
        Some(image_resource_key(image))
    );
    assert_eq!(
        queued.as_slice()[0].instance.uv,
        TextureUvRect::new(Vec2::new(0.25, 0.0), Vec2::splat(0.5))
    );
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches.as_slice()[0].material.image,
        Some(image_resource_key(image))
    );
    assert_eq!(stats.missing_textures, 0);
    assert_eq!(stats.unprepared_textures, 0);
}

#[test]
fn sorting_preserves_source_order_for_equal_keys_and_batches_adjacent_items() {
    let mut items = vec![
        QueuedSpriteItem {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
            material: material_key(None),
            entity_bits: Entity::from_raw_u32(3).unwrap().to_bits(),
            source_order: 0,
            instance: instance(),
        },
        QueuedSpriteItem {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
            material: material_key(None),
            entity_bits: Entity::from_raw_u32(2).unwrap().to_bits(),
            source_order: 1,
            instance: instance(),
        },
        QueuedSpriteItem {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 1,
            sort_key: 0,
            material: material_key(None),
            entity_bits: Entity::from_raw_u32(1).unwrap().to_bits(),
            source_order: 2,
            instance: instance(),
        },
        QueuedSpriteItem {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 1,
            sort_key: 1,
            material: material_key(None),
            entity_bits: Entity::from_raw_u32(4).unwrap().to_bits(),
            source_order: 3,
            instance: instance(),
        },
    ];

    items.sort_by(compare_queued_sprite_items);
    let batches = build_sprite_batches(&items);

    assert_eq!(
        items
            .iter()
            .take(2)
            .map(|item| item.entity_bits)
            .collect::<Vec<_>>(),
        vec![
            Entity::from_raw_u32(3).unwrap().to_bits(),
            Entity::from_raw_u32(2).unwrap().to_bits(),
        ]
    );
    assert_eq!(
        items
            .iter()
            .map(|item| (item.layer, item.sort_key))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 0), (1, 0), (1, 1)]
    );
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].instances.len(), 2);
    assert_eq!(batches[1].instances.len(), 1);
    assert_eq!(batches[2].instances.len(), 1);
}

#[test]
fn material_image_sort_groups_matching_keys_and_splits_batches() {
    let texture_a = image_resource_key(Handle::new(AssetId::from_raw(1)));
    let texture_b = image_resource_key(Handle::new(AssetId::from_raw(2)));
    let mut items = vec![
        QueuedSpriteItem {
            material: material_key(Some(texture_a)),
            ..queued_item(0)
        },
        QueuedSpriteItem {
            material: material_key(Some(texture_b)),
            ..queued_item(1)
        },
        QueuedSpriteItem {
            material: material_key(Some(texture_a)),
            ..queued_item(2)
        },
    ];

    items.sort_by(compare_queued_sprite_items);
    let batches = build_sprite_batches(&items);

    assert_eq!(
        items
            .iter()
            .map(|item| item.source_order)
            .collect::<Vec<_>>(),
        vec![0, 2, 1]
    );
    assert_eq!(
        batches
            .iter()
            .map(|batch| batch.material.image)
            .collect::<Vec<_>>(),
        vec![Some(texture_a), Some(texture_b)]
    );
    assert_eq!(batches[0].instances.len(), 2);
    assert_eq!(batches[1].instances.len(), 1);
}

#[test]
fn same_image_with_different_samplers_splits_batches() {
    let image = Handle::new(AssetId::from_raw(7));
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    insert_loaded_image(&mut app, image);
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Camera2d {
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            ..Camera2d::default()
        });
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Sprite::from_texture(image, Vec2::new(10.0, 10.0)));
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(
            Sprite::from_texture(image, Vec2::new(10.0, 10.0))
                .with_sampler(SamplerDescriptor::NEAREST_CLAMP),
        );

    app.run_once(Duration::ZERO).unwrap();

    let batches = app.world().resource::<SpriteBatches>();
    assert_eq!(batches.len(), 2);
    assert_eq!(
        batches.as_slice()[0].material.image,
        Some(image_resource_key(image))
    );
    assert_eq!(
        batches.as_slice()[1].material.image,
        Some(image_resource_key(image))
    );
    assert_ne!(
        batches.as_slice()[0].material.sampler,
        batches.as_slice()[1].material.sampler
    );
}

#[test]
fn target_or_phase_changes_split_batches() {
    let mut items = vec![
        QueuedSpriteItem {
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TILEMAP_2D,
            ..queued_item(0)
        },
        QueuedSpriteItem {
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            ..queued_item(1)
        },
        QueuedSpriteItem {
            target: RenderTarget::Image(Handle::new(AssetId::from_raw(8))),
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            ..queued_item(2)
        },
    ];

    items.sort_by(compare_queued_sprite_items);
    let batches = build_sprite_batches(&items);

    assert_eq!(batches.len(), 3);
    assert!(
        batches
            .iter()
            .any(|batch| batch.phase == RenderPhaseLabel::TILEMAP_2D)
    );
    assert!(
        batches
            .iter()
            .any(|batch| batch.phase == RenderPhaseLabel::TRANSPARENT_2D)
    );
    assert!(
        batches
            .iter()
            .any(|batch| matches!(batch.target, RenderTarget::Image(_)))
    );
}

#[test]
fn generated_instances_fit_camera_viewport_aspect_and_position() {
    let view = ExtractedView {
        world_position: Vec2::new(10.0, 5.0),
        ..test_view()
    };
    let instance = world_to_clip_instance(
        &view,
        Vec2::new(60.0, 30.0),
        Vec2::new(10.0, 0.0),
        Vec2::new(0.0, 5.0),
        Color::WHITE,
    )
    .unwrap();

    assert_eq!(view_world_extent(&view), Some(Vec2::new(200.0, 100.0)));
    assert!(instance.center.abs_diff_eq(Vec2::new(0.5, 0.5), 0.000_001));
    assert!(
        instance
            .half_size()
            .abs_diff_eq(Vec2::new(0.1, 0.1), 0.000_001)
    );
}

#[test]
fn app_pipeline_extracts_tilemaps_into_batches() {
    let target = RenderTarget::Image(Handle::<RenderImage2d>::new(AssetId::from_raw(9)));
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(Camera2d {
            target,
            viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
            viewport_height: 100.0,
            ..Camera2d::default()
        });
    let mut tilemap = Tilemap::new(Vec2::new(10.0, 10.0));
    tilemap.set_cell(
        TileCoord::new(0, 0),
        TileCell::new(TileIndex::new(1)).with_color(Color::rgb(1.0, 0.0, 0.0)),
    );
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(tilemap);

    app.run_once(Duration::ZERO).unwrap();

    let batches = app.world().resource::<SpriteBatches>();
    assert_eq!(batches.total_instances(), 1);
    assert_eq!(batches.as_slice()[0].phase, RenderPhaseLabel::TILEMAP_2D);
    assert_eq!(batches.as_slice()[0].target, target);
    assert!(
        batches.as_slice()[0].instances[0]
            .center
            .abs_diff_eq(Vec2::new(0.1, 0.1), 0.000_001)
    );
}

#[test]
fn tilemap_extraction_applies_tileset_image_and_atlas_uvs() {
    let tileset = Handle::new(AssetId::from_raw(21));
    let image = Handle::new(AssetId::from_raw(22));
    let mut tilesets = Assets::<TileSet>::default();
    tilesets.insert(
        tileset,
        TileSet::from_image(image, TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 4, 2)),
    );

    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(tilesets);
    let mut tilemap = Tilemap::new(Vec2::new(16.0, 16.0)).with_tileset(tileset);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(5)));
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(tilemap);

    app.run_once(Duration::ZERO).unwrap();

    let extracted = app.world().resource::<ExtractedSprites>();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted.as_slice()[0].material.image, Some(image));
    assert_eq!(
        extracted.as_slice()[0].texture_region,
        TextureUvRect::new(Vec2::new(0.25, 0.5), Vec2::new(0.25, 0.5))
    );
    assert_eq!(
        app.world()
            .resource::<SpriteRenderStats>()
            .invalid_tile_regions,
        0
    );
}

#[test]
fn tilemap_extraction_skips_out_of_range_atlas_tiles() {
    let tileset = Handle::new(AssetId::from_raw(21));
    let image = Handle::new(AssetId::from_raw(22));
    let mut tilesets = Assets::<TileSet>::default();
    tilesets.insert(
        tileset,
        TileSet::from_image(image, TileAtlasLayout::grid(Vec2::new(16.0, 16.0), 4, 2)),
    );

    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(tilesets);
    let mut tilemap = Tilemap::new(Vec2::new(16.0, 16.0)).with_tileset(tileset);
    tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(8)));
    app.world_mut()
        .expect("app should allow world mutation")
        .spawn(tilemap);

    app.run_once(Duration::ZERO).unwrap();

    assert_eq!(app.world().resource::<ExtractedSprites>().len(), 0);
    let stats = app.world().resource::<SpriteRenderStats>();
    assert_eq!(stats.invalid_tile_regions, 1);
    assert_eq!(stats.extracted_tile_cells, 0);
}

#[test]
fn dirty_tile_chunks_can_clear_without_losing_authored_cells() {
    let mut tilemap = Tilemap::default();
    let coord = TileCoord::new(-1, 2);
    let cell = TileCell::new(TileIndex::new(5));

    tilemap.set_cell(coord, cell);
    tilemap.clear_dirty_chunks();

    assert_eq!(tilemap.get_cell(coord), Some(&cell));
    assert_eq!(tilemap.dirty_chunks().count(), 0);
}

fn queued_item(entity_index: u32) -> QueuedSpriteItem {
    QueuedSpriteItem {
        view_index: 0,
        view_order: 0,
        target: RenderTarget::PrimaryWindow,
        phase: RenderPhaseLabel::TRANSPARENT_2D,
        layer: 0,
        sort_key: 0,
        material: material_key(None),
        entity_bits: Entity::from_raw_u32(entity_index).unwrap().to_bits(),
        source_order: entity_index as u64,
        instance: instance(),
    }
}

fn insert_loaded_image(app: &mut App, handle: Handle<ImageAsset>) {
    let image = test_image();
    let source_hash = image.source().source_hash();
    let import_hash = image.source().artifact().key().digest();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    images
        .commit_loaded(
            handle,
            image,
            &mut states,
            &mut events,
            Some(source_hash),
            Some(import_hash),
        )
        .unwrap();
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(images);
    app.world_mut()
        .expect("app should allow world mutation")
        .insert_resource(states);
}

fn test_image() -> ImageAsset {
    let stable_id = StableAssetId::parse_str("ce2ab2f8-c58b-48e3-b94e-9465340262a1").unwrap();
    let source_hash = SourceHash::from_bytes(b"test image");
    let key = ImportArtifactKey::new(
        stable_id,
        source_hash,
        ImportDependencyDigest::empty(),
        ImporterId::new("nara-image").unwrap(),
        ImporterVersion::new(1),
        ImportSettingsHash::default(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    );
    let artifact = ImportArtifactRecord::new(key).unwrap();
    let source = ImageSourceMetadata::new(
        stable_id,
        AssetPath::new("textures/test.png").unwrap(),
        source_hash,
        artifact,
    );
    ImageAsset::new(
        source,
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![255; 16],
    )
}

fn material_key(image: Option<nara_render::RenderResourceKey>) -> SpriteMaterialKey {
    SpriteMaterialKey {
        image,
        sampler: SamplerDescriptor::default(),
        alpha_mode: AlphaMode2d::Blend,
        tint: ColorKey::from_color(Color::WHITE),
    }
}
