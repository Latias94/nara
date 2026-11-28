use bevy_ecs::{
    prelude::{Res, ResMut},
    schedule::{IntoScheduleConfigs, common_conditions::resource_exists},
};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
    PluginError, PluginId,
};
use nara_asset::{ASSET_PLUGIN_ID, AssetAppExt, AssetServer, RuntimeAssetSource};
use nara_scene::spatial::SpatialSet;

use crate::{
    AudioClip, AudioClipLoader, AudioMixer, setup_audio, source::hydrate_audio_sources,
    update_audio_players,
};

pub const AUDIO_PLUGIN_ID: PluginId = PluginId::new("nara.audio");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(AUDIO_PLUGIN_ID, PluginCategory::Runtime);

/// Installs the shared mixer and advances playback once per finalized simulation tick.
///
/// The default runs with a deterministic mock clock. [`AudioPlugin::device`] selects the
/// default output device when available and keeps mock playback available when it cannot open.
/// When `AssetPlugin` is installed, it also registers the clip loader. Install `AssetPlugin`
/// first to load clips synchronously during app setup.
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioPlugin<const DEVICE: bool = false>;

impl AudioPlugin<false> {
    pub const fn mock() -> Self {
        Self
    }
}

impl AudioPlugin<true> {
    pub const fn device() -> Self {
        Self
    }
}

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
            plugin: AUDIO_PLUGIN_ID,
            message: error.to_string(),
        })?;
        setup_audio(app.world_mut()?, mixer);
        register_clip_loader(app)?;
        #[cfg(feature = "scene-runtime")]
        if app.has_plugin(nara_scene::SCENE_PLUGIN_ID) {
            use nara_scene::RegisterSceneComponent as _;

            app.register_scene_component::<crate::AudioClipSource>()
                .map_err(|error| PluginError::SetupFailed {
                    plugin: AUDIO_PLUGIN_ID,
                    message: error.to_string(),
                })?;
        }
        app.add_systems(
            CoreStage::FixedUpdate,
            (
                hydrate_audio_sources.run_if(resource_exists::<RuntimeAssetSource>),
                advance_mock,
                update_audio_players,
            )
                .chain()
                .after(SpatialSet::AfterFixed)
                .in_set(FixedUpdateSet::Finalize),
        )?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        register_clip_loader(app)
    }
}

fn register_clip_loader(app: &mut App) -> Result<(), PluginError> {
    if app.has_plugin(ASSET_PLUGIN_ID)
        && !app
            .world()
            .get_resource::<AssetServer>()
            .is_some_and(|server| server.loader_registry().contains::<AudioClip>())
    {
        app.register_asset_loader(AudioClipLoader)?;
    }
    Ok(())
}

fn advance_mock(time: Res<FixedTime>, mut mixer: ResMut<AudioMixer>) {
    mixer.advance_mock(time.delta());
}
