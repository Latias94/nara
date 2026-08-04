//! Backend-neutral runtime UI render extraction, queueing, and batching.

// Bevy's derive resolves this path; keep the actual dependency on Nara's ECS boundary.
extern crate nara_ecs as bevy_ecs;

mod extract;
mod queue;
mod types;

#[cfg(test)]
mod tests;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::schedule::IntoScheduleConfigs;

pub(crate) use crate::extract::extract_ui;
#[cfg(test)]
pub(crate) use crate::queue::ui_rect_to_clip_instance;
pub(crate) use crate::queue::{compare_queued_ui_items, queue_ui, sort_and_batch_ui};
#[cfg(test)]
pub(crate) use crate::types::UiColorKey;
pub(crate) use crate::types::{
    ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, QueuedUiItem, QueuedUiItems, UiBatch,
    UiBatches, UiClipRect, UiInstance, UiMaterialKey, UiRenderStats, UiTextureRect, material_key,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::types::{UiBatch, UiBatchReader, UiBatchSnapshot};
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UiRenderPlugin;

pub const UI_RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui-render");
const UI_RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_ui::UI_PLUGIN_ID,
    nara_render::RENDER_PLUGIN_ID,
    nara_image::IMAGE_PREPARE_PLUGIN_ID,
];
const UI_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("runtime-ui")];
pub const UI_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_RENDER_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_plugins(UI_RENDER_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(UI_RENDER_PRODUCT_REQUIREMENTS);

impl Plugin for UiRenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ExtractedUiItems>()?;
        app.init_resource::<QueuedUiItems>()?;
        app.insert_resource(UiBatches::new())?;
        app.init_resource::<UiRenderStats>()?;
        app.add_systems(
            CoreStage::Extract,
            extract_ui.after(nara_ui::compute_ui_layouts),
        )?;
        app.add_systems(CoreStage::Queue, queue_ui)?;
        app.add_systems(CoreStage::Sort, sort_and_batch_ui)?;
        Ok(())
    }
}

pub mod prelude {
    pub use crate::UiRenderPlugin;
}
