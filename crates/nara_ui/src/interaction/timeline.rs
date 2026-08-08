use nara_core::Vec2;
use nara_input::{ButtonTransitionPhase, InputEventKind, InputTimelineEntry, KeyCode, MouseButton};
use nara_ui_kernel::{
    UiActivationSource, UiButtonPhase, UiInputEvent, UiNavigation, UiPoint, UiPointerButton,
    UiPointerId, UiRouteStamp, UiSeatId, UiSurfaceId,
};

use crate::runtime::UiRuntimeState;

use super::transaction::{UiInputProjectionIssue, UiRouteRejection, map_adapter_error};

pub(crate) enum UiTimelineProjection {
    PassThrough,
    Routed {
        route: UiRouteStamp,
        input: UiInputEvent,
    },
    Rejected(UiRouteRejection),
}

pub(crate) fn project_timeline_entry(
    state: &UiRuntimeState,
    entry: InputTimelineEntry,
) -> UiTimelineProjection {
    let input_route = entry.route();
    if !input_route.is_ui_routable() {
        return UiTimelineProjection::PassThrough;
    }
    let (Some(surface), Some(epoch)) = (input_route.surface(), input_route.presentation_epoch())
    else {
        return UiTimelineProjection::Rejected(UiRouteRejection::InvalidRouteStamp);
    };
    let Some(surface) = UiSurfaceId::new(surface.get()) else {
        return UiTimelineProjection::Rejected(UiRouteRejection::InvalidRouteStamp);
    };
    let route = UiRouteStamp::new(surface, epoch.get());
    if state.is_retired(surface) {
        return UiTimelineProjection::Rejected(UiRouteRejection::RetiredSurface {
            surface: surface.get(),
        });
    }
    let Some(runtime) = state.surface(surface) else {
        return UiTimelineProjection::Rejected(UiRouteRejection::UnknownSurface {
            surface: surface.get(),
        });
    };
    let presented = match runtime.resolve_presented(route) {
        Ok(presented) => presented,
        Err(error) => return UiTimelineProjection::Rejected(map_adapter_error(error)),
    };
    let descriptor = presented.descriptor();
    let blocks_unhandled_input = presented.blocks_unhandled_input();
    if descriptor.input_window() != input_route.window()
        || descriptor.input_seat() != input_route.seat()
    {
        return UiTimelineProjection::Rejected(UiRouteRejection::RouteEndpointMismatch);
    }

    let seat = UiSeatId::new(input_route.seat().get());
    let pointer = input_route
        .pointer()
        .map(|pointer| UiPointerId::new(pointer.get()));
    if entry.invalidation().is_some()
        && matches!(
            entry.kind(),
            InputEventKind::PointerButton { .. } | InputEventKind::PointerLeft
        )
    {
        let Some(pointer) = pointer else {
            return rejected_projection(UiInputProjectionIssue::MissingPointer);
        };
        return UiTimelineProjection::Routed {
            route,
            input: UiInputEvent::PointerCancelled { seat, pointer },
        };
    }
    let position = match project_point(entry.logical_position(), descriptor.input_scale_factor()) {
        Ok(position) => position,
        Err(error) => {
            return UiTimelineProjection::Rejected(UiRouteRejection::InvalidProjection(error));
        }
    };

    let input = match entry.kind() {
        InputEventKind::PointerMoved => {
            let Some(pointer) = pointer else {
                return rejected_projection(UiInputProjectionIssue::MissingPointer);
            };
            let Some(position) = position else {
                return rejected_projection(UiInputProjectionIssue::MissingPosition);
            };
            UiInputEvent::PointerMoved {
                seat,
                pointer,
                position,
            }
        }
        InputEventKind::PointerButton { button, phase } => {
            let Some(pointer) = pointer else {
                return rejected_projection(UiInputProjectionIssue::MissingPointer);
            };
            UiInputEvent::PointerButton {
                seat,
                pointer,
                button: pointer_button(button),
                phase: button_phase(phase),
                position,
            }
        }
        InputEventKind::PointerLeft => {
            let Some(pointer) = pointer else {
                return rejected_projection(UiInputProjectionIssue::MissingPointer);
            };
            UiInputEvent::PointerCancelled { seat, pointer }
        }
        InputEventKind::Key { key, phase } => {
            let Some(input) = project_key(seat, key, phase) else {
                return unhandled_projection(blocks_unhandled_input);
            };
            input
        }
        InputEventKind::GamepadButton {
            gamepad: _,
            button,
            phase,
        } => {
            let Some(input) = project_gamepad(seat, button, phase) else {
                return unhandled_projection(blocks_unhandled_input);
            };
            input
        }
        InputEventKind::FocusChanged { focused: false } => UiInputEvent::SeatCancelled { seat },
        InputEventKind::FocusChanged { focused: true } | InputEventKind::Wheel { .. } => {
            return unhandled_projection(blocks_unhandled_input);
        }
        InputEventKind::Resized { .. } | InputEventKind::ScaleFactorChanged { .. } => {
            UiInputEvent::GeometryInvalidated
        }
        InputEventKind::AccessibilityAction { action: _ } => UiInputEvent::ActivateFocused {
            seat,
            source: UiActivationSource::Accessibility,
        },
        InputEventKind::CloseRequested | InputEventKind::SurfaceRetired => {
            UiInputEvent::SurfaceCancelled
        }
    };
    UiTimelineProjection::Routed { route, input }
}

fn rejected_projection(issue: UiInputProjectionIssue) -> UiTimelineProjection {
    UiTimelineProjection::Rejected(UiRouteRejection::InvalidProjection(issue))
}

fn unhandled_projection(blocked: bool) -> UiTimelineProjection {
    if blocked {
        UiTimelineProjection::Rejected(UiRouteRejection::UnhandledInputBlocked)
    } else {
        UiTimelineProjection::PassThrough
    }
}

fn project_point(
    position: Option<Vec2>,
    input_scale_factor: f32,
) -> Result<Option<UiPoint>, UiInputProjectionIssue> {
    let Some(position) = position else {
        return Ok(None);
    };
    let x = position.x / input_scale_factor;
    let y = position.y / input_scale_factor;
    if !x.is_finite() || !y.is_finite() {
        return Err(UiInputProjectionIssue::NonFinitePosition);
    }
    Ok(Some(UiPoint::new(x, y)))
}

fn project_key(seat: UiSeatId, key: KeyCode, phase: ButtonTransitionPhase) -> Option<UiInputEvent> {
    if phase != ButtonTransitionPhase::Pressed {
        return None;
    }
    match key {
        KeyCode::ArrowUp => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Up,
        }),
        KeyCode::ArrowDown => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Down,
        }),
        KeyCode::ArrowLeft => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Left,
        }),
        KeyCode::ArrowRight => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Right,
        }),
        KeyCode::Enter | KeyCode::Space => Some(UiInputEvent::ActivateFocused {
            seat,
            source: UiActivationSource::Keyboard,
        }),
        KeyCode::Character('\t') => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Next,
        }),
        KeyCode::Escape | KeyCode::Character(_) => None,
    }
}

fn project_gamepad(
    seat: UiSeatId,
    button: u16,
    phase: ButtonTransitionPhase,
) -> Option<UiInputEvent> {
    if phase != ButtonTransitionPhase::Pressed {
        return None;
    }
    match button {
        0 => Some(UiInputEvent::ActivateFocused {
            seat,
            source: UiActivationSource::Gamepad,
        }),
        12 => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Up,
        }),
        13 => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Down,
        }),
        14 => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Left,
        }),
        15 => Some(UiInputEvent::Navigate {
            seat,
            direction: UiNavigation::Right,
        }),
        _ => None,
    }
}

const fn pointer_button(button: MouseButton) -> UiPointerButton {
    match button {
        MouseButton::Left => UiPointerButton::Primary,
        MouseButton::Right => UiPointerButton::Secondary,
        MouseButton::Middle => UiPointerButton::Middle,
        MouseButton::Other(button) => UiPointerButton::Other(button),
    }
}

const fn button_phase(phase: ButtonTransitionPhase) -> UiButtonPhase {
    match phase {
        ButtonTransitionPhase::Pressed => UiButtonPhase::Pressed,
        ButtonTransitionPhase::Released => UiButtonPhase::Released,
    }
}
