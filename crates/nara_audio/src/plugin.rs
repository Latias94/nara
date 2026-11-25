use bevy_ecs::{
    prelude::{Res, ResMut},
    schedule::IntoScheduleConfigs,
};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
    PluginError, PluginId,
};
use nara_scene::spatial::SpatialSet;

use crate::{AudioMixer, setup_audio, update_audio_players};

pub const AUDIO_PLUGIN_ID: PluginId = PluginId::new("nara.audio");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(AUDIO_PLUGIN_ID, PluginCategory::Runtime);

/// Installs the shared mixer and advances playback once per finalized simulation tick.
///
/// The default runs with a deterministic mock clock. [`AudioPlugin::device`] selects the
/// default output device when available and keeps mock playback available when it cannot open.
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
