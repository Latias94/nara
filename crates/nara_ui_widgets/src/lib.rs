//! Headless controlled widgets reduced from renderer-neutral UI routing facts.
//!
//! Widgets own behavior-local state, bounded draft buffers and typed intents. They do not own product values,
//! callbacks, ECS access, platform events, or presentation state.

mod button;
mod intent;
mod state;
mod state_text;
mod text_field;

pub use button::{UiButton, UiButtonState, UiButtonVisualState, UiWidgetError};
pub use intent::{UiIntent, UiIntentBatch, UiIntentBatchError};
pub use state::{
    UiStateBinding, UiStateChanges, UiStateError, UiStateHandle, UiStateKey, UiStatePass,
    UiStateStore,
};
pub use state_text::UiStateTextInput;
pub use text_field::{UiTextField, UiTextFieldError, UiTextInputToken};
