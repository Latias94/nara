//! Backend-neutral sprite and tilemap render preparation.

mod extract;
mod queue;
mod types;

#[cfg(test)]
mod tests;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::schedule::IntoScheduleConfigs;
use nara_render::RenderPlugin;

pub use crate::extract::{extract_sprite, extract_sprites, extract_tile_cell};
pub use crate::queue::{
    build_sprite_batches, compare_queued_sprite_items, phase_order, project_sprite_to_view,
    project_sprite_to_view_with_uv, queue_sprites, sort_and_batch_sprites, view_world_extent,
    world_to_clip_instance, world_to_clip_instance_with_uv,
};
pub use crate::types::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItem, QueuedSpriteItems,
    SpriteBatch, SpriteBatches, SpriteInstance, SpriteRenderStats, TextureUvRect,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteRenderPlugin;

impl Plugin for SpriteRenderPlugin {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(RenderPlugin)?;
        app.add_plugin_if_missing(nara_image::ImagePreparePlugin)?;
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
        Ok(())
    }
}

pub mod prelude {
    pub use crate::{
        ExtractedSprite, ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItem,
        QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance, SpriteRenderPlugin,
        SpriteRenderStats, TextureUvRect, build_sprite_batches, compare_queued_sprite_items,
        extract_sprites, phase_order, project_sprite_to_view, project_sprite_to_view_with_uv,
        queue_sprites, sort_and_batch_sprites, view_world_extent, world_to_clip_instance,
        world_to_clip_instance_with_uv,
    };
}
