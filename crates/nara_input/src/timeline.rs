use std::collections::{BTreeMap, VecDeque};

use nara_app::{__RuntimeDriverPort, RuntimeState};
use nara_core::Vec2;
use nara_ecs::Resource;

use crate::{
    ActionMap, ButtonDriverInput, ButtonTransitionPhase, KeyCode, MouseButton, PointerDriverInput,
};

#[path = "timeline/types.rs"]
mod types;

#[doc(hidden)]
pub use types::{__InputDriverEvent, __InputDriverOutput};
pub use types::{
    ActionInputProvenance, InputAdmission, InputConsumerCursors, InputEndpointRetirement,
    InputEventKind, InputFrameObservations, InputGeometryGeneration, InputIngressError,
    InputInvalidationReason, InputPointerId, InputPresentationEpoch, InputPresentedRoute,
    InputRouteGeneration, InputRouteStamp, InputSeatId, InputSlotGeneration, InputSurfaceId,
    InputTimelineEntry, InputUiCursorError, InputUiDisposition, InputWindowEvent, InputWindowId,
    MAX_INPUT_ROUTES, MAX_INPUT_TIMELINE_ENTRIES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionRetention {
    Unscanned,
    Deferred,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionDelivery {
    OwnUiDisposition,
    Gameplay,
    Ui,
    FollowPress(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeldInputDisposition {
    Pending,
    Gameplay,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeldInput {
    press_sequence: u64,
    route: InputRouteStamp,
    disposition: HeldInputDisposition,
}

impl HeldInput {
    fn new(press_sequence: u64, route: InputRouteStamp) -> Self {
        Self {
            press_sequence,
            route,
            disposition: if route.is_ui_routable() {
                HeldInputDisposition::Pending
            } else {
                HeldInputDisposition::Gameplay
            },
        }
    }

    const fn action_delivery(self) -> ActionDelivery {
        match self.disposition {
            HeldInputDisposition::Pending => ActionDelivery::FollowPress(self.press_sequence),
            HeldInputDisposition::Gameplay => ActionDelivery::Gameplay,
            HeldInputDisposition::Ui => ActionDelivery::Ui,
        }
    }

    fn can_route_release_through(self, route: InputRouteStamp) -> bool {
        self.disposition != HeldInputDisposition::Gameplay
            && same_presented_lineage(self.route, route)
    }
}

#[derive(Debug, Clone, Copy)]
struct RetainedInput {
    entry: InputTimelineEntry,
    projection_pending: bool,
    observation_pending: bool,
    ui_pending: bool,
    ui_consumed: bool,
    action: ActionRetention,
    action_delivery: ActionDelivery,
}

#[derive(Debug, Clone, Copy)]
struct RouteState {
    surface: InputSurfaceId,
    presentation_epoch: InputPresentationEpoch,
    slot_generation: InputSlotGeneration,
    route_generation: InputRouteGeneration,
    geometry_generation: InputGeometryGeneration,
    binding_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedRouteReservation {
    route: InputPresentedRoute,
    binding_generation: u64,
    changes_route: bool,
    reserves_capacity: bool,
}

/// Move-only reservation for one exact presented-route publication.
///
/// This token is intentionally exposed only through [`crate::__private`]. Dropping it without
/// calling the matching abort or commit entry leaves the endpoint reserved and therefore fails
/// closed rather than allowing another publication to invalidate an in-flight backend attempt.
#[must_use = "a prepared input route must be committed, aborted, or superseded"]
#[derive(Debug)]
pub struct PreparedInputRoutePublication {
    endpoint: (InputWindowId, InputSeatId),
    reservation: PreparedRouteReservation,
}

#[derive(Debug, Clone, Copy)]
struct InputDraft {
    route: InputRouteStamp,
    logical_position: Option<Vec2>,
    kind: InputEventKind,
    invalidation: Option<InputInvalidationReason>,
    action_delivery: ActionDelivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhysicalInvalidationMode {
    PlatformObserved,
    Coordinated,
}

/// Global normalized input authority and independent consumer progress.
#[derive(Debug, Resource)]
pub struct InputTimeline {
    retained: VecDeque<RetainedInput>,
    next_sequence: u64,
    observation_cursor: u64,
    ui_cursor: u64,
    action_scanned_cursor: u64,
    routes: BTreeMap<(InputWindowId, InputSeatId), RouteState>,
    prepared_routes: BTreeMap<(InputWindowId, InputSeatId), PreparedRouteReservation>,
    focused_windows: BTreeMap<InputSeatId, InputWindowId>,
    next_binding_generation: u64,
    routes_poisoned: bool,
    pressed_keys: BTreeMap<(InputWindowId, InputSeatId), BTreeMap<KeyCode, HeldInput>>,
    pressed_mouse_buttons:
        BTreeMap<(InputWindowId, InputSeatId, InputPointerId), BTreeMap<MouseButton, HeldInput>>,
    pointer_positions: BTreeMap<(InputWindowId, InputSeatId, InputPointerId), Vec2>,
}

impl Default for InputTimeline {
    fn default() -> Self {
        Self {
            retained: VecDeque::with_capacity(MAX_INPUT_TIMELINE_ENTRIES),
            next_sequence: 0,
            observation_cursor: 0,
            ui_cursor: 0,
            action_scanned_cursor: 0,
            routes: BTreeMap::new(),
            prepared_routes: BTreeMap::new(),
            focused_windows: BTreeMap::new(),
            next_binding_generation: 0,
            routes_poisoned: false,
            pressed_keys: BTreeMap::new(),
            pressed_mouse_buttons: BTreeMap::new(),
            pointer_positions: BTreeMap::new(),
        }
    }
}

impl InputTimeline {
    #[must_use]
    pub fn retained_len(&self) -> usize {
        self.retained.len()
    }

    pub fn entries(&self) -> impl Iterator<Item = &InputTimelineEntry> {
        self.retained.iter().map(|retained| &retained.entry)
    }

    pub fn pending_ui_entries(&self) -> impl Iterator<Item = &InputTimelineEntry> {
        self.retained
            .iter()
            .filter(|retained| retained.ui_pending)
            .map(|retained| &retained.entry)
    }

    #[must_use]
    pub fn cursors(&self) -> InputConsumerCursors {
        InputConsumerCursors {
            observation: self.observation_cursor,
            ui: self.ui_cursor,
            action_scanned: self.action_scanned_cursor,
            oldest_deferred_action: self
                .retained
                .iter()
                .find(|retained| retained.action == ActionRetention::Deferred)
                .map(|retained| retained.entry.sequence),
        }
    }

    /// Publishes one exact presented route and the slot/geometry generations it represents.
    pub fn publish_presented_route(
        &mut self,
        route: InputPresentedRoute,
    ) -> Result<InputRouteStamp, InputIngressError> {
        let prepared = self.prepare_presented_route(route)?;
        Ok(self.commit_presented_route(prepared))
    }

    fn prepare_presented_route(
        &mut self,
        route: InputPresentedRoute,
    ) -> Result<PreparedInputRoutePublication, InputIngressError> {
        if self.routes_poisoned {
            return Err(InputIngressError::RouteBindingGenerationExhausted {
                route_invalidated: true,
            });
        }
        let window = route.window();
        let surface = route.surface();
        let seat = route.seat();
        let key = (window, seat);
        self.ensure_endpoint_unreserved(window, seat)?;
        let reserves_capacity = !self.routes.contains_key(&key);
        if reserves_capacity {
            let reserved_capacity = self
                .prepared_routes
                .values()
                .filter(|reservation| reservation.reserves_capacity)
                .count();
            if self.routes.len() + reserved_capacity >= MAX_INPUT_ROUTES {
                return Err(InputIngressError::RouteCapacityExceeded {
                    limit: MAX_INPUT_ROUTES,
                });
            }
        }
        let mut exact_binding_generation = None;
        if let Some(current) = self.routes.get(&key) {
            if current.surface != surface {
                return Err(InputIngressError::RouteConflict {
                    window,
                    seat,
                    current_surface: current.surface,
                    requested_surface: surface,
                });
            }
            let same_slot = route.slot_generation() == current.slot_generation;
            let regresses = route.slot_generation() < current.slot_generation
                || route.route_generation() < current.route_generation
                || route.geometry_generation() < current.geometry_generation
                || (same_slot && route.presentation_epoch() < current.presentation_epoch);
            if regresses {
                return Err(InputIngressError::PresentedRouteRegression { window, seat });
            }
            if current.presentation_epoch == route.presentation_epoch()
                && current.slot_generation == route.slot_generation()
                && current.route_generation == route.route_generation()
                && current.geometry_generation == route.geometry_generation()
            {
                exact_binding_generation = Some(current.binding_generation);
            }
        }
        let binding_generation = if let Some(binding_generation) = exact_binding_generation {
            binding_generation
        } else {
            let binding_generation = self.next_binding_generation.checked_add(1).ok_or(
                InputIngressError::RouteBindingGenerationExhausted {
                    route_invalidated: false,
                },
            )?;
            self.next_binding_generation = binding_generation;
            binding_generation
        };
        let reservation = PreparedRouteReservation {
            route,
            binding_generation,
            changes_route: exact_binding_generation.is_none(),
            reserves_capacity,
        };
        self.prepared_routes.insert(key, reservation);
        Ok(PreparedInputRoutePublication {
            endpoint: key,
            reservation,
        })
    }

    fn commit_presented_route(
        &mut self,
        prepared: PreparedInputRoutePublication,
    ) -> InputRouteStamp {
        let (endpoint, reservation) = self.consume_presented_route_reservation(
            prepared,
            "a prepared route can commit only its exact reservation",
        );
        if reservation.changes_route {
            let route = reservation.route;
            self.routes.insert(
                endpoint,
                RouteState {
                    surface: route.surface(),
                    presentation_epoch: route.presentation_epoch(),
                    slot_generation: route.slot_generation(),
                    route_generation: route.route_generation(),
                    geometry_generation: route.geometry_generation(),
                    binding_generation: reservation.binding_generation,
                },
            );
        }
        self.pointer_route(endpoint.0, endpoint.1, None)
    }

    fn release_presented_route_reservation(&mut self, prepared: PreparedInputRoutePublication) {
        let _ = self.consume_presented_route_reservation(
            prepared,
            "a prepared route can release only its exact reservation",
        );
    }

    fn consume_presented_route_reservation(
        &mut self,
        prepared: PreparedInputRoutePublication,
        mismatch_message: &'static str,
    ) -> ((InputWindowId, InputSeatId), PreparedRouteReservation) {
        let PreparedInputRoutePublication {
            endpoint,
            reservation,
        } = prepared;
        let active = self
            .prepared_routes
            .remove(&endpoint)
            .expect("a move-only prepared route keeps its endpoint reservation");
        assert_eq!(active, reservation, "{mismatch_message}");
        (endpoint, reservation)
    }

    #[doc(hidden)]
    pub fn __finalize_ui_entry(
        &mut self,
        sequence: u64,
        disposition: InputUiDisposition,
    ) -> Result<(), InputUiCursorError> {
        let (entry, ui_consumed) = {
            let Some(retained) = self
                .retained
                .iter_mut()
                .find(|retained| retained.entry.sequence == sequence)
            else {
                return Err(InputUiCursorError::UnknownSequence { sequence });
            };
            if !retained.ui_pending {
                return Err(InputUiCursorError::AlreadyFinalized { sequence });
            }
            retained.ui_pending = false;
            retained.ui_consumed = matches!(
                disposition,
                InputUiDisposition::Consumed | InputUiDisposition::RejectedClosed
            );
            (retained.entry, retained.ui_consumed)
        };
        self.resolve_held_press(entry, ui_consumed);
        Ok(())
    }

    /// Unbinds one presented UI endpoint while preserving physical button state.
    ///
    /// Surface Slot and geometry updates invalidate UI routing, focus and capture in their owning
    /// UI session, but the native window may still hold keys or pointer buttons. Those physical
    /// transitions remain tracked so their real release edges can reach the timeline, while the
    /// geometry-scoped pointer position is discarded. A true platform endpoint retirement must
    /// enter through [`Self::retire_presented_endpoint`] or [`InputWindowEvent::SurfaceRetired`],
    /// which synthesize balanced releases before clearing all physical state.
    ///
    /// The expected route is optional because a Surface Slot can change before its first
    /// presentation or after platform lifecycle input already removed the route. A replacement
    /// route at the same endpoint is still a typed mismatch.
    pub fn unbind_presented_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        expected_route: Option<InputRouteStamp>,
    ) -> Result<(), InputIngressError> {
        self.ensure_endpoint_unreserved(window, seat)?;
        self.validate_presented_endpoint(window, seat, expected_route)?;
        let result = self.unbind_route(window, seat);
        self.clear_pointer_positions(window, seat);
        result
    }

    /// Retires one physical endpoint after validating its last presented route, when one exists.
    ///
    /// Unlike [`Self::unbind_presented_endpoint`], retirement synthesizes balanced key,
    /// pointer-button, and pointer-leave transitions and then clears all physical state owned by the
    /// endpoint. A caller that prepared retirement while a route was bound must pass that exact
    /// route stamp; `None` is accepted only while no replacement route owns the endpoint.
    pub fn retire_presented_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        expected_route: Option<InputRouteStamp>,
    ) -> Result<InputEndpointRetirement, InputIngressError> {
        self.ensure_endpoint_unreserved(window, seat)?;
        self.validate_presented_endpoint(window, seat, expected_route)?;
        let output = self.invalidate_physical_endpoint(
            window,
            seat,
            InputEventKind::SurfaceRetired,
            InputInvalidationReason::SurfaceRetired,
            true,
            PhysicalInvalidationMode::Coordinated,
        )?;
        Ok(InputEndpointRetirement::new(
            output.admission,
            output.released_keys,
            output.released_mouse_buttons,
        ))
    }

    /// Admits one normalized keyboard transition into the global timeline.
    ///
    /// Deterministic direct-App and headless hosts use this entry directly. Native platform
    /// adapters should use the runtime-scoped driver helpers so runtime-state gating and their
    /// local device state stay coupled to the same admission.
    pub fn admit_keyboard(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        input: ButtonDriverInput<KeyCode>,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_keyboard(window, seat, input)
            .map(|output| output.admission)
    }

    /// Admits one normalized mouse-button transition for a deterministic direct-App or headless
    /// host. Native managed adapters use the runtime-scoped driver helpers instead.
    pub fn admit_mouse_button(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: ButtonDriverInput<MouseButton>,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_mouse(window, seat, pointer, input)
            .map(|output| output.admission)
    }

    /// Admits one normalized pointer movement or leave event for a deterministic direct-App or
    /// headless host. Native managed adapters use the runtime-scoped driver helpers instead.
    pub fn admit_pointer(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: PointerDriverInput,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_pointer(window, seat, pointer, input)
            .map(|output| output.admission)
    }

    /// Admits one normalized window lifecycle event for a deterministic direct-App or headless
    /// host. Native managed adapters use the runtime-scoped driver helpers instead.
    pub fn admit_window_event(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        event: InputWindowEvent,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_window(window, seat, event)
            .map(|output| output.admission)
    }

    pub fn admit_wheel(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        delta: Vec2,
    ) -> Result<InputAdmission, InputIngressError> {
        if !vec2_is_finite(delta) {
            return Err(InputIngressError::NonFinitePosition);
        }
        let position = self
            .pointer_positions
            .get(&(window, seat, pointer))
            .copied();
        let draft = InputDraft {
            route: self.pointer_route(window, seat, Some(pointer)),
            logical_position: position,
            kind: InputEventKind::Wheel { delta },
            invalidation: None,
            action_delivery: ActionDelivery::OwnUiDisposition,
        };
        self.admit_drafts(&[draft])
    }

    pub fn admit_gamepad_button(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        gamepad: u16,
        button: u16,
        phase: ButtonTransitionPhase,
    ) -> Result<InputAdmission, InputIngressError> {
        let draft = InputDraft {
            route: self.focused_route(window, seat),
            logical_position: None,
            kind: InputEventKind::GamepadButton {
                gamepad,
                button,
                phase,
            },
            invalidation: None,
            action_delivery: ActionDelivery::OwnUiDisposition,
        };
        self.admit_drafts(&[draft])
    }

    fn apply_driver_event(
        &mut self,
        event: __InputDriverEvent,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        match event {
            __InputDriverEvent::Keyboard {
                window,
                seat,
                input,
            } => self.apply_keyboard(window, seat, input),
            __InputDriverEvent::Mouse {
                window,
                seat,
                pointer,
                input,
            } => self.apply_mouse(window, seat, pointer, input),
            __InputDriverEvent::Pointer {
                window,
                seat,
                pointer,
                input,
            } => self.apply_pointer(window, seat, pointer, input),
            __InputDriverEvent::Gamepad {
                window,
                seat,
                gamepad,
                button,
                phase,
            } => self
                .admit_gamepad_button(window, seat, gamepad, button, phase)
                .map(__InputDriverOutput::new),
            __InputDriverEvent::Window {
                window,
                seat,
                event,
            } => self.apply_window(window, seat, event),
        }
    }

    fn apply_keyboard(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        input: ButtonDriverInput<KeyCode>,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        match input {
            ButtonDriverInput::Press(key) => {
                let draft = self.key_draft(window, seat, key, ButtonTransitionPhase::Pressed);
                if self.pressed_key_window(seat, key).is_some() {
                    return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
                }
                let state_key = (draft.route.window(), seat);
                let admission = self.admit_drafts(&[draft])?;
                let press_sequence = admission
                    .first_sequence()
                    .expect("one admitted key press has one sequence");
                self.pressed_keys
                    .entry(state_key)
                    .or_default()
                    .insert(key, HeldInput::new(press_sequence, draft.route));
                Ok(__InputDriverOutput::new(admission))
            }
            ButtonDriverInput::Release(key) => {
                let Some(origin_window) = self.pressed_key_window(seat, key) else {
                    return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
                };
                let draft = self.key_draft_for_window(
                    origin_window,
                    seat,
                    key,
                    ButtonTransitionPhase::Released,
                );
                let state_key = (draft.route.window(), seat);
                let admission = self.admit_drafts(&[draft])?;
                if let Some(pressed) = self.pressed_keys.get_mut(&state_key) {
                    pressed.remove(&key);
                }
                Ok(__InputDriverOutput::new(admission))
            }
            ButtonDriverInput::ReleaseAll => {
                let released = self
                    .pressed_keys
                    .iter()
                    .filter(|((_, event_seat), _)| *event_seat == seat)
                    .flat_map(|((event_window, _), pressed)| {
                        pressed.keys().copied().map(|key| (*event_window, key))
                    })
                    .collect::<Vec<_>>();
                let drafts = released
                    .iter()
                    .copied()
                    .map(|(event_window, key)| {
                        self.key_draft_for_window(
                            event_window,
                            seat,
                            key,
                            ButtonTransitionPhase::Released,
                        )
                    })
                    .collect::<Vec<_>>();
                let admission = self.admit_drafts(&drafts)?;
                self.pressed_keys
                    .retain(|(_, event_seat), _| *event_seat != seat);
                let mut output = __InputDriverOutput::new(admission);
                output.released_keys = released
                    .into_iter()
                    .map(|(_, released_key)| released_key)
                    .collect();
                Ok(output)
            }
        }
    }

    fn apply_mouse(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: ButtonDriverInput<MouseButton>,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let state_key = (window, seat, pointer);
        match input {
            ButtonDriverInput::Press(button) => {
                if self
                    .pressed_mouse_buttons
                    .get(&state_key)
                    .is_some_and(|pressed| pressed.contains_key(&button))
                {
                    return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
                }
                let draft = self.mouse_draft(
                    window,
                    seat,
                    pointer,
                    button,
                    ButtonTransitionPhase::Pressed,
                );
                let admission = self.admit_drafts(&[draft])?;
                let press_sequence = admission
                    .first_sequence()
                    .expect("one admitted pointer press has one sequence");
                self.pressed_mouse_buttons
                    .entry(state_key)
                    .or_default()
                    .insert(button, HeldInput::new(press_sequence, draft.route));
                Ok(__InputDriverOutput::new(admission))
            }
            ButtonDriverInput::Release(button) => {
                if !self
                    .pressed_mouse_buttons
                    .get(&state_key)
                    .is_some_and(|pressed| pressed.contains_key(&button))
                {
                    return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
                }
                let draft = self.mouse_draft(
                    window,
                    seat,
                    pointer,
                    button,
                    ButtonTransitionPhase::Released,
                );
                let admission = self.admit_drafts(&[draft])?;
                if let Some(pressed) = self.pressed_mouse_buttons.get_mut(&state_key) {
                    pressed.remove(&button);
                }
                Ok(__InputDriverOutput::new(admission))
            }
            ButtonDriverInput::ReleaseAll => {
                let released = self
                    .pressed_mouse_buttons
                    .get(&state_key)
                    .map(|pressed| pressed.keys().copied().collect::<Vec<_>>())
                    .unwrap_or_default();
                let drafts = released
                    .iter()
                    .copied()
                    .map(|button| {
                        self.mouse_draft(
                            window,
                            seat,
                            pointer,
                            button,
                            ButtonTransitionPhase::Released,
                        )
                    })
                    .collect::<Vec<_>>();
                let admission = self.admit_drafts(&drafts)?;
                self.pressed_mouse_buttons.remove(&state_key);
                let mut output = __InputDriverOutput::new(admission);
                output.released_mouse_buttons = released;
                Ok(output)
            }
        }
    }

    fn apply_pointer(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: PointerDriverInput,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        match input {
            PointerDriverInput::Moved(position) => {
                if !vec2_is_finite(position) {
                    return Err(InputIngressError::NonFinitePosition);
                }
                let draft = InputDraft {
                    route: self.pointer_route(window, seat, Some(pointer)),
                    logical_position: Some(position),
                    kind: InputEventKind::PointerMoved,
                    invalidation: None,
                    action_delivery: ActionDelivery::OwnUiDisposition,
                };
                let admission = self.admit_drafts(&[draft])?;
                self.pointer_positions
                    .insert((window, seat, pointer), position);
                Ok(__InputDriverOutput::new(admission))
            }
            PointerDriverInput::Left => {
                let state_key = (window, seat, pointer);
                let position = self.pointer_positions.get(&state_key).copied();
                let released = self
                    .pressed_mouse_buttons
                    .get(&state_key)
                    .map(|pressed| pressed.keys().copied().collect::<Vec<_>>())
                    .unwrap_or_default();
                let mut drafts = released
                    .iter()
                    .copied()
                    .map(|button| {
                        let mut draft = self.mouse_draft(
                            window,
                            seat,
                            pointer,
                            button,
                            ButtonTransitionPhase::Released,
                        );
                        draft.invalidation = Some(InputInvalidationReason::PointerLeft);
                        draft
                    })
                    .collect::<Vec<_>>();
                drafts.push(InputDraft {
                    route: self.pointer_route(window, seat, Some(pointer)),
                    logical_position: position,
                    kind: InputEventKind::PointerLeft,
                    invalidation: Some(InputInvalidationReason::PointerLeft),
                    action_delivery: ActionDelivery::OwnUiDisposition,
                });
                let admission = match self.admit_drafts(&drafts) {
                    Ok(admission) => admission,
                    Err(error) => {
                        self.pressed_mouse_buttons.remove(&state_key);
                        self.pointer_positions.remove(&state_key);
                        return Err(error);
                    }
                };
                self.pressed_mouse_buttons.remove(&state_key);
                self.pointer_positions.remove(&state_key);
                let mut output = __InputDriverOutput::new(admission);
                output.released_mouse_buttons = released;
                Ok(output)
            }
        }
    }

    fn apply_window(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        event: InputWindowEvent,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        if let InputWindowEvent::Focused(true) = event {
            let draft = InputDraft {
                route: self.pointer_route(window, seat, None),
                logical_position: None,
                kind: InputEventKind::FocusChanged { focused: true },
                invalidation: None,
                action_delivery: ActionDelivery::OwnUiDisposition,
            };
            let admission = self.admit_drafts(&[draft])?;
            self.focused_windows.insert(seat, window);
            return Ok(__InputDriverOutput::new(admission));
        }

        let owns_keyboard_focus = self
            .focused_windows
            .get(&seat)
            .is_none_or(|focused_window| *focused_window == window);
        match event {
            InputWindowEvent::Focused(false) => self.invalidate_physical_endpoint(
                window,
                seat,
                InputEventKind::FocusChanged { focused: false },
                InputInvalidationReason::FocusLost,
                owns_keyboard_focus,
                PhysicalInvalidationMode::PlatformObserved,
            ),
            InputWindowEvent::Resized { logical_size } => {
                if !vec2_is_finite(logical_size) || logical_size.x < 0.0 || logical_size.y < 0.0 {
                    return Err(InputIngressError::InvalidLogicalSize);
                }
                self.invalidate_logical_route(
                    window,
                    seat,
                    InputEventKind::Resized { logical_size },
                    InputInvalidationReason::Resized,
                )
            }
            InputWindowEvent::ScaleFactorChanged { scale_factor } => {
                if !scale_factor.is_finite() || scale_factor <= 0.0 {
                    return Err(InputIngressError::InvalidScaleFactor);
                }
                self.invalidate_logical_route(
                    window,
                    seat,
                    InputEventKind::ScaleFactorChanged { scale_factor },
                    InputInvalidationReason::ScaleFactorChanged,
                )
            }
            InputWindowEvent::CloseRequested => self.invalidate_physical_endpoint(
                window,
                seat,
                InputEventKind::CloseRequested,
                InputInvalidationReason::CloseRequested,
                owns_keyboard_focus,
                PhysicalInvalidationMode::PlatformObserved,
            ),
            InputWindowEvent::SurfaceRetired => self.retire_endpoint(window, seat),
            InputWindowEvent::Focused(true) => unreachable!("focus gain returned above"),
        }
    }

    fn invalidate_logical_route(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        kind: InputEventKind,
        reason: InputInvalidationReason,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        self.ensure_endpoint_unreserved(window, seat)?;
        let old_route = self.pointer_route(window, seat, None);
        let route_was_bound = old_route.is_ui_routable();
        let draft = InputDraft {
            route: old_route,
            logical_position: None,
            kind,
            invalidation: Some(reason),
            action_delivery: ActionDelivery::OwnUiDisposition,
        };
        let unbind_result = self.unbind_route(window, seat);
        self.clear_pointer_positions(window, seat);
        unbind_result?;
        match self.admit_drafts(&[draft]) {
            Ok(admission) => Ok(__InputDriverOutput::new(admission)),
            Err(error) if route_was_bound => Err(error.after_route_invalidation()),
            Err(error) => Err(error),
        }
    }

    fn retire_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        self.invalidate_physical_endpoint(
            window,
            seat,
            InputEventKind::SurfaceRetired,
            InputInvalidationReason::SurfaceRetired,
            true,
            PhysicalInvalidationMode::PlatformObserved,
        )
    }

    fn invalidate_physical_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        kind: InputEventKind,
        reason: InputInvalidationReason,
        clear_keyboard: bool,
        mode: PhysicalInvalidationMode,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        self.ensure_endpoint_unreserved(window, seat)?;
        let old_route = self.pointer_route(window, seat, None);

        let released_keys = if clear_keyboard {
            self.pressed_keys
                .get(&(window, seat))
                .map(|pressed| {
                    pressed
                        .iter()
                        .map(|(&key, &held)| (key, held))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let released_mouse_buttons = self
            .pressed_mouse_buttons
            .iter()
            .filter(|((event_window, event_seat, _), _)| {
                *event_window == window && *event_seat == seat
            })
            .flat_map(|((_, _, pointer), pressed)| {
                pressed
                    .iter()
                    .map(|(&button, &held)| (*pointer, button, held))
            })
            .collect::<Vec<_>>();
        let pointer_positions = self
            .pointer_positions
            .iter()
            .filter_map(|(&(event_window, event_seat, pointer), &position)| {
                (event_window == window && event_seat == seat).then_some((pointer, position))
            })
            .collect::<Vec<_>>();

        let mut drafts = Vec::with_capacity(
            released_keys.len() + released_mouse_buttons.len() + pointer_positions.len() + 1,
        );
        drafts.extend(released_keys.iter().copied().map(|(key, held)| InputDraft {
            route: if held.can_route_release_through(old_route) {
                old_route
            } else {
                InputRouteStamp::unrouted(window, seat, None)
            },
            logical_position: None,
            kind: InputEventKind::Key {
                key,
                phase: ButtonTransitionPhase::Released,
            },
            invalidation: Some(reason),
            action_delivery: held.action_delivery(),
        }));
        drafts.extend(
            released_mouse_buttons
                .iter()
                .copied()
                .map(|(pointer, button, held)| InputDraft {
                    route: {
                        let routed = InputRouteStamp {
                            pointer: Some(pointer),
                            ..old_route
                        };
                        if held.can_route_release_through(routed) {
                            routed
                        } else {
                            InputRouteStamp::unrouted(window, seat, Some(pointer))
                        }
                    },
                    logical_position: held
                        .can_route_release_through(InputRouteStamp {
                            pointer: Some(pointer),
                            ..old_route
                        })
                        .then(|| {
                            self.pointer_positions
                                .get(&(window, seat, pointer))
                                .copied()
                        })
                        .flatten(),
                    kind: InputEventKind::PointerButton {
                        button,
                        phase: ButtonTransitionPhase::Released,
                    },
                    invalidation: Some(reason),
                    action_delivery: held.action_delivery(),
                }),
        );
        drafts.extend(
            pointer_positions
                .iter()
                .copied()
                .map(|(pointer, position)| InputDraft {
                    route: InputRouteStamp {
                        pointer: Some(pointer),
                        ..old_route
                    },
                    logical_position: Some(position),
                    kind: InputEventKind::PointerLeft,
                    invalidation: Some(reason),
                    action_delivery: ActionDelivery::OwnUiDisposition,
                }),
        );
        drafts.push(InputDraft {
            route: old_route,
            logical_position: None,
            kind,
            invalidation: Some(reason),
            action_delivery: ActionDelivery::OwnUiDisposition,
        });

        let admission = match mode {
            PhysicalInvalidationMode::Coordinated => {
                let binding_generation = self.next_binding_generation_for_change()?;
                let admission = self.admit_drafts(&drafts)?;
                self.commit_failed_closed_window(window, seat, binding_generation);
                admission
            }
            PhysicalInvalidationMode::PlatformObserved => {
                if let Err(error) = self.fail_closed_window(window, seat) {
                    self.clear_physical_route_state(window, seat, clear_keyboard);
                    return Err(error);
                }
                match self.admit_drafts(&drafts) {
                    Ok(admission) => admission,
                    Err(error) => {
                        self.clear_physical_route_state(window, seat, clear_keyboard);
                        return Err(error.after_route_invalidation());
                    }
                }
            }
        };
        self.clear_physical_route_state(window, seat, clear_keyboard);
        let mut output = __InputDriverOutput::new(admission);
        output.released_keys = released_keys.into_iter().map(|(key, _)| key).collect();
        output.released_mouse_buttons = released_mouse_buttons
            .into_iter()
            .map(|(_, button, _)| button)
            .collect();
        Ok(output)
    }

    fn resolve_held_press(&mut self, entry: InputTimelineEntry, ui_consumed: bool) {
        let disposition = if ui_consumed {
            HeldInputDisposition::Ui
        } else {
            HeldInputDisposition::Gameplay
        };
        let sequence = entry.sequence();
        match entry.kind() {
            InputEventKind::Key {
                key,
                phase: ButtonTransitionPhase::Pressed,
            } => {
                if let Some(held) = self
                    .pressed_keys
                    .get_mut(&(entry.route().window(), entry.route().seat()))
                    .and_then(|pressed| pressed.get_mut(&key))
                    && held.press_sequence == sequence
                {
                    held.disposition = disposition;
                }
            }
            InputEventKind::PointerButton {
                button,
                phase: ButtonTransitionPhase::Pressed,
            } => {
                if let Some(pointer) = entry.route().pointer()
                    && let Some(held) = self
                        .pressed_mouse_buttons
                        .get_mut(&(entry.route().window(), entry.route().seat(), pointer))
                        .and_then(|pressed| pressed.get_mut(&button))
                    && held.press_sequence == sequence
                {
                    held.disposition = disposition;
                }
            }
            _ => return,
        }

        let delivery = if ui_consumed {
            ActionDelivery::Ui
        } else {
            ActionDelivery::Gameplay
        };
        for retained in &mut self.retained {
            if retained.action_delivery == ActionDelivery::FollowPress(sequence) {
                retained.action_delivery = delivery;
            }
        }
    }

    fn pressed_key_window(&self, seat: InputSeatId, key: KeyCode) -> Option<InputWindowId> {
        self.pressed_keys
            .iter()
            .find_map(|(&(window, event_seat), pressed)| {
                (event_seat == seat && pressed.contains_key(&key)).then_some(window)
            })
    }

    fn key_draft(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        key: KeyCode,
        phase: ButtonTransitionPhase,
    ) -> InputDraft {
        let focused_window = self.focused_windows.get(&seat).copied().unwrap_or(window);
        self.key_draft_for_window(focused_window, seat, key, phase)
    }

    fn key_draft_for_window(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        key: KeyCode,
        phase: ButtonTransitionPhase,
    ) -> InputDraft {
        let held = (phase == ButtonTransitionPhase::Released)
            .then(|| self.pressed_keys.get(&(window, seat))?.get(&key).copied())
            .flatten();
        let action_delivery =
            held.map_or(ActionDelivery::OwnUiDisposition, HeldInput::action_delivery);
        let current_route = self.pointer_route(window, seat, None);
        let route = if held.is_none_or(|held| held.can_route_release_through(current_route)) {
            current_route
        } else {
            InputRouteStamp::unrouted(window, seat, None)
        };
        InputDraft {
            route,
            logical_position: None,
            kind: InputEventKind::Key { key, phase },
            invalidation: None,
            action_delivery,
        }
    }

    fn mouse_draft(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        button: MouseButton,
        phase: ButtonTransitionPhase,
    ) -> InputDraft {
        let state_key = (window, seat, pointer);
        let held = (phase == ButtonTransitionPhase::Released)
            .then(|| {
                self.pressed_mouse_buttons
                    .get(&state_key)?
                    .get(&button)
                    .copied()
            })
            .flatten();
        let current_route = self.pointer_route(window, seat, Some(pointer));
        let route_through_ui =
            held.is_none_or(|held| held.can_route_release_through(current_route));
        InputDraft {
            route: if route_through_ui {
                current_route
            } else {
                InputRouteStamp::unrouted(window, seat, Some(pointer))
            },
            logical_position: route_through_ui
                .then(|| self.pointer_positions.get(&state_key).copied())
                .flatten(),
            kind: InputEventKind::PointerButton { button, phase },
            invalidation: None,
            action_delivery: held
                .map_or(ActionDelivery::OwnUiDisposition, HeldInput::action_delivery),
        }
    }

    fn focused_route(&self, fallback_window: InputWindowId, seat: InputSeatId) -> InputRouteStamp {
        let window = self
            .focused_windows
            .get(&seat)
            .copied()
            .unwrap_or(fallback_window);
        self.pointer_route(window, seat, None)
    }

    fn pointer_route(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: Option<InputPointerId>,
    ) -> InputRouteStamp {
        let Some(route) = self.routes.get(&(window, seat)) else {
            return InputRouteStamp::unrouted(window, seat, pointer);
        };
        InputRouteStamp {
            window,
            surface: Some(route.surface),
            seat,
            pointer,
            presentation_epoch: Some(route.presentation_epoch),
            slot_generation: Some(route.slot_generation),
            route_generation: Some(route.route_generation),
            geometry_generation: Some(route.geometry_generation),
            binding_generation: route.binding_generation,
        }
    }

    fn validate_presented_endpoint(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        expected_route: Option<InputRouteStamp>,
    ) -> Result<(), InputIngressError> {
        if expected_route.is_some_and(|route| route.window() != window || route.seat() != seat) {
            return Err(InputIngressError::PresentedRouteMismatch { window, seat });
        }
        if self.routes.contains_key(&(window, seat)) {
            let Some(expected_route) = expected_route else {
                return Err(InputIngressError::PresentedRouteMismatch { window, seat });
            };
            if self.pointer_route(window, seat, expected_route.pointer()) != expected_route {
                return Err(InputIngressError::PresentedRouteMismatch { window, seat });
            }
        }
        Ok(())
    }

    fn ensure_endpoint_unreserved(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<(), InputIngressError> {
        if self.prepared_routes.contains_key(&(window, seat)) {
            return Err(InputIngressError::RouteReservationConflict { window, seat });
        }
        Ok(())
    }

    fn unbind_route(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<(), InputIngressError> {
        self.ensure_endpoint_unreserved(window, seat)?;
        if !self.routes.contains_key(&(window, seat)) {
            return Ok(());
        }
        let binding_generation = self.next_binding_generation.checked_add(1).ok_or_else(|| {
            if self.prepared_routes.is_empty() {
                self.poison_routes();
            }
            InputIngressError::RouteBindingGenerationExhausted {
                route_invalidated: self.routes_poisoned,
            }
        })?;
        self.next_binding_generation = binding_generation;
        self.routes.remove(&(window, seat));
        Ok(())
    }

    fn next_binding_generation_for_change(&self) -> Result<u64, InputIngressError> {
        self.next_binding_generation.checked_add(1).ok_or(
            InputIngressError::RouteBindingGenerationExhausted {
                route_invalidated: false,
            },
        )
    }

    fn commit_failed_closed_window(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        binding_generation: u64,
    ) {
        self.focused_windows.retain(|focused_seat, focused_window| {
            *focused_seat != seat || *focused_window != window
        });
        self.routes.remove(&(window, seat));
        self.next_binding_generation = binding_generation;
    }

    fn fail_closed_window(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<(), InputIngressError> {
        self.ensure_endpoint_unreserved(window, seat)?;
        let Some(binding_generation) = self.next_binding_generation.checked_add(1) else {
            if self.prepared_routes.is_empty() {
                self.poison_routes();
            }
            return Err(InputIngressError::RouteBindingGenerationExhausted {
                route_invalidated: self.routes_poisoned,
            });
        };
        self.commit_failed_closed_window(window, seat, binding_generation);
        Ok(())
    }

    fn poison_routes(&mut self) {
        debug_assert!(self.prepared_routes.is_empty());
        self.routes_poisoned = true;
        self.routes.clear();
        self.focused_windows.clear();
    }

    fn clear_physical_route_state(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        clear_keyboard: bool,
    ) {
        if clear_keyboard {
            self.pressed_keys.remove(&(window, seat));
        }
        self.pressed_mouse_buttons
            .retain(|&(event_window, event_seat, _), _| {
                event_window != window || event_seat != seat
            });
        self.clear_pointer_positions(window, seat);
    }

    fn clear_pointer_positions(&mut self, window: InputWindowId, seat: InputSeatId) {
        self.pointer_positions
            .retain(|&(event_window, event_seat, _), _| {
                event_window != window || event_seat != seat
            });
    }

    fn admit_drafts(&mut self, drafts: &[InputDraft]) -> Result<InputAdmission, InputIngressError> {
        if drafts.is_empty() {
            return Ok(InputAdmission::IGNORED);
        }
        if self.retained.len().saturating_add(drafts.len()) > MAX_INPUT_TIMELINE_ENTRIES {
            return Err(InputIngressError::CapacityExceeded {
                limit: MAX_INPUT_TIMELINE_ENTRIES,
                retained: self.retained.len(),
                requested: drafts.len(),
                route_invalidated: false,
            });
        }
        let count =
            u64::try_from(drafts.len()).map_err(|_| InputIngressError::SequenceExhausted {
                requested: drafts.len(),
                route_invalidated: false,
            })?;
        let last_sequence =
            self.next_sequence
                .checked_add(count)
                .ok_or(InputIngressError::SequenceExhausted {
                    requested: drafts.len(),
                    route_invalidated: false,
                })?;
        let first_sequence = self.next_sequence + 1;
        for (offset, draft) in drafts.iter().copied().enumerate() {
            let sequence = first_sequence + u64::try_from(offset).expect("bounded offset fits u64");
            let press_origin = match draft.kind {
                InputEventKind::Key {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                }
                | InputEventKind::PointerButton {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                } => Some((sequence, draft.route)),
                InputEventKind::Key {
                    key,
                    phase: ButtonTransitionPhase::Released,
                } => self
                    .pressed_keys
                    .get(&(draft.route.window(), draft.route.seat()))
                    .and_then(|pressed| pressed.get(&key))
                    .map(|held| (held.press_sequence, held.route)),
                InputEventKind::PointerButton {
                    button,
                    phase: ButtonTransitionPhase::Released,
                } => draft.route.pointer().and_then(|pointer| {
                    self.pressed_mouse_buttons
                        .get(&(draft.route.window(), draft.route.seat(), pointer))
                        .and_then(|pressed| pressed.get(&button))
                        .map(|held| (held.press_sequence, held.route))
                }),
                _ => None,
            };
            self.retained.push_back(RetainedInput {
                entry: InputTimelineEntry {
                    sequence,
                    route: draft.route,
                    logical_position: draft.logical_position,
                    kind: draft.kind,
                    invalidation: draft.invalidation,
                    press_sequence: press_origin.map(|(sequence, _)| sequence),
                    press_route: press_origin.map(|(_, route)| route),
                },
                projection_pending: true,
                observation_pending: true,
                ui_pending: draft.route.is_ui_routable(),
                ui_consumed: false,
                action: ActionRetention::Unscanned,
                action_delivery: draft.action_delivery,
            });
        }
        self.next_sequence = last_sequence;
        Ok(InputAdmission {
            first_sequence: Some(first_sequence),
            last_sequence: Some(last_sequence),
            admitted: drafts.len(),
        })
    }

    pub(crate) fn projection_entries(&self) -> Vec<InputTimelineEntry> {
        self.retained
            .iter()
            .filter(|retained| retained.projection_pending)
            .map(|retained| retained.entry)
            .collect()
    }

    pub(crate) fn finish_projection(&mut self) {
        for retained in &mut self.retained {
            retained.projection_pending = false;
        }
    }

    pub(crate) fn publish_observations(&mut self, observations: &mut InputFrameObservations) {
        observations.entries.clear();
        for retained in &mut self.retained {
            if retained.observation_pending {
                observations.entries.push(retained.entry);
                retained.observation_pending = false;
                self.observation_cursor = retained.entry.sequence;
            }
        }
    }

    pub(crate) fn finalize_ui_routing(&mut self) {
        let mut unresolved_presses = Vec::new();
        for retained in &mut self.retained {
            if retained.ui_pending {
                retained.ui_pending = false;
                if matches!(
                    retained.entry.kind(),
                    InputEventKind::Key {
                        phase: ButtonTransitionPhase::Pressed,
                        ..
                    } | InputEventKind::PointerButton {
                        phase: ButtonTransitionPhase::Pressed,
                        ..
                    }
                ) {
                    unresolved_presses.push(retained.entry);
                }
            }
            self.ui_cursor = self.ui_cursor.max(retained.entry.sequence);
        }
        for entry in unresolved_presses {
            self.resolve_held_press(entry, false);
        }
    }

    pub(crate) fn stage_actions(&mut self, action_map: &ActionMap) {
        for retained in &mut self.retained {
            if retained.action != ActionRetention::Unscanned {
                continue;
            }
            retained.action = match retained.entry.action_transition() {
                Some((binding, _))
                    if action_map.binding_indices_for_input(binding).any(|index| {
                        action_map.binding_at(index).is_some_and(|action_binding| {
                            action_map.is_context_enabled(&action_binding.context)
                        })
                    }) =>
                {
                    ActionRetention::Deferred
                }
                _ => ActionRetention::Done,
            };
            self.action_scanned_cursor = retained.entry.sequence;
        }
    }

    pub(crate) fn take_deferred_actions(&mut self) -> Vec<InputTimelineEntry> {
        let press_delivery = self
            .retained
            .iter()
            .filter(|retained| !retained.ui_pending)
            .map(|retained| (retained.entry.sequence, !retained.ui_consumed))
            .collect::<BTreeMap<_, _>>();
        let mut entries = Vec::new();
        for retained in &mut self.retained {
            if retained.action != ActionRetention::Deferred {
                continue;
            }
            retained.action = ActionRetention::Done;
            let deliver = match retained.action_delivery {
                ActionDelivery::OwnUiDisposition => !retained.ui_consumed,
                ActionDelivery::Gameplay => true,
                ActionDelivery::Ui => false,
                ActionDelivery::FollowPress(sequence) => {
                    press_delivery.get(&sequence).copied().unwrap_or(false)
                }
            };
            if deliver {
                entries.push(retained.entry);
            }
        }
        entries
    }

    pub(crate) fn reclaim_completed(&mut self) {
        while self.retained.front().is_some_and(|retained| {
            !retained.projection_pending
                && !retained.observation_pending
                && !retained.ui_pending
                && retained.action == ActionRetention::Done
        }) {
            self.retained.pop_front();
        }
    }
}

/// Reserves all fallible input-route publication facts before backend submission.
#[doc(hidden)]
pub fn prepare_presented_route(
    timeline: &mut InputTimeline,
    route: InputPresentedRoute,
) -> Result<PreparedInputRoutePublication, InputIngressError> {
    timeline.prepare_presented_route(route)
}

/// Commits one exact prepared route without a fallible post-submit arm.
#[doc(hidden)]
pub fn commit_presented_route(
    timeline: &mut InputTimeline,
    prepared: PreparedInputRoutePublication,
) -> InputRouteStamp {
    timeline.commit_presented_route(prepared)
}

/// Aborts one prepared route and releases its endpoint/capacity reservation.
#[doc(hidden)]
pub fn abort_presented_route(
    timeline: &mut InputTimeline,
    prepared: PreparedInputRoutePublication,
) {
    timeline.release_presented_route_reservation(prepared);
}

/// Supersedes one prepared route and releases its endpoint/capacity reservation.
#[doc(hidden)]
pub fn supersede_presented_route(
    timeline: &mut InputTimeline,
    prepared: PreparedInputRoutePublication,
) {
    timeline.release_presented_route_reservation(prepared);
}

impl __RuntimeDriverPort for InputTimeline {
    type Input = __InputDriverEvent;
    type Output = Result<__InputDriverOutput, InputIngressError>;

    fn accepts_driver_input(state: RuntimeState, input: &Self::Input) -> bool {
        matches!(state, RuntimeState::Running | RuntimeState::Paused)
            || matches!(
                (state, input),
                (
                    RuntimeState::Faulted | RuntimeState::Stopping | RuntimeState::CloseIncomplete,
                    __InputDriverEvent::Window {
                        event: InputWindowEvent::SurfaceRetired,
                        ..
                    }
                )
            )
    }

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.apply_driver_event(input)
    }
}

fn same_presented_lineage(left: InputRouteStamp, right: InputRouteStamp) -> bool {
    left.is_ui_routable()
        && right.is_ui_routable()
        && left.window() == right.window()
        && left.seat() == right.seat()
        && left.pointer() == right.pointer()
        && left.surface() == right.surface()
        && left.slot_generation() == right.slot_generation()
        && left.route_generation() == right.route_generation()
        && left.geometry_generation() == right.geometry_generation()
}

fn vec2_is_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionId, ActionMap};

    fn slot_generation(value: u64) -> InputSlotGeneration {
        InputSlotGeneration::new(value).unwrap()
    }

    fn route_generation(value: u64) -> InputRouteGeneration {
        InputRouteGeneration::new(value).unwrap()
    }

    fn geometry_generation(value: u64) -> InputGeometryGeneration {
        InputGeometryGeneration::new(value).unwrap()
    }

    fn published_timeline() -> InputTimeline {
        let mut timeline = InputTimeline::default();
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                InputWindowId::new(1),
                InputSurfaceId::new(7),
                InputSeatId::PRIMARY,
                InputPresentationEpoch::new(11).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        timeline
            .apply_window(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(true),
            )
            .unwrap();
        timeline.finish_projection();
        let mut observations = InputFrameObservations::default();
        timeline.publish_observations(&mut observations);
        timeline.finalize_ui_routing();
        timeline.stage_actions(&ActionMap::default());
        timeline.reclaim_completed();
        timeline
    }

    #[test]
    fn fault_retirement_admits_only_surface_retirement_input() {
        let retired = __InputDriverEvent::Window {
            window: InputWindowId::new(1),
            seat: InputSeatId::PRIMARY,
            event: InputWindowEvent::SurfaceRetired,
        };
        let focused = __InputDriverEvent::Window {
            window: InputWindowId::new(1),
            seat: InputSeatId::PRIMARY,
            event: InputWindowEvent::Focused(false),
        };

        for state in [
            RuntimeState::Faulted,
            RuntimeState::Stopping,
            RuntimeState::CloseIncomplete,
        ] {
            assert!(<InputTimeline as __RuntimeDriverPort>::accepts_driver_input(state, &retired));
            assert!(!<InputTimeline as __RuntimeDriverPort>::accepts_driver_input(state, &focused));
        }
        assert!(
            <InputTimeline as __RuntimeDriverPort>::accepts_driver_input(
                RuntimeState::Running,
                &focused,
            )
        );
    }

    #[test]
    fn global_sequence_preserves_cross_device_order() {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let pointer = InputPointerId::PRIMARY;

        timeline
            .apply_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(4.0, 8.0)),
            )
            .unwrap();
        timeline
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        timeline
            .apply_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(5.0, 9.0)),
            )
            .unwrap();
        timeline
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Release(MouseButton::Left),
            )
            .unwrap();

        let entries = timeline.entries().copied().collect::<Vec<_>>();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.sequence())
                .collect::<Vec<_>>(),
            [2, 3, 4, 5]
        );
        assert!(matches!(entries[0].kind(), InputEventKind::PointerMoved));
        assert!(matches!(
            entries[1].kind(),
            InputEventKind::PointerButton {
                phase: ButtonTransitionPhase::Pressed,
                ..
            }
        ));
        assert_eq!(entries[1].logical_position(), Some(Vec2::new(4.0, 8.0)));
        assert_eq!(entries[3].logical_position(), Some(Vec2::new(5.0, 9.0)));
    }

    #[test]
    fn keyboard_uses_focused_surface_while_unpublished_window_is_unrouted() {
        let mut timeline = published_timeline();
        timeline
            .apply_keyboard(
                InputWindowId::new(99),
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Enter),
            )
            .unwrap();
        timeline
            .apply_keyboard(
                InputWindowId::new(2),
                InputSeatId::new(8),
                ButtonDriverInput::Press(KeyCode::Space),
            )
            .unwrap();

        let entries = timeline.entries().copied().collect::<Vec<_>>();
        assert_eq!(entries[0].route().window(), InputWindowId::new(1));
        assert_eq!(entries[0].route().surface(), Some(InputSurfaceId::new(7)));
        assert!(entries[0].route().is_ui_routable());
        assert_eq!(entries[1].route().window(), InputWindowId::new(2));
        assert!(!entries[1].route().is_ui_routable());
    }

    #[test]
    fn presented_route_ownership_is_idempotent_conflict_safe_and_releasable() {
        let mut timeline = InputTimeline::default();
        let window = InputWindowId::new(3);
        let seat = InputSeatId::new(4);
        let first = InputSurfaceId::new(10);
        let second = InputSurfaceId::new(20);
        let epoch = InputPresentationEpoch::new(7).unwrap();
        let slot = slot_generation(3);
        let geometry = geometry_generation(5);

        let initial = timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                first,
                seat,
                epoch,
                slot,
                route_generation(4),
                geometry,
            ))
            .unwrap();
        let repeated = timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                first,
                seat,
                epoch,
                slot,
                route_generation(4),
                geometry,
            ))
            .unwrap();
        assert_eq!(repeated, initial);
        assert_eq!(initial.slot_generation(), Some(slot));
        assert_eq!(initial.geometry_generation(), Some(geometry));

        let next_geometry = geometry_generation(6);
        let advanced = timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                first,
                seat,
                InputPresentationEpoch::new(8).unwrap(),
                slot,
                route_generation(5),
                next_geometry,
            ))
            .unwrap();
        assert!(advanced.binding_generation() > initial.binding_generation());
        assert_eq!(initial.route_generation(), Some(route_generation(4)));
        assert_eq!(advanced.route_generation(), Some(route_generation(5)));
        assert_eq!(advanced.slot_generation(), Some(slot));
        assert_eq!(advanced.geometry_generation(), Some(next_geometry));

        assert_eq!(
            timeline.publish_presented_route(InputPresentedRoute::new(
                window,
                second,
                seat,
                InputPresentationEpoch::new(9).unwrap(),
                slot_generation(4),
                route_generation(4),
                geometry_generation(1),
            )),
            Err(InputIngressError::RouteConflict {
                window,
                seat,
                current_surface: first,
                requested_surface: second,
            })
        );
        assert_eq!(timeline.pointer_route(window, seat, None), advanced);
        assert_eq!(
            timeline.publish_presented_route(InputPresentedRoute::new(
                window,
                first,
                seat,
                InputPresentationEpoch::new(7).unwrap(),
                slot_generation(2),
                route_generation(6),
                geometry_generation(6),
            )),
            Err(InputIngressError::PresentedRouteRegression { window, seat })
        );
        assert_eq!(
            timeline.unbind_presented_endpoint(window, seat, Some(initial)),
            Err(InputIngressError::PresentedRouteMismatch { window, seat })
        );
        timeline
            .unbind_presented_endpoint(window, seat, Some(advanced))
            .unwrap();
        assert_eq!(
            timeline.unbind_presented_endpoint(window, seat, Some(advanced)),
            Ok(())
        );

        let reclaimed = timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                second,
                seat,
                InputPresentationEpoch::new(9).unwrap(),
                slot_generation(4),
                route_generation(4),
                geometry_generation(1),
            ))
            .unwrap();
        assert_eq!(reclaimed.surface(), Some(second));
    }

    #[test]
    fn prepared_route_reserves_the_endpoint_without_changing_active_authority() {
        let mut timeline = InputTimeline::default();
        let window = InputWindowId::new(31);
        let seat = InputSeatId::PRIMARY;
        let surface = InputSurfaceId::new(41);
        let initial = timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                surface,
                seat,
                InputPresentationEpoch::new(1).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        let next_route = InputPresentedRoute::new(
            window,
            surface,
            seat,
            InputPresentationEpoch::new(2).unwrap(),
            slot_generation(1),
            route_generation(2),
            geometry_generation(2),
        );

        let prepared = prepare_presented_route(&mut timeline, next_route).unwrap();

        assert_eq!(timeline.pointer_route(window, seat, None), initial);
        assert_eq!(
            timeline.publish_presented_route(next_route),
            Err(InputIngressError::RouteReservationConflict { window, seat })
        );
        assert_eq!(
            timeline.unbind_presented_endpoint(window, seat, Some(initial)),
            Err(InputIngressError::RouteReservationConflict { window, seat })
        );
        assert_eq!(
            timeline.retire_presented_endpoint(window, seat, Some(initial)),
            Err(InputIngressError::RouteReservationConflict { window, seat })
        );
        assert_eq!(
            timeline.admit_window_event(
                window,
                seat,
                InputWindowEvent::Resized {
                    logical_size: Vec2::new(640.0, 480.0),
                },
            ),
            Err(InputIngressError::RouteReservationConflict { window, seat })
        );
        let committed = commit_presented_route(&mut timeline, prepared);
        assert_eq!(
            committed.presentation_epoch(),
            Some(next_route.presentation_epoch())
        );
        assert_eq!(
            committed.route_generation(),
            Some(next_route.route_generation())
        );
        assert_eq!(
            committed.geometry_generation(),
            Some(next_route.geometry_generation())
        );
        assert!(committed.binding_generation() > initial.binding_generation());
        assert_eq!(timeline.pointer_route(window, seat, None), committed);
    }

    #[test]
    fn aborted_and_superseded_route_reservations_release_the_endpoint() {
        let mut timeline = InputTimeline::default();
        let window = InputWindowId::new(32);
        let seat = InputSeatId::PRIMARY;
        let route = InputPresentedRoute::new(
            window,
            InputSurfaceId::new(42),
            seat,
            InputPresentationEpoch::new(1).unwrap(),
            slot_generation(1),
            route_generation(1),
            geometry_generation(1),
        );

        let aborted = prepare_presented_route(&mut timeline, route).unwrap();
        assert!(!timeline.pointer_route(window, seat, None).is_ui_routable());
        abort_presented_route(&mut timeline, aborted);

        let superseded = prepare_presented_route(&mut timeline, route).unwrap();
        supersede_presented_route(&mut timeline, superseded);

        let published = timeline.publish_presented_route(route).unwrap();
        assert_eq!(published.surface(), Some(route.surface()));
    }

    #[test]
    fn prepared_routes_reserve_capacity_until_released() {
        let mut timeline = InputTimeline::default();
        for raw in 1..MAX_INPUT_ROUTES {
            let raw = u64::try_from(raw).unwrap();
            timeline
                .publish_presented_route(InputPresentedRoute::new(
                    InputWindowId::new(raw),
                    InputSurfaceId::new(raw),
                    InputSeatId::PRIMARY,
                    InputPresentationEpoch::new(1).unwrap(),
                    slot_generation(1),
                    route_generation(1),
                    geometry_generation(1),
                ))
                .unwrap();
        }
        let reserved_route = InputPresentedRoute::new(
            InputWindowId::new(100),
            InputSurfaceId::new(100),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(1).unwrap(),
            slot_generation(1),
            route_generation(1),
            geometry_generation(1),
        );
        let prepared = prepare_presented_route(&mut timeline, reserved_route).unwrap();
        let overflow_route = InputPresentedRoute::new(
            InputWindowId::new(101),
            InputSurfaceId::new(101),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(1).unwrap(),
            slot_generation(1),
            route_generation(1),
            geometry_generation(1),
        );

        assert!(matches!(
            prepare_presented_route(&mut timeline, overflow_route),
            Err(InputIngressError::RouteCapacityExceeded {
                limit: MAX_INPUT_ROUTES,
            })
        ));
        abort_presented_route(&mut timeline, prepared);
        let reusable = prepare_presented_route(&mut timeline, overflow_route).unwrap();
        abort_presented_route(&mut timeline, reusable);
    }

    #[test]
    fn prepared_route_rejects_regression_and_exhaustion_before_reserving() {
        let mut timeline = InputTimeline::default();
        let window = InputWindowId::new(33);
        let seat = InputSeatId::PRIMARY;
        let surface = InputSurfaceId::new(43);
        let active = timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                surface,
                seat,
                InputPresentationEpoch::new(5).unwrap(),
                slot_generation(2),
                route_generation(3),
                geometry_generation(4),
            ))
            .unwrap();
        let regression = InputPresentedRoute::new(
            window,
            surface,
            seat,
            InputPresentationEpoch::new(4).unwrap(),
            slot_generation(2),
            route_generation(3),
            geometry_generation(4),
        );

        assert!(matches!(
            prepare_presented_route(&mut timeline, regression),
            Err(InputIngressError::PresentedRouteRegression {
                window: error_window,
                seat: error_seat,
            }) if error_window == window && error_seat == seat
        ));
        timeline.next_binding_generation = u64::MAX;
        let advancing = InputPresentedRoute::new(
            window,
            surface,
            seat,
            InputPresentationEpoch::new(6).unwrap(),
            slot_generation(2),
            route_generation(4),
            geometry_generation(5),
        );
        let error = prepare_presented_route(&mut timeline, advancing).unwrap_err();
        assert!(matches!(
            error,
            InputIngressError::RouteBindingGenerationExhausted {
                route_invalidated: false,
            }
        ));
        assert!(!error.route_was_invalidated());
        assert_eq!(timeline.pointer_route(window, seat, None), active);
        assert!(timeline.prepared_routes.is_empty());
    }

    #[test]
    fn unbind_generation_exhaustion_reports_fail_closed_invalidation() {
        let mut timeline = InputTimeline::default();
        let window = InputWindowId::new(33);
        let seat = InputSeatId::PRIMARY;
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                InputSurfaceId::new(43),
                seat,
                InputPresentationEpoch::new(5).unwrap(),
                slot_generation(2),
                route_generation(3),
                geometry_generation(4),
            ))
            .unwrap();
        timeline.next_binding_generation = u64::MAX;

        let error = timeline.unbind_route(window, seat).unwrap_err();

        assert!(matches!(
            error,
            InputIngressError::RouteBindingGenerationExhausted {
                route_invalidated: true,
            }
        ));
        assert!(error.route_was_invalidated());
        assert!(timeline.routes_poisoned);
        assert_eq!(
            timeline.pointer_route(window, seat, None),
            InputRouteStamp::unrouted(window, seat, None)
        );
    }

    #[test]
    fn unbinding_a_route_preserves_the_platform_keyboard_focus() {
        let mut timeline = InputTimeline::default();
        let seat = InputSeatId::PRIMARY;
        let first_window = InputWindowId::new(3);
        let second_window = InputWindowId::new(4);
        let first = timeline
            .publish_presented_route(InputPresentedRoute::new(
                first_window,
                InputSurfaceId::new(10),
                seat,
                InputPresentationEpoch::new(1).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                second_window,
                InputSurfaceId::new(20),
                seat,
                InputPresentationEpoch::new(2).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        timeline
            .apply_window(first_window, seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .unbind_presented_endpoint(first_window, seat, Some(first))
            .unwrap();
        timeline
            .apply_keyboard(
                second_window,
                seat,
                ButtonDriverInput::Press(KeyCode::Enter),
            )
            .unwrap();

        let route = timeline
            .entries()
            .last()
            .expect("keyboard input should be retained")
            .route();
        assert_eq!(route.window(), first_window);
        assert_eq!(route.surface(), None);
        assert!(!route.is_ui_routable());
    }

    #[test]
    fn keyboard_releases_follow_the_physical_origin_after_focus_migration() {
        let mut timeline = InputTimeline::default();
        let seat = InputSeatId::PRIMARY;
        let first_window = InputWindowId::new(3);
        let second_window = InputWindowId::new(4);
        let first = timeline
            .publish_presented_route(InputPresentedRoute::new(
                first_window,
                InputSurfaceId::new(10),
                seat,
                InputPresentationEpoch::new(1).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                second_window,
                InputSurfaceId::new(20),
                seat,
                InputPresentationEpoch::new(2).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        timeline
            .apply_window(first_window, seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .apply_keyboard(first_window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        timeline
            .apply_keyboard(first_window, seat, ButtonDriverInput::Press(KeyCode::Space))
            .unwrap();
        timeline
            .unbind_presented_endpoint(first_window, seat, Some(first))
            .unwrap();
        timeline
            .apply_window(second_window, seat, InputWindowEvent::Focused(true))
            .unwrap();

        let individual_release = timeline
            .apply_keyboard(
                second_window,
                seat,
                ButtonDriverInput::Release(KeyCode::Enter),
            )
            .unwrap();
        assert_eq!(individual_release.admission.admitted(), 1);
        assert!(!timeline.entries().last().unwrap().route().is_ui_routable());

        let release_all = timeline
            .apply_keyboard(second_window, seat, ButtonDriverInput::ReleaseAll)
            .unwrap();
        assert_eq!(release_all.released_keys, [KeyCode::Space]);
        assert!(!timeline.entries().last().unwrap().route().is_ui_routable());
        assert!(timeline.pressed_key_window(seat, KeyCode::Enter).is_none());
        assert!(timeline.pressed_key_window(seat, KeyCode::Space).is_none());
    }

    #[test]
    fn unbinding_a_presented_route_preserves_balanced_physical_edges() {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let pointer = InputPointerId::PRIMARY;
        timeline
            .apply_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(20.0, 30.0)),
            )
            .unwrap();
        timeline
            .apply_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        timeline
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        let presented = timeline.pointer_route(window, seat, None);

        timeline
            .unbind_presented_endpoint(window, seat, Some(presented))
            .unwrap();

        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                InputSurfaceId::new(7),
                seat,
                InputPresentationEpoch::new(1).unwrap(),
                slot_generation(2),
                route_generation(2),
                geometry_generation(2),
            ))
            .unwrap();

        assert!(
            !timeline
                .pointer_positions
                .contains_key(&(window, seat, pointer))
        );

        let mouse_release = timeline
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Release(MouseButton::Left),
            )
            .unwrap();
        let key_release = timeline
            .apply_keyboard(window, seat, ButtonDriverInput::ReleaseAll)
            .unwrap();
        assert_eq!(mouse_release.admission.admitted(), 1);
        assert_eq!(key_release.admission.admitted(), 1);
        assert_eq!(key_release.released_keys, [KeyCode::Enter]);
        let entries = timeline.entries().copied().collect::<Vec<_>>();
        let releases = &entries[entries.len() - 2..];
        assert!(releases.iter().all(|entry| !entry.route().is_ui_routable()));
        assert!(releases.iter().all(|entry| entry.invalidation().is_none()));
        assert_eq!(
            releases
                .iter()
                .find(|entry| {
                    matches!(
                        entry.kind(),
                        InputEventKind::PointerButton {
                            button: MouseButton::Left,
                            phase: ButtonTransitionPhase::Released,
                        }
                    )
                })
                .and_then(|entry| entry.logical_position()),
            None
        );
        assert!(releases.iter().any(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::PointerButton {
                    button: MouseButton::Left,
                    phase: ButtonTransitionPhase::Released,
                }
            )
        }));
        assert!(releases.iter().any(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Enter,
                    phase: ButtonTransitionPhase::Released,
                }
            )
        }));
    }

    #[test]
    fn presented_endpoint_retirement_balances_edges_and_allows_endpoint_reuse() {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let pointer = InputPointerId::PRIMARY;
        let presented = timeline.pointer_route(window, seat, None);
        timeline
            .apply_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(20.0, 30.0)),
            )
            .unwrap();
        timeline
            .apply_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        timeline
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        let before_retirement = timeline.entries().count();

        let retirement = timeline
            .retire_presented_endpoint(window, seat, Some(presented))
            .unwrap();

        assert_eq!(retirement.released_keys(), [KeyCode::Enter]);
        assert_eq!(retirement.released_mouse_buttons(), [MouseButton::Left]);
        assert_eq!(retirement.admission().admitted(), 4);
        assert!(!timeline.routes.contains_key(&(window, seat)));
        assert!(!timeline.focused_windows.contains_key(&seat));
        assert!(!timeline.pressed_keys.contains_key(&(window, seat)));
        assert!(
            !timeline
                .pressed_mouse_buttons
                .contains_key(&(window, seat, pointer))
        );
        assert!(
            !timeline
                .pointer_positions
                .contains_key(&(window, seat, pointer))
        );
        let lifecycle_entries = timeline
            .entries()
            .skip(before_retirement)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(lifecycle_entries.len(), 4);
        assert!(lifecycle_entries.iter().all(|entry| {
            entry.invalidation() == Some(InputInvalidationReason::SurfaceRetired)
        }));
        assert!(matches!(
            lifecycle_entries[0].kind(),
            InputEventKind::Key {
                key: KeyCode::Enter,
                phase: ButtonTransitionPhase::Released,
            }
        ));
        assert!(matches!(
            lifecycle_entries[1].kind(),
            InputEventKind::PointerButton {
                button: MouseButton::Left,
                phase: ButtonTransitionPhase::Released,
            }
        ));
        assert!(matches!(
            lifecycle_entries[2].kind(),
            InputEventKind::PointerLeft
        ));
        assert!(matches!(
            lifecycle_entries[3].kind(),
            InputEventKind::SurfaceRetired
        ));

        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                InputSurfaceId::new(8),
                seat,
                InputPresentationEpoch::new(1).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(1),
            ))
            .unwrap();
        timeline
            .apply_window(window, seat, InputWindowEvent::Focused(true))
            .unwrap();
        assert_eq!(
            timeline
                .apply_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter),)
                .unwrap()
                .admission
                .admitted(),
            1
        );
        assert_eq!(
            timeline
                .apply_mouse(
                    window,
                    seat,
                    pointer,
                    ButtonDriverInput::Press(MouseButton::Left),
                )
                .unwrap()
                .admission
                .admitted(),
            1
        );
    }

    #[test]
    fn retirement_releases_follow_the_consumer_that_received_each_press() {
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let pointer = InputPointerId::PRIMARY;
        let mut actions = ActionMap::default();
        actions.bind_key(ActionId::new("confirm").unwrap(), KeyCode::Enter);
        actions.bind_mouse(ActionId::new("fire").unwrap(), MouseButton::Left);

        let mut gameplay_owned = published_timeline();
        let gameplay_route = gameplay_owned.pointer_route(window, seat, None);
        gameplay_owned
            .apply_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        gameplay_owned
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        gameplay_owned.finalize_ui_routing();
        gameplay_owned.stage_actions(&actions);
        assert_eq!(gameplay_owned.take_deferred_actions().len(), 2);

        gameplay_owned
            .retire_presented_endpoint(window, seat, Some(gameplay_route))
            .unwrap();
        gameplay_owned.finalize_ui_routing();
        gameplay_owned.stage_actions(&actions);
        let gameplay_releases = gameplay_owned.take_deferred_actions();
        assert_eq!(gameplay_releases.len(), 2);
        assert!(gameplay_releases.iter().any(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Enter,
                    phase: ButtonTransitionPhase::Released,
                }
            )
        }));
        assert!(gameplay_releases.iter().any(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::PointerButton {
                    button: MouseButton::Left,
                    phase: ButtonTransitionPhase::Released,
                }
            )
        }));
        assert!(
            gameplay_releases
                .iter()
                .all(|entry| !entry.route().is_ui_routable())
        );

        let mut ui_owned = published_timeline();
        let ui_route = ui_owned.pointer_route(window, seat, None);
        let key_press = ui_owned
            .apply_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap()
            .admission;
        let mouse_press = ui_owned
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap()
            .admission;
        ui_owned
            .__finalize_ui_entry(
                key_press.first_sequence().unwrap(),
                InputUiDisposition::Consumed,
            )
            .unwrap();
        ui_owned
            .__finalize_ui_entry(
                mouse_press.first_sequence().unwrap(),
                InputUiDisposition::Consumed,
            )
            .unwrap();
        ui_owned.finalize_ui_routing();
        ui_owned.stage_actions(&actions);
        assert!(ui_owned.take_deferred_actions().is_empty());

        ui_owned
            .retire_presented_endpoint(window, seat, Some(ui_route))
            .unwrap();
        ui_owned.finalize_ui_routing();
        ui_owned.stage_actions(&actions);
        assert!(ui_owned.take_deferred_actions().is_empty());
    }

    #[test]
    fn retained_entries_keep_their_exact_slot_and_geometry_generations() {
        let mut timeline = InputTimeline::default();
        let window = InputWindowId::new(3);
        let surface = InputSurfaceId::new(10);
        let seat = InputSeatId::PRIMARY;
        let first_slot = slot_generation(7);
        let first_geometry = geometry_generation(11);
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                surface,
                seat,
                InputPresentationEpoch::new(13).unwrap(),
                first_slot,
                route_generation(9),
                first_geometry,
            ))
            .unwrap();
        timeline
            .admit_pointer(
                window,
                seat,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
            )
            .unwrap();

        let second_slot = slot_generation(8);
        let second_geometry = geometry_generation(12);
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                surface,
                seat,
                InputPresentationEpoch::new(14).unwrap(),
                second_slot,
                route_generation(10),
                second_geometry,
            ))
            .unwrap();
        timeline
            .admit_pointer(
                window,
                seat,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(3.0, 4.0)),
            )
            .unwrap();

        let routes = timeline
            .pending_ui_entries()
            .map(|entry| entry.route())
            .collect::<Vec<_>>();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].slot_generation(), Some(first_slot));
        assert_eq!(routes[0].route_generation(), Some(route_generation(9)));
        assert_eq!(routes[0].geometry_generation(), Some(first_geometry));
        assert_eq!(routes[1].slot_generation(), Some(second_slot));
        assert_eq!(routes[1].route_generation(), Some(route_generation(10)));
        assert_eq!(routes[1].geometry_generation(), Some(second_geometry));
        assert!(routes[1].binding_generation() > routes[0].binding_generation());
    }

    #[test]
    fn windows_seats_and_pointers_keep_distinct_event_time_routes() {
        let mut timeline = InputTimeline::default();
        let first_window = InputWindowId::new(1);
        let second_window = InputWindowId::new(2);
        let first_seat = InputSeatId::PRIMARY;
        let second_seat = InputSeatId::new(5);
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                first_window,
                InputSurfaceId::new(10),
                first_seat,
                InputPresentationEpoch::new(100).unwrap(),
                slot_generation(1),
                route_generation(1),
                geometry_generation(2),
            ))
            .unwrap();
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                second_window,
                InputSurfaceId::new(20),
                second_seat,
                InputPresentationEpoch::new(200).unwrap(),
                slot_generation(3),
                route_generation(3),
                geometry_generation(4),
            ))
            .unwrap();
        timeline
            .apply_window(first_window, first_seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .apply_window(second_window, second_seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .apply_pointer(
                first_window,
                first_seat,
                InputPointerId::new(3),
                PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
            )
            .unwrap();
        timeline
            .apply_pointer(
                second_window,
                second_seat,
                InputPointerId::new(4),
                PointerDriverInput::Moved(Vec2::new(3.0, 4.0)),
            )
            .unwrap();

        let routes = timeline
            .entries()
            .filter(|entry| matches!(entry.kind(), InputEventKind::PointerMoved))
            .map(|entry| entry.route())
            .collect::<Vec<_>>();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].window(), first_window);
        assert_eq!(routes[0].surface(), Some(InputSurfaceId::new(10)));
        assert_eq!(routes[0].seat(), first_seat);
        assert_eq!(routes[0].pointer(), Some(InputPointerId::new(3)));
        assert_eq!(
            routes[0].presentation_epoch(),
            InputPresentationEpoch::new(100)
        );
        assert_eq!(routes[1].window(), second_window);
        assert_eq!(routes[1].surface(), Some(InputSurfaceId::new(20)));
        assert_eq!(routes[1].seat(), second_seat);
        assert_eq!(routes[1].pointer(), Some(InputPointerId::new(4)));
        assert_eq!(
            routes[1].presentation_epoch(),
            InputPresentationEpoch::new(200)
        );
    }

    #[test]
    fn pointer_leave_releases_only_the_exact_pointer_state() {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let first_pointer = InputPointerId::new(3);
        let second_pointer = InputPointerId::new(4);
        for (index, pointer) in [first_pointer, second_pointer].into_iter().enumerate() {
            timeline
                .apply_pointer(
                    window,
                    seat,
                    pointer,
                    PointerDriverInput::Moved(Vec2::new(index as f32, 2.0)),
                )
                .unwrap();
            timeline
                .apply_mouse(
                    window,
                    seat,
                    pointer,
                    ButtonDriverInput::Press(MouseButton::Left),
                )
                .unwrap();
        }

        let output = timeline
            .apply_pointer(window, seat, first_pointer, PointerDriverInput::Left)
            .unwrap();

        assert_eq!(output.released_mouse_buttons, [MouseButton::Left]);
        assert!(
            !timeline
                .pressed_mouse_buttons
                .contains_key(&(window, seat, first_pointer))
        );
        assert!(
            timeline
                .pressed_mouse_buttons
                .get(&(window, seat, second_pointer))
                .is_some_and(|pressed| pressed.contains_key(&MouseButton::Left))
        );
        let entries = timeline.entries().copied().collect::<Vec<_>>();
        let cancellation = &entries[entries.len() - 2..];
        assert!(matches!(
            cancellation[0].kind(),
            InputEventKind::PointerButton {
                button: MouseButton::Left,
                phase: ButtonTransitionPhase::Released,
            }
        ));
        assert_eq!(cancellation[0].route().pointer(), Some(first_pointer));
        assert!(matches!(
            cancellation[1].kind(),
            InputEventKind::PointerLeft
        ));
        assert_eq!(cancellation[1].route().pointer(), Some(first_pointer));

        assert_eq!(
            timeline
                .apply_mouse(
                    window,
                    seat,
                    second_pointer,
                    ButtonDriverInput::Release(MouseButton::Left),
                )
                .unwrap()
                .admission
                .admitted(),
            1
        );
    }

    #[test]
    fn losing_an_unfocused_window_does_not_release_the_focused_window_keys() {
        let mut timeline = InputTimeline::default();
        let first_window = InputWindowId::new(1);
        let second_window = InputWindowId::new(2);
        let seat = InputSeatId::PRIMARY;
        for (window, surface, epoch) in [(first_window, 10, 100), (second_window, 20, 200)] {
            timeline
                .publish_presented_route(InputPresentedRoute::new(
                    window,
                    InputSurfaceId::new(surface),
                    seat,
                    InputPresentationEpoch::new(epoch).unwrap(),
                    slot_generation(surface),
                    route_generation(surface),
                    geometry_generation(epoch),
                ))
                .unwrap();
        }
        timeline
            .apply_window(first_window, seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .apply_window(second_window, seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .apply_keyboard(
                second_window,
                seat,
                ButtonDriverInput::Press(KeyCode::Enter),
            )
            .unwrap();

        let focus_loss = timeline
            .apply_window(first_window, seat, InputWindowEvent::Focused(false))
            .unwrap();

        assert!(focus_loss.released_keys.is_empty());
        assert!(
            timeline
                .pressed_keys
                .get(&(second_window, seat))
                .is_some_and(|pressed| pressed.contains_key(&KeyCode::Enter))
        );
        assert_eq!(
            timeline
                .apply_keyboard(
                    second_window,
                    seat,
                    ButtonDriverInput::Release(KeyCode::Enter),
                )
                .unwrap()
                .admission
                .admitted(),
            1
        );
    }

    #[test]
    fn independent_cursors_and_ui_consumption_preserve_paused_action_edges() {
        let mut timeline = published_timeline();
        let mut actions = ActionMap::default();
        actions.bind_key(ActionId::new("confirm").unwrap(), KeyCode::Enter);
        timeline
            .apply_pointer(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
            )
            .unwrap();
        timeline
            .apply_keyboard(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Enter),
            )
            .unwrap();

        let mut observations = InputFrameObservations::default();
        timeline.publish_observations(&mut observations);
        assert_eq!(observations.as_slice().len(), 2);
        assert_eq!(timeline.cursors().observation(), 3);
        assert_eq!(timeline.cursors().ui(), 1);

        timeline
            .__finalize_ui_entry(3, InputUiDisposition::Consumed)
            .unwrap();
        timeline.finalize_ui_routing();
        timeline.stage_actions(&actions);
        let cursors = timeline.cursors();
        assert_eq!(cursors.ui(), 3);
        assert_eq!(cursors.action_scanned(), 3);
        assert_eq!(cursors.oldest_deferred_action(), Some(3));
        assert!(timeline.take_deferred_actions().is_empty());
        assert_eq!(timeline.cursors().oldest_deferred_action(), None);
    }

    #[test]
    fn full_timeline_still_invalidates_a_surface_route() {
        let mut timeline = published_timeline();
        for index in 0..MAX_INPUT_TIMELINE_ENTRIES {
            timeline
                .admit_gamepad_button(
                    InputWindowId::new(1),
                    InputSeatId::PRIMARY,
                    0,
                    u16::try_from(index % usize::from(u16::MAX)).unwrap(),
                    ButtonTransitionPhase::Pressed,
                )
                .unwrap();
        }
        let error = timeline
            .apply_window(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputWindowEvent::SurfaceRetired,
            )
            .unwrap_err();

        assert!(error.route_was_invalidated());
        assert!(
            !timeline
                .pointer_route(
                    InputWindowId::new(1),
                    InputSeatId::PRIMARY,
                    Some(InputPointerId::PRIMARY),
                )
                .is_ui_routable()
        );
        assert_eq!(timeline.retained_len(), MAX_INPUT_TIMELINE_ENTRIES);
    }

    #[test]
    fn coordinated_endpoint_retirement_aborts_atomically_when_timeline_is_full() {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let pointer = InputPointerId::PRIMARY;
        timeline
            .apply_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(20.0, 30.0)),
            )
            .unwrap();
        timeline
            .apply_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        timeline
            .apply_mouse(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        let route = timeline.pointer_route(window, seat, None);
        while timeline.retained_len() < MAX_INPUT_TIMELINE_ENTRIES {
            let button = u16::try_from(timeline.retained_len()).unwrap();
            timeline
                .admit_gamepad_button(window, seat, 0, button, ButtonTransitionPhase::Pressed)
                .unwrap();
        }

        assert_eq!(
            timeline.retire_presented_endpoint(window, seat, Some(route)),
            Err(InputIngressError::CapacityExceeded {
                limit: MAX_INPUT_TIMELINE_ENTRIES,
                retained: MAX_INPUT_TIMELINE_ENTRIES,
                requested: 4,
                route_invalidated: false,
            })
        );
        assert_eq!(timeline.pointer_route(window, seat, None), route);
        assert!(timeline.pressed_keys.contains_key(&(window, seat)));
        assert!(
            timeline
                .pressed_mouse_buttons
                .contains_key(&(window, seat, pointer))
        );
        assert!(
            timeline
                .pointer_positions
                .contains_key(&(window, seat, pointer))
        );
        assert_eq!(timeline.retained_len(), MAX_INPUT_TIMELINE_ENTRIES);
    }

    #[test]
    fn sequence_exhaustion_does_not_partially_publish_or_change_pressed_state() {
        let mut timeline = published_timeline();
        timeline.next_sequence = u64::MAX;

        let error = timeline
            .apply_keyboard(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Space),
            )
            .unwrap_err();

        assert_eq!(
            error,
            InputIngressError::SequenceExhausted {
                requested: 1,
                route_invalidated: false,
            }
        );
        assert!(timeline.entries().next().is_none());
        assert!(
            !timeline
                .pressed_keys
                .get(&(InputWindowId::new(1), InputSeatId::PRIMARY))
                .is_some_and(|pressed| pressed.contains_key(&KeyCode::Space))
        );
    }
}
