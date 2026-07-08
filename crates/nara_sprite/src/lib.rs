//! Sprite authoring data for 2D scenes.

use nara_app::{App, Plugin};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Texture2d {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextureRegion {
    pub min: Vec2,
    pub size: Vec2,
}

impl TextureRegion {
    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
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
    pub texture: Option<Handle<Texture2d>>,
    pub texture_region: Option<TextureRegion>,
    pub color: Color,
    pub size: Vec2,
    pub anchor: SpriteAnchor,
    pub layer: i32,
    pub sort_key: i32,
}

impl Sprite {
    #[must_use]
    pub fn from_color(size: Vec2, color: Color) -> Self {
        Self {
            texture: None,
            texture_region: None,
            color,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
        }
    }

    #[must_use]
    pub fn from_texture(texture: Handle<Texture2d>, size: Vec2) -> Self {
        Self {
            texture: Some(texture),
            texture_region: None,
            color: Color::WHITE,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
        }
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
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, _app: &mut App) {}
}

pub mod prelude {
    pub use crate::{Sprite, SpriteAnchor, SpritePlugin, Texture2d, TextureRegion};
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetId;

    #[test]
    fn creates_color_sprite_with_default_authoring_state() {
        let sprite = Sprite::from_color(Vec2::new(16.0, 16.0), Color::WHITE);

        assert_eq!(sprite.texture, None);
        assert_eq!(sprite.texture_region, None);
        assert_eq!(sprite.size, Vec2::new(16.0, 16.0));
        assert_eq!(sprite.anchor, SpriteAnchor::CENTER);
        assert_eq!(sprite.layer, 0);
        assert_eq!(sprite.sort_key, 0);
    }

    #[test]
    fn creates_texture_sprite_without_backend_handles() {
        let texture = Handle::new(AssetId::from_raw(7));
        let sprite = Sprite::from_texture(texture, Vec2::new(32.0, 32.0));

        assert_eq!(sprite.texture, Some(texture));
        assert_eq!(sprite.color, Color::WHITE);
    }

    #[test]
    fn records_layer_and_sort_key() {
        let sprite = Sprite::from_color(Vec2::new(8.0, 8.0), Color::WHITE)
            .with_layer(3)
            .with_sort_key(-4);

        assert_eq!(sprite.layer, 3);
        assert_eq!(sprite.sort_key, -4);
    }
}
