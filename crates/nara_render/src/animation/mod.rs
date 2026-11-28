//! Sprite clips, animation assets, and ECS playback.
//!
//! Sample clips with a game-owned clock or install [`SpriteAnimationPlugin`] to drive
//! [`SpriteAnimator`] components. Clips select normalized regions within the sprite's image.

mod animator;
mod asset;
mod clip;
#[cfg(feature = "animation-scene")]
mod hydration;
#[cfg(feature = "animation-authoring")]
mod source;

pub use animator::{
    SPRITE_ANIMATION_PLUGIN_ID, SpriteAnimationClock, SpriteAnimationEvent, SpriteAnimationPlugin,
    SpriteAnimationSpeedError, SpriteAnimationUpdateSet, SpriteAnimator,
};
pub use asset::{
    MAX_SPRITE_ANIMATION_SOURCE_BYTES, SpriteAnimationAssetError, SpriteAnimationLoader,
    SpriteAnimationLoaderError,
};
pub use clip::{
    Clip, ClipError, Frame, FrameSample, MAX_CLIP_NAME_BYTES, MAX_CLIPS, MAX_FRAME_MARKER_BYTES,
    MAX_FRAMES_PER_CLIP, MAX_MARKERS_PER_STEP, MAX_TOTAL_FRAMES, PlaybackMode, SpriteAnimationSet,
};
