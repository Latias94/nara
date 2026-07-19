use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Resource};
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{RenderResourceKey, RenderTarget};
use nara_ui::UiRect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiTextureRect {
    pub min: Vec2,
    pub size: Vec2,
}

impl UiTextureRect {
    pub const FULL: Self = Self {
        min: Vec2::ZERO,
        size: Vec2::ONE,
    };

    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiInstance {
    pub center: Vec2,
    pub x_axis: Vec2,
    pub y_axis: Vec2,
    pub color: Color,
    pub uv: UiTextureRect,
}

impl UiInstance {
    #[must_use]
    pub const fn axis_aligned(center: Vec2, half_size: Vec2, color: Color) -> Self {
        Self {
            center,
            x_axis: Vec2::new(half_size.x, 0.0),
            y_axis: Vec2::new(0.0, half_size.y),
            color,
            uv: UiTextureRect::FULL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiMaterialKey {
    pub image: Option<RenderResourceKey>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: UiColorKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiColorKey {
    pub r: u32,
    pub g: u32,
    pub b: u32,
    pub a: u32,
}

impl UiColorKey {
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
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl UiClipRect {
    #[must_use]
    pub fn from_rect_clamped(rect: UiRect, viewport: nara_render::ViewportRect) -> Option<Self> {
        if !rect.is_non_empty() {
            return None;
        }

        let viewport_left = f64::from(viewport.physical_x);
        let viewport_top = f64::from(viewport.physical_y);
        let viewport_right = viewport_left + f64::from(viewport.physical_width);
        let viewport_bottom = viewport_top + f64::from(viewport.physical_height);
        let left = f64::from(rect.min.x).floor().max(viewport_left);
        let top = f64::from(rect.min.y).floor().max(viewport_top);
        let right = f64::from(rect.max().x).ceil().min(viewport_right);
        let bottom = f64::from(rect.max().y).ceil().min(viewport_bottom);
        if right <= left || bottom <= top {
            return None;
        }

        let x = left as u32;
        let y = top as u32;
        let right = right as u32;
        let bottom = bottom as u32;
        Some(Self {
            x,
            y,
            width: right.saturating_sub(x),
            height: bottom.saturating_sub(y),
        })
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
        tint: UiColorKey::from_color(material.tint),
    }
}
