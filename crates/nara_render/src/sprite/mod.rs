//! Code-first sprites and texture regions.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
use crate::image::ImageAsset;
use crate::material::{AlphaMode2d, SamplerDescriptor};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::Component;

#[cfg(feature = "sprite-source-hydration")]
mod hydration;
#[cfg(feature = "sprite-source-hydration")]
pub use hydration::{
    SPRITE_SOURCE_HYDRATION_PLUGIN_DECLARATION, SPRITE_SOURCE_HYDRATION_PLUGIN_ID,
    SpriteSourceHydrationPlugin, SpriteSourceHydrationSet,
};
#[cfg(feature = "sprite-source-hydration")]
mod property_animation;
#[cfg(feature = "sprite-source-hydration")]
pub use property_animation::SpritePropertyAnimationPlugin;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextureRegion {
    pub min: Vec2,
    pub size: Vec2,
}

impl TextureRegion {
    pub const FULL: Self = Self {
        min: Vec2::ZERO,
        size: Vec2::ONE,
    };

    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }

    #[must_use]
    pub fn from_pixels(min: Vec2, size: Vec2, image_size: Vec2) -> Option<Self> {
        if image_size.x <= 0.0
            || image_size.y <= 0.0
            || !image_size.is_finite()
            || !min.is_finite()
            || !size.is_finite()
            || size.x <= 0.0
            || size.y <= 0.0
        {
            return None;
        }

        let region = Self::new(min / image_size, size / image_size);
        region.is_valid_uv().then_some(region)
    }

    #[must_use]
    pub fn max(self) -> Vec2 {
        self.min + self.size
    }

    #[must_use]
    pub fn is_valid_uv(self) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpriteAnchor {
    pub normalized: Vec2,
}

impl SpriteAnchor {
    pub const CENTER: Self = Self {
        normalized: Vec2::ZERO,
    };
}

impl Default for SpriteAnchor {
    fn default() -> Self {
        Self::CENTER
    }
}

#[derive(Debug, Clone, PartialEq, Component)]
pub struct Sprite {
    pub material: SpriteMaterial,
    pub texture_region: Option<TextureRegion>,
    pub size: Vec2,
    pub anchor: SpriteAnchor,
    pub layer: i32,
    /// Explicit sort group within a layer. Y sorting never crosses this key.
    pub sort_key: i32,
    /// Local point used for descending world-Y order within the sort group.
    /// `None` participates at world Y = 0 and keeps authored order with equal anchors.
    pub y_sort_anchor: Option<Vec2>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl SpriteMaterial {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint,
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>) -> Self {
        Self {
            image: Some(image),
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint: Color::WHITE,
        }
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }
}

impl Default for SpriteMaterial {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

impl Sprite {
    #[must_use]
    pub fn from_color(size: Vec2, color: Color) -> Self {
        Self {
            material: SpriteMaterial::from_color(color),
            texture_region: None,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
            y_sort_anchor: None,
        }
    }

    #[must_use]
    pub fn from_texture(texture: Handle<ImageAsset>, size: Vec2) -> Self {
        Self {
            material: SpriteMaterial::from_image(texture),
            texture_region: None,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
            y_sort_anchor: None,
        }
    }

    #[must_use]
    pub fn with_texture_region(mut self, region: TextureRegion) -> Self {
        self.texture_region = Some(region);
        self
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.material.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.material.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.material.tint = tint;
        self
    }

    #[must_use]
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }

    #[must_use]
    pub fn with_sort_key(mut self, sort_key: i32) -> Self {
        self.sort_key = sort_key;
        self
    }

    #[must_use]
    pub fn with_y_sort_anchor(mut self, anchor: Vec2) -> Self {
        self.y_sort_anchor = Some(anchor);
        self
    }
}
