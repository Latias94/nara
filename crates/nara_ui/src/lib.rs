//! Runtime UI authoring data, layout projection, and input state.

mod codec;
mod interaction;
mod layout;
mod style;

use nara_app::{App, CoreStage, Plugin, PluginError, PluginPreflightContext};
use nara_asset::Handle;
use nara_core::Color;
use nara_ecs::Component;
use nara_ecs::schedule::IntoScheduleConfigs;
use nara_image::ImageAsset;
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::RenderTarget;

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

pub const UI_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui");
pub const UI_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.ui.components");
pub const UI_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.ui.components");
pub const UI_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        UI_SCHEMA_OWNER_ID,
        UI_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.ui.components.native", 1),
        ui_schema_catalog,
        crate::codec::validate_ui_components,
        register_ui_components,
    );
const UI_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    nara_render::RENDER_PLUGIN_ID,
    nara_input::INPUT_PLUGIN_ID,
    nara_hierarchy::HIERARCHY_PLUGIN_ID,
];
const UI_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("runtime-ui")];
pub const UI_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_PLUGIN_ID, nara_app::PluginCategory::Runtime)
        .requires_plugins(UI_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(UI_PRODUCT_REQUIREMENTS)
        .provides_schema(&[UI_SCHEMA_PROVIDER_ID]);

fn ui_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(crate::codec::ui_schema_catalog())
}

impl Plugin for UiPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            UI_PLUGIN_ID,
            UI_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        UI_SCHEMA_PROVIDER.preflight(registry).map_err(|error| {
            PluginError::component_registration(UI_PLUGIN_ID, UI_SCHEMA_PROVIDER_ID.as_str(), error)
        })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara_reflect::register_schema_provider_for_plugin(
            app,
            UI_PLUGIN_ID,
            UI_SCHEMA_PROVIDER_ID.as_str(),
            &UI_SCHEMA_PROVIDER,
        )?;
        app.init_resource::<ComputedUiLayouts>()?;
        app.init_resource::<UiInteractionState>()?;
        app.add_systems(
            CoreStage::Extract,
            (
                compute_ui_layouts.after(nara_render::extract_views),
                update_ui_interaction.after(compute_ui_layouts),
            ),
        )?;
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
