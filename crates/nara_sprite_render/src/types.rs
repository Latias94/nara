use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Resource};
use nara_render::{RenderPhaseLabel, RenderTarget};
use nara_sprite::Texture2d;
use nara_tilemap::{TileCoord, TileIndex};

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
        self.items.sort_by(crate::compare_queued_sprite_items);
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
