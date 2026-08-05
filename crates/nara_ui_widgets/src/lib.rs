//! Headless controlled widgets reduced from renderer-neutral UI routing facts.
//!
//! Widgets own behavior-local state and produce typed intents. They do not own product values,
//! callbacks, ECS access, platform events, or presentation state.

mod button;
mod intent;

pub use button::{UiButton, UiButtonState, UiButtonVisualState, UiWidgetError};
pub use intent::{UiIntent, UiIntentBatch, UiIntentBatchError};
