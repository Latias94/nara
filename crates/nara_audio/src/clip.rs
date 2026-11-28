use std::{collections::TryReserveError, fmt, io, path::Path, time::Duration};

use kira::{Frame, sound::static_sound::StaticSoundData};
use nara_asset::{AssetLoadContext, AssetLoader, NpakEntryKind, RuntimeAssetMode};

mod decode;
#[cfg(test)]
mod tests;

const MAGIC: &[u8; 4] = b"NAUD";
const VERSION: u16 = 1;
const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;
const MAX_DECODED_FRAMES: usize = MAX_DECODED_BYTES / size_of::<Frame>();

/// Size in bytes of the fixed NAUD v1 header.
pub const NAUD_HEADER_BYTES: usize = 24;

/// The source encoding stored in a cooked audio clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum AudioCodec {
    OggVorbis = 1,
    PcmWav = 2,
}

/// A decoded WAV (PCM) or Ogg Vorbis asset with shared, immutable samples.
#[derive(Clone)]
pub struct AudioClip(pub(super) StaticSoundData);

impl AudioClip {
    /// Decodes a complete clip from memory with up to 128 MiB of stereo PCM frames.
    pub fn from_bytes(bytes: impl AsRef<[u8]> + Send + Sync) -> Result<Self, AudioClipError> {
        Self::decode(bytes.as_ref(), None)
    }

    /// Reads and decodes a clip from a local path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, AudioClipError> {
        Self::from_bytes(std::fs::read(path)?)
    }

    /// Decodes a NAUD v1 clip and checks its header against the payload and decoded frames.
    pub fn from_cooked(bytes: &[u8]) -> Result<Self, AudioClipError> {
        let (header, payload) = parse_cooked_header(bytes)?;
        Self::decode(payload, Some(&header))
    }

    fn decode(bytes: &[u8], header: Option<&AudioMetadata>) -> Result<Self, AudioClipError> {
        let mut frames = Vec::new();
        let metadata = decode::source(bytes, header, Some(&mut frames))?;
        Ok(Self(StaticSoundData {
            sample_rate: metadata.sample_rate,
            // Kira needs an Arc slice. The conversion can briefly hold two bounded PCM buffers.
            frames: frames.into(),
            settings: Default::default(),
            slice: None,
        }))
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
    #[error("invalid cooked audio clip ({0})")]
    InvalidCooked(&'static str),
    #[error("could not allocate audio clip data ({0})")]
    Capacity(#[from] TryReserveError),
    #[error("audio clip contains no playable samples")]
    Empty,
    #[error("audio clip contains non-finite samples")]
    NonFiniteSamples,
}

/// Fully decodes and validates a source clip, then prefixes its original bytes with a NAUD v1
/// header. The header stores the magic, version (`u16`), codec (`u16`), sample rate (`u32`),
/// channels (`u16`), zero flags (`u16`), and frame count (`u64`) in little-endian order.
pub fn cook_audio(bytes: &[u8]) -> Result<Vec<u8>, AudioClipError> {
    let metadata = decode::source(bytes, None, None)?;
    let mut cooked = Vec::new();
    cooked.try_reserve_exact(NAUD_HEADER_BYTES + bytes.len())?;
    cooked.extend_from_slice(MAGIC);
    cooked.extend_from_slice(&VERSION.to_le_bytes());
    cooked.extend_from_slice(&(metadata.codec as u16).to_le_bytes());
    cooked.extend_from_slice(&metadata.sample_rate.to_le_bytes());
    cooked.extend_from_slice(&metadata.channels.to_le_bytes());
    cooked.extend_from_slice(&0_u16.to_le_bytes());
    cooked.extend_from_slice(&metadata.frame_count.to_le_bytes());
    cooked.extend_from_slice(bytes);
    Ok(cooked)
}

/// Reads the codec from a validated NAUD v1 header without inspecting or decoding the payload.
pub fn cooked_audio_codec(bytes: &[u8]) -> Result<AudioCodec, AudioClipError> {
    parse_cooked_header(bytes).map(|(header, _)| header.codec)
}

struct AudioMetadata {
    codec: AudioCodec,
    sample_rate: u32,
    channels: u16,
    frame_count: u64,
}

fn parse_cooked_header(bytes: &[u8]) -> Result<(AudioMetadata, &[u8]), AudioClipError> {
    let (header, payload) = bytes
        .split_first_chunk::<NAUD_HEADER_BYTES>()
        .ok_or(AudioClipError::InvalidCooked("truncated header"))?;
    if &header[..4] != MAGIC {
        return Err(AudioClipError::InvalidCooked("incorrect magic"));
    }
    if u16::from_le_bytes([header[4], header[5]]) != VERSION {
        return Err(AudioClipError::InvalidCooked("unsupported version"));
    }
    let codec = match u16::from_le_bytes([header[6], header[7]]) {
        1 => AudioCodec::OggVorbis,
        2 => AudioCodec::PcmWav,
        _ => return Err(AudioClipError::InvalidCooked("unknown codec")),
    };
    let sample_rate =
        u32::from_le_bytes(header[8..12].try_into().expect("fixed sample rate field"));
    let channels = u16::from_le_bytes([header[12], header[13]]);
    let frame_count =
        u64::from_le_bytes(header[16..24].try_into().expect("fixed frame count field"));
    if sample_rate == 0 || !matches!(channels, 1 | 2) || frame_count == 0 {
        return Err(AudioClipError::InvalidCooked("invalid sample metadata"));
    }
    if frame_count > MAX_DECODED_FRAMES as u64 {
        return Err(AudioClipError::InvalidCooked(
            "frame count exceeds the decoded PCM limit",
        ));
    }
    if header[14..16] != [0, 0] {
        return Err(AudioClipError::InvalidCooked("unknown flags"));
    }
    Ok((
        AudioMetadata {
            codec,
            sample_rate,
            channels,
            frame_count,
        },
        payload,
    ))
}

/// Loads source clips and NAUD v1 assets through the runtime asset server.
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioClipLoader;

impl AssetLoader for AudioClipLoader {
    type Asset = AudioClip;
    type Error = AudioClipError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Audio)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<AudioClip, AudioClipError> {
        match context.mode() {
            RuntimeAssetMode::Source => AudioClip::from_bytes(bytes),
            RuntimeAssetMode::Cooked => AudioClip::from_cooked(bytes),
        }
    }
}
