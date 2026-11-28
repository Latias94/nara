use bevy_ecs::{
    change_detection::{DetectChanges, Ref},
    lifecycle::HookContext,
    prelude::{Component, Entity, Query, Res, ResMut},
    system::Commands,
    world::{DeferredWorld, EntityWorldMut},
};
use nara_asset::{AssetServer, AssetStates, Assets, Handle, LoadState};
use nara_core::AssetId;
use nara_reflect::Schema;

use crate::{
    AudioBusId, AudioClip, AudioError, AudioPlayback, AudioPlayer, PlaybackState, SpatialAudio,
    mixer::validate_volume,
};

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

#[derive(Component)]
pub(super) struct AudioClipRequest {
    id: AssetId,
    handle: Option<Handle<AudioClip>>,
    source_generation: u64,
}

pub(super) fn hydrate_audio_sources(
    mut commands: Commands,
    mut server: ResMut<AssetServer>,
    clips: Res<Assets<AudioClip>>,
    states: Res<AssetStates>,
    mut sources: Query<(
        Entity,
        Ref<AudioClipSource>,
        &AudioPlayback,
        Option<&AudioPlayer>,
        Option<&mut AudioClipRequest>,
    )>,
) {
    let source_generation = server.runtime_source_generation();
    for (entity, source, playback, player, request) in &mut sources {
        let mut cached = request.filter(|request| request.id == source.clip);
        let previous = cached.as_ref().and_then(|request| request.handle);
        let remounted = cached
            .as_ref()
            .is_some_and(|request| request.source_generation != source_generation);
        let missing = previous.is_none_or(|clip| clips.get(clip).is_none());
        if remounted
            && !missing
            && let Some(cached) = &mut cached
        {
            cached.source_generation = source_generation;
        }
        let retired = missing
            && (source.is_changed() || player.is_none() || playback.needs_clip())
            && previous.is_some_and(|clip| {
                states.state(clip.id()).is_some_and(|state| {
                    matches!(state.load_state(), LoadState::Removed | LoadState::Loaded)
                })
            });
        let clip = if cached.is_none() || missing && (remounted || retired) {
            let request = server.load_id(source.clip);
            let mut entity = commands.entity(entity);
            entity.try_insert(AudioClipRequest {
                id: source.clip,
                handle: request.as_ref().ok().copied(),
                source_generation,
            });
            match request {
                Ok(clip) => clip,
                Err(error) => {
                    let same_clip = cached.is_some();
                    entity.queue_silenced(move |mut entity: EntityWorldMut| {
                        // Preserve transport state across the removal hook for the same clip.
                        let mut playback = entity
                            .get_mut::<AudioPlayback>()
                            .map(|mut playback| std::mem::take(&mut *playback))
                            .unwrap_or_default();
                        entity.remove::<(AudioPlayer, SpatialAudio)>();
                        if !same_clip {
                            playback
                                .fail_clip_switch(AudioError::ClipLoadFailed(error.to_string()));
                        } else if playback.state() != PlaybackState::Stopped {
                            playback.fail(AudioError::ClipLoadFailed(error.to_string()));
                        }
                        entity.insert(playback);
                    });
                    continue;
                }
            }
        } else if let Some(clip) = previous {
            clip
        } else {
            continue;
        };

        if player.is_some() && !source.is_changed() && previous == Some(clip) {
            continue;
        }
        let mut entity = commands.entity(entity);
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

pub(super) fn restart_recovered_audio_sources(
    clips: Res<Assets<AudioClip>>,
    mut sources: Query<(
        Ref<AudioClipSource>,
        &AudioClipRequest,
        &AudioPlayer,
        &mut AudioPlayback,
    )>,
) {
    for (source, request, player, mut playback) in &mut sources {
        if !source.autoplay || request.id != source.clip || request.handle != Some(player.clip) {
            continue;
        }
        let settings_changed = source.is_changed();
        let retry = match playback.error() {
            Some(AudioError::ClipLoadFailed(_)) => clips.get(player.clip).is_some(),
            Some(
                AudioError::UnknownBus(bus)
                | AudioError::SoundCapacity(bus)
                | AudioError::SoundInitialization(bus),
            ) => settings_changed && player.bus != *bus,
            Some(AudioError::MasterIsOutputOnly) => {
                settings_changed && player.bus != AudioBusId::MASTER
            }
            Some(AudioError::InvalidVolume) => {
                settings_changed && validate_volume(player.volume).is_ok()
            }
            _ => false,
        };
        if retry {
            playback.retry_after_failure();
        }
    }
}

fn remove_audio_source(mut world: DeferredWorld<'_>, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .queue_silenced(|mut entity: EntityWorldMut| {
            if !entity.contains::<AudioClipSource>() {
                entity.remove::<(AudioPlayer, SpatialAudio, AudioClipRequest)>();
            }
        });
}
