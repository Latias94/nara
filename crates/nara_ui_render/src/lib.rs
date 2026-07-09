//! Backend-neutral runtime UI render extraction, queueing, and batching.

mod extract;
mod queue;
mod types;

#[cfg(test)]
mod tests;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::schedule::IntoScheduleConfigs;
use nara_render::RenderPlugin;
use nara_ui::UiPlugin;

pub use crate::extract::extract_ui;
pub use crate::queue::{
    build_ui_batches, compare_queued_ui_items, queue_ui, sort_and_batch_ui,
    ui_rect_to_clip_instance,
};
pub use crate::types::{
    ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, QueuedUiItem, QueuedUiItems, UiBatch,
    UiBatches, UiClipRect, UiInstance, UiMaterialKey, UiRenderStats, UiTextureRect, material_key,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct UiRenderPlugin;

impl Plugin for UiRenderPlugin {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(UiPlugin)?;
        app.add_plugin_if_missing(RenderPlugin)?;
        app.add_plugin_if_missing(nara_image::ImagePreparePlugin)?;
        app.init_resource::<ExtractedUiItems>();
        app.init_resource::<QueuedUiItems>();
        app.init_resource::<UiBatches>();
        app.init_resource::<UiRenderStats>();
        app.add_systems(
            CoreStage::Extract,
            extract_ui.after(nara_ui::compute_ui_layouts),
        );
        app.add_systems(CoreStage::Queue, queue_ui);
        app.add_systems(CoreStage::Sort, sort_and_batch_ui);
        Ok(())
    }
}

pub mod prelude {
    pub use crate::{
        ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, QueuedUiItem, QueuedUiItems,
        UiBatch, UiBatches, UiClipRect, UiInstance, UiMaterialKey, UiRenderPlugin, UiRenderStats,
        UiTextureRect, build_ui_batches, compare_queued_ui_items, extract_ui, material_key,
        queue_ui, sort_and_batch_ui, ui_rect_to_clip_instance,
    };
}
