use std::{
    borrow::Cow,
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bevy_ecs::prelude::Resource;
use kira::{
    Decibels, Frame, Panning, PlaySoundError, PlaybackRate, Tween,
    effect::{
        Effect, EffectBuilder,
        volume_control::{VolumeControlBuilder, VolumeControlHandle},
    },
    info::Info,
    sound::{
        Region,
        static_sound::{StaticSoundHandle, StaticSoundSettings},
    },
    track::{MainTrackBuilder, TrackBuilder, TrackHandle},
};

use crate::{AudioClip, backend::OutputBackend};

/// The name of a mixer bus. Gameplay sounds default to [`Self::SFX`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AudioBusId(Cow<'static, str>);

impl AudioBusId {
    pub const MASTER: Self = Self(Cow::Borrowed("master"));
    pub const SFX: Self = Self(Cow::Borrowed("sfx"));
    pub const BGM: Self = Self(Cow::Borrowed("bgm"));
    pub const VOICE: Self = Self(Cow::Borrowed("voice"));

    #[must_use]
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AudioBusId {
    fn default() -> Self {
        Self::SFX
    }
}

impl fmt::Display for AudioBusId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioOutput {
    Device,
    Mock,
}

/// The target controls of a bus. `volume` is linear gain in `0.0..=2.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioBusSettings {
    pub volume: f32,
    pub muted: bool,
    pub soloed: bool,
}

/// Peak linear amplitudes from the most recent rendered block, after the bus's own gain.
///
/// Levels can exceed `1.0` before device clipping. A child bus is measured before master gain.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioBusMeter {
    pub peak_left: f32,
    pub peak_right: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("could not load audio clip ({0})")]
    ClipLoadFailed(String),
    #[error("audio bus '{0}' is not registered")]
    UnknownBus(AudioBusId),
    #[error("audio bus '{0}' already exists")]
    DuplicateBus(AudioBusId),
    #[error("audio bus names must not be empty")]
    EmptyBusName,
    #[error("the master bus controls the final output only")]
    MasterIsOutputOnly,
    #[error("the audio mixer has reached its bus capacity")]
    BusCapacity,
    #[error("audio bus '{0}' has reached its sound capacity")]
    SoundCapacity(AudioBusId),
    #[error("could not initialize a sound on audio bus '{0}'")]
    SoundInitialization(AudioBusId),
    #[error("volume must be finite and between 0.0 and 2.0")]
    InvalidVolume,
    #[error("pitch must be finite and between 0.25 and 4.0")]
    InvalidPitch,
}

/// Owns audio output and routes sounds through the SFX, BGM, voice, and custom buses.
#[derive(Resource)]
pub struct AudioMixer {
    backend: OutputBackend,
    master: BusMix,
    buses: BTreeMap<AudioBusId, Bus>,
    device_error: Option<String>,
}

impl AudioMixer {
    /// Opens the default desktop output when the `device` feature is enabled.
    /// A device startup failure leaves a working [`AudioOutput::Mock`] mixer. After a
    /// successful start, Kira's desktop backend owns device reconnection.
    pub fn new() -> Result<Self, AudioError> {
        #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
        {
            Self::open_device(OutputBackend::device)
        }
        #[cfg(not(all(feature = "device", not(target_arch = "wasm32"))))]
        Self::mock()
    }

    #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
    fn open_device(
        open: impl FnOnce(MainTrackBuilder) -> Result<OutputBackend, String>,
    ) -> Result<Self, AudioError> {
        let (builder, master) = Self::main_track();
        match open(builder) {
            Ok(backend) => Self::with_backend(backend, master),
            Err(error) => {
                let mut mixer = Self::mock()?;
                mixer.device_error = Some(error);
                Ok(mixer)
            }
        }
    }

    /// Creates a silent mixer driven only by [`Self::advance_mock`].
    pub fn mock() -> Result<Self, AudioError> {
        let (builder, master) = Self::main_track();
        Self::with_backend(OutputBackend::mock(builder), master)
    }

    fn main_track() -> (MainTrackBuilder, BusMix) {
        let mut builder = MainTrackBuilder::new();
        let mix = BusMix::new(true, |effect| builder.add_built_effect(effect));
        (builder, mix)
    }

    fn with_backend(backend: OutputBackend, master: BusMix) -> Result<Self, AudioError> {
        let mut mixer = Self {
            backend,
            master,
            buses: BTreeMap::new(),
            device_error: None,
        };
        for bus in [AudioBusId::SFX, AudioBusId::BGM, AudioBusId::VOICE] {
            mixer.add_bus(bus)?;
        }
        Ok(mixer)
    }

    #[must_use]
    pub fn output(&self) -> AudioOutput {
        self.backend.output()
    }

    /// Returns the startup diagnostic if opening the device fell back to the mock backend.
    #[must_use]
    pub fn device_error(&self) -> Option<&str> {
        self.device_error.as_deref()
    }

    /// Renders and discards elapsed mock audio using a fixed sample rate. Device output owns
    /// its clock and ignores this call. Duration shorter than one sample carries into the next call.
    pub fn advance_mock(&mut self, elapsed: Duration) {
        self.backend.advance_mock(elapsed);
    }

    /// Adds a named bus routed to master.
    pub fn add_bus(&mut self, id: AudioBusId) -> Result<(), AudioError> {
        if id.as_str().is_empty() {
            return Err(AudioError::EmptyBusName);
        }
        if id == AudioBusId::MASTER || self.buses.contains_key(&id) {
            return Err(AudioError::DuplicateBus(id));
        }

        let mut builder = TrackBuilder::new();
        let mix = BusMix::new(!self.any_soloed(), |effect| {
            builder.add_built_effect(effect)
        });
        let track = self
            .backend
            .add_track(builder)
            .map_err(|_| AudioError::BusCapacity)?;
        self.buses.insert(id, Bus { track, mix });
        Ok(())
    }

    #[must_use]
    pub fn bus_settings(&self, id: &AudioBusId) -> Option<AudioBusSettings> {
        if *id == AudioBusId::MASTER {
            Some(self.master.settings)
        } else {
            self.buses.get(id).map(|bus| bus.mix.settings)
        }
    }

    pub fn set_bus_volume(&mut self, id: &AudioBusId, volume: f32) -> Result<(), AudioError> {
        self.fade_bus_volume(id, volume, Duration::ZERO)
    }

    /// Fades to the target volume on the audio clock. Muted or excluded buses stay silent and
    /// retain the target volume for when they become audible.
    pub fn fade_bus_volume(
        &mut self,
        id: &AudioBusId,
        volume: f32,
        duration: Duration,
    ) -> Result<(), AudioError> {
        validate_volume(volume)?;
        let (mix, enabled) = self.mix_mut(id)?;
        mix.settings.volume = volume;
        mix.apply_gain(enabled, Some(duration));
        Ok(())
    }

    pub fn set_bus_muted(&mut self, id: &AudioBusId, muted: bool) -> Result<(), AudioError> {
        let (mix, enabled) = self.mix_mut(id)?;
        mix.settings.muted = muted;
        mix.apply_gain(enabled, None);
        Ok(())
    }

    /// Auditions one or more child buses together. Mute still takes precedence on each bus.
    pub fn set_bus_soloed(&mut self, id: &AudioBusId, soloed: bool) -> Result<(), AudioError> {
        if *id == AudioBusId::MASTER {
            return Err(AudioError::MasterIsOutputOnly);
        }
        let bus = self
            .buses
            .get_mut(id)
            .ok_or_else(|| AudioError::UnknownBus(id.clone()))?;
        bus.mix.settings.soloed = soloed;
        let any_soloed = self.any_soloed();
        for bus in self.buses.values_mut() {
            bus.mix
                .apply_gain(!any_soloed || bus.mix.settings.soloed, None);
        }
        Ok(())
    }

    #[must_use]
    pub fn meter_snapshot(&self) -> BTreeMap<AudioBusId, AudioBusMeter> {
        std::iter::once((AudioBusId::MASTER, self.master.meter.read()))
            .chain(
                self.buses
                    .iter()
                    .map(|(id, bus)| (id.clone(), bus.mix.meter.read())),
            )
            .collect()
    }

    fn any_soloed(&self) -> bool {
        self.buses.values().any(|bus| bus.mix.settings.soloed)
    }

    fn mix_mut(&mut self, id: &AudioBusId) -> Result<(&mut BusMix, bool), AudioError> {
        if *id == AudioBusId::MASTER {
            return Ok((&mut self.master, true));
        }
        let any_soloed = self.any_soloed();
        let bus = self
            .buses
            .get_mut(id)
            .ok_or_else(|| AudioError::UnknownBus(id.clone()))?;
        let enabled = !any_soloed || bus.mix.settings.soloed;
        Ok((&mut bus.mix, enabled))
    }

    pub(super) fn start_sound(
        &mut self,
        clip: &AudioClip,
        id: &AudioBusId,
        settings: VoiceSettings,
    ) -> Result<StaticSoundHandle, AudioError> {
        if *id == AudioBusId::MASTER {
            return Err(AudioError::MasterIsOutputOnly);
        }
        let bus = self
            .buses
            .get_mut(id)
            .ok_or_else(|| AudioError::UnknownBus(id.clone()))?;
        let sound = clip.0.with_settings(
            StaticSoundSettings::new()
                .volume(decibels(settings.gain))
                .playback_rate(PlaybackRate(f64::from(settings.pitch)))
                .panning(Panning(settings.pan))
                .loop_region(loop_region(settings.looping)),
        );
        bus.track.play(sound).map_err(|error| match error {
            PlaySoundError::SoundLimitReached => AudioError::SoundCapacity(id.clone()),
            PlaySoundError::IntoSoundError(()) => AudioError::SoundInitialization(id.clone()),
        })
    }
}

struct Bus {
    track: TrackHandle,
    mix: BusMix,
}

struct BusMix {
    volume: VolumeControlHandle,
    meter: PeakMeter,
    settings: AudioBusSettings,
    audible: bool,
}

impl BusMix {
    fn new(audible: bool, mut add_effect: impl FnMut(Box<dyn Effect>)) -> Self {
        let initial_gain = if audible { 1.0 } else { 0.0 };
        let (effect, volume) = VolumeControlBuilder::new(decibels(initial_gain)).build();
        let meter = PeakMeter::default();
        add_effect(effect);
        add_effect(Box::new(meter.clone()));
        Self {
            volume,
            meter,
            settings: AudioBusSettings {
                volume: 1.0,
                muted: false,
                soloed: false,
            },
            audible,
        }
    }

    fn apply_gain(&mut self, enabled: bool, fade: Option<Duration>) {
        let audible = enabled && !self.settings.muted;
        if self.audible != audible || (audible && fade.is_some()) {
            let target = if audible { self.settings.volume } else { 0.0 };
            self.volume
                .set_volume(decibels(target), tween(fade.unwrap_or_default()));
            self.audible = audible;
        }
    }
}

#[derive(Clone, Default)]
struct PeakMeter(Arc<AtomicU64>);

impl PeakMeter {
    fn read(&self) -> AudioBusMeter {
        let peaks = self.0.load(Ordering::Relaxed);
        AudioBusMeter {
            peak_left: f32::from_bits(peaks as u32),
            peak_right: f32::from_bits((peaks >> 32) as u32),
        }
    }
}

impl Effect for PeakMeter {
    fn process(&mut self, frames: &mut [Frame], _: f64, _: &Info) {
        let (left, right) = frames
            .iter()
            .fold((0.0_f32, 0.0_f32), |(left, right), frame| {
                (left.max(frame.left.abs()), right.max(frame.right.abs()))
            });
        self.0.store(
            u64::from(left.to_bits()) | (u64::from(right.to_bits()) << 32),
            Ordering::Relaxed,
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct VoiceSettings {
    pub(super) gain: f32,
    pub(super) pitch: f32,
    pub(super) pan: f32,
    pub(super) looping: bool,
}

pub(super) fn validate_volume(volume: f32) -> Result<(), AudioError> {
    if (0.0..=2.0).contains(&volume) {
        Ok(())
    } else {
        Err(AudioError::InvalidVolume)
    }
}

pub(super) fn decibels(volume: f32) -> Decibels {
    if volume > 0.0 {
        Decibels(20.0 * volume.log10())
    } else {
        Decibels::SILENCE
    }
}

pub(super) fn loop_region(looping: bool) -> Option<Region> {
    looping.then(|| Region::from(..))
}

pub(super) fn tween(duration: Duration) -> Tween {
    Tween {
        duration,
        ..Default::default()
    }
}

#[cfg(all(test, feature = "device", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn device_startup_failure_keeps_a_usable_mock_mixer() {
        let mut mixer = AudioMixer::open_device(|_| Err(String::from("no output device"))).unwrap();

        assert_eq!(mixer.output(), AudioOutput::Mock);
        assert_eq!(mixer.device_error(), Some("no output device"));
        mixer.set_bus_volume(&AudioBusId::SFX, 0.5).unwrap();
        mixer.advance_mock(Duration::from_millis(16));
        assert_eq!(mixer.bus_settings(&AudioBusId::SFX).unwrap().volume, 0.5);
    }
}
