//! Bounded sprite animation resources sampled at caller-owned elapsed time.
//!
//! Clips contain normalized texture regions and independent frame durations. The caller owns
//! the clock, playback state, image selection, and application of the sampled region to a Sprite.
//! Construction admits immutable resources; sampling does not allocate or revalidate frames.

mod clip;

pub use clip::{
    Clip, ClipError, Frame, FrameSample, MAX_CLIP_NAME_BYTES, MAX_CLIPS, MAX_FRAMES_PER_CLIP,
    MAX_TOTAL_FRAMES, PlaybackMode, SpriteAnimationSet,
};
