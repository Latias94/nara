//! Renderer-facing data and backend seam.

use nara_app::{App, CoreStage, Plugin};
use nara_asset::Handle;
pub use nara_core::Color;
use nara_core::Vec2;
use nara_ecs::{Component, Resource, World};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Extent2d {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClearColor(pub Color);

impl Default for ClearColor {
    fn default() -> Self {
        Self(Color::BLACK)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Texture2d {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Component)]
pub struct Sprite {
    pub texture: Option<Handle<Texture2d>>,
    pub color: Color,
    pub size: Vec2,
    pub sort_key: i32,
}

impl Sprite {
    #[must_use]
    pub fn from_color(size: Vec2, color: Color) -> Self {
        Self {
            texture: None,
            color,
            size,
            sort_key: 0,
        }
    }

    #[must_use]
    pub fn from_texture(texture: Handle<Texture2d>, size: Vec2) -> Self {
        Self {
            texture: Some(texture),
            color: Color::WHITE,
            size,
            sort_key: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Camera2d {
    pub clear_color: Option<Color>,
    pub viewport_height: f32,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            clear_color: None,
            viewport_height: 720.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Resource)]
pub struct FrameStats {
    pub draw_calls: u32,
    pub sprites: u32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RenderError {
    #[error("render backend unavailable")]
    BackendUnavailable,
    #[error("surface unavailable")]
    SurfaceUnavailable,
}

pub trait RenderBackend {
    fn resize(&mut self, size: Extent2d);

    fn render(&mut self, world: &World) -> Result<FrameStats, RenderError>;
}

#[derive(Debug, Default)]
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor::default());
        app.add_systems(CoreStage::Extract, || {});
        app.add_systems(CoreStage::Render, || {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_color_sprite() {
        let sprite = Sprite::from_color(Vec2::new(16.0, 16.0), Color::WHITE);

        assert_eq!(sprite.texture, None);
        assert_eq!(sprite.size, Vec2::new(16.0, 16.0));
    }
}
