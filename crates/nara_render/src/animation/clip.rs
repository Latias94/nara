use std::time::Duration;

use crate::sprite::TextureRegion;
use thiserror::Error;

/// Maximum number of named clips in one animation set.
pub const MAX_CLIPS: usize = 32;
/// Maximum number of frames in one clip.
pub const MAX_FRAMES_PER_CLIP: usize = 256;
/// Maximum aggregate number of frames in one animation set.
pub const MAX_TOTAL_FRAMES: usize = 4096;
/// Maximum byte length of an ASCII clip name.
pub const MAX_CLIP_NAME_BYTES: usize = 64;

/// Behavior at and after the clip's total duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PlaybackMode {
    /// Repeat forever; the exact end of each cycle samples the first frame.
    Loop,
    /// Hold the last frame and report completion at the exact end of the clip.
    Once,
}

/// Rejection of sprite animation data during construction.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClipError {
    /// A region is non-finite, empty, or outside the normalized image bounds.
    #[error("frame region must be finite, have positive size, and remain within [0, 1]")]
    InvalidRegion,
    /// A frame has no duration.
    #[error("frame duration must be greater than zero")]
    ZeroFrameDuration,
    /// A frame or cumulative clip duration exceeds the supported nanosecond range.
    #[error("frame and total clip durations must fit in u64 nanoseconds")]
    DurationOverflow,
    /// Sampling requires at least one frame.
    #[error("a clip must contain at least one frame")]
    EmptyClip,
    /// A clip exceeds the frame limit.
    #[error("a clip may contain at most {MAX_FRAMES_PER_CLIP} frames")]
    TooManyFrames,
    /// A set exceeds the named clip limit.
    #[error("an animation set may contain at most {MAX_CLIPS} clips")]
    TooManyClips,
    /// A set exceeds the aggregate frame limit.
    #[error("an animation set may contain at most {MAX_TOTAL_FRAMES} total frames")]
    TooManyTotalFrames,
    /// A name is empty, too long, or contains unsupported characters.
    #[error("clip names must contain 1 to {MAX_CLIP_NAME_BYTES} ASCII letters, digits, _, ., or -")]
    InvalidClipName,
    /// A set contains the same name more than once.
    #[error("duplicate clip name: {0}")]
    DuplicateClipName(String),
}

/// One admitted normalized texture region and its strictly positive duration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    region: TextureRegion,
    duration_ns: u64,
}

impl Frame {
    /// Admits a finite, nonempty UV rectangle and a positive duration fitting in u64 nanoseconds.
    pub fn new(region: TextureRegion, duration: Duration) -> Result<Self, ClipError> {
        if !region.is_valid_uv() {
            return Err(ClipError::InvalidRegion);
        }
        if duration.is_zero() {
            return Err(ClipError::ZeroFrameDuration);
        }
        let duration_ns =
            u64::try_from(duration.as_nanos()).map_err(|_| ClipError::DurationOverflow)?;
        Ok(Self {
            region,
            duration_ns,
        })
    }

    /// Returns the normalized region without binding an image or sprite.
    #[must_use]
    pub const fn region(&self) -> TextureRegion {
        self.region
    }

    #[must_use]
    pub const fn duration(&self) -> Duration {
        Duration::from_nanos(self.duration_ns)
    }
}

/// An immutable, nonempty sequence of independently timed sprite frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    mode: PlaybackMode,
    frames: Box<[Frame]>,
    frame_ends_ns: Box<[u64]>,
    duration_ns: u64,
}

impl Clip {
    /// Admits at most [`MAX_FRAMES_PER_CLIP`] frames with a total fitting in u64 nanoseconds.
    ///
    /// The iterator is consumed only up to the first rejected frame or the supported limit
    /// plus one; its size hint does not control allocation.
    pub fn new(
        mode: PlaybackMode,
        frames: impl IntoIterator<Item = Frame>,
    ) -> Result<Self, ClipError> {
        let mut admitted = Vec::new();
        let mut frame_ends_ns = Vec::new();
        let mut duration_ns = 0_u64;
        for frame in frames {
            if admitted.len() == MAX_FRAMES_PER_CLIP {
                return Err(ClipError::TooManyFrames);
            }
            duration_ns = duration_ns
                .checked_add(frame.duration_ns)
                .ok_or(ClipError::DurationOverflow)?;
            admitted.push(frame);
            frame_ends_ns.push(duration_ns);
        }
        if admitted.is_empty() {
            return Err(ClipError::EmptyClip);
        }
        Ok(Self {
            mode,
            frames: admitted.into_boxed_slice(),
            frame_ends_ns: frame_ends_ns.into_boxed_slice(),
            duration_ns,
        })
    }

    #[must_use]
    pub const fn mode(&self) -> PlaybackMode {
        self.mode
    }

    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    #[must_use]
    pub const fn duration(&self) -> Duration {
        Duration::from_nanos(self.duration_ns)
    }

    /// Samples caller-owned elapsed time without allocation or frame validation.
    ///
    /// Each frame owns its start boundary and excludes its end. Looping wraps to frame zero
    /// at the exact clip end and never finishes. Once playback holds the last frame and
    /// reports `finished` from the exact clip end onward, including arbitrarily large times.
    #[must_use]
    pub fn sample(&self, elapsed: Duration) -> FrameSample {
        let elapsed_ns = elapsed.as_nanos();
        let duration_ns = u128::from(self.duration_ns);
        let finished = self.mode == PlaybackMode::Once && elapsed_ns >= duration_ns;
        let index = if finished {
            self.frames.len() - 1
        } else {
            let phase_ns = match self.mode {
                PlaybackMode::Loop => elapsed_ns % duration_ns,
                PlaybackMode::Once => elapsed_ns,
            };
            self.frame_ends_ns
                .partition_point(|end| u128::from(*end) <= phase_ns)
        };
        FrameSample {
            index,
            region: self.frames[index].region,
            finished,
        }
    }
}

/// A sampled frame, ready for the caller to apply to its own sprite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameSample {
    /// Index into the clip's authored frame sequence.
    pub index: usize,
    /// Normalized UV region; image ownership remains with the sprite.
    pub region: TextureRegion,
    /// True only for Once playback at or after the total clip duration.
    pub finished: bool,
}

/// Named sprite clips for a game animation.
///
/// Names are case-sensitive, contain 1 to [`MAX_CLIP_NAME_BYTES`] ASCII letters, digits,
/// underscores, periods, or hyphens, and are unique within the set. An empty set is allowed.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteAnimationSet {
    clips: Box<[(Box<str>, Clip)]>,
}

impl SpriteAnimationSet {
    /// Admits the clip count, names, uniqueness, and aggregate frame count once.
    ///
    /// Individual frames and clips have already been admitted by their constructors.
    pub fn new(clips: impl IntoIterator<Item = (String, Clip)>) -> Result<Self, ClipError> {
        let mut admitted = Vec::new();
        let mut total_frames = 0;
        for (name, clip) in clips {
            if admitted.len() == MAX_CLIPS {
                return Err(ClipError::TooManyClips);
            }
            if name.is_empty()
                || name.len() > MAX_CLIP_NAME_BYTES
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
            {
                return Err(ClipError::InvalidClipName);
            }
            total_frames += clip.frames.len();
            if total_frames > MAX_TOTAL_FRAMES {
                return Err(ClipError::TooManyTotalFrames);
            }
            admitted.push((name.into_boxed_str(), clip));
        }
        admitted.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        for pair in admitted.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(ClipError::DuplicateClipName(pair[0].0.to_string()));
            }
        }
        Ok(Self {
            clips: admitted.into_boxed_slice(),
        })
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Clip> {
        self.clips
            .binary_search_by(|(candidate, _)| candidate.as_ref().cmp(name))
            .ok()
            .map(|index| &self.clips[index].1)
    }

    /// Borrows all named clips in ascending name order without allocation.
    pub fn clips(&self) -> impl ExactSizeIterator<Item = (&str, &Clip)> {
        self.clips.iter().map(|(name, clip)| (name.as_ref(), clip))
    }
}
