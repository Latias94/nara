//! Backend-neutral sprite and tilemap render preparation.

use std::cmp::Ordering;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_render::{ExtractedView, ExtractedViews, RenderPhaseLabel, RenderPlugin, RenderTarget};
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
    pub source_order: u64,
    pub kind: ExtractedSpriteKind,
    pub texture: Option<Handle<Texture2d>>,
    pub world_center: Vec2,
    pub world_x_axis: Vec2,
    pub world_y_axis: Vec2,
    pub color: Color,
    pub phase: RenderPhaseLabel,
    pub layer: i32,
    pub sort_key: i32,
}

impl ExtractedSprite {
    #[must_use]
    pub fn is_textured(self) -> bool {
        self.texture.is_some()
    }
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
    pub x_axis: Vec2,
    pub y_axis: Vec2,
    pub color: Color,
}

impl SpriteInstance {
    #[must_use]
    pub const fn axis_aligned(center: Vec2, half_size: Vec2, color: Color) -> Self {
        Self {
            center,
            x_axis: Vec2::new(half_size.x, 0.0),
            y_axis: Vec2::new(0.0, half_size.y),
            color,
        }
    }

    #[must_use]
    pub fn half_size(self) -> Vec2 {
        Vec2::new(self.x_axis.length(), self.y_axis.length())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueuedSpriteItem {
    pub view_index: usize,
    pub view_order: i32,
    pub target: RenderTarget,
    pub phase: RenderPhaseLabel,
    pub layer: i32,
    pub sort_key: i32,
    pub entity_bits: u64,
    pub source_order: u64,
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
    pub target: RenderTarget,
    pub phase: RenderPhaseLabel,
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

    pub fn for_view(&self, view_index: usize) -> impl Iterator<Item = &SpriteBatch> {
        self.batches
            .iter()
            .filter(move |batch| batch.view_index == view_index)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.batches.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    #[must_use]
    pub fn total_instances(&self) -> usize {
        self.batches.iter().map(|batch| batch.instances.len()).sum()
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
        app.add_systems(
            CoreStage::Extract,
            extract_sprites.after(nara_render::extract_views),
        );
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
        let source_order = next_source_order(&extracted);
        extracted.push(extract_sprite(entity, sprite, transform, source_order));
        stats.extracted_sprites = stats.extracted_sprites.saturating_add(1);
    }

    for (entity, tilemap, transform) in tilemaps.iter() {
        for (coord, cell) in tilemap.cells() {
            let source_order = next_source_order(&extracted);
            extracted.push(extract_tile_cell(
                entity,
                tilemap,
                transform,
                coord,
                cell.tile,
                cell.color,
                source_order,
            ));
            stats.extracted_tile_cells = stats.extracted_tile_cells.saturating_add(1);
        }
    }
}

pub fn queue_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut stats: ResMut<SpriteRenderStats>,
    views: Res<ExtractedViews>,
    extracted: Res<ExtractedSprites>,
) {
    queued.clear();
    stats.unsupported_textured_sprites = saturating_u32(
        extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.is_textured())
            .count(),
    );

    for (view_index, view) in views.as_slice().iter().enumerate() {
        for sprite in extracted
            .as_slice()
            .iter()
            .filter(|sprite| !sprite.is_textured())
        {
            let Some(instance) = project_sprite_to_view(view, sprite) else {
                continue;
            };

            queued.push(QueuedSpriteItem {
                view_index,
                view_order: view.order,
                target: view.target,
                phase: sprite.phase,
                layer: sprite.layer,
                sort_key: sprite.sort_key,
                entity_bits: sprite.entity.to_bits(),
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
pub fn extract_sprite(
    entity: Entity,
    sprite: &Sprite,
    transform: Option<&Transform2d>,
    source_order: u64,
) -> ExtractedSprite {
    let transform = transform.copied().unwrap_or_default();
    let matrix = transform.matrix();
    let local_center = -sprite.anchor.normalized * sprite.size;
    let local_x_axis = Vec2::new(sprite.size.x * 0.5, 0.0);
    let local_y_axis = Vec2::new(0.0, sprite.size.y * 0.5);

    ExtractedSprite {
        entity,
        source_order,
        kind: ExtractedSpriteKind::Sprite,
        texture: sprite.texture,
        world_center: matrix.transform_point2(local_center),
        world_x_axis: matrix.transform_vector2(local_x_axis),
        world_y_axis: matrix.transform_vector2(local_y_axis),
        color: sprite.color,
        phase: RenderPhaseLabel::TRANSPARENT_2D,
        layer: sprite.layer,
        sort_key: sprite.sort_key,
    }
}

#[must_use]
pub fn extract_tile_cell(
    entity: Entity,
    tilemap: &Tilemap,
    transform: Option<&Transform2d>,
    coord: TileCoord,
    tile: TileIndex,
    color: Color,
    source_order: u64,
) -> ExtractedSprite {
    let transform = transform.copied().unwrap_or_default();
    let matrix = transform.matrix();
    let tile_size = tilemap.tile_size;
    let local_center = Vec2::new(
        (coord.x as f32 + 0.5) * tile_size.x,
        (coord.y as f32 + 0.5) * tile_size.y,
    );
    let local_x_axis = Vec2::new(tile_size.x * 0.5, 0.0);
    let local_y_axis = Vec2::new(0.0, tile_size.y * 0.5);

    ExtractedSprite {
        entity,
        source_order,
        kind: ExtractedSpriteKind::TilemapCell { coord, tile },
        texture: None,
        world_center: matrix.transform_point2(local_center),
        world_x_axis: matrix.transform_vector2(local_x_axis),
        world_y_axis: matrix.transform_vector2(local_y_axis),
        color,
        phase: RenderPhaseLabel::TILEMAP_2D,
        layer: tilemap.layer.index,
        sort_key: tilemap.sort_key,
    }
}

#[must_use]
pub fn project_sprite_to_view(
    view: &ExtractedView,
    sprite: &ExtractedSprite,
) -> Option<SpriteInstance> {
    world_to_clip_instance(
        view,
        sprite.world_center,
        sprite.world_x_axis,
        sprite.world_y_axis,
        sprite.color,
    )
}

#[must_use]
pub fn world_to_clip_instance(
    view: &ExtractedView,
    world_center: Vec2,
    world_x_axis: Vec2,
    world_y_axis: Vec2,
    color: Color,
) -> Option<SpriteInstance> {
    let world_extent = view_world_extent(view)?;
    let clip_scale = Vec2::new(2.0 / world_extent.x, 2.0 / world_extent.y);

    Some(SpriteInstance {
        center: (world_center - view.world_position) * clip_scale,
        x_axis: world_x_axis * clip_scale,
        y_axis: world_y_axis * clip_scale,
        color,
    })
}

#[must_use]
pub fn view_world_extent(view: &ExtractedView) -> Option<Vec2> {
    if view.viewport.physical_height == 0
        || view.viewport_height <= 0.0
        || !view.viewport_height.is_finite()
    {
        return None;
    }

    let aspect = view.viewport.physical_width as f32 / view.viewport.physical_height as f32;
    let width = view.viewport_height * aspect;
    if width <= 0.0 || !width.is_finite() {
        return None;
    }

    Some(Vec2::new(width, view.viewport_height))
}

#[must_use]
pub fn build_sprite_batches(items: &[QueuedSpriteItem]) -> Vec<SpriteBatch> {
    let mut batches = Vec::<SpriteBatch>::new();

    for item in items {
        if let Some(batch) = batches.last_mut()
            && batch.view_index == item.view_index
            && batch.target == item.target
            && batch.phase == item.phase
            && batch.layer == item.layer
            && batch.sort_key == item.sort_key
        {
            batch.instances.push(item.instance);
            continue;
        }

        batches.push(SpriteBatch {
            view_index: item.view_index,
            view_order: item.view_order,
            target: item.target,
            phase: item.phase,
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
        phase_order(left.phase),
        left.layer,
        left.sort_key,
        left.entity_bits,
        left.source_order,
    )
        .cmp(&(
            right.view_order,
            right.view_index,
            phase_order(right.phase),
            right.layer,
            right.sort_key,
            right.entity_bits,
            right.source_order,
        ))
}

#[must_use]
pub fn phase_order(phase: RenderPhaseLabel) -> u8 {
    if phase == RenderPhaseLabel::OPAQUE_2D {
        0
    } else if phase == RenderPhaseLabel::TILEMAP_2D {
        1
    } else if phase == RenderPhaseLabel::TRANSPARENT_2D {
        2
    } else if phase == RenderPhaseLabel::GIZMO {
        3
    } else if phase == RenderPhaseLabel::UI {
        4
    } else {
        u8::MAX
    }
}

fn next_source_order(extracted: &ExtractedSprites) -> u64 {
    extracted.len() as u64
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
        phase_order, project_sprite_to_view, queue_sprites, sort_and_batch_sprites,
        view_world_extent, world_to_clip_instance,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use nara_app::App;
    use nara_asset::AssetId;
    use nara_render::{Camera2d, RenderImage2d, ViewportRect};
    use nara_tilemap::{TileCell, TileCoord, TileIndex};

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
    fn sorting_is_stable_and_batches_adjacent_compatible_items() {
        let mut items = vec![
            QueuedSpriteItem {
                view_index: 0,
                view_order: 0,
                target: RenderTarget::PrimaryWindow,
                phase: RenderPhaseLabel::TRANSPARENT_2D,
                layer: 1,
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
                entity_bits: Entity::from_raw_u32(1).unwrap().to_bits(),
                source_order: 0,
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
                source_order: 0,
                instance: instance(),
            },
        ];

        items.sort_by(compare_queued_sprite_items);
        let batches = build_sprite_batches(&items);

        let mut expected_equal_key_bits = [
            Entity::from_raw_u32(1).unwrap().to_bits(),
            Entity::from_raw_u32(2).unwrap().to_bits(),
        ];
        expected_equal_key_bits.sort();

        assert_eq!(
            items
                .iter()
                .take(2)
                .map(|item| item.entity_bits)
                .collect::<Vec<_>>(),
            expected_equal_key_bits
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
            source_order: 0,
            instance: instance(),
        }
    }
}
