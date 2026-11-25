use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginError, PluginGroup,
    PluginGroupBuilder, PluginGroupId, PluginId, PluginSlot, PluginSlotId,
};

mod defaults;
#[cfg(feature = "project-runtime")]
mod profile;
#[cfg(feature = "project-runtime")]
mod recipe;
#[cfg(feature = "project-runtime")]
mod runtime_time;
#[cfg(feature = "runtime-ui")]
mod ui_surface;

pub use defaults::DefaultPlugins;
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub(crate) use defaults::run_desktop_app;
#[cfg(feature = "project-runtime")]
pub(crate) use profile::ProjectProfilePlugins;
#[cfg(all(feature = "project-runtime", feature = "runtime-window"))]
pub use profile::window_from_settings;
#[cfg(feature = "project-runtime")]
pub(crate) use recipe::data_contributor_mismatch;
#[cfg(feature = "project-runtime")]
pub use recipe::{
    ProductConfiguration, ProductRecipe, ProductRecipeEntry, ProductRecipeEntryKind,
    ProductRecipeError, SchemaContribution,
};
#[cfg(feature = "runtime-ui")]
pub(crate) use ui_surface::PROJECT_PRIMARY_UI_SURFACE_ID;

const SERVER_TIME_POLICY_PLUGIN_ID: PluginId = PluginId::new("nara.server-time-policy");
const SERVER_TIME_POLICY_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SERVER_TIME_POLICY_PLUGIN_ID, PluginCategory::Core);

const SLOT_COMPONENT_REGISTRY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.component-registry");
const SLOT_HIERARCHY: PluginSlotId = PluginSlotId::new("nara.plugins.slot.hierarchy");
const SLOT_SCENE_COMPONENTS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.scene-components");
const SLOT_DIAGNOSTICS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.diagnostics");
const SLOT_TASKS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tasks");
const SLOT_ASSET: PluginSlotId = PluginSlotId::new("nara.plugins.slot.asset");
const SLOT_TRANSFORM: PluginSlotId = PluginSlotId::new("nara.plugins.slot.transform");
const SLOT_SPATIAL: PluginSlotId = PluginSlotId::new("nara.plugins.slot.spatial");
#[cfg(feature = "runtime-input")]
const SLOT_INPUT: PluginSlotId = PluginSlotId::new("nara.plugins.slot.input");
const SLOT_SERVER_TIME_POLICY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.server-time-policy");
#[cfg(feature = "runtime-2d")]
const SLOT_SPRITE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite");
#[cfg(feature = "runtime-2d")]
const SLOT_TILEMAP: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tilemap");
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
const SLOT_TILESET_ASSETS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tileset-assets");
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
const SLOT_MATERIAL: PluginSlotId = PluginSlotId::new("nara.plugins.slot.material2d");
#[cfg(any(
    feature = "runtime-2d",
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "render-wgpu"
))]
const SLOT_RENDER_COMPONENTS: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-components");
#[cfg(any(
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "render-wgpu"
))]
const SLOT_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render");
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
const SLOT_IMAGE_PREPARE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image-prepare");
#[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
const SLOT_IMAGE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image");
#[cfg(feature = "presentation-2d")]
const SLOT_SPRITE_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite-render");
#[cfg(feature = "runtime-ui")]
const SLOT_UI: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui");
#[cfg(feature = "presentation-ui")]
const SLOT_UI_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui-render");
#[cfg(feature = "presentation-ui")]
const SLOT_UI_PRESENTATION_BRIDGE: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.ui-presentation-bridge");
#[cfg(feature = "desktop-winit")]
const SLOT_WINDOW: PluginSlotId = PluginSlotId::new("nara.plugins.slot.window");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_CAPTURE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render-wgpu.capture");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_WINDOW_PRESENTATION: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.window-presentation");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_STOCK_POLICY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.stock-policy");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_STOCK_CAPTURE: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.stock-capture");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_SELECTED_POLICY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.selected-policy");

/// Minimal runtime defaults for headless examples and code-first games.
#[derive(Debug, Default, Clone, Copy)]
pub struct MinimalPlugins;

impl PluginGroup for MinimalPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.minimal");

    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_COMPONENT_REGISTRY,
                    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_reflect::ComponentRegistryPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_HIERARCHY, nara_scene::hierarchy::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::hierarchy::HierarchyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SCENE_COMPONENTS,
                    nara_scene::SCENE_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_scene::SceneComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_DIAGNOSTICS,
                    nara_app::diagnostics::DIAGNOSTICS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_app::diagnostics::DiagnosticsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TASKS, nara_app::tasks::TASK_PLUGIN_ID),
                PluginDefinition::for_default::<nara_app::tasks::TaskPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_ASSET, nara_asset::ASSET_PLUGIN_ID),
                PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TRANSFORM, nara_scene::TRANSFORM_COMPONENTS_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::TransformComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_SPATIAL, nara_scene::SPATIAL_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::SpatialPlugin>(),
            );
        #[cfg(feature = "runtime-input")]
        let builder = builder.add_slot(
            PluginSlot::required(SLOT_INPUT, nara_input::INPUT_PLUGIN_ID),
            PluginDefinition::for_default::<nara_input::InputPlugin>(),
        );
        builder
    }
}

/// Local headless defaults with the selected input adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeadlessRuntimePlugins;

impl PluginGroup for HeadlessRuntimePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.headless-runtime");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_group(MinimalPlugins)
    }
}

/// Dedicated-server defaults without raw input, window, render, or tooling installation.
#[derive(Debug, Default, Clone, Copy)]
pub struct ServerPlugins;

#[derive(Debug, Default, Clone, Copy)]
struct ServerTimePolicyPlugin;

impl Plugin for ServerTimePolicyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SERVER_TIME_POLICY_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let Some(mut fixed_time) = app.world_mut()?.get_resource_mut::<nara_app::FixedTime>()
        else {
            return Err(PluginError::SetupFailed {
                plugin: SERVER_TIME_POLICY_PLUGIN_ID,
                message: "server time policy requires FixedTime".to_owned(),
            });
        };
        fixed_time
            .set_catch_up_policy(nara_app::FixedCatchUpPolicy::PreserveDebt)
            .map_err(|error| PluginError::SetupFailed {
                plugin: SERVER_TIME_POLICY_PLUGIN_ID,
                message: error.to_string(),
            })?;
        Ok(())
    }
}

impl PluginGroup for ServerPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.server");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(SLOT_SERVER_TIME_POLICY, SERVER_TIME_POLICY_PLUGIN_ID),
                PluginDefinition::for_default::<ServerTimePolicyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_COMPONENT_REGISTRY,
                    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_reflect::ComponentRegistryPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_HIERARCHY, nara_scene::hierarchy::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::hierarchy::HierarchyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SCENE_COMPONENTS,
                    nara_scene::SCENE_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_scene::SceneComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_DIAGNOSTICS,
                    nara_app::diagnostics::DIAGNOSTICS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_app::diagnostics::DiagnosticsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TASKS, nara_app::tasks::TASK_PLUGIN_ID),
                PluginDefinition::for_default::<nara_app::tasks::TaskPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_ASSET, nara_asset::ASSET_PLUGIN_ID),
                PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TRANSFORM, nara_scene::TRANSFORM_COMPONENTS_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::TransformComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_SPATIAL, nara_scene::SPATIAL_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::SpatialPlugin>(),
            )
    }
}

#[cfg(feature = "runtime-2d")]
#[derive(Debug, Default, Clone, Copy)]
pub struct Runtime2dPlugins;

#[cfg(feature = "runtime-2d")]
impl PluginGroup for Runtime2dPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.runtime-2d");

    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_SPRITE, nara_render::sprite::SPRITE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::sprite::SpritePlugin>(),
            )
            .add_slot(
                PluginSlot::optional(SLOT_TILEMAP, nara_render::tilemap::TILEMAP_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::tilemap::TilemapPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE, nara_render::image::IMAGE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::image::ImagePlugin>(),
            );
        #[cfg(feature = "serde")]
        let builder = builder
            .add_slot(
                PluginSlot::optional(
                    SLOT_TILESET_ASSETS,
                    nara_render::tilemap::TILESET_ASSET_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::tilemap::TileSetAssetPlugin>(),
            )
            .add_slot(
                PluginSlot::optional(SLOT_MATERIAL, nara_render::material::MATERIAL_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::material::Material2dPlugin>(),
            );
        builder
    }
}

#[cfg(feature = "runtime-ui")]
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeUiPlugins;

#[cfg(feature = "runtime-ui")]
impl PluginGroup for RuntimeUiPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.runtime-ui");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(SLOT_UI, nara_ui::UI_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui::UiPlugin>(),
            )
    }
}

/// Backend-neutral UI packet preparation for products that present runtime UI.
#[cfg(feature = "presentation-ui")]
#[derive(Debug, Default, Clone, Copy)]
pub struct PresentationUiPlugins;

#[cfg(feature = "presentation-ui")]
impl PluginGroup for PresentationUiPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.presentation-ui");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                nara_render::plugin(None),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_IMAGE_PREPARE,
                    nara_render::image::IMAGE_PREPARE_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::image::ImagePreparePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE, nara_render::image::IMAGE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::image::ImagePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_UI_RENDER, nara_render::hud::UI_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::hud::UiRenderPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_UI_PRESENTATION_BRIDGE,
                    nara_render::hud::UI_PRESENTATION_BRIDGE_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::hud::UiPresentationBridgePlugin>(),
            )
    }
}

/// Backend-neutral 2D packet production for products that present sprite and tilemap content.
#[cfg(feature = "presentation-2d")]
#[derive(Debug, Default, Clone, Copy)]
pub struct Presentation2dPlugins;

#[cfg(feature = "presentation-2d")]
impl PluginGroup for Presentation2dPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.presentation-2d");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                nara_render::plugin(None),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_IMAGE_PREPARE,
                    nara_render::image::IMAGE_PREPARE_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::image::ImagePreparePlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SPRITE_RENDER,
                    nara_render::sprite_render::SPRITE_RENDER_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::sprite_render::SpriteRenderPlugin>(),
            )
    }
}

#[cfg(feature = "desktop-winit")]
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopWinitPlugins;

#[cfg(feature = "desktop-winit")]
impl PluginGroup for DesktopWinitPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.desktop-winit");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(SLOT_WINDOW, nara_window::WINDOW_PLUGIN_ID),
            PluginDefinition::for_default::<nara_window::WindowPlugin>(),
        )
    }
}

/// Stock WGPU capture, policy and native window presentation over shared render components.
///
/// Sprite and UI capture contributors are selected explicitly. Embedded products can disable the
/// optional window-presentation slot while retaining the same capture and policy installation.
#[cfg(feature = "render-wgpu")]
#[derive(Debug, Default, Clone, Copy)]
pub struct WgpuBackendPlugins;

#[cfg(feature = "render-wgpu")]
impl PluginGroup for WgpuBackendPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.render-wgpu");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_RENDER_COMPONENTS,
                    nara_render::RENDER_COMPONENTS_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::RenderComponentsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                nara_render::plugin(None),
            )
            .add_slot(
                PluginSlot::required(SLOT_WGPU_CAPTURE, nara_render::gpu::WGPU_CAPTURE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::gpu::WgpuCapturePlugin>(),
            )
            .add_slot(
                PluginSlot::optional(
                    SLOT_WGPU_WINDOW_PRESENTATION,
                    nara_render::gpu::WGPU_WINDOW_PRESENTATION_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::gpu::WgpuWindowPresentationPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_WGPU_STOCK_CAPTURE,
                    nara_render::gpu::WGPU_STOCK_CAPTURE_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::gpu::WgpuStockCapturePlugin>(),
            )
            .add_slot(
                PluginSlot::optional(
                    SLOT_WGPU_STOCK_POLICY,
                    nara_render::gpu::WGPU_STOCK_POLICY_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::gpu::WgpuStockPolicyPlugin>(),
            )
    }
}
