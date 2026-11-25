//! Game audio for typed clips, ECS players, and a shared mixer.
//!
//! Install [`AudioPlugin`] to run audio after fixed-step spatial propagation. When scheduling
//! audio directly, install an [`AudioMixer`] with [`setup_audio`] and call
//! [`AudioMixer::advance_mock`] before [`update_audio_players`] in headless runs. The device
//! callback owns the playback clock when hardware is active.
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
mod plugin;

pub use clip::{AudioClip, AudioClipError};
pub use mixer::{AudioBusId, AudioBusMeter, AudioBusSettings, AudioError, AudioMixer, AudioOutput};
pub use player::{
    AudioFinished, AudioListener, AudioPlayback, AudioPlayer, AudioPosition2d, OneShot,
    PlaybackState, SpatialAudio, setup_audio, update_audio_players,
};
pub use plugin::{AUDIO_PLUGIN_ID, AudioPlugin};
