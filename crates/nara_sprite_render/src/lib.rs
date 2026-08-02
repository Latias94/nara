//! Backend-neutral sprite and tilemap render preparation.

mod extract;
mod queue;
mod types;

#[cfg(test)]
mod tests;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::schedule::IntoScheduleConfigs;

use crate::extract::extract_sprites;
pub use crate::extract::{extract_sprite, extract_tile_cell};
pub use crate::queue::{
    build_sprite_batches, compare_queued_sprite_items, phase_order, project_sprite_to_view,
    project_sprite_to_view_with_uv, queue_sprites, sort_and_batch_sprites, view_world_extent,
    world_to_clip_instance, world_to_clip_instance_with_uv,
};
pub use crate::types::{
    ColorKey, ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
    QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance,
    SpriteMaterialKey, SpriteRenderStats, TextureUvRect,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteRenderPlugin;

pub const SPRITE_RENDER_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.sprite-render");
const SPRITE_RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_render::RENDER_PLUGIN_ID,
    nara_image::IMAGE_PREPARE_PLUGIN_ID,
];
const SPRITE_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("runtime-2d")];
pub const SPRITE_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(SPRITE_RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(SPRITE_RENDER_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(SPRITE_RENDER_PRODUCT_REQUIREMENTS);

impl Plugin for SpriteRenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &SPRITE_RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ExtractedSprites>()?;
        app.init_resource::<extract::ExtractedSpriteScratch>()?;
        app.init_resource::<QueuedSpriteItems>()?;
        app.init_resource::<SpriteBatches>()?;
        app.init_resource::<SpriteRenderStats>()?;
        app.add_systems(
            CoreStage::Extract,
            extract_sprites.after(nara_render::__private::RenderExtractSet::Views),
        )?;
        app.add_systems(CoreStage::Queue, queue_sprites)?;
        app.add_systems(CoreStage::Sort, sort_and_batch_sprites)?;
        Ok(())
    }
}

pub mod prelude {
    pub use crate::{
        ColorKey, ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
        QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance,
        SpriteMaterialKey, SpriteRenderPlugin, SpriteRenderStats, TextureUvRect,
        build_sprite_batches, compare_queued_sprite_items, phase_order, project_sprite_to_view,
        project_sprite_to_view_with_uv, queue_sprites, sort_and_batch_sprites, view_world_extent,
        world_to_clip_instance, world_to_clip_instance_with_uv,
    };
}
