use nara_asset::Handle;
use nara_core::Color;
use nara_ecs::{Entity, Resource};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{RenderResourceKey, RenderTarget};
use nara_sprite_render::{ColorKey, SpriteInstance, SpriteMaterialKey, TextureUvRect};
use nara_ui::UiRect;

pub type UiInstance = SpriteInstance;
pub type UiMaterialKey = SpriteMaterialKey;
pub type UiTextureRect = TextureUvRect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractedUiItem {
    pub entity: Entity,
    pub source_order: u64,
    pub root: Entity,
    pub view_index: usize,
    pub target: RenderTarget,
    pub order: i32,
    pub z_index: i32,
    pub rect: UiRect,
    pub clip_rect: Option<UiRect>,
    pub material: ExtractedUiMaterial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractedUiMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl ExtractedUiMaterial {
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

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct ExtractedUiItems {
    items: Vec<ExtractedUiItem>,
}

impl ExtractedUiItems {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn push(&mut self, item: ExtractedUiItem) {
        self.items.push(item);
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ExtractedUiItem] {
        &self.items
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueuedUiItem {
    pub view_index: usize,
    pub view_order: i32,
    pub target: RenderTarget,
    pub order: i32,
    pub z_index: i32,
    pub material: UiMaterialKey,
    pub clip_rect: Option<UiClipRect>,
    pub entity_bits: u64,
    pub source_order: u64,
    pub instance: UiInstance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiClipRect {
    pub min_x: u32,
    pub min_y: u32,
    pub width: u32,
    pub height: u32,
}

impl UiClipRect {
    #[must_use]
    pub fn from_rect(rect: UiRect) -> Option<Self> {
        rect.is_non_empty().then_some(Self {
            min_x: rect.min.x.to_bits(),
            min_y: rect.min.y.to_bits(),
            width: rect.size.x.to_bits(),
            height: rect.size.y.to_bits(),
        })
    }

    #[must_use]
    pub fn to_rect(self) -> UiRect {
        UiRect::new(
            nara_core::Vec2::new(f32::from_bits(self.min_x), f32::from_bits(self.min_y)),
            nara_core::Vec2::new(f32::from_bits(self.width), f32::from_bits(self.height)),
        )
    }
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct QueuedUiItems {
    items: Vec<QueuedUiItem>,
}

impl QueuedUiItems {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn push(&mut self, item: QueuedUiItem) {
        self.items.push(item);
    }

    pub fn sort(&mut self) {
        self.items.sort_by(crate::compare_queued_ui_items);
    }

    #[must_use]
    pub fn as_slice(&self) -> &[QueuedUiItem] {
        &self.items
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
pub struct UiBatch {
    pub view_index: usize,
    pub view_order: i32,
    pub target: RenderTarget,
    pub order: i32,
    pub z_index: i32,
    pub material: UiMaterialKey,
    pub clip_rect: Option<UiClipRect>,
    pub instances: Vec<UiInstance>,
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct UiBatches {
    batches: Vec<UiBatch>,
}

impl UiBatches {
    pub fn clear(&mut self) {
        self.batches.clear();
    }

    pub fn replace(&mut self, batches: Vec<UiBatch>) {
        self.batches = batches;
    }

    #[must_use]
    pub fn as_slice(&self) -> &[UiBatch] {
        &self.batches
    }

    pub fn for_view(&self, view_index: usize) -> impl Iterator<Item = &UiBatch> {
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
pub struct UiRenderStats {
    pub extracted_panels: u32,
    pub missing_images: u32,
    pub unprepared_images: u32,
    pub queued_items: u32,
    pub batches: u32,
}

#[must_use]
pub fn material_key(
    material: ExtractedUiMaterial,
    image: Option<RenderResourceKey>,
) -> UiMaterialKey {
    UiMaterialKey {
        image,
        sampler: material.sampler,
        alpha_mode: material.alpha_mode,
        tint: ColorKey::from_color(material.tint),
    }
}
