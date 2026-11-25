use std::any::type_name;

use nara_app::{RuntimeDriverScope, RuntimeState, RuntimeWorldAccessError};
use nara_core::Vec2;

use super::{
    GamepadConnection, InputAdmission, InputEndpointClaim, InputEventOrigin, InputIngressError,
    InputPointerId, InputSeatId, InputTimeline, InputWindowEvent, InputWindowId,
};
use crate::{
    AxisValue, ButtonDriverInput, ButtonInputDriverError, ButtonTransitionPhase, KeyCode,
    MouseButton, MouseWheelUnit, PointerDriverInput, TouchPhase, WindowDeviceInput,
};

pub(super) fn accepts_driver_input(state: RuntimeState, surface_retirement: bool) -> bool {
    matches!(state, RuntimeState::Running | RuntimeState::Paused)
        || (surface_retirement
            && matches!(
                state,
                RuntimeState::Faulted | RuntimeState::Stopping | RuntimeState::CloseIncomplete
            ))
}

fn apply_input<Output>(
    scope: &mut RuntimeDriverScope<'_>,
    surface_retirement: bool,
    apply: impl FnOnce(&mut InputTimeline) -> Result<Output, InputIngressError>,
) -> Result<Output, ButtonInputDriverError> {
    let state = scope.state();
    if !accepts_driver_input(state, surface_retirement) {
        return Err(RuntimeWorldAccessError::Unavailable { state }.into());
    }
    scope.with_world_mut(|world| {
        let mut timeline = world.get_resource_mut::<InputTimeline>().ok_or(
            RuntimeWorldAccessError::MissingResource {
                type_name: type_name::<InputTimeline>(),
            },
        )?;
        apply(&mut timeline).map_err(ButtonInputDriverError::from)
    })
}

/// Connects one explicitly named platform endpoint before its first ingress operation.
pub fn connect_input_endpoint(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
) -> Result<(), ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.connect_endpoint(window, seat)
    })
}

/// Connects a pointer child of an explicitly connected endpoint.
pub fn connect_input_pointer(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
) -> Result<(), ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.connect_pointer(window, seat, pointer)
    })
}

/// Retires one connected pointer without retiring its keyboard/window endpoint.
pub fn retire_input_pointer(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        let batch = timeline.apply_pointer(
            window,
            seat,
            pointer,
            PointerDriverInput::Left,
            InputEventOrigin::Platform,
            true,
        )?;
        Ok((batch.admission, batch.released_mouse_buttons))
    })
}

/// Sends synthetic input through the owner's once-admitted default endpoint.
pub fn apply_keyboard_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: ButtonDriverInput<KeyCode>,
) -> Result<Vec<KeyCode>, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline
            .apply_keyboard(
                InputWindowId::UNSPECIFIED,
                InputSeatId::PRIMARY,
                input,
                InputEventOrigin::Synthetic,
            )
            .map(|batch| batch.released_keys)
    })
}

/// Admits synthetic keyboard input at the exact endpoint retained by its original claim.
///
/// Foreign, retired, and recycled claims refuse before even an empty ReleaseAll can be ignored.
/// The original timeline owns membership, ordering, and reserved terminal records. A Release of
/// a restored or physical member preserves that member's origin independently of this trigger.
pub fn apply_claimed_keyboard_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    claim: InputEndpointClaim,
    input: ButtonDriverInput<KeyCode>,
) -> Result<InputAdmission, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.admit_claimed_keyboard(&claim, input)
    })
}

/// Sends platform input from an explicitly connected endpoint.
pub fn apply_keyboard_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    input: ButtonDriverInput<KeyCode>,
) -> Result<Vec<KeyCode>, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline
            .apply_keyboard(window, seat, input, InputEventOrigin::Platform)
            .map(|batch| batch.released_keys)
    })
}

pub fn apply_mouse_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: ButtonDriverInput<MouseButton>,
) -> Result<Vec<MouseButton>, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline
            .apply_mouse(
                InputWindowId::UNSPECIFIED,
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                input,
                InputEventOrigin::Synthetic,
            )
            .map(|batch| batch.released_mouse_buttons)
    })
}

pub fn apply_mouse_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
    input: ButtonDriverInput<MouseButton>,
) -> Result<Vec<MouseButton>, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline
            .apply_mouse(window, seat, pointer, input, InputEventOrigin::Platform)
            .map(|batch| batch.released_mouse_buttons)
    })
}

pub fn apply_pointer_driver_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: PointerDriverInput,
) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        let batch = timeline.apply_pointer(
            InputWindowId::UNSPECIFIED,
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            input,
            InputEventOrigin::Synthetic,
            false,
        )?;
        Ok((batch.admission, batch.released_mouse_buttons))
    })
}

pub fn apply_pointer_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
    input: PointerDriverInput,
) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        let batch = timeline.apply_pointer(
            window,
            seat,
            pointer,
            input,
            InputEventOrigin::Platform,
            false,
        )?;
        Ok((batch.admission, batch.released_mouse_buttons))
    })
}

/// Connects one platform controller stream to an already connected endpoint.
pub fn connect_gamepad_input(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    gamepad: u16,
) -> Result<GamepadConnection, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.connect_gamepad(window, seat, gamepad)
    })
}

/// Connects a synthetic controller through the same finite membership and terminal owner.
pub fn connect_synthetic_gamepad_input(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    gamepad: u16,
) -> Result<GamepadConnection, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.connect_synthetic_gamepad(window, seat, gamepad)
    })
}

/// Releases and neutralizes the original connection before retiring its device slot.
pub fn disconnect_gamepad_input(
    scope: &mut RuntimeDriverScope<'_>,
    connection: &GamepadConnection,
) -> Result<InputAdmission, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.disconnect_gamepad(connection)
    })
}

/// Admits a button through the origin, endpoint and generation captured by its connection.
pub fn apply_gamepad_button_input(
    scope: &mut RuntimeDriverScope<'_>,
    connection: &GamepadConnection,
    button: u16,
    phase: ButtonTransitionPhase,
) -> Result<InputAdmission, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.admit_gamepad_button(connection, button, phase)
    })
}

/// Admits an already validated normalized scalar through the original connection.
pub fn apply_gamepad_axis_input(
    scope: &mut RuntimeDriverScope<'_>,
    connection: &GamepadConnection,
    axis: u16,
    value: AxisValue,
) -> Result<InputAdmission, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.admit_gamepad_axis(connection, axis, value)
    })
}

pub fn apply_window_input_driver_event(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    event: InputWindowEvent,
) -> Result<(InputAdmission, Vec<KeyCode>, Vec<MouseButton>), ButtonInputDriverError> {
    let surface_retirement = matches!(event, InputWindowEvent::SurfaceRetired);
    apply_input(scope, surface_retirement, move |timeline| {
        let batch = timeline.apply_window(window, seat, event)?;
        Ok((
            batch.admission,
            batch.released_keys,
            batch.released_mouse_buttons,
        ))
    })
}

pub fn apply_window_device_input(
    scope: &mut RuntimeDriverScope<'_>,
    input: WindowDeviceInput,
) -> Result<(), ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.admit_window_device_input(input)
    })
}

pub fn apply_mouse_wheel_driver_input_at(
    scope: &mut RuntimeDriverScope<'_>,
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
    delta: Vec2,
    unit: MouseWheelUnit,
    phase: TouchPhase,
) -> Result<InputAdmission, ButtonInputDriverError> {
    apply_input(scope, false, move |timeline| {
        timeline.admit_window_wheel(window, seat, pointer, delta, unit, phase)
    })
}
