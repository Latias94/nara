use nara_app::{
    PluginDefinition, PluginGroup, PluginGroupBuilder, PluginGroupId, PluginSlot, PluginSlotId,
};
#[cfg(feature = "runtime-ui")]
use nara_scene::runtime_configuration::EffectivePrimaryUiSurface;
use nara_scene::runtime_configuration::{
    EffectiveRuntimeConfiguration, ProductCapability, ProductCapabilitySet, RuntimePreset,
};

use super::{HeadlessRuntimePlugins, MinimalPlugins, ServerPlugins};

#[cfg(all(feature = "presentation-2d", feature = "render-wgpu"))]
const SLOT_WGPU_STOCK_SPRITE: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.stock-sprite");
#[cfg(all(feature = "presentation-ui", feature = "render-wgpu"))]
const SLOT_WGPU_STOCK_UI: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.render-wgpu.stock-ui");
#[cfg(feature = "runtime-ui")]
const SLOT_PROJECT_PRIMARY_UI_SURFACE: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.project-primary-ui-surface");
const SLOT_STARTUP_SCENE_ACTIVATION: PluginSlotId =
    PluginSlotId::new("nara.plugins.slot.startup-scene-activation");
const SLOT_RUNTIME_TIME: PluginSlotId = PluginSlotId::new("nara.plugins.slot.runtime-time");

#[derive(Debug, Clone)]
pub(crate) struct ProjectProfilePlugins {
    preset: RuntimePreset,
    capabilities: ProductCapabilitySet,
    task_config: nara_app::tasks::TaskPoolConfig,
    time_definition: PluginDefinition,
    #[cfg(feature = "desktop-winit")]
    primary_window: Option<nara_window::Window>,
    #[cfg(any(
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    render_definition: PluginDefinition,
    #[cfg(feature = "runtime-ui")]
    primary_ui_surface: Option<EffectivePrimaryUiSurface>,
}

impl ProjectProfilePlugins {
    pub(crate) fn new(
        settings: &EffectiveRuntimeConfiguration,
        capabilities: ProductCapabilitySet,
    ) -> Self {
        Self {
            preset: settings.runtime_preset(),
            capabilities,
            task_config: settings.tasks().pool_config,
            time_definition: super::runtime_time::definition(settings.runtime()),
            #[cfg(feature = "desktop-winit")]
            primary_window: window_from_settings(settings.window()),
            #[cfg(any(
                feature = "presentation-2d",
                feature = "presentation-ui",
                feature = "render-wgpu"
            ))]
            render_definition: nara_render::plugin(settings.render().world_canvas.map(|canvas| {
                nara_render::WorldCanvas::new(canvas.width, canvas.height)
                    .expect("resolved project world canvas extents are positive")
            })),
            #[cfg(feature = "runtime-ui")]
            primary_ui_surface: settings.ui().primary_surface,
        }
    }
}

impl PluginGroup for ProjectProfilePlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.project-runtime");

    fn build(self) -> PluginGroupBuilder {
        let task_plugin = nara_app::tasks::plugin(self.task_config);
        let product_group_includes_minimal =
            self.capabilities.contains(ProductCapability::Runtime2d)
                || self.capabilities.contains(ProductCapability::RuntimeUi);
        let builder = PluginGroupBuilder::new().add_slot(
            PluginSlot::required(SLOT_RUNTIME_TIME, super::runtime_time::PLUGIN_ID),
            self.time_definition,
        );
        let builder = match self.preset {
            RuntimePreset::Minimal if product_group_includes_minimal => builder,
            RuntimePreset::Minimal => {
                builder.add_edited_group(MinimalPlugins.edit().configure(task_plugin.clone()))
            }
            RuntimePreset::LocalHeadless => builder
                .add_edited_group(HeadlessRuntimePlugins.edit().configure(task_plugin.clone())),
            RuntimePreset::Server => {
                builder.add_edited_group(ServerPlugins.edit().configure(task_plugin.clone()))
            }
        };

        #[cfg(feature = "runtime-2d")]
        let builder = if self.capabilities.contains(ProductCapability::Runtime2d) {
            builder.add_edited_group(
                super::Runtime2dPlugins
                    .edit()
                    .configure(task_plugin.clone()),
            )
        } else {
            builder
        };

        #[cfg(feature = "runtime-ui")]
        let builder = if self.capabilities.contains(ProductCapability::RuntimeUi) {
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
                    super::ui_surface::project_primary_ui_surface_definition(
                        profile.logical_width,
                        profile.logical_height,
                        [
                            profile.safe_area.left,
                            profile.safe_area.top,
                            profile.safe_area.right,
                            profile.safe_area.bottom,
                        ],
                    ),
                ),
                None => builder,
            }
        } else {
            builder
        };

        #[cfg(feature = "presentation-2d")]
        let builder = if self
            .capabilities
            .contains(ProductCapability::Presentation2d)
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
            .contains(ProductCapability::PresentationUi)
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
        let builder = if self.capabilities.contains(ProductCapability::DesktopWinit) {
            builder.add_edited_group(
                super::DesktopWinitPlugins
                    .edit()
                    .configure(nara_window::plugin(self.primary_window)),
            )
        } else {
            builder
        };

        #[cfg(feature = "render-wgpu")]
        let builder = if self.capabilities.contains(ProductCapability::RenderWgpu) {
            let backend = super::WgpuBackendPlugins
                .edit()
                .configure_slot(super::SLOT_RENDER, self.render_definition.clone());
            let backend = if self.capabilities.contains(ProductCapability::DesktopWinit) {
                backend
            } else {
                backend.disable_slot(super::SLOT_WGPU_WINDOW_PRESENTATION)
            };
            builder.add_edited_group(backend)
        } else {
            builder
        };

        let builder = add_product_contributions(builder, &self.capabilities);

        builder.add_slot(
            PluginSlot::required(
                SLOT_STARTUP_SCENE_ACTIVATION,
                crate::startup_scene::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<crate::startup_scene::StartupSceneActivationPlugin>(),
        )
    }
}

/// Lowers the admitted project window settings for a runtime window plugin.
#[cfg(feature = "runtime-window")]
#[must_use]
pub fn window_from_settings(
    settings: &nara_scene::runtime_configuration::EffectiveWindowSettings,
) -> Option<nara_window::Window> {
    use nara_scene::runtime_configuration::{ProjectPresentMode, ProjectWindowMode};
    use nara_window::{PresentMode, Window, WindowMode, WindowResolution};

    if !settings.enabled {
        return None;
    }
    let resolution = WindowResolution::new(settings.width, settings.height)
        .with_scale_factor(settings.scale_factor);
    let mut window = Window::new(settings.title.clone(), resolution);
    window.mode = match settings.mode {
        ProjectWindowMode::Windowed => WindowMode::Windowed,
        ProjectWindowMode::BorderlessFullscreen => WindowMode::BorderlessFullscreen,
        ProjectWindowMode::Fullscreen => WindowMode::Fullscreen,
    };
    window.present_mode = match settings.present_mode {
        ProjectPresentMode::AutoVsync => PresentMode::AutoVsync,
        ProjectPresentMode::AutoNoVsync => PresentMode::AutoNoVsync,
        ProjectPresentMode::Fifo => PresentMode::Fifo,
        ProjectPresentMode::Immediate => PresentMode::Immediate,
        ProjectPresentMode::Mailbox => PresentMode::Mailbox,
    };
    window.resizable = settings.resizable;
    Some(window)
}

fn add_product_contributions(
    builder: PluginGroupBuilder,
    capabilities: &ProductCapabilitySet,
) -> PluginGroupBuilder {
    let _ = capabilities;

    #[cfg(all(feature = "presentation-2d", feature = "render-wgpu"))]
    let builder = if capabilities.contains(ProductCapability::Presentation2d)
        && capabilities.contains(ProductCapability::RenderWgpu)
    {
        builder.add_slot(
            PluginSlot::required(
                SLOT_WGPU_STOCK_SPRITE,
                nara_render::gpu::WGPU_STOCK_SPRITE_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<nara_render::gpu::WgpuStockSpritePlugin>(),
        )
    } else {
        builder
    };

    #[cfg(all(feature = "presentation-ui", feature = "render-wgpu"))]
    let builder = if capabilities.contains(ProductCapability::PresentationUi)
        && capabilities.contains(ProductCapability::RenderWgpu)
    {
        builder.add_slot(
            PluginSlot::required(
                SLOT_WGPU_STOCK_UI,
                nara_render::gpu::WGPU_STOCK_UI_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<nara_render::gpu::WgpuStockUiPlugin>(),
        )
    } else {
        builder
    };

    builder
}
