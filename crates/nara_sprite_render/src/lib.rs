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
    queue_sprites, sort_and_batch_sprites, view_world_extent, world_to_clip_instance,
};
pub use crate::types::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItem, QueuedSpriteItems,
    SpriteBatch, SpriteBatches, SpriteInstance, SpriteRenderStats,
};

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
