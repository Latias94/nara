use bevy_ecs::{
    lifecycle::HookContext,
    prelude::{Changed, Component, Entity, Or, Query, ResMut, Without},
    system::Commands,
    world::{DeferredWorld, EntityWorldMut},
};
use nara_asset::AssetServer;
use nara_core::AssetId;
use nara_reflect::Schema;

use crate::{AudioBusId, AudioError, AudioPlayback, AudioPlayer, SpatialAudio};

/// Authored clip and playback settings. [`AudioPlugin`](crate::AudioPlugin) resolves the clip
/// ID and installs the player on this entity. Use [`AudioPlayback`] to trigger a manual start.
#[derive(Component, Schema, Clone, Debug, PartialEq)]
#[require(AudioPlayback)]
#[component(on_remove = remove_audio_source)]
#[schema(
    crate = "nara_reflect",
    id = "nara.AudioClipSource",
    version = 1,
    alias = "Audio Clip Source"
)]
pub struct AudioClipSource {
    #[schema(alias = "Clip", capabilities(scene, inspect, edit, asset_ref))]
    pub clip: AssetId,
    #[schema(alias = "Bus", default = AudioBusId::SFX.as_str().to_owned())]
    pub bus: String,
    #[schema(alias = "Volume", default = 1.0, range = 0.0..=2.0)]
    pub volume: f32,
    #[schema(alias = "Looping", default)]
    pub looping: bool,
    #[schema(alias = "Autoplay", default = true)]
    pub autoplay: bool,
    /// Enables 2D attenuation with an initial range of 1 to 10 scene units.
    #[schema(alias = "Spatial", default)]
    pub spatial: bool,
}

pub(super) fn hydrate_audio_sources(
    mut commands: Commands,
    mut server: ResMut<AssetServer>,
    sources: Query<
        (Entity, &AudioClipSource, Option<&AudioPlayer>),
        Or<(Changed<AudioClipSource>, Without<AudioPlayer>)>,
    >,
) {
    for (entity, source, player) in &sources {
        let mut entity = commands.entity(entity);
        let clip = match server.load_id(source.clip) {
            Ok(clip) => clip,
            Err(error) => {
                let mut playback = AudioPlayback::default();
                playback.fail(AudioError::ClipLoadFailed(error.to_string()));
                entity
                    .try_remove::<(AudioPlayer, SpatialAudio)>()
                    .try_insert(playback);
                continue;
            }
        };
        let mut player = player.cloned().unwrap_or_else(|| AudioPlayer::new(clip));
        player.clip = clip;
        player.bus = AudioBusId::new(source.bus.clone());
        player.volume = source.volume;
        player.looping = source.looping;
        player.autoplay = source.autoplay;
        entity.try_insert(player);
        if source.spatial {
            entity.try_insert_if_new(SpatialAudio {
                min_distance: 1.0,
                max_distance: 10.0,
            });
        } else {
            entity.try_remove::<SpatialAudio>();
        }
    }
}

fn remove_audio_source(mut world: DeferredWorld<'_>, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .queue_silenced(|mut entity: EntityWorldMut| {
            if !entity.contains::<AudioClipSource>() {
                entity.remove::<(AudioPlayer, SpatialAudio)>();
            }
        });
}
