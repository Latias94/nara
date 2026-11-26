use nara_app::{PluginGroup, PluginGroupBuilder};

#[derive(Clone, Copy, Debug, Default)]
pub struct MinimalPlugins;

impl PluginGroup for MinimalPlugins {
    fn build(self) -> PluginGroupBuilder {
        let plugins = PluginGroupBuilder::new()
            .add_default::<nara_reflect::ComponentRegistryPlugin>()
            .add_default::<nara_app::diagnostics::DiagnosticsPlugin>()
            .add_default::<nara_asset::AssetPlugin>()
            .add_default::<nara_scene::SpatialPlugin>();
        #[cfg(feature = "runtime-input")]
        let plugins = plugins.add_default::<nara_input::InputPlugin>();
        plugins
    }
}

/// Installs the game plugins selected by the enabled Cargo features.
#[derive(Clone, Debug)]
pub struct DefaultPlugins {
    #[cfg(all(
        feature = "desktop-winit",
        feature = "render-wgpu",
        not(target_family = "wasm")
    ))]
    window: Option<nara_window::Window>,
}

impl Default for DefaultPlugins {
    fn default() -> Self {
        Self::new("Nara")
    }
}

impl DefaultPlugins {
    /// Uses a native window when Winit and WGPU are enabled.
    pub fn new(title: impl Into<String>) -> Self {
        #[cfg(not(all(
            feature = "desktop-winit",
            feature = "render-wgpu",
            not(target_family = "wasm")
        )))]
        let _ = title;
        Self {
            #[cfg(all(
                feature = "desktop-winit",
                feature = "render-wgpu",
                not(target_family = "wasm")
            ))]
            window: Some(nara_window::Window::new(title, Default::default())),
        }
    }

    /// Drives the game without opening a window or a physical audio device.
    pub const fn headless() -> Self {
        Self {
            #[cfg(all(
                feature = "desktop-winit",
                feature = "render-wgpu",
                not(target_family = "wasm")
            ))]
            window: None,
        }
    }

    /// Sets the initial width and height for a desktop run.
    pub fn with_window_size(self, width: u32, height: u32) -> Self {
        #[cfg(all(
            feature = "desktop-winit",
            feature = "render-wgpu",
            not(target_family = "wasm")
        ))]
        {
            let mut plugins = self;
            if let Some(window) = &mut plugins.window {
                window.resolution = nara_window::WindowResolution::new(width, height);
            }
            plugins
        }
        #[cfg(not(all(
            feature = "desktop-winit",
            feature = "render-wgpu",
            not(target_family = "wasm")
        )))]
        {
            let _ = (width, height);
            self
        }
    }
}

impl PluginGroup for DefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        let plugins = PluginGroupBuilder::new().add_group(MinimalPlugins);
        #[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
        let plugins = plugins.add_default::<nara_render::image::ImagePlugin>();
        #[cfg(feature = "runtime-ui")]
        let plugins = plugins.add_default::<nara_ui::UiPlugin>();
        #[cfg(feature = "physics-2d")]
        let plugins = plugins.add_default::<nara_physics2d::Physics2dPlugin>();

        #[cfg(all(
            feature = "desktop-winit",
            feature = "render-wgpu",
            not(target_family = "wasm")
        ))]
        if let Some(window) = self.window {
            #[cfg(feature = "audio")]
            let plugins = plugins
                .add_default::<nara_audio::AudioPlugin<{ cfg!(feature = "audio-device") }>>();
            return desktop::plugins(plugins, window);
        }
        #[cfg(feature = "audio")]
        let plugins = plugins.add_default::<nara_audio::AudioPlugin<false>>();
        plugins
    }
}

#[cfg(all(
    feature = "desktop-winit",
    feature = "render-wgpu",
    not(target_family = "wasm")
))]
mod desktop {
    use nara_app::{
        App, AppExit, AppRunError, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginGroupBuilder, PluginId, RuntimeInstance,
    };
    use nara_render::gpu::{
        WgpuCapturePlugin, WgpuStockCapturePlugin, WgpuStockPolicyPlugin,
        WgpuWindowPresentationPlugin,
        desktop::{
            DesktopPresentationDriver, DesktopPresentationOptions, DesktopPresentationReports,
        },
    };

    pub(super) fn plugins(
        plugins: PluginGroupBuilder,
        window: nara_window::Window,
    ) -> PluginGroupBuilder {
        #[cfg(feature = "presentation-ui")]
        let primary_ui = primary_ui::definition(
            window.resolution.physical_width,
            window.resolution.physical_height,
        );

        let plugins = plugins
            .add_definition(nara_window::plugin(Some(window)))
            .add_default::<nara_render::RenderPlugin>()
            .add_default::<WgpuCapturePlugin>()
            .add_default::<WgpuStockCapturePlugin>()
            .add_default::<WgpuStockPolicyPlugin>()
            .add_default::<WgpuWindowPresentationPlugin>();
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        let plugins = plugins.add_default::<nara_render::image::ImagePreparePlugin>();
        #[cfg(feature = "presentation-2d")]
        let plugins = plugins
            .add_default::<nara_render::sprite_render::SpriteRenderPlugin>()
            .add_default::<nara_render::gpu::WgpuStockSpritePlugin>();
        #[cfg(feature = "presentation-ui")]
        let plugins = plugins
            .add_definition(primary_ui)
            .add_default::<nara_render::hud::UiRenderPlugin>()
            .add_default::<nara_render::hud::UiPresentationBridgePlugin>()
            .add_default::<nara_render::gpu::WgpuStockUiPlugin>();
        plugins.add_default::<DesktopRunner>()
    }

    #[derive(Default)]
    struct DesktopRunner;

    const RUNNER_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(PluginId::new("nara.winit-runner"), PluginCategory::Platform);

    impl Plugin for DesktopRunner {
        fn declaration() -> &'static PluginDeclaration {
            &RUNNER_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.set_runner(run);
            Ok(())
        }
    }

    fn run(app: App) -> Result<AppExit, AppRunError> {
        let runtime = RuntimeInstance::start(app)?;
        let presentation = DesktopPresentationDriver::new(
            &runtime,
            DesktopPresentationOptions::default(),
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

    #[cfg(feature = "presentation-ui")]
    mod primary_ui {
        use nara_app::{
            App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
            PluginError, PluginId,
        };
        use nara_input::{InputPointerId, InputSeatId, InputWindowId};
        use nara_ui::{
            UiPrimarySurfaceMetrics, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot,
            install_ui_surface,
        };

        const ID: PluginId = PluginId::new("nara.primary-ui-surface");
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
            .requires_plugins(&[nara_ui::UI_PLUGIN_ID]);
        struct PrimaryUi(UiPrimarySurfaceMetrics);

        impl Plugin for PrimaryUi {
            fn declaration() -> &'static PluginDeclaration {
                &DECLARATION
            }

            fn build(&self, app: &mut App) -> Result<(), PluginError> {
                let endpoint = UiSurfaceEndpoint::new(
                    InputWindowId::new(nara_window::WindowId::PRIMARY.get()),
                    InputSeatId::PRIMARY,
                    InputPointerId::PRIMARY,
                );
                let transform = self
                    .0
                    .direct_transform()
                    .ok_or_else(|| setup_error("invalid surface dimensions"))?;
                let slot = UiSurfaceSlot::new(self.0.surface_id(), endpoint, transform)
                    .map_err(setup_error)?;
                app.insert_resource(self.0)?;
                install_ui_surface(app.world_mut()?, slot).map_err(setup_error)
            }
        }

        fn setup_error(error: impl std::fmt::Display) -> PluginError {
            PluginError::SetupFailed {
                plugin: ID,
                message: error.to_string(),
            }
        }

        pub(super) fn definition(width: u32, height: u32) -> PluginDefinition {
            let surface = UiSurfaceId::new(1).expect("the primary HUD surface ID is nonzero");
            let metrics = UiPrimarySurfaceMetrics::new(surface, width, height, [0; 4]);
            let configuration = [width.to_le_bytes(), height.to_le_bytes()].concat();
            PluginDefinition::infallible::<PrimaryUi, _>(
                PluginDefinitionId::new("nara.primary-ui-surface.configured", 1),
                configuration,
                move || PrimaryUi(metrics),
            )
        }
    }
}
