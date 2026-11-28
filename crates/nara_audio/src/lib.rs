//! Game audio for typed clips, ECS players, and a shared mixer.
//!
//! Install [`AudioPlugin`] to run audio after fixed-step spatial propagation. When scheduling
//! audio directly, install an [`AudioMixer`] with [`setup_audio`] and call
//! [`AudioMixer::advance_mock`] before [`update_audio_players`] in headless runs. The device
//! callback owns the playback clock when hardware is active.
//!
//! With [`AssetPlugin`](nara_asset::AssetPlugin), [`AudioPlugin`] registers a loader for source
//! WAV, Ogg Vorbis, and cooked NAUD clips. [`AudioClipSource`] resolves a stable clip ID into
//! an ECS player with the same transport as a player created in code. Spatial audio reads
//! propagated scene transforms and uses [`AudioPosition2d`] as a fallback.

mod backend;
mod clip;
mod mixer;
mod player;
mod plugin;
mod source;

pub use clip::{
    AudioClip, AudioClipError, AudioClipLoader, AudioCodec, NAUD_HEADER_BYTES, cook_audio,
    cooked_audio_codec,
};
pub use mixer::{AudioBusId, AudioBusMeter, AudioBusSettings, AudioError, AudioMixer, AudioOutput};
pub use player::{
    AudioFinished, AudioListener, AudioPlayback, AudioPlayer, AudioPosition2d, OneShot,
    PlaybackState, SpatialAudio, setup_audio, update_audio_players,
};
pub use plugin::{AUDIO_PLUGIN_ID, AudioPlugin};
pub use source::AudioClipSource;
