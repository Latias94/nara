use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Resource};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{RenderPhaseLabel, RenderResourceKey, RenderTarget};
use nara_sprite::TextureRegion;
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
    pub material: ExtractedSpriteMaterial,
    pub texture_region: TextureUvRect,
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
        self.material.image.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractedSpriteMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl ExtractedSpriteMaterial {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureUvRect {
    pub min: Vec2,
    pub size: Vec2,
}

impl TextureUvRect {
    pub const FULL: Self = Self {
        min: Vec2::ZERO,
        size: Vec2::ONE,
    };

    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }

    #[must_use]
    pub fn from_texture_region(region: TextureRegion) -> Self {
        Self::new(region.min, region.size)
    }

    #[must_use]
    pub fn max(self) -> Vec2 {
        self.min + self.size
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.size.is_finite()
            && self.size.x > 0.0
            && self.size.y > 0.0
            && self.min.x >= 0.0
            && self.min.y >= 0.0
            && self.max().x <= 1.0
            && self.max().y <= 1.0
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

    pub(crate) fn replace(&mut self, candidate: &mut Vec<ExtractedSprite>) {
        std::mem::swap(&mut self.sprites, candidate);
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
    pub uv: TextureUvRect,
}

impl SpriteInstance {
    #[must_use]
    pub const fn axis_aligned(center: Vec2, half_size: Vec2, color: Color) -> Self {
        Self {
            center,
            x_axis: Vec2::new(half_size.x, 0.0),
            y_axis: Vec2::new(0.0, half_size.y),
            color,
            uv: TextureUvRect::FULL,
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
    pub material: SpriteMaterialKey,
    pub entity_bits: u64,
    pub source_order: u64,
    pub instance: SpriteInstance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpriteMaterialKey {
    pub image: Option<RenderResourceKey>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: ColorKey,
}

impl SpriteMaterialKey {
    #[must_use]
    pub fn from_material(
        material: ExtractedSpriteMaterial,
        image: Option<RenderResourceKey>,
    ) -> Self {
        Self {
            image,
            sampler: material.sampler,
            alpha_mode: material.alpha_mode,
            tint: ColorKey::from_color(material.tint),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColorKey {
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub a: u32,
}

impl ColorKey {
    #[must_use]
    pub const fn from_color(color: Color) -> Self {
        Self {
            r: color.r.to_bits(),
            g: color.g.to_bits(),
            b: color.b.to_bits(),
            a: color.a.to_bits(),
        }
    }
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
    pub material: SpriteMaterialKey,
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
    pub missing_tilesets: u32,
    pub invalid_tile_regions: u32,
    pub missing_textures: u32,
    pub unprepared_textures: u32,
    pub invalid_texture_regions: u32,
    pub queued_items: u32,
    pub batches: u32,
}
