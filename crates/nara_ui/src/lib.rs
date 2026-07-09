//! Runtime UI authoring data, layout projection, and input state.

mod codec;
mod interaction;
mod layout;
mod style;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::Handle;
use nara_core::Color;
use nara_ecs::Component;
use nara_ecs::schedule::IntoScheduleConfigs;
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_reflect::ComponentRegistry;
use nara_render::{RenderPlugin, RenderTarget};

pub use crate::codec::register_ui_components;
pub use crate::interaction::{
    UiInteractionState, UiInteractionTarget, UiPointerRoute, top_hit, top_hit_target,
    update_ui_interaction,
};
pub use crate::layout::{
    ComputedUiLayout, ComputedUiLayouts, UiRect, compute_ui_layouts, rect_from_viewport,
    resolve_node_rect,
};
pub use crate::style::{UiStyle, UiVal};

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UiRoot {
    pub target: RenderTarget,
    pub order: i32,
}

impl UiRoot {
    #[must_use]
    pub const fn primary_window() -> Self {
        Self {
            target: RenderTarget::PrimaryWindow,
            order: 0,
        }
    }

    #[must_use]
    pub const fn with_order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }
}

impl Default for UiRoot {
    fn default() -> Self {
        Self::primary_window()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UiNode {
    pub style: UiStyle,
    pub z_index: i32,
    pub visible: bool,
    pub focusable: bool,
    pub clip: bool,
}

impl UiNode {
    #[must_use]
    pub const fn new(style: UiStyle) -> Self {
        Self {
            style,
            z_index: 0,
            visible: true,
            focusable: false,
            clip: false,
        }
    }

    #[must_use]
    pub const fn fill() -> Self {
        Self::new(UiStyle::fill())
    }

    #[must_use]
    pub const fn with_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }

    #[must_use]
    pub const fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[must_use]
    pub const fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    #[must_use]
    pub const fn clipping_children(mut self) -> Self {
        self.clip = true;
        self
    }
}

impl Default for UiNode {
    fn default() -> Self {
        Self::fill()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UiPanel {
    pub material: UiPanelMaterial,
}

impl UiPanel {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            material: UiPanelMaterial::from_color(tint),
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>) -> Self {
        Self {
            material: UiPanelMaterial::from_image(image),
        }
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
}

impl Default for UiPanel {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPanelMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl UiPanelMaterial {
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

impl Default for UiPanelMaterial {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.ui"),
            nara_app::PluginCategory::Runtime,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(RenderPlugin)?;
        app.add_plugin_if_missing(nara_input::InputPlugin)?;
        app.add_plugin_if_missing(nara_scene::HierarchyPlugin)?;
        app.init_resource::<ComponentRegistry>();
        register_ui_components(&mut app.world_mut().resource_mut::<ComponentRegistry>());
        app.init_resource::<ComputedUiLayouts>();
        app.init_resource::<UiInteractionState>();
        app.add_systems(
            CoreStage::Extract,
            (
                compute_ui_layouts.after(nara_render::extract_views),
                update_ui_interaction.after(compute_ui_layouts),
            ),
        );
        Ok(())
    }
}

pub mod prelude {
    pub use crate::{
        ComputedUiLayout, ComputedUiLayouts, UiInteractionState, UiInteractionTarget, UiNode,
        UiPanel, UiPanelMaterial, UiPlugin, UiPointerRoute, UiRect, UiRoot, UiStyle, UiVal,
        compute_ui_layouts, top_hit, top_hit_target, update_ui_interaction,
    };
}

#[cfg(test)]
mod tests;
