use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    InputAdmission, InputConsumerCursors, InputEventKind, InputFrameObservations,
    InputIngressError, InputInvalidationReason, InputPointerId, InputPresentationEpoch,
    InputRouteStamp, InputSeatId, InputSurfaceId, InputTimelineEntry, InputUiCursorError,
    InputUiDisposition, InputWindowEvent, InputWindowId, MAX_INPUT_ROUTES,
    MAX_INPUT_TIMELINE_ENTRIES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionRetention {
    Unscanned,
    Deferred,
    Done,
}

#[derive(Debug, Clone, Copy)]
struct RetainedInput {
    entry: InputTimelineEntry,
    projection_pending: bool,
    observation_pending: bool,
    ui_pending: bool,
    ui_consumed: bool,
    action: ActionRetention,
}

#[derive(Debug, Clone, Copy)]
struct RouteState {
    surface: InputSurfaceId,
    presentation_epoch: InputPresentationEpoch,
    generation: u64,
}

#[derive(Debug, Clone, Copy)]
struct InputDraft {
    route: InputRouteStamp,
    logical_position: Option<Vec2>,
    kind: InputEventKind,
    invalidation: Option<InputInvalidationReason>,
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
    focused_windows: BTreeMap<InputSeatId, InputWindowId>,
    next_route_generation: u64,
    routes_poisoned: bool,
    pressed_keys: BTreeMap<(InputWindowId, InputSeatId), BTreeSet<KeyCode>>,
    pressed_mouse_buttons:
        BTreeMap<(InputWindowId, InputSeatId, InputPointerId), BTreeSet<MouseButton>>,
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
            focused_windows: BTreeMap::new(),
            next_route_generation: 0,
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

    pub fn publish_presented_route(
        &mut self,
        window: InputWindowId,
        surface: InputSurfaceId,
        seat: InputSeatId,
        presentation_epoch: InputPresentationEpoch,
    ) -> Result<InputRouteStamp, InputIngressError> {
        if self.routes_poisoned {
            return Err(InputIngressError::RouteGenerationExhausted);
        }
        let key = (window, seat);
        if !self.routes.contains_key(&key) && self.routes.len() >= MAX_INPUT_ROUTES {
            return Err(InputIngressError::RouteCapacityExceeded {
                limit: MAX_INPUT_ROUTES,
            });
        }
        if let Some(current) = self.routes.get(&key) {
            if current.surface != surface {
                return Err(InputIngressError::RouteConflict {
                    window,
                    seat,
                    current_surface: current.surface,
                    requested_surface: surface,
                });
            }
            if current.presentation_epoch == presentation_epoch {
                return Ok(self.pointer_route(window, seat, None));
            }
        }
        let generation = self.next_route_generation.checked_add(1).ok_or_else(|| {
            self.poison_routes();
            InputIngressError::RouteGenerationExhausted
        })?;
        self.next_route_generation = generation;
        self.routes.insert(
            key,
            RouteState {
                surface,
                presentation_epoch,
                generation,
            },
        );
        Ok(self.pointer_route(window, seat, None))
    }

    #[doc(hidden)]
    pub fn __finalize_ui_entry(
        &mut self,
        sequence: u64,
        disposition: InputUiDisposition,
    ) -> Result<(), InputUiCursorError> {
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
        Ok(())
    }

    /// Releases one endpoint only when it is still owned by the matching surface.
    pub fn release_presented_route(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        surface: InputSurfaceId,
    ) -> Result<bool, InputIngressError> {
        let key = (window, seat);
        if !self
            .routes
            .get(&key)
            .is_some_and(|route| route.surface == surface)
        {
            return Ok(false);
        }
        let generation = self.next_route_generation.checked_add(1).ok_or_else(|| {
            self.poison_routes();
            InputIngressError::RouteGenerationExhausted
        })?;
        self.next_route_generation = generation;
        self.routes.remove(&key);
        Ok(true)
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
        };
        self.admit_drafts(&[draft])
    }

    pub fn admit_accessibility_action(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        action: u32,
    ) -> Result<InputAdmission, InputIngressError> {
        let draft = InputDraft {
            route: self.focused_route(window, seat),
            logical_position: None,
            kind: InputEventKind::AccessibilityAction { action },
            invalidation: None,
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
            __InputDriverEvent::Accessibility {
                window,
                seat,
                action,
            } => self
                .admit_accessibility_action(window, seat, action)
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
                let state_key = (draft.route.window(), seat);
                if self
                    .pressed_keys
                    .get(&state_key)
                    .is_some_and(|pressed| pressed.contains(&key))
                {
                    return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
                }
                let admission = self.admit_drafts(&[draft])?;
                self.pressed_keys.entry(state_key).or_default().insert(key);
                Ok(__InputDriverOutput::new(admission))
            }
            ButtonDriverInput::Release(key) => {
                let draft = self.key_draft(window, seat, key, ButtonTransitionPhase::Released);
                let state_key = (draft.route.window(), seat);
                if !self
                    .pressed_keys
                    .get(&state_key)
                    .is_some_and(|pressed| pressed.contains(&key))
                {
                    return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
                }
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
                        pressed.iter().copied().map(|key| (*event_window, key))
                    })
                    .collect::<Vec<_>>();
                let drafts = released
                    .iter()
                    .copied()
                    .map(|(event_window, key)| InputDraft {
                        route: self.pointer_route(event_window, seat, None),
                        logical_position: None,
                        kind: InputEventKind::Key {
                            key,
                            phase: ButtonTransitionPhase::Released,
                        },
                        invalidation: None,
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
                    .is_some_and(|pressed| pressed.contains(&button))
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
                self.pressed_mouse_buttons
                    .entry(state_key)
                    .or_default()
                    .insert(button);
                Ok(__InputDriverOutput::new(admission))
            }
            ButtonDriverInput::Release(button) => {
                if !self
                    .pressed_mouse_buttons
                    .get(&state_key)
                    .is_some_and(|pressed| pressed.contains(&button))
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
                    .map(|pressed| pressed.iter().copied().collect::<Vec<_>>())
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
                };
                let admission = self.admit_drafts(&[draft])?;
                self.pointer_positions
                    .insert((window, seat, pointer), position);
                Ok(__InputDriverOutput::new(admission))
            }
            PointerDriverInput::Left => {
                let route = self.pointer_route(window, seat, Some(pointer));
                let state_key = (window, seat, pointer);
                let position = self.pointer_positions.get(&state_key).copied();
                let released = self
                    .pressed_mouse_buttons
                    .get(&state_key)
                    .map(|pressed| pressed.iter().copied().collect::<Vec<_>>())
                    .unwrap_or_default();
                let mut drafts = released
                    .iter()
                    .copied()
                    .map(|button| InputDraft {
                        route,
                        logical_position: position,
                        kind: InputEventKind::PointerButton {
                            button,
                            phase: ButtonTransitionPhase::Released,
                        },
                        invalidation: Some(InputInvalidationReason::PointerLeft),
                    })
                    .collect::<Vec<_>>();
                drafts.push(InputDraft {
                    route,
                    logical_position: position,
                    kind: InputEventKind::PointerLeft,
                    invalidation: Some(InputInvalidationReason::PointerLeft),
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
            };
            let admission = self.admit_drafts(&[draft])?;
            self.focused_windows.insert(seat, window);
            return Ok(__InputDriverOutput::new(admission));
        }

        let old_route = self.pointer_route(window, seat, None);
        let owns_keyboard_focus = self
            .focused_windows
            .get(&seat)
            .is_none_or(|focused_window| *focused_window == window);
        let (kind, reason, clear_keyboard) = match event {
            InputWindowEvent::Focused(false) => (
                InputEventKind::FocusChanged { focused: false },
                InputInvalidationReason::FocusLost,
                owns_keyboard_focus,
            ),
            InputWindowEvent::Resized { logical_size } => {
                if !vec2_is_finite(logical_size) || logical_size.x < 0.0 || logical_size.y < 0.0 {
                    return Err(InputIngressError::InvalidLogicalSize);
                }
                (
                    InputEventKind::Resized { logical_size },
                    InputInvalidationReason::Resized,
                    false,
                )
            }
            InputWindowEvent::ScaleFactorChanged { scale_factor } => {
                if !scale_factor.is_finite() || scale_factor <= 0.0 {
                    return Err(InputIngressError::InvalidScaleFactor);
                }
                (
                    InputEventKind::ScaleFactorChanged { scale_factor },
                    InputInvalidationReason::ScaleFactorChanged,
                    false,
                )
            }
            InputWindowEvent::CloseRequested => (
                InputEventKind::CloseRequested,
                InputInvalidationReason::CloseRequested,
                owns_keyboard_focus,
            ),
            InputWindowEvent::SurfaceRetired => (
                InputEventKind::SurfaceRetired,
                InputInvalidationReason::SurfaceRetired,
                owns_keyboard_focus,
            ),
            InputWindowEvent::Focused(true) => unreachable!("focus gain returned above"),
        };

        let released_keys = if clear_keyboard {
            self.pressed_keys
                .get(&(window, seat))
                .map(|pressed| pressed.iter().copied().collect::<Vec<_>>())
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
                pressed.iter().copied().map(|button| (*pointer, button))
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
        drafts.extend(released_keys.iter().copied().map(|key| InputDraft {
            route: old_route,
            logical_position: None,
            kind: InputEventKind::Key {
                key,
                phase: ButtonTransitionPhase::Released,
            },
            invalidation: Some(reason),
        }));
        drafts.extend(
            released_mouse_buttons
                .iter()
                .copied()
                .map(|(pointer, button)| InputDraft {
                    route: InputRouteStamp {
                        pointer: Some(pointer),
                        ..old_route
                    },
                    logical_position: self
                        .pointer_positions
                        .get(&(window, seat, pointer))
                        .copied(),
                    kind: InputEventKind::PointerButton {
                        button,
                        phase: ButtonTransitionPhase::Released,
                    },
                    invalidation: Some(reason),
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
                }),
        );
        drafts.push(InputDraft {
            route: old_route,
            logical_position: None,
            kind,
            invalidation: Some(reason),
        });

        self.fail_closed_window(window, seat)?;
        let admission = match self.admit_drafts(&drafts) {
            Ok(admission) => admission,
            Err(error) => {
                self.clear_physical_route_state(window, seat, clear_keyboard);
                return Err(error.after_route_invalidation());
            }
        };
        self.clear_physical_route_state(window, seat, clear_keyboard);
        let mut output = __InputDriverOutput::new(admission);
        output.released_keys = released_keys;
        output.released_mouse_buttons = released_mouse_buttons
            .into_iter()
            .map(|(_, button)| button)
            .collect();
        Ok(output)
    }

    fn key_draft(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        key: KeyCode,
        phase: ButtonTransitionPhase,
    ) -> InputDraft {
        InputDraft {
            route: self.focused_route(window, seat),
            logical_position: None,
            kind: InputEventKind::Key { key, phase },
            invalidation: None,
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
        InputDraft {
            route: self.pointer_route(window, seat, Some(pointer)),
            logical_position: self
                .pointer_positions
                .get(&(window, seat, pointer))
                .copied(),
            kind: InputEventKind::PointerButton { button, phase },
            invalidation: None,
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
            route_generation: route.generation,
        }
    }

    fn fail_closed_window(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<(), InputIngressError> {
        self.focused_windows.retain(|focused_seat, focused_window| {
            *focused_seat != seat || *focused_window != window
        });
        self.routes.remove(&(window, seat));
        let Some(generation) = self.next_route_generation.checked_add(1) else {
            self.poison_routes();
            return Err(InputIngressError::RouteGenerationExhausted);
        };
        self.next_route_generation = generation;
        Ok(())
    }

    fn poison_routes(&mut self) {
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
            self.retained.push_back(RetainedInput {
                entry: InputTimelineEntry {
                    sequence,
                    route: draft.route,
                    logical_position: draft.logical_position,
                    kind: draft.kind,
                    invalidation: draft.invalidation,
                },
                projection_pending: true,
                observation_pending: true,
                ui_pending: draft.route.is_ui_routable(),
                ui_consumed: false,
                action: ActionRetention::Unscanned,
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
        for retained in &mut self.retained {
            if retained.ui_pending {
                retained.ui_pending = false;
            }
            self.ui_cursor = self.ui_cursor.max(retained.entry.sequence);
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
        let mut entries = Vec::new();
        for retained in &mut self.retained {
            if retained.action != ActionRetention::Deferred {
                continue;
            }
            retained.action = ActionRetention::Done;
            if !retained.ui_consumed {
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

fn vec2_is_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionId, ActionMap};

    fn published_timeline() -> InputTimeline {
        let mut timeline = InputTimeline::default();
        timeline
            .publish_presented_route(
                InputWindowId::new(1),
                InputSurfaceId::new(7),
                InputSeatId::PRIMARY,
                InputPresentationEpoch::new(11).unwrap(),
            )
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

        let initial = timeline
            .publish_presented_route(window, first, seat, epoch)
            .unwrap();
        let repeated = timeline
            .publish_presented_route(window, first, seat, epoch)
            .unwrap();
        assert_eq!(repeated, initial);

        let advanced = timeline
            .publish_presented_route(window, first, seat, InputPresentationEpoch::new(8).unwrap())
            .unwrap();
        assert!(advanced.route_generation() > initial.route_generation());

        assert_eq!(
            timeline.publish_presented_route(
                window,
                second,
                seat,
                InputPresentationEpoch::new(9).unwrap(),
            ),
            Err(InputIngressError::RouteConflict {
                window,
                seat,
                current_surface: first,
                requested_surface: second,
            })
        );
        assert_eq!(timeline.pointer_route(window, seat, None), advanced);
        assert!(
            !timeline
                .release_presented_route(window, seat, second)
                .unwrap()
        );
        assert!(
            timeline
                .release_presented_route(window, seat, first)
                .unwrap()
        );

        let reclaimed = timeline
            .publish_presented_route(
                window,
                second,
                seat,
                InputPresentationEpoch::new(9).unwrap(),
            )
            .unwrap();
        assert_eq!(reclaimed.surface(), Some(second));
    }

    #[test]
    fn windows_seats_and_pointers_keep_distinct_event_time_routes() {
        let mut timeline = InputTimeline::default();
        let first_window = InputWindowId::new(1);
        let second_window = InputWindowId::new(2);
        let first_seat = InputSeatId::PRIMARY;
        let second_seat = InputSeatId::new(5);
        timeline
            .publish_presented_route(
                first_window,
                InputSurfaceId::new(10),
                first_seat,
                InputPresentationEpoch::new(100).unwrap(),
            )
            .unwrap();
        timeline
            .publish_presented_route(
                second_window,
                InputSurfaceId::new(20),
                second_seat,
                InputPresentationEpoch::new(200).unwrap(),
            )
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
                .is_some_and(|pressed| pressed.contains(&MouseButton::Left))
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
                .publish_presented_route(
                    window,
                    InputSurfaceId::new(surface),
                    seat,
                    InputPresentationEpoch::new(epoch).unwrap(),
                )
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
                .is_some_and(|pressed| pressed.contains(&KeyCode::Enter))
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
                .is_some_and(|pressed| pressed.contains(&KeyCode::Space))
        );
    }
}
