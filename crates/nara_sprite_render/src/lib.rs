//! Backend-neutral sprite and tilemap render preparation.

use std::cmp::Ordering;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Query, Res, ResMut, Resource};
use nara_render::{ExtractedView, ExtractedViews, RenderPlugin};
use nara_sprite::{Sprite, Texture2d};
use nara_tilemap::{TileCoord, TileIndex, Tilemap};
use nara_transform::Transform2d;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractedSpriteKind {
    Sprite,
    TilemapCell { coord: TileCoord, tile: TileIndex },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractedSprite {
    pub entity: Entity,
    pub source_order: u32,
    pub kind: ExtractedSpriteKind,
    pub texture: Option<Handle<Texture2d>>,
    pub world_center: Vec2,
    pub world_size: Vec2,
    pub color: Color,
    pub layer: i32,
    pub sort_key: i32,
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct ExtractedSprites {
    sprites: Vec<ExtractedSprite>,
}

impl ExtractedSprites {
    pub fn clear(&mut self) {
        self.sprites.clear();
    }

    pub fn push(&mut self, sprite: ExtractedSprite) {
        self.sprites.push(sprite);
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ExtractedSprite] {
        &self.sprites
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteInstance {
    pub center: Vec2,
    pub half_size: Vec2,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueuedSpriteItem {
    pub view_index: usize,
    pub view_order: i32,
    pub layer: i32,
    pub sort_key: i32,
    pub entity_index: u32,
    pub source_order: u32,
    pub instance: SpriteInstance,
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct QueuedSpriteItems {
    items: Vec<QueuedSpriteItem>,
}

impl QueuedSpriteItems {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn push(&mut self, item: QueuedSpriteItem) {
        self.items.push(item);
    }

    pub fn sort(&mut self) {
        self.items.sort_by(compare_queued_sprite_items);
    }

    #[must_use]
    pub fn as_slice(&self) -> &[QueuedSpriteItem] {
        &self.items
    }

    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [QueuedSpriteItem] {
        &mut self.items
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteBatch {
    pub view_index: usize,
    pub view_order: i32,
    pub layer: i32,
    pub sort_key: i32,
    pub instances: Vec<SpriteInstance>,
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct SpriteBatches {
    batches: Vec<SpriteBatch>,
}

impl SpriteBatches {
    pub fn clear(&mut self) {
        self.batches.clear();
    }

    pub fn replace(&mut self, batches: Vec<SpriteBatch>) {
        self.batches = batches;
    }

    #[must_use]
    pub fn as_slice(&self) -> &[SpriteBatch] {
        &self.batches
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.batches.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct SpriteRenderStats {
    pub extracted_sprites: u32,
    pub extracted_tile_cells: u32,
    pub unsupported_textured_sprites: u32,
    pub queued_items: u32,
    pub batches: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteRenderPlugin;

impl Plugin for SpriteRenderPlugin {
    fn build(&self, app: &mut App) {
        add_plugin_or_ignore_duplicate(app, RenderPlugin);
        app.init_resource::<ExtractedSprites>();
        app.init_resource::<QueuedSpriteItems>();
        app.init_resource::<SpriteBatches>();
        app.init_resource::<SpriteRenderStats>();
        app.add_systems(CoreStage::Extract, extract_sprites);
        app.add_systems(CoreStage::Queue, queue_sprites);
        app.add_systems(CoreStage::Sort, sort_and_batch_sprites);
    }
}

pub fn extract_sprites(
    mut extracted: ResMut<ExtractedSprites>,
    mut stats: ResMut<SpriteRenderStats>,
    sprites: Query<(Entity, &Sprite, Option<&Transform2d>)>,
    tilemaps: Query<(Entity, &Tilemap, Option<&Transform2d>)>,
) {
    extracted.clear();
    *stats = SpriteRenderStats::default();

    for (entity, sprite, transform) in sprites.iter() {
        let transform = transform.copied().unwrap_or_default();
        let world_size = sprite.size * transform.scale.abs();
        let world_center = transform.translation - sprite.anchor.normalized * world_size;
        let source_order = next_source_order(&extracted);

        extracted.push(ExtractedSprite {
            entity,
            source_order,
            kind: ExtractedSpriteKind::Sprite,
            texture: sprite.texture,
            world_center,
            world_size,
            color: sprite.color,
            layer: sprite.layer,
            sort_key: sprite.sort_key,
        });
        stats.extracted_sprites = stats.extracted_sprites.saturating_add(1);
    }

    for (entity, tilemap, transform) in tilemaps.iter() {
        let transform = transform.copied().unwrap_or_default();
        let world_size = tilemap.tile_size * transform.scale.abs();

        for (coord, cell) in tilemap.cells() {
            let local_center = Vec2::new(
                (coord.x as f32 + 0.5) * tilemap.tile_size.x,
                (coord.y as f32 + 0.5) * tilemap.tile_size.y,
            );
            let world_center = transform.translation + local_center * transform.scale;
            let source_order = next_source_order(&extracted);

            extracted.push(ExtractedSprite {
                entity,
                source_order,
                kind: ExtractedSpriteKind::TilemapCell {
                    coord,
                    tile: cell.tile,
                },
                texture: None,
                world_center,
                world_size,
                color: cell.color,
                layer: tilemap.layer.index,
                sort_key: tilemap.sort_key,
            });
            stats.extracted_tile_cells = stats.extracted_tile_cells.saturating_add(1);
        }
    }
}

pub fn queue_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut stats: ResMut<SpriteRenderStats>,
    views: Res<ExtractedViews>,
    extracted: Res<ExtractedSprites>,
    camera_transforms: Query<&Transform2d>,
) {
    queued.clear();
    stats.unsupported_textured_sprites = saturating_u32(
        extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.texture.is_some())
            .count(),
    );

    for (view_index, view) in views.as_slice().iter().enumerate() {
        let camera_transform = camera_transforms
            .get(view.camera_entity)
            .copied()
            .unwrap_or_default();

        for sprite in extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.texture.is_none())
        {
            let Some(instance) = world_to_clip_instance(
                view,
                camera_transform,
                sprite.world_center,
                sprite.world_size,
                sprite.color,
            ) else {
                continue;
            };

            queued.push(QueuedSpriteItem {
                view_index,
                view_order: view.order,
                layer: sprite.layer,
                sort_key: sprite.sort_key,
                entity_index: sprite.entity.index_u32(),
                source_order: sprite.source_order,
                instance,
            });
        }
    }

    stats.queued_items = saturating_u32(queued.len());
}

pub fn sort_and_batch_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut batches: ResMut<SpriteBatches>,
    mut stats: ResMut<SpriteRenderStats>,
) {
    queued.sort();
    let built_batches = build_sprite_batches(queued.as_slice());
    stats.batches = saturating_u32(built_batches.len());
    batches.replace(built_batches);
}

#[must_use]
pub fn world_to_clip_instance(
    view: &ExtractedView,
    camera_transform: Transform2d,
    world_center: Vec2,
    world_size: Vec2,
    color: Color,
) -> Option<SpriteInstance> {
    if view.viewport_height <= 0.0 || !view.viewport_height.is_finite() {
        return None;
    }

    let viewport_width = view.viewport_height * viewport_aspect_ratio(view)?;
    if viewport_width <= 0.0 || !viewport_width.is_finite() {
        return None;
    }

    let relative_center = world_center - camera_transform.translation;
    Some(SpriteInstance {
        center: Vec2::new(
            relative_center.x / (viewport_width * 0.5),
            relative_center.y / (view.viewport_height * 0.5),
        ),
        half_size: Vec2::new(
            world_size.x.abs() / viewport_width,
            world_size.y.abs() / view.viewport_height,
        ),
        color,
    })
}

#[must_use]
pub fn build_sprite_batches(items: &[QueuedSpriteItem]) -> Vec<SpriteBatch> {
    let mut batches = Vec::<SpriteBatch>::new();

    for item in items {
        if let Some(batch) = batches.last_mut()
            && batch.view_index == item.view_index
            && batch.layer == item.layer
            && batch.sort_key == item.sort_key
        {
            batch.instances.push(item.instance);
            continue;
        }

        batches.push(SpriteBatch {
            view_index: item.view_index,
            view_order: item.view_order,
            layer: item.layer,
            sort_key: item.sort_key,
            instances: vec![item.instance],
        });
    }

    batches
}

pub fn compare_queued_sprite_items(left: &QueuedSpriteItem, right: &QueuedSpriteItem) -> Ordering {
    (
        left.view_order,
        left.view_index,
        left.layer,
        left.sort_key,
        left.entity_index,
        left.source_order,
    )
        .cmp(&(
            right.view_order,
            right.view_index,
            right.layer,
            right.sort_key,
            right.entity_index,
            right.source_order,
        ))
}

fn viewport_aspect_ratio(view: &ExtractedView) -> Option<f32> {
    if view.viewport.physical_height == 0 {
        return None;
    }

    Some(view.viewport.physical_width as f32 / view.viewport.physical_height as f32)
}

fn next_source_order(extracted: &ExtractedSprites) -> u32 {
    saturating_u32(extracted.len())
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn add_plugin_or_ignore_duplicate(app: &mut App, plugin: impl Plugin) {
    match app.add_plugin(plugin) {
        Ok(_) | Err(PluginError::Duplicate { .. }) => {}
        Err(error) => panic!("failed to install sprite render prerequisite plugin: {error}"),
    }
}

pub mod prelude {
    pub use crate::{
        ExtractedSprite, ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItem,
        QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance, SpriteRenderPlugin,
        SpriteRenderStats, build_sprite_batches, compare_queued_sprite_items, extract_sprites,
        queue_sprites, sort_and_batch_sprites, world_to_clip_instance,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use nara_app::App;
    use nara_asset::AssetId;
    use nara_render::{Camera2d, RenderTarget, ViewportRect};
    use nara_tilemap::{TileCell, TileCoord, TileIndex};

    fn test_view() -> ExtractedView {
        ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 200, 100).unwrap(),
            viewport_height: 100.0,
            order: 0,
            clear_color: Color::BLACK,
        }
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
                world_size: Vec2::new(1.0, 1.0),
                color: Color::WHITE,
                layer: 0,
                sort_key: 0,
            });
        app.world_mut()
            .spawn(Sprite::from_color(Vec2::new(16.0, 8.0), Color::WHITE));

        app.run_once(Duration::ZERO).unwrap();

        let extracted = app.world().resource::<ExtractedSprites>();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted.as_slice()[0].world_center, Vec2::ZERO);
        assert_eq!(extracted.as_slice()[0].world_size, Vec2::new(16.0, 8.0));
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
        assert_eq!(extracted.as_slice()[0].world_size, Vec2::new(10.0, 20.0));
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
    fn sorting_is_stable_and_batches_adjacent_compatible_items() {
        let instance = SpriteInstance {
            center: Vec2::ZERO,
            half_size: Vec2::splat(0.5),
            color: Color::WHITE,
        };
        let mut items = vec![
            QueuedSpriteItem {
                view_index: 0,
                view_order: 0,
                layer: 1,
                sort_key: 0,
                entity_index: 3,
                source_order: 0,
                instance,
            },
            QueuedSpriteItem {
                view_index: 0,
                view_order: 0,
                layer: 0,
                sort_key: 0,
                entity_index: 2,
                source_order: 0,
                instance,
            },
            QueuedSpriteItem {
                view_index: 0,
                view_order: 0,
                layer: 0,
                sort_key: 0,
                entity_index: 1,
                source_order: 0,
                instance,
            },
            QueuedSpriteItem {
                view_index: 0,
                view_order: 0,
                layer: 1,
                sort_key: 1,
                entity_index: 4,
                source_order: 0,
                instance,
            },
        ];

        items.sort_by(compare_queued_sprite_items);
        let batches = build_sprite_batches(&items);

        assert_eq!(
            items
                .iter()
                .map(|item| (item.layer, item.sort_key, item.entity_index))
                .collect::<Vec<_>>(),
            vec![(0, 0, 1), (0, 0, 2), (1, 0, 3), (1, 1, 4)]
        );
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].instances.len(), 2);
        assert_eq!(batches[1].instances.len(), 1);
        assert_eq!(batches[2].instances.len(), 1);
    }

    #[test]
    fn generated_instances_fit_camera_viewport_aspect() {
        let instance = world_to_clip_instance(
            &test_view(),
            Transform2d::default(),
            Vec2::new(50.0, 25.0),
            Vec2::new(20.0, 10.0),
            Color::WHITE,
        )
        .unwrap();

        assert_eq!(instance.center, Vec2::new(0.5, 0.5));
        assert_eq!(instance.half_size, Vec2::new(0.1, 0.1));
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
}
