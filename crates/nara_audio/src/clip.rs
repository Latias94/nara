use std::{fmt, io, path::Path, time::Duration};

use kira::sound::static_sound::StaticSoundData;

/// A decoded WAV (PCM) or Ogg Vorbis asset with shared, immutable samples.
#[derive(Clone)]
pub struct AudioClip(pub(super) StaticSoundData);

impl AudioClip {
    /// Decodes a complete clip from memory.
    pub fn from_bytes(bytes: impl AsRef<[u8]> + Send + Sync) -> Result<Self, AudioClipError> {
        let data = StaticSoundData::from_cursor(io::Cursor::new(bytes))
            .map_err(|error| AudioClipError::Decode(error.to_string()))?;
        if data.sample_rate == 0 || data.frames.is_empty() {
            return Err(AudioClipError::Empty);
        }
        if data
            .frames
            .iter()
            .any(|frame| !frame.left.is_finite() || !frame.right.is_finite())
        {
            return Err(AudioClipError::NonFiniteSamples);
        }
        Ok(Self(data))
    }

    /// Reads and decodes a clip from a local path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, AudioClipError> {
        Self::from_bytes(std::fs::read(path)?)
    }

    #[must_use]
    pub fn duration(&self) -> Duration {
        self.0.duration()
    }
}

impl fmt::Debug for AudioClip {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioClip")
            .field("duration", &self.duration())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AudioClipError {
    #[error("could not read audio clip ({0})")]
    Io(#[from] io::Error),
    #[error("could not decode audio clip ({0})")]
    Decode(String),
    #[error("audio clip contains no playable samples")]
    Empty,
    #[error("audio clip contains non-finite samples")]
    NonFiniteSamples,
}
