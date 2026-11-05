//! Headless controlled widgets reduced from renderer-neutral UI routing facts.
//!
//! Widgets own behavior-local state, bounded draft buffers and typed intents. They do not own product values,
//! callbacks, ECS access, platform events, or presentation state.

mod anchored_popup;
mod appearance;
mod button;
mod control_grid;
mod intent;
mod state;
mod state_text;
mod text_field;

pub use anchored_popup::UiAnchoredPopup;
pub use appearance::{
    UiAppearance, UiAppearanceError, UiAppearancePalette, UiButtonStyle, UiButtonVariant,
    UiControlMetrics, UiControlSize, UiDensity, UiTextRole,
};
pub use button::{UiButton, UiButtonState, UiButtonVisualState, UiWidgetError};
pub use control_grid::UiControlGrid;
pub use intent::{UiIntent, UiIntentBatch, UiIntentBatchError};
pub use state::{
    UiStateBinding, UiStateChanges, UiStateError, UiStateHandle, UiStateKey, UiStatePass,
    UiStateStore,
};
pub use state_text::UiStateTextInput;
pub use text_field::{UiTextField, UiTextFieldAppearance, UiTextFieldError, UiTextInputToken};
