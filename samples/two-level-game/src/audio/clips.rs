//! Preparation of the game's two immutable one-shots, before creating a device callback.

use std::{error::Error, fmt, io::Cursor, sync::Arc};

pub(super) const SAMPLE_RATE: u32 = 48_000;
pub(super) const MAX_CLIP_FRAMES: usize = SAMPLE_RATE as usize;
pub(super) const PICKUP_WAV: &[u8] = include_bytes!("../../assets/audio/pickup.wav");
pub(super) const TRAVEL_WAV: &[u8] = include_bytes!("../../assets/audio/travel.wav");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClipKind {
    Pickup,
    Travel,
}

/// Mono samples on the fixed 48 kHz output clock. Construction may allocate; playback does not.
#[derive(Clone, Debug)]
pub(super) struct PreparedClips {
    pickup: Arc<[f32]>,
    travel: Arc<[f32]>,
}

impl PreparedClips {
    pub(super) fn embedded() -> Result<Self, ClipError> {
        Self::decode(PICKUP_WAV, TRAVEL_WAV)
    }

    /// Accept only mono, 48 kHz, signed PCM16 WAVs. There is no implicit format conversion.
    pub(super) fn decode(pickup: &[u8], travel: &[u8]) -> Result<Self, ClipError> {
        Self::new(
            decode_wav(ClipKind::Pickup, pickup)?,
            decode_wav(ClipKind::Travel, travel)?,
        )
    }

    /// Samples supplied directly must already use the fixed sample clock and normalized range.
    pub(super) fn new(pickup: Arc<[f32]>, travel: Arc<[f32]>) -> Result<Self, ClipError> {
        validate_samples(ClipKind::Pickup, &pickup)?;
        validate_samples(ClipKind::Travel, &travel)?;
        Ok(Self { pickup, travel })
    }

    pub(super) fn samples(&self, clip: ClipKind) -> &[f32] {
        match clip {
            ClipKind::Pickup => &self.pickup,
            ClipKind::Travel => &self.travel,
        }
    }
}

#[derive(Debug)]
pub(super) enum ClipError {
    Decode {
        clip: ClipKind,
        source: hound::Error,
    },
    UnsupportedFormat {
        clip: ClipKind,
        spec: hound::WavSpec,
    },
    Empty {
        clip: ClipKind,
    },
    TooLong {
        clip: ClipKind,
        frames: usize,
    },
    InvalidSample {
        clip: ClipKind,
        frame: usize,
    },
}

impl fmt::Display for ClipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode { clip, source } => write!(f, "cannot decode {clip:?} WAV: {source}"),
            Self::UnsupportedFormat { clip, spec } => write!(
                f,
                "{clip:?} requires mono 48000 Hz PCM16 WAV, received {spec:?}"
            ),
            Self::Empty { clip } => write!(f, "{clip:?} must contain at least one sample"),
            Self::TooLong { clip, frames } => {
                write!(
                    f,
                    "{clip:?} has {frames} frames; limit is {MAX_CLIP_FRAMES}"
                )
            }
            Self::InvalidSample { clip, frame } => {
                write!(
                    f,
                    "{clip:?} sample {frame} must be finite and within [-1, 1]"
                )
            }
        }
    }
}

impl Error for ClipError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn decode_wav(clip: ClipKind, bytes: &[u8]) -> Result<Arc<[f32]>, ClipError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|source| ClipError::Decode { clip, source })?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != SAMPLE_RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(ClipError::UnsupportedFormat { clip, spec });
    }
    let frames = reader.duration() as usize;
    if frames == 0 {
        return Err(ClipError::Empty { clip });
    }
    if frames > MAX_CLIP_FRAMES {
        return Err(ClipError::TooLong { clip, frames });
    }
    let mut samples = Vec::with_capacity(frames);
    for sample in reader.samples::<i16>() {
        let sample = sample.map_err(|source| ClipError::Decode { clip, source })?;
        samples.push(f32::from(sample) / 32_768.0);
    }
    Ok(samples.into())
}

fn validate_samples(clip: ClipKind, samples: &[f32]) -> Result<(), ClipError> {
    if samples.is_empty() {
        return Err(ClipError::Empty { clip });
    }
    if samples.len() > MAX_CLIP_FRAMES {
        return Err(ClipError::TooLong {
            clip,
            frames: samples.len(),
        });
    }
    if let Some(frame) = samples
        .iter()
        .position(|sample| !sample.is_finite() || !(-1.0..=1.0).contains(sample))
    {
        return Err(ClipError::InvalidSample { clip, frame });
    }
    Ok(())
}

#[cfg(test)]
#[path = "clip_tests.rs"]
mod tests;
