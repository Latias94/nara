use std::{collections::TryReserveError, time::Duration};

use nara_asset::{AssetLoadContext, AssetLoader, NpakEntryKind, RuntimeAssetMode};
use nara_core::Vec2;
use thiserror::Error;

use super::{
    Clip, ClipError, Frame, MAX_CLIP_NAME_BYTES, MAX_CLIPS, MAX_FRAME_MARKER_BYTES,
    MAX_FRAMES_PER_CLIP, MAX_TOTAL_FRAMES, PlaybackMode, SpriteAnimationSet,
};
use crate::sprite::TextureRegion;

/// Maximum byte length of a `.sanim.ron` document.
pub const MAX_SPRITE_ANIMATION_SOURCE_BYTES: usize = 8 * 1024 * 1024;

const MAGIC: &[u8; 4] = b"NSAN";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const CLIP_HEADER_BYTES: usize = 4;
const FRAME_BYTES: usize = 25;

impl SpriteAnimationSet {
    /// Encodes deterministic NSAN v1 bytes with clips in ascending name order.
    ///
    /// The header is `NSAN`, version `u16`, and clip count `u16`. Each clip contains a
    /// length-prefixed name, a mode byte (0 loop, 1 once, 2 ping-pong), and a `u16` frame
    /// count. Each frame contains four `f32` values (UV min x/y and size x/y), a `u64`
    /// duration in nanoseconds, and a length-prefixed marker. Text uses a `u8` byte length
    /// followed by UTF-8 bytes. A zero marker length represents an absent marker. Numbers
    /// use little-endian encoding.
    pub fn encode_cooked(&self) -> Result<Vec<u8>, SpriteAnimationAssetError> {
        let clip_bytes = self
            .clips()
            .map(|(name, clip)| {
                CLIP_HEADER_BYTES
                    + name.len()
                    + clip
                        .frames()
                        .iter()
                        .map(|frame| FRAME_BYTES + frame.event().map_or(0, str::len))
                        .sum::<usize>()
            })
            .sum::<usize>();
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(HEADER_BYTES + clip_bytes)?;
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(self.clips().len() as u16).to_le_bytes());
        for (name, clip) in self.clips() {
            write_text(&mut bytes, name);
            bytes.push(match clip.mode() {
                PlaybackMode::Loop => 0,
                PlaybackMode::Once => 1,
                PlaybackMode::PingPong => 2,
            });
            bytes.extend_from_slice(&(clip.frames().len() as u16).to_le_bytes());
            for frame in clip.frames() {
                let region = frame.region();
                for value in [region.min.x, region.min.y, region.size.x, region.size.y] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                bytes.extend_from_slice(&(frame.duration().as_nanos() as u64).to_le_bytes());
                write_text(&mut bytes, frame.event().unwrap_or_default());
            }
        }
        Ok(bytes)
    }

    /// Decodes NSAN v1 with bounded record counts and validates the complete payload.
    pub fn from_cooked(mut bytes: &[u8]) -> Result<Self, SpriteAnimationAssetError> {
        if read_field::<4>(&mut bytes)? != *MAGIC {
            return Err(SpriteAnimationAssetError::InvalidCooked("incorrect magic"));
        }
        let version = u16::from_le_bytes(read_field(&mut bytes)?);
        if version != VERSION {
            return Err(SpriteAnimationAssetError::UnsupportedVersion(version));
        }
        let count = usize::from(u16::from_le_bytes(read_field(&mut bytes)?));
        if count > MAX_CLIPS {
            return Err(ClipError::TooManyClips.into());
        }

        let mut clips = Vec::with_capacity(count);
        let mut total_frames = 0;
        let mut previous_name = None;
        for _ in 0..count {
            let name = read_text(&mut bytes, MAX_CLIP_NAME_BYTES)?;
            if previous_name.is_some_and(|previous| previous >= name) {
                return Err(SpriteAnimationAssetError::InvalidCooked(
                    "clip names must be strictly ascending",
                ));
            }
            previous_name = Some(name);
            let mode = match read_field::<1>(&mut bytes)? {
                [0] => PlaybackMode::Loop,
                [1] => PlaybackMode::Once,
                [2] => PlaybackMode::PingPong,
                [_] => return Err(SpriteAnimationAssetError::InvalidCooked("unknown mode")),
            };
            let frame_count = usize::from(u16::from_le_bytes(read_field(&mut bytes)?));
            if frame_count > MAX_FRAMES_PER_CLIP {
                return Err(ClipError::TooManyFrames.into());
            }
            total_frames += frame_count;
            if total_frames > MAX_TOTAL_FRAMES {
                return Err(ClipError::TooManyTotalFrames.into());
            }
            let mut frames = Vec::with_capacity(frame_count);
            for _ in 0..frame_count {
                frames.push(read_frame(&mut bytes)?);
            }
            clips.push((name.to_owned(), Clip::new(mode, frames)?));
        }
        if !bytes.is_empty() {
            return Err(SpriteAnimationAssetError::InvalidCooked("trailing bytes"));
        }
        Ok(Self::new(clips)?)
    }
}

fn read_frame(bytes: &mut &[u8]) -> Result<Frame, SpriteAnimationAssetError> {
    let min = read_vec2(bytes)?;
    let size = read_vec2(bytes)?;
    let duration = Duration::from_nanos(u64::from_le_bytes(read_field(bytes)?));
    let frame = Frame::new(TextureRegion::new(min, size), duration)?;
    let event = read_text(bytes, MAX_FRAME_MARKER_BYTES)?;
    if event.is_empty() {
        Ok(frame)
    } else {
        Ok(frame.with_event(event)?)
    }
}

fn read_vec2(bytes: &mut &[u8]) -> Result<Vec2, SpriteAnimationAssetError> {
    Ok(Vec2::new(
        f32::from_le_bytes(read_field(bytes)?),
        f32::from_le_bytes(read_field(bytes)?),
    ))
}

fn read_field<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], SpriteAnimationAssetError> {
    let (field, rest) = bytes
        .split_first_chunk::<N>()
        .ok_or(SpriteAnimationAssetError::InvalidCooked("truncated field"))?;
    *bytes = rest;
    Ok(*field)
}

fn read_text<'a>(
    bytes: &mut &'a [u8],
    max_len: usize,
) -> Result<&'a str, SpriteAnimationAssetError> {
    let [len] = read_field::<1>(bytes)?;
    let len = usize::from(len);
    if len > max_len {
        return Err(SpriteAnimationAssetError::InvalidCooked(
            "text field exceeds its limit",
        ));
    }
    let (text, rest) = bytes
        .split_at_checked(len)
        .ok_or(SpriteAnimationAssetError::InvalidCooked("truncated text"))?;
    *bytes = rest;
    std::str::from_utf8(text).map_err(|_| SpriteAnimationAssetError::InvalidCooked("invalid UTF-8"))
}

fn write_text(bytes: &mut Vec<u8>, text: &str) {
    bytes.push(text.len() as u8);
    bytes.extend_from_slice(text.as_bytes());
}

/// Invalid sprite animation source, cooked data, or clip contents.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpriteAnimationAssetError {
    #[error(transparent)]
    Clip(#[from] ClipError),
    #[error("invalid cooked sprite animation: {0}")]
    InvalidCooked(&'static str),
    #[error("unsupported NSAN version {0}")]
    UnsupportedVersion(u16),
    #[error("sprite animation source exceeds {MAX_SPRITE_ANIMATION_SOURCE_BYTES} bytes")]
    SourceTooLarge,
    #[cfg(feature = "animation-authoring")]
    #[error(transparent)]
    Ron(#[from] ron::error::SpannedError),
    #[error("could not allocate cooked sprite animation: {0}")]
    Allocation(#[from] TryReserveError),
}

/// Loads an NSAN sprite animation or an authored `.sanim.ron` document.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpriteAnimationLoader;

impl AssetLoader for SpriteAnimationLoader {
    type Asset = SpriteAnimationSet;
    type Error = SpriteAnimationLoaderError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::SpriteAnimation)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<SpriteAnimationSet, Self::Error> {
        match context.mode() {
            RuntimeAssetMode::Cooked => Ok(SpriteAnimationSet::from_cooked(bytes)?),
            #[cfg(feature = "animation-authoring")]
            RuntimeAssetMode::Source => {
                super::source::validate_source_len(bytes.len())?;
                let source =
                    std::str::from_utf8(bytes).map_err(SpriteAnimationLoaderError::SourceUtf8)?;
                Ok(SpriteAnimationSet::from_ron(source)?)
            }
            #[cfg(not(feature = "animation-authoring"))]
            RuntimeAssetMode::Source => Err(SpriteAnimationLoaderError::SourceDisabled),
        }
    }
}

/// Failure to load a sprite animation asset through the runtime asset server.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpriteAnimationLoaderError {
    #[cfg(not(feature = "animation-authoring"))]
    #[error("loading source sprite animations requires the 'animation-authoring' feature")]
    SourceDisabled,
    #[cfg(feature = "animation-authoring")]
    #[error("sprite animation source is not UTF-8")]
    SourceUtf8(#[source] std::str::Utf8Error),
    #[error(transparent)]
    Asset(#[from] SpriteAnimationAssetError),
}
