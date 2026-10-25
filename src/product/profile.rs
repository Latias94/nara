use nara_app::{
    PluginDefinition, PluginGroup, PluginGroupBuilder, PluginGroupId, PluginSlot, PluginSlotId,
};
use nara_project::{EffectiveProjectSettings, ProductCapabilitySet, RuntimePreset};

use super::{HeadlessRuntimePlugins, MinimalPlugins, ServerPlugins};

#[cfg(all(feature = "desktop-winit", feature = "presentation-ui"))]
const SLOT_UI_WINIT_ACCESSIBILITY: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.ui-winit-accessibility");
#[cfg(all(feature = "presentation-2d", feature = "render-wgpu"))]
const SLOT_WGPU_SPRITE_SUBMITTER: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.sprite-submitter");
#[cfg(all(feature = "presentation-ui", feature = "render-wgpu"))]
const SLOT_WGPU_UI_SUBMITTER: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.ui-submitter");
#[cfg(feature = "runtime-ui")]
const SLOT_PROJECT_PRIMARY_UI_SURFACE: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.project-primary-ui-surface");
const SLOT_STARTUP_SCENE_ACTIVATION: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.startup-scene-activation");

#[derive(Debug, Clone)]
pub(crate) struct ProjectProfilePlugins {
    preset: RuntimePreset,
    capabilities: ProductCapabilitySet,
    task_config: nara_tasks::TaskPoolConfig,
    primary_window: Option<nara_window::Window>,
    #[cfg(any(
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    render_definition: PluginDefinition,
    #[cfg(feature = "runtime-ui")]
    primary_ui_surface: Option<super::ui_surface::AcceptedPrimaryUiSurfaceProfile>,
    placement: ProjectProfilePlacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectProfilePlacement {
    Shipping,
    EditorPlay,
}

impl ProjectProfilePlugins {
    pub(crate) fn new(
        settings: &EffectiveProjectSettings,
        capabilities: ProductCapabilitySet,
    ) -> Self {
        Self::with_placement(settings, capabilities, ProjectProfilePlacement::Shipping)
    }

    pub(crate) fn for_editor_play(
        settings: &EffectiveProjectSettings,
        capabilities: ProductCapabilitySet,
    ) -> Self {
        Self::with_placement(settings, capabilities, ProjectProfilePlacement::EditorPlay)
    }

    fn with_placement(
        settings: &EffectiveProjectSettings,
        capabilities: ProductCapabilitySet,
        placement: ProjectProfilePlacement,
    ) -> Self {
        Self {
            preset: settings.runtime_preset,
            capabilities,
            task_config: settings.tasks.pool_config,
            primary_window: settings.window.to_window(),
            #[cfg(any(
                feature = "presentation-2d",
                feature = "presentation-ui",
                feature = "render-wgpu"
            ))]
            render_definition: nara_render::plugin(settings.render.world_canvas.map(|canvas| {
                nara_render::WorldCanvas::new(canvas.width, canvas.height)
                    .expect("resolved project world canvas extents are positive")
            })),
            #[cfg(feature = "runtime-ui")]
            primary_ui_surface: settings
                .ui
                .primary_surface
                .map(super::ui_surface::AcceptedPrimaryUiSurfaceProfile::from_effective),
            placement,
        }
    }
}

impl PluginGroup for ProjectProfilePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime");

    fn build(self) -> PluginGroupBuilder {
        let _ = &self.primary_window;
        let task_plugin = match self.placement {
            ProjectProfilePlacement::Shipping => nara_tasks::plugin(self.task_config),
            ProjectProfilePlacement::EditorPlay => nara_tasks::lazy_plugin(self.task_config),
        };
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
            builder.add_edited_group(
                super::Runtime2dPlugins
                    .edit()
                    .configure(task_plugin.clone()),
            )
        } else {
            builder
        };

        #[cfg(feature = "runtime-ui")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::RuntimeUi)
        {
            let builder = builder.add_edited_group(
                super::RuntimeUiPlugins
                    .edit()
                    .configure(task_plugin.clone()),
            );
            match self.primary_ui_surface {
                Some(profile) => builder.add_slot(
                    PluginSlot::required(
                        SLOT_PROJECT_PRIMARY_UI_SURFACE,
                        super::ui_surface::PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
                    ),
                    super::ui_surface::project_primary_ui_surface_definition(profile),
                ),
                None => builder,
            }
        } else {
            builder
        };

        #[cfg(feature = "presentation-2d")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::Presentation2d)
        {
            builder.add_edited_group(
                super::Presentation2dPlugins
                    .edit()
                    .configure_slot(super::SLOT_RENDER, self.render_definition.clone()),
            )
        } else {
            builder
        };

        #[cfg(feature = "presentation-ui")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::PresentationUi)
        {
            builder.add_edited_group(
                super::PresentationUiPlugins
                    .edit()
                    .configure_slot(super::SLOT_RENDER, self.render_definition.clone()),
            )
        } else {
            builder
        };

        #[cfg(feature = "desktop-winit")]
        let builder = if self
            .capabilities
            .contains(nara_project::ProductCapability::DesktopWinit)
        {
            builder.add_edited_group(
                super::DesktopWinitPlugins
                    .edit()
                    .configure(nara_window::plugin(self.primary_window)),
            )
        } else {
            builder
        };

        #[cfg(feature = "render-wgpu")]
        let builder = if self.placement == ProjectProfilePlacement::Shipping
            && self
                .capabilities
                .contains(nara_project::ProductCapability::RenderWgpu)
        {
            builder.add_edited_group(
                super::WgpuBackendPlugins
                    .edit()
                    .configure_slot(super::SLOT_RENDER, self.render_definition.clone()),
            )
        } else {
            builder
        };

        let builder = match self.placement {
            ProjectProfilePlacement::Shipping => {
                add_product_contributions(builder, &self.capabilities)
            }
            ProjectProfilePlacement::EditorPlay => builder,
        };

        builder.add_slot(
            PluginSlot::required(
                SLOT_STARTUP_SCENE_ACTIVATION,
                crate::startup_scene::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<crate::startup_scene::StartupSceneActivationPlugin>(),
        )
    }
}

fn add_product_contributions(
    builder: PluginGroupBuilder,
    capabilities: &ProductCapabilitySet,
) -> PluginGroupBuilder {
    let _ = capabilities;

    #[cfg(all(feature = "desktop-winit", feature = "presentation-ui"))]
    let builder = if capabilities.contains(nara_project::ProductCapability::DesktopWinit)
        && capabilities.contains(nara_project::ProductCapability::PresentationUi)
    {
        builder.add_slot(
            PluginSlot::required(
                SLOT_UI_WINIT_ACCESSIBILITY,
                crate::ui_presentation::WINIT_UI_ACCESSIBILITY_CONTRIBUTION_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<
                crate::ui_presentation::WinitUiAccessibilityContributionPlugin,
            >(),
        )
    } else {
        builder
    };

    #[cfg(all(feature = "presentation-2d", feature = "render-wgpu"))]
    let builder = if capabilities.contains(nara_project::ProductCapability::Presentation2d)
        && capabilities.contains(nara_project::ProductCapability::RenderWgpu)
    {
        builder.add_slot(
            PluginSlot::required(
                SLOT_WGPU_SPRITE_SUBMITTER,
                nara_render_wgpu::WGPU_SPRITE_SUBMITTER_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<nara_render_wgpu::WgpuSpriteSubmitterPlugin>(),
        )
    } else {
        builder
    };

    #[cfg(all(feature = "presentation-ui", feature = "render-wgpu"))]
    let builder = if capabilities.contains(nara_project::ProductCapability::PresentationUi)
        && capabilities.contains(nara_project::ProductCapability::RenderWgpu)
    {
        builder.add_slot(
            PluginSlot::required(
                SLOT_WGPU_UI_SUBMITTER,
                nara_render_wgpu::WGPU_UI_SUBMITTER_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<nara_render_wgpu::WgpuUiSubmitterPlugin>(),
        )
    } else {
        builder
    };

    builder
}
