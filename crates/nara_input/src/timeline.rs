//! Bounded normalized Input ownership, observation and presentation-route publication.

#[path = "timeline/action_outputs.rs"]
mod action_outputs;
#[path = "timeline/capacity.rs"]
mod capacity;
#[path = "timeline/driver.rs"]
mod driver;
#[path = "timeline/owner.rs"]
mod owner;
#[path = "timeline/topology.rs"]
mod topology;
#[path = "timeline/types.rs"]
mod types;

pub use driver::{
    apply_claimed_keyboard_driver_input, apply_gamepad_axis_input, apply_gamepad_button_input,
    apply_keyboard_driver_input, apply_keyboard_driver_input_at, apply_mouse_driver_input,
    apply_mouse_driver_input_at, apply_mouse_wheel_driver_input_at, apply_pointer_driver_input,
    apply_pointer_driver_input_at, apply_window_device_input, apply_window_input_driver_event,
    connect_gamepad_input, connect_input_endpoint, connect_input_pointer,
    connect_synthetic_gamepad_input, disconnect_gamepad_input, retire_input_pointer,
};
use owner::HeldInput;
pub use owner::{
    GamepadConnection, InputEndpointClaim, InputRouteCommit, InputTimeline,
    PreparedInputRoutePublication,
};
pub use types::{
    ActionInputProvenance, InputAdmission, InputConfigurationError, InputConsumerCursors,
    InputEndpointObservation, InputEndpointRetirement, InputEventKind, InputEventOrigin,
    InputFrameObservations, InputGamepadGeneration, InputGeometryGeneration, InputIngressError,
    InputInvalidationReason, InputLimits, InputPointerId, InputPresentationEpoch,
    InputPresentedRoute, InputRouteGeneration, InputRouteStamp, InputSeatId, InputSlotGeneration,
    InputSurfaceId, InputTimelineEntry, InputUiCursorError, InputUiDisposition, InputWindowEvent,
    InputWindowId, MAX_INPUT_ROUTES, MAX_INPUT_TIMELINE_ENTRIES,
};

#[cfg(test)]
#[path = "timeline/gamepad_tests.rs"]
mod gamepad_tests;
#[cfg(test)]
#[path = "timeline/retention_tests.rs"]
mod retention_tests;
#[cfg(test)]
#[path = "timeline/tests.rs"]
mod tests;
