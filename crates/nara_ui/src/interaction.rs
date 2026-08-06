use nara_core::Vec2;
use nara_ecs::{Mut, Resource, World};
use nara_input::{
    ButtonTransitionPhase, InputEventKind, InputTimeline, InputTimelineEntry, InputUiDisposition,
    KeyCode, MouseButton,
};
use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiButtonPhase, UiDispatchBatch, UiInputEvent, UiKey,
    UiNavigation, UiPoint, UiPointerButton, UiPointerId, UiRouteStamp, UiSeatId, UiSurfaceId,
};
use nara_ui_widgets::UiIntent;

use crate::runtime::{UiAdapterError, UiRuntimeState};

const MAX_ACTION_EVENTS: usize = 64;

/// One typed semantic action emitted by the runtime UI adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiActionEvent {
    target: UiKey,
    action: UiActionId,
    source: UiActivationSource,
}

impl UiActionEvent {
    #[must_use]
    pub const fn target(&self) -> &UiKey {
        &self.target
    }

    #[must_use]
    pub const fn action(&self) -> &UiActionId {
        &self.action
    }

    #[must_use]
    pub const fn source(&self) -> UiActivationSource {
        self.source
    }
}

/// Bounded action collection published for runtime-domain command mapping.
#[derive(Debug, Default, Resource)]
pub struct UiActionEvents {
    events: Vec<UiActionEvent>,
}

impl UiActionEvents {
    #[must_use]
    pub fn as_slice(&self) -> &[UiActionEvent] {
        &self.events
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    fn extend_intents(&mut self, intents: &[UiIntent]) {
        for intent in intents {
            if self.events.len() == MAX_ACTION_EVENTS {
                break;
            }
            match intent {
                UiIntent::Activate {
                    target,
                    action,
                    source,
                } => self.events.push(UiActionEvent {
                    target: target.clone(),
                    action: action.clone(),
                    source: *source,
                }),
            }
        }
    }
}

pub(crate) fn route_timeline_ui_input(world: &mut World) {
    let entries = world
        .resource::<InputTimeline>()
        .pending_ui_entries()
        .copied()
        .collect::<Vec<_>>();
    world.resource_mut::<UiActionEvents>().clear();
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        for entry in entries {
            let Some((route, input)) = project_timeline_entry(&state, entry) else {
                continue;
            };
            let Ok((dispatch, intents)) = state.dispatch(route, input) else {
                continue;
            };
            let consumed = !dispatch.events().is_empty() || !intents.intents().is_empty();
            if consumed {
                world
                    .resource_mut::<UiActionEvents>()
                    .extend_intents(intents.intents());
                let _ = world
                    .resource_mut::<InputTimeline>()
                    .__finalize_ui_entry(entry.sequence(), InputUiDisposition::Consumed);
            }
        }
    });
}

/// Routes one normalized event against an exact retained frame/receipt pair.
pub fn dispatch_presented_ui_input(
    world: &mut World,
    route: UiRouteStamp,
    input: UiInputEvent,
) -> Result<UiDispatchBatch, UiAdapterError> {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let (dispatch, intents) = state.dispatch(route, input)?;
        world
            .resource_mut::<UiActionEvents>()
            .extend_intents(intents.intents());
        Ok(dispatch)
    })
}

fn project_timeline_entry(
    state: &UiRuntimeState,
    entry: InputTimelineEntry,
) -> Option<(UiRouteStamp, UiInputEvent)> {
    let input_route = entry.route();
    let surface = input_route.surface()?;
    let surface = UiSurfaceId::new(surface.get())?;
    let epoch = input_route.presentation_epoch()?.get();
    let route = UiRouteStamp::new(surface, epoch);
    let seat = UiSeatId::new(input_route.seat().get());
    let pointer = input_route
        .pointer()
        .map(|pointer| UiPointerId::new(pointer.get()));
    let position = entry
        .logical_position()
        .and_then(|position| ui_point(state, surface, position));

    let input = match entry.kind() {
        InputEventKind::PointerMoved => UiInputEvent::PointerMoved {
            seat,
            pointer: pointer?,
            position: position?,
        },
        InputEventKind::PointerButton { button, phase } => UiInputEvent::PointerButton {
            seat,
            pointer: pointer?,
            button: pointer_button(button),
            phase: button_phase(phase),
            position,
        },
        InputEventKind::PointerLeft => UiInputEvent::PointerCancelled {
            seat,
            pointer: pointer?,
        },
        InputEventKind::Key { key, phase } => project_key(seat, key, phase)?,
        InputEventKind::GamepadButton {
            gamepad: _,
            button,
            phase,
        } => project_gamepad(seat, button, phase)?,
        InputEventKind::FocusChanged { focused: false } => UiInputEvent::SeatCancelled { seat },
        InputEventKind::FocusChanged { focused: true } => return None,
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
        InputEventKind::Wheel { .. } => return None,
    };
    Some((route, input))
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

fn ui_point(state: &UiRuntimeState, surface: UiSurfaceId, position: Vec2) -> Option<UiPoint> {
    let input_scale_factor = state.surface_descriptor(surface)?.input_scale_factor();
    let x = position.x / input_scale_factor;
    let y = position.y / input_scale_factor;
    (x.is_finite() && y.is_finite()).then_some(UiPoint::new(x, y))
}

#[cfg(test)]
mod tests {
    use nara_app::App;
    use nara_input::{InputSeatId, InputWindowId};
    use nara_reflect::ComponentRegistryPlugin;
    use nara_ui_kernel::UiPoint;

    use super::*;

    #[test]
    fn platform_logical_pointer_coordinates_are_normalized_by_host_mapping() {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_input::InputPlugin,
            crate::UiPlugin,
        ))
        .expect("UI input test plugins should install");
        let surface = UiSurfaceId::new(9).expect("test surface should be non-zero");
        let descriptor = crate::UiSurfaceDescriptor::new(
            surface,
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            960.0,
            540.0,
            2.0,
            1,
            1,
            1,
        )
        .expect("test surface descriptor should be valid")
        .with_surface_mapping(2.0, 0.5)
        .expect("test input mapping should be valid");
        crate::install_ui_surface(
            app.world_mut().expect("test app should expose its world"),
            descriptor,
        )
        .expect("test surface should install");

        let state = app.world().resource::<UiRuntimeState>();
        assert_eq!(
            ui_point(state, surface, Vec2::new(50.0, 25.0)),
            Some(UiPoint::new(100.0, 50.0))
        );
    }
}
