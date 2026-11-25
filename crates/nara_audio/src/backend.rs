use std::{convert::Infallible, sync::Mutex, time::Duration};

#[cfg(all(feature = "device", not(target_arch = "wasm32")))]
use kira::backend::cpal::CpalBackend;
use kira::{
    AudioManager, AudioManagerSettings, ResourceLimitReached,
    backend::{Backend as KiraBackend, Renderer},
    track::{MainTrackBuilder, TrackBuilder, TrackHandle},
};

use crate::AudioOutput;

pub(super) enum OutputBackend {
    #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
    Device(AudioManager<CpalBackend>),
    Mock(AudioManager<MockBackend>),
}

impl OutputBackend {
    #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
    pub(super) fn device(main_track_builder: MainTrackBuilder) -> Result<Self, String> {
        AudioManager::new(AudioManagerSettings {
            main_track_builder,
            ..Default::default()
        })
        .map(Self::Device)
        .map_err(|error| error.to_string())
    }

    pub(super) fn mock(main_track_builder: MainTrackBuilder) -> Self {
        match AudioManager::new(AudioManagerSettings {
            main_track_builder,
            ..Default::default()
        }) {
            Ok(manager) => Self::Mock(manager),
            Err(never) => match never {},
        }
    }

    pub(super) fn output(&self) -> AudioOutput {
        match self {
            #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
            Self::Device(_) => AudioOutput::Device,
            Self::Mock(_) => AudioOutput::Mock,
        }
    }

    pub(super) fn add_track(
        &mut self,
        builder: TrackBuilder,
    ) -> Result<TrackHandle, ResourceLimitReached> {
        match self {
            #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
            Self::Device(manager) => manager.add_sub_track(builder),
            Self::Mock(manager) => manager.add_sub_track(builder),
        }
    }

    pub(super) fn advance_mock(&mut self, elapsed: Duration) {
        match self {
            #[cfg(all(feature = "device", not(target_arch = "wasm32")))]
            Self::Device(_) => (),
            Self::Mock(manager) => manager.backend_mut().advance(elapsed),
        }
    }
}

const MOCK_SAMPLE_RATE: u32 = 48_000;
const BLOCK_FRAMES: usize = 128;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Default)]
pub(super) struct MockBackend {
    renderer: Option<Mutex<Renderer>>,
    frame_remainder: u128,
}

impl MockBackend {
    fn advance(&mut self, elapsed: Duration) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let renderer = renderer
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sample_nanos = elapsed.as_nanos() * u128::from(MOCK_SAMPLE_RATE) + self.frame_remainder;
        let mut remaining_frames = sample_nanos / NANOS_PER_SECOND;
        self.frame_remainder = sample_nanos % NANOS_PER_SECOND;

        let mut samples = [0.0; BLOCK_FRAMES * 2];
        renderer.on_start_processing();
        while remaining_frames != 0 {
            let count = remaining_frames.min(BLOCK_FRAMES as u128) as usize;
            renderer.process(&mut samples[..count * 2], 2);
            remaining_frames -= count as u128;
        }
    }
}

impl KiraBackend for MockBackend {
    type Settings = ();
    type Error = Infallible;

    fn setup(_: Self::Settings, _: usize) -> Result<(Self, u32), Self::Error> {
        Ok((Self::default(), MOCK_SAMPLE_RATE))
    }

    fn start(&mut self, renderer: Renderer) -> Result<(), Self::Error> {
        self.renderer = Some(Mutex::new(renderer));
        Ok(())
    }
}
