//! Backend-neutral sprite and tilemap render preparation.
//!
//! Within a view, draws paint in ascending order of authored layer plus world Z, then
//! ascending sort key and descending world Y. Equal keys use hierarchy order within each
//! root and entity index across roots.

mod extract;
mod queue;
mod types;

#[cfg(test)]
mod tests;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::schedule::IntoScheduleConfigs;

use crate::sprite_render::extract::extract_sprites;
#[cfg(test)]
use crate::sprite_render::queue::{
    build_sprite_batches, compare_queued_sprite_items, world_to_clip_instance,
};
use crate::sprite_render::queue::{queue_sprites, sort_and_batch_sprites};
pub use crate::sprite_render::types::SpriteRenderStats;
#[doc(hidden)]
pub use crate::sprite_render::types::SpriteTileChunk;
pub(crate) use crate::sprite_render::types::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
    PreparedMaterial2d, QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatchKind,
    SpriteBatches, SpriteInstance, SpriteMaterialKey, TextureUvRect,
};

/// Read-only packet and geometry integration used by the first-party backend and Editor.
#[doc(hidden)]
pub mod batching {
    pub use crate::sprite_render::types::{
        ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
        PreparedMaterial2d, QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatchKind,
        SpriteBatches, SpriteInstance, SpriteMaterialKey, TextureUvRect,
    };
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteRenderPlugin;

pub const SPRITE_RENDER_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.sprite-render");
const SPRITE_RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    crate::RENDER_PLUGIN_ID,
    crate::image::IMAGE_PREPARE_PLUGIN_ID,
];
pub const SPRITE_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(SPRITE_RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(SPRITE_RENDER_PLUGIN_REQUIREMENTS);

impl Plugin for SpriteRenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &SPRITE_RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ExtractedSprites>()?;
        app.init_resource::<extract::ExtractedSpriteScratch>()?;
        app.init_resource::<QueuedSpriteItems>()?;
        app.insert_resource(SpriteBatches::new())?;
        app.init_resource::<SpriteRenderStats>()?;
        app.add_systems(
            CoreStage::Extract,
            extract_sprites.after(crate::runtime_integration::RenderExtractSet::Views),
        )?;
        app.add_systems(CoreStage::Queue, queue_sprites)?;
        app.add_systems(CoreStage::Sort, sort_and_batch_sprites)?;
        Ok(())
    }
}

pub mod prelude {
    pub use crate::sprite_render::{SpriteRenderPlugin, SpriteRenderStats};
}
