//! Game audio for typed clips, ECS players, and a shared mixer.
//!
//! Install an [`AudioMixer`] with [`setup_audio`] and add [`update_audio_players`] to the audio
//! update schedule. For headless runs, call [`AudioMixer::advance_mock`] with the elapsed time
//! before that system. On a device the audio callback owns the playback clock.
//!
//! Decode WAV or Ogg Vorbis data with [`AudioClip`] and publish it in
//! [`Assets<AudioClip>`](nara_asset::Assets). Spatial audio reads propagated scene transforms and
//! uses [`AudioPosition2d`] on emitters or listeners with no scene transform.
//!
//! Clips are fully decoded in memory and published by the caller. Scene-schema and asset-loader
//! registration belong to the scene and asset integrations.

mod backend;
mod clip;
mod mixer;
mod player;

pub use clip::{AudioClip, AudioClipError};
pub use mixer::{AudioBusId, AudioBusMeter, AudioBusSettings, AudioError, AudioMixer, AudioOutput};
pub use player::{
    AudioFinished, AudioListener, AudioPlayback, AudioPlayer, AudioPosition2d, OneShot,
    PlaybackState, SpatialAudio, setup_audio, update_audio_players,
};
