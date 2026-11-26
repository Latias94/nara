//! Direct physical input, window device messages, and action mapping.

mod action_types;
mod actions;
mod axis;
mod binding;
mod definition;
mod devices;
mod events;
mod gamepad;
#[cfg(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_os = "macos")
))]
mod gilrs_input;
mod keys;
mod physical;
mod plugin;
mod state;

pub use action_types::{
    ActionContext, ActionId, ActionIdError, ActionPhase, ActionValue, InputBinding,
};
pub use actions::{ActionEdge, ActionSeat, ActionState};
pub use axis::{AxisDeadZone, AxisValue, AxisValueError};
pub use binding::{ActionBinding, ActionTransform, ActionValueKind, AxisDirection};
pub use definition::{
    ActionContextDefinition, ActionMapBindings, ActionMapBuilder, ActionMapCompileCost,
    ActionMapDefinition, ActionMapError, ActionMapLimitKind, ActionMapLimits, MAX_ACTION_BINDINGS,
    MAX_ACTION_CONTEXTS, MAX_ACTION_IDENTIFIER_BYTES, MAX_BINDINGS_PER_INPUT,
};
pub use devices::{
    ImeEvent, ImeEventKind, MouseWheel, MouseWheelUnit, TextInput, TouchInput, TouchPhase,
};
pub use events::{
    ButtonTransitionPhase, InputEdge, InputEdgeKind, InputFrame, InputPointerId, InputSeatId,
    InputWindowEvent, InputWindowId, MouseButton,
};
pub use gamepad::{GamepadAxis, GamepadButton, GamepadInput, GamepadInputKind};
#[cfg(all(
    feature = "gamepad",
    any(windows, target_os = "linux", target_os = "macos")
))]
pub use gilrs_input::{
    GILRS_INPUT_PLUGIN_ID, GamepadAssignmentError, GamepadRoute, GilrsDevice, GilrsDeviceToken,
    GilrsDevices, GilrsInputPlugin,
};
pub use keys::KeyCode;
pub use physical::{ButtonInput, ButtonTransition, PointerState};
pub use plugin::{INPUT_PLUGIN_DECLARATION, INPUT_PLUGIN_ID, InputPlugin, InputSet};
pub use state::{ActionMap, ActionMapInstallError, ActionMaps, MAX_SEAT_ACTION_MAPS};
