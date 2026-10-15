use nara_core::{Color, Mat3, Vec2};
use nara_ecs::{Component, Entity, Resource};

use crate::{RenderTarget, ViewportRect};

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Camera2d {
    pub target: RenderTarget,
    pub viewport: Option<ViewportRect>,
    /// Explicit target clear. Otherwise the first view clears with `ClearColor` and later views load.
    pub clear_color: Option<Color>,
    pub viewport_height: f32,
    pub order: i32,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            target: RenderTarget::PrimaryWindow,
            viewport: None,
            clear_color: None,
            viewport_height: 720.0,
            order: 0,
        }
    }
}

/// One validated invertible affine camera/projection mapping, shared by drawing and picking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewMapping2d {
    world_to_clip: Mat3,
    clip_to_world: Mat3,
}

impl ViewMapping2d {
    #[must_use]
    pub fn orthographic(
        world_from_camera: Mat3,
        viewport_height: f32,
        viewport: ViewportRect,
    ) -> Option<Self> {
        if !world_from_camera.is_finite()
            || !viewport_height.is_finite()
            || viewport_height <= 0.0
            || viewport.physical_width == 0
            || viewport.physical_height == 0
            || world_from_camera.determinant() == 0.0
        {
            return None;
        }
        let width =
            viewport_height * viewport.physical_width as f32 / viewport.physical_height as f32;
        let projection = Mat3::from_scale(Vec2::new(2.0 / width, 2.0 / viewport_height));
        let world_to_clip = projection * world_from_camera.inverse();
        let clip_to_world = world_to_clip.inverse();
        (world_to_clip.is_finite() && clip_to_world.is_finite()).then_some(Self {
            world_to_clip,
            clip_to_world,
        })
    }

    #[must_use]
    pub fn world_to_clip(self, point: Vec2) -> Vec2 {
        self.world_to_clip.transform_point2(point)
    }

    #[must_use]
    pub fn world_vector_to_clip(self, vector: Vec2) -> Vec2 {
        self.world_to_clip.transform_vector2(vector)
    }

    #[must_use]
    pub fn clip_to_world(self, point: Vec2) -> Vec2 {
        self.clip_to_world.transform_point2(point)
    }
}

/// Color attachment operation. A clear affects the entire target, including outside the viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewLoadOp {
    Clear(Color),
    Load,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtractedView {
    pub camera_entity: Entity,
    pub target: RenderTarget,
    pub viewport: ViewportRect,
    pub mapping: ViewMapping2d,
    pub order: i32,
    pub load: ViewLoadOp,
}

impl ExtractedView {
    #[must_use]
    pub fn world_to_viewport(&self, point: Vec2) -> Vec2 {
        viewport_clip_to_physical(self.viewport, self.mapping.world_to_clip(point))
    }

    #[must_use]
    pub fn viewport_to_world(&self, point: Vec2) -> Option<Vec2> {
        let clip = viewport_physical_to_clip(self.viewport, point)?;
        Some(self.mapping.clip_to_world(clip))
    }
}

/// Converts target physical pixels to clip space without clamping points outside the viewport.
#[must_use]
pub fn viewport_physical_to_clip(viewport: ViewportRect, point: Vec2) -> Option<Vec2> {
    if !point.is_finite() || viewport.physical_width == 0 || viewport.physical_height == 0 {
        return None;
    }
    let local = point - Vec2::new(viewport.physical_x as f32, viewport.physical_y as f32);
    Some(Vec2::new(
        local.x / viewport.physical_width as f32 * 2.0 - 1.0,
        1.0 - local.y / viewport.physical_height as f32 * 2.0,
    ))
}

#[must_use]
pub fn viewport_clip_to_physical(viewport: ViewportRect, clip: Vec2) -> Vec2 {
    Vec2::new(
        viewport.physical_x as f32 + (clip.x + 1.0) * 0.5 * viewport.physical_width as f32,
        viewport.physical_y as f32 + (1.0 - clip.y) * 0.5 * viewport.physical_height as f32,
    )
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct ExtractedViews {
    views: Vec<ExtractedView>,
}

impl ExtractedViews {
    pub fn clear(&mut self) {
        self.views.clear();
    }
    pub fn push(&mut self, view: ExtractedView) {
        self.views.push(view);
    }
    pub(crate) fn replace(&mut self, candidate: &mut Vec<ExtractedView>) {
        std::mem::swap(&mut self.views, candidate);
    }
    #[must_use]
    pub fn as_slice(&self) -> &[ExtractedView] {
        &self.views
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.views.len()
    }
}
