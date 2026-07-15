use nara_app::{
    App, Plugin, PluginDeclaration, PluginDefinition, PluginError, PluginGroup, PluginGroupBuilder,
    PluginGroupId, PluginId, PluginSlot, PluginSlotId,
};
use nara_project::{EffectiveProjectSettings, ProductCapabilitySet, RuntimePreset};

const SERVER_TIME_POLICY_PLUGIN_ID: PluginId = PluginId::new("nara.server-time-policy");
const SERVER_TIME_POLICY_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SERVER_TIME_POLICY_PLUGIN_ID, nara_app::PluginCategory::Core);

const SLOT_COMPONENT_REGISTRY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.component-registry");
const SLOT_HIERARCHY: PluginSlotId = PluginSlotId::new("nara.plugins.slot.hierarchy");
const SLOT_DIAGNOSTICS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.diagnostics");
const SLOT_TASKS: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tasks");
const SLOT_ASSET: PluginSlotId = PluginSlotId::new("nara.plugins.slot.asset");
const SLOT_TRANSFORM: PluginSlotId = PluginSlotId::new("nara.plugins.slot.transform");
const SLOT_INPUT: PluginSlotId = PluginSlotId::new("nara.plugins.slot.input");
const SLOT_GAMEPLAY_COMMANDS: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.gameplay-commands");
const SLOT_SERVER_TIME_POLICY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.server-time-policy");
#[cfg(feature = "runtime-2d")]
const SLOT_SPRITE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite");
#[cfg(feature = "runtime-2d")]
const SLOT_TILEMAP: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tilemap");
#[cfg(any(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "render-wgpu"
))]
const SLOT_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render");
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
const SLOT_IMAGE_PREPARE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image-prepare");
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
const SLOT_IMAGE: PluginSlotId = PluginSlotId::new("nara.plugins.slot.image");
#[cfg(feature = "runtime-2d")]
const SLOT_SPRITE_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.sprite-render");
#[cfg(feature = "runtime-ui")]
const SLOT_UI: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui");
#[cfg(feature = "runtime-ui")]
const SLOT_UI_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.ui-render");
#[cfg(feature = "desktop-winit")]
const SLOT_WINDOW: PluginSlotId = PluginSlotId::new("nara.plugins.slot.window");
#[cfg(feature = "render-wgpu")]
const SLOT_WGPU_RENDER: PluginSlotId = PluginSlotId::new("nara.plugins.slot.render-wgpu");
#[cfg(feature = "tooling")]
const SLOT_TOOLING: PluginSlotId = PluginSlotId::new("nara.plugins.slot.tooling");

/// Minimal runtime defaults for headless examples and code-first games.
#[derive(Debug, Default, Clone, Copy)]
pub struct MinimalPlugins;

impl PluginGroup for MinimalPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.minimal");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(
                    SLOT_COMPONENT_REGISTRY,
                    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_reflect::ComponentRegistryPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_HIERARCHY, nara_scene::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::HierarchyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_DIAGNOSTICS, nara_diagnostic::DIAGNOSTICS_PLUGIN_ID),
                PluginDefinition::for_default::<nara_diagnostic::DiagnosticsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TASKS, nara_tasks::TASK_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tasks::TaskPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_ASSET, nara_asset::ASSET_PLUGIN_ID),
                PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TRANSFORM, nara_transform::TRANSFORM_PLUGIN_ID),
                PluginDefinition::for_default::<nara_transform::TransformPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_INPUT, nara_input::INPUT_PLUGIN_ID),
                PluginDefinition::for_default::<nara_input::InputPlugin>(),
            )
    }
}

/// Local headless defaults with input observations and semantic gameplay commands.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeadlessRuntimePlugins;

impl PluginGroup for HeadlessRuntimePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.headless-runtime");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(
                    SLOT_GAMEPLAY_COMMANDS,
                    nara_gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_gameplay::GameplayCommandPlugin>(),
            )
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
                PluginSlot::required(SLOT_HIERARCHY, nara_scene::HIERARCHY_PLUGIN_ID),
                PluginDefinition::for_default::<nara_scene::HierarchyPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_DIAGNOSTICS, nara_diagnostic::DIAGNOSTICS_PLUGIN_ID),
                PluginDefinition::for_default::<nara_diagnostic::DiagnosticsPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TASKS, nara_tasks::TASK_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tasks::TaskPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_ASSET, nara_asset::ASSET_PLUGIN_ID),
                PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_TRANSFORM, nara_transform::TRANSFORM_PLUGIN_ID),
                PluginDefinition::for_default::<nara_transform::TransformPlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_GAMEPLAY_COMMANDS,
                    nara_gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_gameplay::GameplayCommandPlugin>(),
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
        PluginGroupBuilder::new()
            .add_group(MinimalPlugins)
            .add_slot(
                PluginSlot::required(SLOT_SPRITE, nara_sprite::SPRITE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_sprite::SpritePlugin>(),
            )
            .add_slot(
                PluginSlot::optional(SLOT_TILEMAP, nara_tilemap::TILEMAP_PLUGIN_ID),
                PluginDefinition::for_default::<nara_tilemap::TilemapPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::RenderPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE_PREPARE, nara_image::IMAGE_PREPARE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePreparePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE, nara_image::IMAGE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePlugin>(),
            )
            .add_slot(
                PluginSlot::required(
                    SLOT_SPRITE_RENDER,
                    nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_sprite_render::SpriteRenderPlugin>(),
            )
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
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::RenderPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE_PREPARE, nara_image::IMAGE_PREPARE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePreparePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_IMAGE, nara_image::IMAGE_PLUGIN_ID),
                PluginDefinition::for_default::<nara_image::ImagePlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_UI, nara_ui::UI_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui::UiPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_UI_RENDER, nara_ui_render::UI_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_ui_render::UiRenderPlugin>(),
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

#[cfg(feature = "render-wgpu")]
#[derive(Debug, Default, Clone, Copy)]
pub struct WgpuBackendPlugins;

#[cfg(feature = "render-wgpu")]
impl PluginGroup for WgpuBackendPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.render-wgpu");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_slot(
                PluginSlot::required(SLOT_RENDER, nara_render::RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render::RenderPlugin>(),
            )
            .add_slot(
                PluginSlot::required(SLOT_WGPU_RENDER, nara_render_wgpu::WGPU_RENDER_PLUGIN_ID),
                PluginDefinition::for_default::<nara_render_wgpu::WgpuRenderPlugin>(),
            )
    }
}

#[cfg(feature = "tooling")]
#[derive(Debug, Default, Clone, Copy)]
pub struct ToolingPlugins;

#[cfg(feature = "tooling")]
impl PluginGroup for ToolingPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.tooling");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new().add_slot(
            PluginSlot::required(SLOT_TOOLING, nara_tooling::TOOLING_PLUGIN_ID),
            PluginDefinition::for_default::<nara_tooling::ToolingPlugin>(),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectProfilePlugins {
    preset: RuntimePreset,
    capabilities: ProductCapabilitySet,
    task_config: nara_tasks::TaskPoolConfig,
    primary_window: Option<nara_window::Window>,
}

impl ProjectProfilePlugins {
    pub(crate) fn new(
        settings: &EffectiveProjectSettings,
        capabilities: ProductCapabilitySet,
    ) -> Self {
        Self {
            preset: settings.runtime_preset,
            capabilities,
            task_config: settings.tasks.pool_config,
            primary_window: settings.window.to_window(),
        }
    }
}

impl PluginGroup for ProjectProfilePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime");

    fn build(self) -> PluginGroupBuilder {
        let _ = &self.primary_window;
        let task_plugin = nara_tasks::plugin(self.task_config);
        let product_group_includes_minimal = self
            .capabilities
            .contains(nara_project::ProductCapability::Runtime2d)
            || self
                .capabilities
                .contains(nara_project::ProductCapability::RuntimeUi);
        let builder = match self.preset {
            RuntimePreset::Minimal if product_group_includes_minimal => PluginGroupBuilder::new(),
            RuntimePreset::Minimal => PluginGroupBuilder::new()
                .add_edited_group(MinimalPlugins.edit().configure(task_plugin.clone())),
            RuntimePreset::LocalHeadless => PluginGroupBuilder::new()
                .add_edited_group(HeadlessRuntimePlugins.edit().configure(task_plugin.clone())),
            RuntimePreset::Server => PluginGroupBuilder::new()
                .add_edited_group(ServerPlugins.edit().configure(task_plugin.clone())),
        };

        #[cfg(feature = "runtime-2d")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::Runtime2d)
        {
            builder.add_edited_group(Runtime2dPlugins.edit().configure(task_plugin.clone()))
        } else {
            builder
        };

        #[cfg(feature = "runtime-ui")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::RuntimeUi)
        {
            builder.add_edited_group(RuntimeUiPlugins.edit().configure(task_plugin.clone()))
        } else {
            builder
        };

        #[cfg(feature = "tooling")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::Tooling)
        {
            builder.add_group(ToolingPlugins)
        } else {
            builder
        };

        #[cfg(feature = "desktop-winit")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::DesktopWinit)
        {
            builder.add_edited_group(
                DesktopWinitPlugins
                    .edit()
                    .configure(nara_window::plugin(self.primary_window)),
            )
        } else {
            builder
        };

        #[cfg(feature = "render-wgpu")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::RenderWgpu)
        {
            builder.add_group(WgpuBackendPlugins)
        } else {
            builder
        };

        builder
    }
}
