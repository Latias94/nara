#[cfg(feature = "physics-2d")]
use nara_app::{PluginDefinition, PluginSlot, PluginSlotId};
use nara_app::{PluginGroup, PluginGroupBuilder, PluginGroupId};

#[cfg(all(
    feature = "project-runtime",
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub(crate) use desktop::run as run_desktop_app;

/// Installs the game plugins selected by the enabled Cargo features.
///
/// With desktop Winit and WGPU enabled, [`Self::new`] also installs a windowed runner.
/// Use [`Self::headless`] to drive the same game systems without a native window.
#[derive(Clone, Debug)]
pub struct DefaultPlugins {
    #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
    window: Option<nara_window::Window>,
}

impl Default for DefaultPlugins {
    fn default() -> Self {
        Self::new("Nara")
    }
}

impl DefaultPlugins {
    pub fn new(title: impl Into<String>) -> Self {
        #[cfg(not(all(feature = "desktop-winit", feature = "render-wgpu")))]
        let _ = title;
        Self {
            #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
            window: Some(nara_window::Window::new(title, Default::default())),
        }
    }

    pub const fn headless() -> Self {
        Self {
            #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
            window: None,
        }
    }

    pub fn with_window_size(self, width: u32, height: u32) -> Self {
        #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
        {
            let mut plugins = self;
            if let Some(window) = &mut plugins.window {
                window.resolution = nara_window::WindowResolution::new(width, height);
            }
            plugins
        }
        #[cfg(not(all(feature = "desktop-winit", feature = "render-wgpu")))]
        {
            let _ = (width, height);
            self
        }
    }
}

impl PluginGroup for DefaultPlugins {
    const ID: PluginGroupId = PluginGroupId::new("nara.plugins.default");

    fn build(self) -> PluginGroupBuilder {
        let plugins = PluginGroupBuilder::new().add_group(super::MinimalPlugins);
        #[cfg(feature = "runtime-2d")]
        let plugins = plugins.add_group(super::Runtime2dPlugins);
        #[cfg(feature = "runtime-ui")]
        let plugins = plugins.add_group(super::RuntimeUiPlugins);
        #[cfg(feature = "physics-2d")]
        let plugins = plugins.add_slot(
            PluginSlot::required(
                PluginSlotId::new("nara.plugins.slot.physics-2d"),
                nara_physics2d::PHYSICS_2D_PLUGIN_ID,
            ),
            PluginDefinition::for_default::<nara_physics2d::Physics2dPlugin>(),
        );

        #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
        if let Some(window) = self.window {
            #[cfg(feature = "audio")]
            let plugins = audio::add::<{ cfg!(feature = "audio-device") }>(plugins);
            return desktop::add(plugins, window);
        }
        #[cfg(feature = "audio")]
        let plugins = audio::add::<false>(plugins);
        plugins
    }
}

#[cfg(feature = "audio")]
mod audio {
    use nara_app::{
        App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
        PluginDefinition, PluginError, PluginGroupBuilder, PluginId, PluginSlot, PluginSlotId,
    };
    use nara_audio::{AudioMixer, setup_audio, update_audio_players};
    use nara_ecs::{Res, ResMut, schedule::IntoScheduleConfigs};
    use nara_scene::spatial::SpatialSet;

    const ID: PluginId = PluginId::new("nara.audio");
    const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime);

    #[derive(Default)]
    struct AudioPlugin<const DEVICE: bool>;

    impl<const DEVICE: bool> Plugin for AudioPlugin<DEVICE> {
        fn declaration() -> &'static PluginDeclaration {
            &DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            let mixer = if DEVICE {
                AudioMixer::new()
            } else {
                AudioMixer::mock()
            };
            let mixer = mixer.map_err(|error| PluginError::SetupFailed {
                plugin: ID,
                message: error.to_string(),
            })?;
            setup_audio(app.world_mut()?, mixer);
            app.add_systems(
                CoreStage::FixedUpdate,
                (advance_mock, update_audio_players)
                    .chain()
                    .after(SpatialSet::AfterFixed)
                    .in_set(FixedUpdateSet::Finalize),
            )?;
            Ok(())
        }
    }

    fn advance_mock(time: Res<FixedTime>, mut mixer: ResMut<AudioMixer>) {
        mixer.advance_mock(time.delta());
    }

    pub(super) fn add<const DEVICE: bool>(plugins: PluginGroupBuilder) -> PluginGroupBuilder {
        plugins.add_slot(
            PluginSlot::required(PluginSlotId::new("nara.plugins.slot.audio"), ID),
            PluginDefinition::for_default::<AudioPlugin<DEVICE>>(),
        )
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
mod desktop {
    use nara_app::{
        App, AppExit, AppRunError, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginError, PluginGroupBuilder, PluginId, PluginSlot, PluginSlotId, RuntimeInstance,
    };

    use crate::desktop_presentation::{
        DesktopPresentationDriver, DesktopPresentationOptions, DesktopPresentationReports,
    };

    const RUNNER_ID: PluginId = PluginId::new("nara.plugins.winit-runner");
    const RUNNER_SLOT: PluginSlotId = PluginSlotId::new("nara.plugins.slot.winit-runner");
    const RUNNER_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(RUNNER_ID, PluginCategory::Runtime);

    #[derive(Default)]
    struct DesktopRunner;

    impl Plugin for DesktopRunner {
        fn declaration() -> &'static PluginDeclaration {
            &RUNNER_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.set_runner(run);
            Ok(())
        }
    }

    pub(super) fn add(
        plugins: PluginGroupBuilder,
        window: nara_window::Window,
    ) -> PluginGroupBuilder {
        #[cfg(feature = "presentation-ui")]
        let ui_definition = super::super::ui_surface::project_primary_ui_surface_definition(
            window.resolution.physical_width,
            window.resolution.physical_height,
            [0; 4],
        );
        let plugins = plugins
            .add_slot(
                PluginSlot::required(super::super::SLOT_WINDOW, nara_window::WINDOW_PLUGIN_ID),
                nara_window::plugin(Some(window)),
            )
            .add_group(super::super::WgpuBackendPlugins)
            .add_slot(
                PluginSlot::required(RUNNER_SLOT, RUNNER_ID),
                PluginDefinition::for_default::<DesktopRunner>(),
            );
        #[cfg(feature = "presentation-2d")]
        let plugins = plugins
            .add_group(super::super::Presentation2dPlugins)
            .add_slot(
                PluginSlot::required(
                    PluginSlotId::new("nara.plugins.slot.render-wgpu.stock-sprite"),
                    nara_render::gpu::WGPU_STOCK_SPRITE_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::gpu::WgpuStockSpritePlugin>(),
            );
        #[cfg(feature = "presentation-ui")]
        let plugins = plugins
            .add_group(super::super::PresentationUiPlugins)
            .add_slot(
                PluginSlot::required(
                    PluginSlotId::new("nara.plugins.slot.primary-ui-surface"),
                    super::super::ui_surface::PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
                ),
                ui_definition,
            )
            .add_slot(
                PluginSlot::required(
                    PluginSlotId::new("nara.plugins.slot.render-wgpu.stock-ui"),
                    nara_render::gpu::WGPU_STOCK_UI_PLUGIN_ID,
                ),
                PluginDefinition::for_default::<nara_render::gpu::WgpuStockUiPlugin>(),
            );
        plugins
    }

    pub(crate) fn run(app: App) -> Result<AppExit, AppRunError> {
        let runtime =
            RuntimeInstance::start(app).map_err(|error| AppRunError::runner(error.to_string()))?;
        let presentation = DesktopPresentationDriver::new(
            &runtime,
            DesktopPresentationOptions::default(),
            #[cfg(all(feature = "project-runtime", feature = "runtime-2d", feature = "serde"))]
            None,
            DesktopPresentationReports::default(),
        );
        let mut session = nara_window::winit::WinitRunner::default().start_with_presentation_on(
            runtime,
            Box::new(presentation),
            std::time::Duration::from_secs(5),
            None,
        );
        session.execute();
        session
            .into_retired()
            .map_err(|_| AppRunError::runner("could not finish retiring the desktop window"))?
            .result
    }
}
