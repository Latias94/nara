use super::*;
use std::time::Duration;

use nara_app::App;
use nara_asset::{AssetId, Handle};
use nara_core::{Color, Vec2};
use nara_ecs::Entity;
use nara_render::{
    Camera2d, ExtractedView, RenderImage2d, RenderPhaseLabel, RenderTarget, ViewportRect,
};
use nara_sprite::Sprite;
use nara_tilemap::{TileCell, TileCoord, TileIndex, Tilemap};
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
        .resource_mut::<ExtractedSprites>()
        .push(ExtractedSprite {
            entity: Entity::PLACEHOLDER,
            source_order: 99,
            kind: ExtractedSpriteKind::Sprite,
            texture: None,
            world_center: Vec2::new(9.0, 9.0),
            world_x_axis: Vec2::X,
            world_y_axis: Vec2::Y,
            color: Color::WHITE,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
        });
    app.world_mut()
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
    app.world_mut().spawn((
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
fn queueing_skips_textured_sprites_and_records_count() {
    let mut app = App::new();
    app.add_plugin(SpriteRenderPlugin).unwrap();
    app.world_mut().spawn(Camera2d {
        viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
        ..Camera2d::default()
    });
    app.world_mut().spawn(Sprite::from_texture(
        Handle::new(AssetId::from_raw(7)),
        Vec2::new(10.0, 10.0),
    ));

    app.run_once(Duration::ZERO).unwrap();

    let queued = app.world().resource::<QueuedSpriteItems>();
    let stats = app.world().resource::<SpriteRenderStats>();
    assert!(queued.is_empty());
    assert_eq!(stats.unsupported_textured_sprites, 1);
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
    app.world_mut().spawn(Camera2d {
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
    app.world_mut().spawn(tilemap);

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
        entity_bits: Entity::from_raw_u32(entity_index).unwrap().to_bits(),
        source_order: entity_index as u64,
        instance: instance(),
    }
}
