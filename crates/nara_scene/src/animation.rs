//! Headless property animation over the same field paths used by scene authoring.

mod clip;
#[cfg(feature = "runtime")]
mod player;
#[cfg(feature = "runtime")]
mod tween;

pub use clip::*;
#[cfg(feature = "runtime")]
pub use player::{
    AnimationClock, AnimationPlayer, AnimationPlugin, AnimationSet, ApplyProperty,
    PropertyAnimationError, PropertyAnimationEvent, PropertyAppliers, PropertyWrite,
    SPRITE_COMPONENT_ID,
};
#[cfg(feature = "runtime")]
pub use tween::{Tween2d, TweenFinished, TweenPlugin};

#[cfg(feature = "runtime")]
pub(crate) use player::apply_schema_field;
