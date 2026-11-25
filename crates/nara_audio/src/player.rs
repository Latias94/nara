use std::time::Duration;

use bevy_ecs::{
    lifecycle::HookContext,
    prelude::{Component, Entity, EntityEvent, Query, Res, ResMut, With, World},
    system::Commands,
    world::DeferredWorld,
};
use kira::{
    Panning, PlaybackRate, Tween,
    sound::{PlaybackState as KiraPlaybackState, static_sound::StaticSoundHandle},
};
use nara_asset::{Assets, Handle};
use nara_core::Vec2;
use nara_scene::spatial::GlobalTransform2d;

use crate::{
    AudioBusId, AudioClip, AudioError, AudioMixer,
    mixer::{VoiceSettings, decibels, loop_region, tween, validate_volume},
};

/// A clip and the controls for its ECS-owned playback.
///
/// The clip and bus are selected at start. Changing the clip handle replaces the current
/// playback and applies `autoplay` again. Call [`AudioPlayback::play`] to change the route of
/// a sound already playing. Volume, pitch, looping, and spatial controls update during playback.
#[derive(Component, Clone, Debug)]
#[require(AudioPlayback)]
#[component(on_remove = clear_playback)]
pub struct AudioPlayer {
    pub clip: Handle<AudioClip>,
    pub bus: AudioBusId,
    pub volume: f32,
    pub pitch: f32,
    pub looping: bool,
    pub autoplay: bool,
    pub despawn_on_finish: bool,
}

impl AudioPlayer {
    /// Creates an autoplaying, non-looping SFX player.
    #[must_use]
    pub fn new(clip: Handle<AudioClip>) -> Self {
        Self {
            clip,
            bus: AudioBusId::SFX,
            volume: 1.0,
            pitch: 1.0,
            looping: false,
            autoplay: true,
            despawn_on_finish: false,
        }
    }

    /// Plays once and removes the emitter when playback finishes.
    #[must_use]
    pub fn one_shot(clip: Handle<AudioClip>) -> Self {
        Self {
            despawn_on_finish: true,
            ..Self::new(clip)
        }
    }

    fn voice_settings(&self, spatial: SpatialMix) -> Result<VoiceSettings, AudioError> {
        validate_volume(self.volume)?;
        if !(0.25..=4.0).contains(&self.pitch) {
            return Err(AudioError::InvalidPitch);
        }
        Ok(VoiceSettings {
            gain: self.volume * spatial.gain,
            pitch: self.pitch,
            pan: spatial.pan,
            looping: self.looping,
        })
    }
}

fn clear_playback(mut world: DeferredWorld<'_>, context: HookContext) {
    if let Some(mut playback) = world.get_mut::<AudioPlayback>(context.entity) {
        *playback = AudioPlayback::default();
    }
}

/// The world position of a spatial emitter or listener, in scene units.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioPosition2d(pub Vec2);

/// Marks the active 2D listener. If several have finite positions, the lowest entity ID wins.
#[derive(Component, Clone, Copy, Debug, Default)]
#[require(AudioPosition2d)]
pub struct AudioListener;

/// Adds linear distance attenuation and horizontal panning to an [`AudioPlayer`].
///
/// Inside `min_distance` the sound keeps its full gain. At and beyond `max_distance` it is
/// silent. A spatial player stays silent while it has no valid listener or position.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(AudioPosition2d)]
pub struct SpatialAudio {
    pub min_distance: f32,
    pub max_distance: f32,
}

impl SpatialAudio {
    fn mix(self, emitter: Option<Vec2>, listener: Option<Vec2>) -> SpatialMix {
        let (Some(emitter), Some(listener)) = (emitter, listener) else {
            return SpatialMix::SILENT;
        };
        let min = self.min_distance;
        let max = self.max_distance;
        if !emitter.is_finite() || !min.is_finite() || !max.is_finite() || min < 0.0 || max <= min {
            return SpatialMix::SILENT;
        }
        let offset = emitter - listener;
        let distance = offset.x.hypot(offset.y);
        SpatialMix {
            gain: ((max - distance) / (max - min)).clamp(0.0, 1.0),
            pan: (offset.x / max).clamp(-1.0, 1.0),
        }
    }
}

#[derive(Clone, Copy)]
struct SpatialMix {
    gain: f32,
    pan: f32,
}

impl SpatialMix {
    const CENTER: Self = Self {
        gain: 1.0,
        pan: 0.0,
    };
    const SILENT: Self = Self {
        gain: 0.0,
        pan: 0.0,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Idle,
    WaitingForClip,
    Playing,
    Paused,
    Stopped,
    Finished,
    Failed,
}

/// Transport and status for an [`AudioPlayer`], inserted automatically with the player.
///
/// Transport methods set the next update's command. When called several times between updates,
/// the last command wins. `Playing` and `Paused` reflect the requested transport while the
/// audio renderer applies commands at its next processing boundary.
#[derive(Component, Default)]
pub struct AudioPlayback {
    clip: Option<Handle<AudioClip>>,
    voice: Option<PlayerVoice>,
    command: Option<PlaybackCommand>,
    state: PlaybackState,
    error: Option<AudioError>,
}

impl AudioPlayback {
    #[must_use]
    pub fn state(&self) -> PlaybackState {
        self.state
    }

    #[must_use]
    pub fn error(&self) -> Option<&AudioError> {
        self.error.as_ref()
    }

    /// Starts the current clip from the beginning, replacing any current playback.
    pub fn play(&mut self) {
        self.command = Some(PlaybackCommand::Play);
    }

    /// Pauses active playback at its current position.
    pub fn pause(&mut self) {
        self.command = Some(PlaybackCommand::Pause);
    }

    /// Resumes active, paused playback.
    pub fn resume(&mut self) {
        self.command = Some(PlaybackCommand::Resume);
    }

    /// Cancels current playback or a start waiting for its clip.
    pub fn stop(&mut self) {
        self.command = Some(PlaybackCommand::Stop);
    }

    fn reset(&mut self, state: PlaybackState) {
        self.voice = None;
        self.state = state;
        self.error = None;
    }

    fn fail(&mut self, error: AudioError) {
        self.reset(PlaybackState::Failed);
        self.error = Some(error);
    }

    fn update(
        &mut self,
        player: &AudioPlayer,
        spatial: SpatialMix,
        clips: &Assets<AudioClip>,
        mixer: &mut AudioMixer,
    ) -> bool {
        if self.clip != Some(player.clip) {
            self.clip = Some(player.clip);
            self.reset(if player.autoplay {
                PlaybackState::WaitingForClip
            } else {
                PlaybackState::Idle
            });
        }

        let command = self.command.take();
        match command {
            Some(PlaybackCommand::Play) => self.reset(PlaybackState::WaitingForClip),
            Some(PlaybackCommand::Stop) => self.reset(PlaybackState::Stopped),
            _ => (),
        }

        if let Some(voice) = &mut self.voice {
            let native_state = voice.handle.state();
            if native_state == KiraPlaybackState::Stopped {
                self.reset(PlaybackState::Finished);
                return true;
            }
            let settings = match player.voice_settings(spatial) {
                Ok(settings) => settings,
                Err(error) => {
                    self.fail(error);
                    return false;
                }
            };
            self.state = match command {
                Some(PlaybackCommand::Pause) => PlaybackState::Paused,
                Some(PlaybackCommand::Resume) => PlaybackState::Playing,
                _ => self.state,
            };
            voice.update(settings, self.state == PlaybackState::Paused);
        } else if self.state == PlaybackState::WaitingForClip
            && let Some(clip) = clips.get(player.clip)
        {
            let started = player.voice_settings(spatial).and_then(|settings| {
                mixer
                    .start_sound(clip, &player.bus, settings)
                    .map(|handle| PlayerVoice {
                        handle,
                        settings,
                        paused: false,
                    })
            });
            match started {
                Ok(voice) => {
                    self.voice = Some(voice);
                    self.state = PlaybackState::Playing;
                }
                Err(error) => self.fail(error),
            }
        }
        false
    }
}

#[derive(Clone, Copy)]
enum PlaybackCommand {
    Play,
    Pause,
    Resume,
    Stop,
}

struct PlayerVoice {
    handle: StaticSoundHandle,
    settings: VoiceSettings,
    paused: bool,
}

impl PlayerVoice {
    fn update(&mut self, settings: VoiceSettings, paused: bool) {
        if self.paused != paused {
            if paused {
                self.handle.pause(tween(Duration::ZERO));
            } else {
                self.handle.resume(tween(Duration::ZERO));
            }
            self.paused = paused;
        }
        let smooth = Tween::default();
        if settings.gain != self.settings.gain {
            self.handle.set_volume(decibels(settings.gain), smooth);
        }
        if settings.pitch != self.settings.pitch {
            self.handle
                .set_playback_rate(PlaybackRate(f64::from(settings.pitch)), smooth);
        }
        if settings.pan != self.settings.pan {
            self.handle.set_panning(Panning(settings.pan), smooth);
        }
        if settings.looping != self.settings.looping {
            self.handle.set_loop_region(loop_region(settings.looping));
        }
        self.settings = settings;
    }
}

impl Drop for PlayerVoice {
    fn drop(&mut self) {
        if self.handle.state() != KiraPlaybackState::Stopped {
            self.handle.stop(tween(Duration::ZERO));
        }
    }
}

/// Fired when an ECS player's clip reaches its end, before optional despawn.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct AudioFinished {
    pub entity: Entity,
    pub clip: Handle<AudioClip>,
}

/// Observes a one-shot. Dropping it leaves the sound playing on its bus.
pub struct OneShot(StaticSoundHandle);

impl OneShot {
    #[must_use]
    pub fn state(&self) -> PlaybackState {
        if self.0.state() == KiraPlaybackState::Stopped {
            PlaybackState::Finished
        } else {
            PlaybackState::Playing
        }
    }
}

impl AudioMixer {
    /// Starts a non-looping clip immediately at normal volume and pitch.
    pub fn play_one_shot(
        &mut self,
        clip: &AudioClip,
        bus: &AudioBusId,
    ) -> Result<OneShot, AudioError> {
        self.start_sound(
            clip,
            bus,
            VoiceSettings {
                gain: 1.0,
                pitch: 1.0,
                pan: 0.0,
                looping: false,
            },
        )
        .map(OneShot)
    }
}

/// Installs audio resources during world setup. An existing clip store is retained.
pub fn setup_audio(world: &mut World, mixer: AudioMixer) {
    world.init_resource::<Assets<AudioClip>>();
    world.insert_resource(mixer);
}

/// Submits player commands and observes natural completion. Run after scene position updates
/// and any mock audio clock advance.
pub fn update_audio_players(
    mut commands: Commands,
    mut mixer: ResMut<AudioMixer>,
    clips: Res<Assets<AudioClip>>,
    listeners: Query<
        (Entity, Option<&GlobalTransform2d>, Option<&AudioPosition2d>),
        With<AudioListener>,
    >,
    mut players: Query<(
        Entity,
        &AudioPlayer,
        &mut AudioPlayback,
        Option<&SpatialAudio>,
        Option<&GlobalTransform2d>,
        Option<&AudioPosition2d>,
    )>,
) {
    let listener = listeners
        .iter()
        .filter_map(|(entity, global, position)| {
            world_position(global, position).map(|position| (entity, position))
        })
        .min_by_key(|(entity, _)| entity.to_bits())
        .map(|(_, position)| position);

    for (entity, player, mut playback, spatial, global, position) in &mut players {
        let spatial = spatial.map_or(SpatialMix::CENTER, |spatial| {
            spatial.mix(world_position(global, position), listener)
        });
        if playback.update(player, spatial, &clips, &mut mixer) {
            commands.trigger(AudioFinished {
                entity,
                clip: player.clip,
            });
            if player.despawn_on_finish {
                commands.entity(entity).try_despawn();
            }
        }
    }
}

fn world_position(
    global: Option<&GlobalTransform2d>,
    position: Option<&AudioPosition2d>,
) -> Option<Vec2> {
    global
        .map(GlobalTransform2d::translation)
        .or_else(|| position.map(|position| position.0))
        .filter(|position| position.is_finite())
}
