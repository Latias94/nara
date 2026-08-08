use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_core::Vec2;
use nara_ecs::Resource;

use crate::{
    ButtonDriverInput, ButtonTransitionPhase, InputBinding, KeyCode, MouseButton,
    PointerDriverInput,
};

/// Maximum number of retained normalized platform events.
pub const MAX_INPUT_TIMELINE_ENTRIES: usize = 4_096;

/// Maximum number of simultaneously known window/seat presentation routes.
pub const MAX_INPUT_ROUTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputWindowId(u64);

impl InputWindowId {
    pub const UNSPECIFIED: Self = Self(0);

    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputSurfaceId(u64);

impl InputSurfaceId {
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputSeatId(u32);

impl InputSeatId {
    pub const PRIMARY: Self = Self(0);

    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputPointerId(u32);

impl InputPointerId {
    pub const PRIMARY: Self = Self(0);

    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputPresentationEpoch(u64);

impl InputPresentationEpoch {
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Host-owned generation of one installed UI surface slot snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputSlotGeneration(u64);

impl InputSlotGeneration {
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Host-owned generation of the exact UI route retained by one presented receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputRouteGeneration(u64);

impl InputRouteGeneration {
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Host-owned generation of the exact UI geometry used to route an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputGeometryGeneration(u64);

impl InputGeometryGeneration {
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Complete immutable route publication accepted by the normalized input authority.
///
/// The UI adapter constructs this value from one exact presented receipt. Keeping the endpoint and
/// all generations together prevents a caller from publishing a partially updated route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputPresentedRoute {
    pub(super) window: InputWindowId,
    pub(super) surface: InputSurfaceId,
    pub(super) seat: InputSeatId,
    pub(super) presentation_epoch: InputPresentationEpoch,
    pub(super) slot_generation: InputSlotGeneration,
    pub(super) route_generation: InputRouteGeneration,
    pub(super) geometry_generation: InputGeometryGeneration,
}

impl InputPresentedRoute {
    #[must_use]
    pub const fn new(
        window: InputWindowId,
        surface: InputSurfaceId,
        seat: InputSeatId,
        presentation_epoch: InputPresentationEpoch,
        slot_generation: InputSlotGeneration,
        route_generation: InputRouteGeneration,
        geometry_generation: InputGeometryGeneration,
    ) -> Self {
        Self {
            window,
            surface,
            seat,
            presentation_epoch,
            slot_generation,
            route_generation,
            geometry_generation,
        }
    }

    #[must_use]
    pub const fn window(self) -> InputWindowId {
        self.window
    }

    #[must_use]
    pub const fn surface(self) -> InputSurfaceId {
        self.surface
    }

    #[must_use]
    pub const fn seat(self) -> InputSeatId {
        self.seat
    }

    #[must_use]
    pub const fn presentation_epoch(self) -> InputPresentationEpoch {
        self.presentation_epoch
    }

    #[must_use]
    pub const fn slot_generation(self) -> InputSlotGeneration {
        self.slot_generation
    }

    #[must_use]
    pub const fn route_generation(self) -> InputRouteGeneration {
        self.route_generation
    }

    #[must_use]
    pub const fn geometry_generation(self) -> InputGeometryGeneration {
        self.geometry_generation
    }
}

/// Event-time routing facts captured without depending on UI or render types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputRouteStamp {
    pub(super) window: InputWindowId,
    pub(super) surface: Option<InputSurfaceId>,
    pub(super) seat: InputSeatId,
    pub(super) pointer: Option<InputPointerId>,
    pub(super) presentation_epoch: Option<InputPresentationEpoch>,
    pub(super) slot_generation: Option<InputSlotGeneration>,
    pub(super) route_generation: Option<InputRouteGeneration>,
    pub(super) geometry_generation: Option<InputGeometryGeneration>,
    pub(super) binding_generation: u64,
}

impl InputRouteStamp {
    #[must_use]
    pub const fn unrouted(
        window: InputWindowId,
        seat: InputSeatId,
        pointer: Option<InputPointerId>,
    ) -> Self {
        Self {
            window,
            surface: None,
            seat,
            pointer,
            presentation_epoch: None,
            slot_generation: None,
            route_generation: None,
            geometry_generation: None,
            binding_generation: 0,
        }
    }

    #[must_use]
    pub const fn window(self) -> InputWindowId {
        self.window
    }

    #[must_use]
    pub const fn surface(self) -> Option<InputSurfaceId> {
        self.surface
    }

    #[must_use]
    pub const fn seat(self) -> InputSeatId {
        self.seat
    }

    #[must_use]
    pub const fn pointer(self) -> Option<InputPointerId> {
        self.pointer
    }

    #[must_use]
    pub const fn presentation_epoch(self) -> Option<InputPresentationEpoch> {
        self.presentation_epoch
    }

    #[must_use]
    pub const fn slot_generation(self) -> Option<InputSlotGeneration> {
        self.slot_generation
    }

    #[must_use]
    pub const fn route_generation(self) -> Option<InputRouteGeneration> {
        self.route_generation
    }

    #[must_use]
    pub const fn geometry_generation(self) -> Option<InputGeometryGeneration> {
        self.geometry_generation
    }

    #[must_use]
    pub const fn binding_generation(self) -> u64 {
        self.binding_generation
    }

    #[must_use]
    pub const fn is_ui_routable(self) -> bool {
        self.surface.is_some()
            && self.presentation_epoch.is_some()
            && self.slot_generation.is_some()
            && self.route_generation.is_some()
            && self.geometry_generation.is_some()
            && self.binding_generation != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputInvalidationReason {
    PointerLeft,
    FocusLost,
    Resized,
    ScaleFactorChanged,
    CloseRequested,
    SurfaceRetired,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEventKind {
    PointerMoved,
    PointerButton {
        button: MouseButton,
        phase: ButtonTransitionPhase,
    },
    PointerLeft,
    Wheel {
        delta: Vec2,
    },
    Key {
        key: KeyCode,
        phase: ButtonTransitionPhase,
    },
    GamepadButton {
        gamepad: u16,
        button: u16,
        phase: ButtonTransitionPhase,
    },
    FocusChanged {
        focused: bool,
    },
    Resized {
        logical_size: Vec2,
    },
    ScaleFactorChanged {
        scale_factor: f64,
    },
    CloseRequested,
    SurfaceRetired,
}

/// One fixed-shape normalized platform event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputTimelineEntry {
    pub(super) sequence: u64,
    pub(super) route: InputRouteStamp,
    pub(super) logical_position: Option<Vec2>,
    pub(super) kind: InputEventKind,
    pub(super) invalidation: Option<InputInvalidationReason>,
}

impl InputTimelineEntry {
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn route(self) -> InputRouteStamp {
        self.route
    }

    #[must_use]
    pub const fn logical_position(self) -> Option<Vec2> {
        self.logical_position
    }

    #[must_use]
    pub const fn kind(self) -> InputEventKind {
        self.kind
    }

    #[must_use]
    pub const fn invalidation(self) -> Option<InputInvalidationReason> {
        self.invalidation
    }

    pub(crate) fn action_transition(self) -> Option<(InputBinding, ButtonTransitionPhase)> {
        match self.kind {
            InputEventKind::Key { key, phase } => Some((InputBinding::Key(key), phase)),
            InputEventKind::PointerButton { button, phase } => {
                Some((InputBinding::Mouse(button), phase))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputAdmission {
    pub(super) first_sequence: Option<u64>,
    pub(super) last_sequence: Option<u64>,
    pub(super) admitted: usize,
}

impl InputAdmission {
    pub(super) const IGNORED: Self = Self {
        first_sequence: None,
        last_sequence: None,
        admitted: 0,
    };

    #[must_use]
    pub const fn first_sequence(self) -> Option<u64> {
        self.first_sequence
    }

    #[must_use]
    pub const fn last_sequence(self) -> Option<u64> {
        self.last_sequence
    }

    #[must_use]
    pub const fn admitted(self) -> usize {
        self.admitted
    }

    #[must_use]
    pub const fn is_ignored(self) -> bool {
        self.admitted == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputIngressError {
    CapacityExceeded {
        limit: usize,
        retained: usize,
        requested: usize,
        route_invalidated: bool,
    },
    SequenceExhausted {
        requested: usize,
        route_invalidated: bool,
    },
    RouteCapacityExceeded {
        limit: usize,
    },
    RouteConflict {
        window: InputWindowId,
        seat: InputSeatId,
        current_surface: InputSurfaceId,
        requested_surface: InputSurfaceId,
    },
    PresentedRouteMismatch {
        window: InputWindowId,
        seat: InputSeatId,
    },
    PresentedRouteRegression {
        window: InputWindowId,
        seat: InputSeatId,
    },
    RouteBindingGenerationExhausted,
    NonFinitePosition,
    InvalidLogicalSize,
    InvalidScaleFactor,
}

impl InputIngressError {
    pub(super) fn after_route_invalidation(self) -> Self {
        match self {
            Self::CapacityExceeded {
                limit,
                retained,
                requested,
                ..
            } => Self::CapacityExceeded {
                limit,
                retained,
                requested,
                route_invalidated: true,
            },
            Self::SequenceExhausted { requested, .. } => Self::SequenceExhausted {
                requested,
                route_invalidated: true,
            },
            other => other,
        }
    }

    #[must_use]
    pub const fn route_was_invalidated(self) -> bool {
        matches!(
            self,
            Self::CapacityExceeded {
                route_invalidated: true,
                ..
            } | Self::SequenceExhausted {
                route_invalidated: true,
                ..
            } | Self::RouteBindingGenerationExhausted
        )
    }
}

impl Display for InputIngressError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded {
                limit,
                retained,
                requested,
                route_invalidated,
            } => write!(
                formatter,
                "input timeline limit {limit} cannot retain {requested} entries after {retained}{}",
                if *route_invalidated {
                    "; the affected route was invalidated"
                } else {
                    ""
                }
            ),
            Self::SequenceExhausted {
                requested,
                route_invalidated,
            } => write!(
                formatter,
                "input timeline sequence cannot admit {requested} entries{}",
                if *route_invalidated {
                    "; the affected route was invalidated"
                } else {
                    ""
                }
            ),
            Self::RouteCapacityExceeded { limit } => {
                write!(formatter, "input route limit {limit} was exceeded")
            }
            Self::RouteConflict {
                window,
                seat,
                current_surface,
                requested_surface,
            } => write!(
                formatter,
                "input endpoint ({}, {}) is owned by surface {}; surface {} cannot replace it without retirement",
                window.get(),
                seat.get(),
                current_surface.get(),
                requested_surface.get(),
            ),
            Self::PresentedRouteMismatch { window, seat } => write!(
                formatter,
                "input endpoint ({}, {}) no longer owns the exact presented route being released",
                window.get(),
                seat.get(),
            ),
            Self::PresentedRouteRegression { window, seat } => write!(
                formatter,
                "input endpoint ({}, {}) rejected a presented route that regresses its active generations",
                window.get(),
                seat.get(),
            ),
            Self::RouteBindingGenerationExhausted => {
                formatter.write_str("input route binding generation was exhausted")
            }
            Self::NonFinitePosition => formatter.write_str("input position must be finite"),
            Self::InvalidLogicalSize => {
                formatter.write_str("input logical size must be finite and non-negative")
            }
            Self::InvalidScaleFactor => {
                formatter.write_str("input scale factor must be finite and positive")
            }
        }
    }
}

impl Error for InputIngressError {}

/// Final UI decision for one retained input entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputUiDisposition {
    Consumed,
    RejectedClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputUiCursorError {
    UnknownSequence { sequence: u64 },
    AlreadyFinalized { sequence: u64 },
}

impl Display for InputUiCursorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSequence { sequence } => {
                write!(formatter, "input sequence {sequence} is not retained")
            }
            Self::AlreadyFinalized { sequence } => {
                write!(
                    formatter,
                    "input sequence {sequence} already passed UI routing"
                )
            }
        }
    }
}

impl Error for InputUiCursorError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputConsumerCursors {
    pub(super) observation: u64,
    pub(super) ui: u64,
    pub(super) action_scanned: u64,
    pub(super) oldest_deferred_action: Option<u64>,
}

impl InputConsumerCursors {
    #[must_use]
    pub const fn observation(self) -> u64 {
        self.observation
    }

    #[must_use]
    pub const fn ui(self) -> u64 {
        self.ui
    }

    #[must_use]
    pub const fn action_scanned(self) -> u64 {
        self.action_scanned
    }

    #[must_use]
    pub const fn oldest_deferred_action(self) -> Option<u64> {
        self.oldest_deferred_action
    }
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct InputFrameObservations {
    pub(super) entries: Vec<InputTimelineEntry>,
}

impl InputFrameObservations {
    #[must_use]
    pub fn as_slice(&self) -> &[InputTimelineEntry] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputWindowEvent {
    Focused(bool),
    Resized { logical_size: Vec2 },
    ScaleFactorChanged { scale_factor: f64 },
    CloseRequested,
    SurfaceRetired,
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub enum __InputDriverEvent {
    Keyboard {
        window: InputWindowId,
        seat: InputSeatId,
        input: ButtonDriverInput<KeyCode>,
    },
    Mouse {
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: ButtonDriverInput<MouseButton>,
    },
    Pointer {
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: PointerDriverInput,
    },
    Gamepad {
        window: InputWindowId,
        seat: InputSeatId,
        gamepad: u16,
        button: u16,
        phase: ButtonTransitionPhase,
    },
    Window {
        window: InputWindowId,
        seat: InputSeatId,
        event: InputWindowEvent,
    },
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct __InputDriverOutput {
    pub admission: InputAdmission,
    pub released_keys: Vec<KeyCode>,
    pub released_mouse_buttons: Vec<MouseButton>,
}

impl __InputDriverOutput {
    pub(super) fn new(admission: InputAdmission) -> Self {
        Self {
            admission,
            released_keys: Vec::new(),
            released_mouse_buttons: Vec::new(),
        }
    }
}
