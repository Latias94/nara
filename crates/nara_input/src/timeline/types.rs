use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_core::Vec2;
use nara_ecs::Resource;

use crate::{
    AxisValue, ButtonDriverInput, ButtonTransitionPhase, InputBinding, KeyCode, MouseButton,
    PointerDriverInput,
};

/// Maximum number of retained normalized platform events.
pub const MAX_INPUT_TIMELINE_ENTRIES: usize = 4_096;

/// Maximum number of simultaneously known window/seat presentation routes.
pub const MAX_INPUT_ROUTES: usize = 64;

/// Finite live Input storage, including the once-connected synthetic endpoint and pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputLimits {
    events: usize,
    endpoints: usize,
    pointers: usize,
    members: usize,
    gamepads: usize,
}

impl Default for InputLimits {
    fn default() -> Self {
        Self {
            events: MAX_INPUT_TIMELINE_ENTRIES,
            endpoints: 64,
            pointers: 256,
            members: 1_024,
            gamepads: 64,
        }
    }
}

impl InputLimits {
    pub fn new(
        events: usize,
        endpoints: usize,
        pointers: usize,
        members: usize,
    ) -> Result<Self, InputConfigurationError> {
        if endpoints == 0 || pointers == 0 || members == 0 {
            return Err(InputConfigurationError::ZeroCapacity);
        }
        if events < 3 {
            return Err(InputConfigurationError::EventCapacityTooSmall { limit: events });
        }
        if events > MAX_INPUT_TIMELINE_ENTRIES {
            return Err(InputConfigurationError::EventCapacityExceeded { limit: events });
        }
        events
            .checked_add(endpoints)
            .and_then(|count| count.checked_add(pointers))
            .and_then(|count| count.checked_add(members))
            .and_then(|count| count.checked_add(64))
            .and_then(|count| count.checked_mul(crate::MAX_BINDINGS_PER_INPUT))
            .ok_or(InputConfigurationError::CapacityOverflow)?;
        Ok(Self {
            events,
            endpoints,
            pointers,
            members,
            gamepads: 64,
        })
    }

    /// Sets the finite connection allowance, including disconnected connections still retained.
    /// Zero disables gamepad admission without changing keyboard or pointer capacity.
    pub fn with_gamepads(mut self, gamepads: usize) -> Result<Self, InputConfigurationError> {
        self.events
            .checked_add(self.endpoints)
            .and_then(|count| count.checked_add(self.pointers))
            .and_then(|count| count.checked_add(self.members))
            .and_then(|count| count.checked_add(gamepads))
            .and_then(|count| count.checked_mul(crate::MAX_BINDINGS_PER_INPUT))
            .ok_or(InputConfigurationError::CapacityOverflow)?;
        self.gamepads = gamepads;
        Ok(self)
    }

    #[must_use]
    pub const fn events(self) -> usize {
        self.events
    }
    #[must_use]
    pub const fn endpoints(self) -> usize {
        self.endpoints
    }
    #[must_use]
    pub const fn pointers(self) -> usize {
        self.pointers
    }
    #[must_use]
    pub const fn members(self) -> usize {
        self.members
    }
    #[must_use]
    pub const fn gamepads(self) -> usize {
        self.gamepads
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputConfigurationError {
    ZeroCapacity,
    EventCapacityTooSmall { limit: usize },
    EventCapacityExceeded { limit: usize },
    CapacityOverflow,
    AllocationRefused,
}

impl Display for InputConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => {
                formatter.write_str("input lifecycle and member capacities must be non-zero")
            }
            Self::EventCapacityTooSmall { limit } => write!(
                formatter,
                "input event capacity {limit} cannot reserve three synthetic lifecycle controls"
            ),
            Self::EventCapacityExceeded { limit } => write!(
                formatter,
                "input event capacity {limit} exceeds the supported maximum {MAX_INPUT_TIMELINE_ENTRIES}"
            ),
            Self::CapacityOverflow => formatter.write_str("input storage capacity overflowed"),
            Self::AllocationRefused => formatter.write_str("input storage allocation was refused"),
        }
    }
}

impl Error for InputConfigurationError {}

/// Origin is independent from route authority and from the original held member's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InputEventOrigin {
    Platform,
    Synthetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

/// Non-reused metadata identifying one connection within its original Input owner.
///
/// The generation is observational data; only a [`super::GamepadConnection`] grants ingress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct InputGamepadGeneration(pub(super) u64);

impl InputGamepadGeneration {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for InputGamepadGeneration {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let raw = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if raw == 0 {
            return Err(serde::de::Error::custom(
                "gamepad generation must be non-zero",
            ));
        }
        Ok(Self(raw))
    }
}

#[cfg(feature = "serde")]
macro_rules! nonzero_input_generation_deserialize {
    ($($generation:ty),* $(,)?) => {
        $(impl<'de> serde::Deserialize<'de> for $generation {
            fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
            where
                Deserializer: serde::Deserializer<'de>,
            {
                let raw = <u64 as serde::Deserialize>::deserialize(deserializer)?;
                Self::new(raw).ok_or_else(|| serde::de::Error::custom("input generation must be non-zero"))
            }
        })*
    };
}

#[cfg(feature = "serde")]
nonzero_input_generation_deserialize!(
    InputPresentationEpoch,
    InputSlotGeneration,
    InputRouteGeneration,
    InputGeometryGeneration,
);

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// Event-time endpoint and original held-press lineage for one semantic action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActionInputProvenance {
    sequence: u64,
    origin: InputEventOrigin,
    route: InputRouteStamp,
    press_sequence: Option<u64>,
    press_route: Option<InputRouteStamp>,
    gamepad_connection_generation: Option<InputGamepadGeneration>,
}

impl ActionInputProvenance {
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn origin(self) -> InputEventOrigin {
        self.origin
    }

    #[must_use]
    pub const fn route(self) -> InputRouteStamp {
        self.route
    }

    #[must_use]
    pub const fn press_sequence(self) -> Option<u64> {
        self.press_sequence
    }

    #[must_use]
    pub const fn press_route(self) -> Option<InputRouteStamp> {
        self.press_route
    }

    #[must_use]
    pub const fn gamepad_connection_generation(self) -> Option<InputGamepadGeneration> {
        self.gamepad_connection_generation
    }
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
    GamepadDisconnected,
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
    GamepadConnected {
        gamepad: u16,
        connection_generation: InputGamepadGeneration,
    },
    GamepadDisconnected {
        gamepad: u16,
        connection_generation: InputGamepadGeneration,
    },
    GamepadButton {
        gamepad: u16,
        connection_generation: InputGamepadGeneration,
        button: u16,
        phase: ButtonTransitionPhase,
    },
    GamepadAxis {
        gamepad: u16,
        connection_generation: InputGamepadGeneration,
        axis: u16,
        value: AxisValue,
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
    pub(super) origin: InputEventOrigin,
    pub(super) route: InputRouteStamp,
    pub(super) logical_position: Option<Vec2>,
    pub(super) kind: InputEventKind,
    pub(super) invalidation: Option<InputInvalidationReason>,
    pub(super) press_sequence: Option<u64>,
    pub(super) press_route: Option<InputRouteStamp>,
}

impl InputTimelineEntry {
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn origin(self) -> InputEventOrigin {
        self.origin
    }

    #[must_use]
    pub const fn press_sequence(self) -> Option<u64> {
        self.press_sequence
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

    #[must_use]
    pub const fn action_provenance(self) -> ActionInputProvenance {
        ActionInputProvenance {
            sequence: self.sequence,
            origin: self.origin,
            route: self.route,
            press_sequence: self.press_sequence,
            press_route: self.press_route,
            gamepad_connection_generation: self.gamepad_connection_generation(),
        }
    }

    #[must_use]
    pub const fn gamepad_connection_generation(self) -> Option<InputGamepadGeneration> {
        match self.kind {
            InputEventKind::GamepadConnected {
                connection_generation,
                ..
            }
            | InputEventKind::GamepadDisconnected {
                connection_generation,
                ..
            }
            | InputEventKind::GamepadButton {
                connection_generation,
                ..
            }
            | InputEventKind::GamepadAxis {
                connection_generation,
                ..
            } => Some(connection_generation),
            _ => None,
        }
    }

    #[must_use]
    pub const fn action_input(self) -> Option<InputBinding> {
        match self.kind {
            InputEventKind::Key { key, .. } => Some(InputBinding::Key(key)),
            InputEventKind::PointerButton { button, .. } => Some(InputBinding::Mouse(button)),
            InputEventKind::GamepadButton { button, .. } => {
                Some(InputBinding::GamepadButton(button))
            }
            InputEventKind::GamepadAxis { axis, .. } => Some(InputBinding::GamepadAxis(axis)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn axis_value(self) -> Option<AxisValue> {
        match self.kind {
            InputEventKind::GamepadAxis { value, .. } => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub const fn action_transition(self) -> Option<(InputBinding, ButtonTransitionPhase)> {
        match self.kind {
            InputEventKind::Key { key, phase } => Some((InputBinding::Key(key), phase)),
            InputEventKind::PointerButton { button, phase } => {
                Some((InputBinding::Mouse(button), phase))
            }
            InputEventKind::GamepadButton { button, phase, .. } => {
                Some((InputBinding::GamepadButton(button), phase))
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

/// Result of retiring one physical input endpoint.
///
/// Retirement is distinct from logical UI route invalidation: it emits balanced release edges,
/// clears the endpoint's physical state, and returns the exact admitted range for callers that
/// coordinate platform teardown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEndpointRetirement {
    admission: InputAdmission,
    released_keys: Vec<KeyCode>,
    released_mouse_buttons: Vec<MouseButton>,
}

impl InputEndpointRetirement {
    pub(super) fn new(
        admission: InputAdmission,
        released_keys: Vec<KeyCode>,
        released_mouse_buttons: Vec<MouseButton>,
    ) -> Self {
        Self {
            admission,
            released_keys,
            released_mouse_buttons,
        }
    }

    #[must_use]
    pub const fn admission(&self) -> InputAdmission {
        self.admission
    }

    #[must_use]
    pub fn released_keys(&self) -> &[KeyCode] {
        &self.released_keys
    }

    #[must_use]
    pub fn released_mouse_buttons(&self) -> &[MouseButton] {
        &self.released_mouse_buttons
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputIngressError {
    ForeignGamepadConnection,
    GamepadConnectionExpired {
        connection_generation: InputGamepadGeneration,
    },
    GamepadCapacityExceeded {
        limit: usize,
    },
    ForeignEndpointClaim,
    EndpointClaimExpired {
        window: InputWindowId,
        seat: InputSeatId,
    },
    EndpointCapacityExceeded {
        limit: usize,
    },
    PointerCapacityExceeded {
        limit: usize,
    },
    MemberCapacityExceeded {
        limit: usize,
    },
    UnknownEndpoint {
        window: InputWindowId,
        seat: InputSeatId,
    },
    UnknownPointer {
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    },
    EndpointRetiring {
        window: InputWindowId,
        seat: InputSeatId,
    },
    PointerRetiring {
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    },
    EndpointGenerationExhausted,
    CapacityExceeded {
        limit: usize,
        retained: usize,
        requested: usize,
    },
    SequenceExhausted {
        requested: usize,
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
    RouteReservationConflict {
        window: InputWindowId,
        seat: InputSeatId,
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

impl Display for InputIngressError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForeignGamepadConnection => {
                formatter.write_str("gamepad connection belongs to a different Input owner")
            }
            Self::GamepadConnectionExpired {
                connection_generation,
            } => write!(
                formatter,
                "gamepad connection {} has disconnected or its endpoint has retired",
                connection_generation.get(),
            ),
            Self::GamepadCapacityExceeded { limit } => {
                write!(
                    formatter,
                    "input gamepad connection capacity {limit} was exceeded"
                )
            }
            Self::ForeignEndpointClaim => {
                formatter.write_str("input endpoint claim belongs to a different Input owner")
            }
            Self::EndpointClaimExpired { window, seat } => write!(
                formatter,
                "the original input endpoint ({}, {}) has retired",
                window.get(),
                seat.get()
            ),
            Self::EndpointCapacityExceeded { limit } => {
                write!(formatter, "input endpoint capacity {limit} was exceeded")
            }
            Self::PointerCapacityExceeded { limit } => {
                write!(formatter, "input pointer capacity {limit} was exceeded")
            }
            Self::MemberCapacityExceeded { limit } => {
                write!(formatter, "input member capacity {limit} was exceeded")
            }
            Self::UnknownEndpoint { window, seat } => write!(
                formatter,
                "input endpoint ({}, {}) is not connected",
                window.get(),
                seat.get()
            ),
            Self::UnknownPointer {
                window,
                seat,
                pointer,
            } => write!(
                formatter,
                "input pointer ({}, {}, {}) is not connected",
                window.get(),
                seat.get(),
                pointer.get()
            ),
            Self::EndpointRetiring { window, seat } => write!(
                formatter,
                "input endpoint ({}, {}) is still retiring",
                window.get(),
                seat.get()
            ),
            Self::PointerRetiring {
                window,
                seat,
                pointer,
            } => write!(
                formatter,
                "input pointer ({}, {}, {}) is still retiring",
                window.get(),
                seat.get(),
                pointer.get()
            ),
            Self::EndpointGenerationExhausted => {
                formatter.write_str("input endpoint identity was exhausted")
            }
            Self::CapacityExceeded {
                limit,
                retained,
                requested,
            } => write!(
                formatter,
                "input timeline limit {limit} cannot admit {requested} records and terminal credits after {retained} retained records",
            ),
            Self::SequenceExhausted { requested } => write!(
                formatter,
                "input timeline sequence cannot admit {requested} records and terminal credits",
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
            Self::RouteReservationConflict { window, seat } => write!(
                formatter,
                "input endpoint ({}, {}) already has a prepared presented-route publication",
                window.get(),
                seat.get(),
            ),
            Self::PresentedRouteMismatch { window, seat } => write!(
                formatter,
                "input endpoint ({}, {}) no longer owns the exact presented route being unbound",
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
    pub(crate) fn with_capacity(capacity: usize) -> Result<Self, InputConfigurationError> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(capacity)
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        Ok(Self { entries })
    }

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
    ConnectEndpoint {
        window: InputWindowId,
        seat: InputSeatId,
    },
    ConnectPointer {
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    },
    RetirePointer {
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    },
    SyntheticKeyboard {
        input: ButtonDriverInput<KeyCode>,
    },
    SyntheticMouse {
        input: ButtonDriverInput<MouseButton>,
    },
    SyntheticPointer {
        input: PointerDriverInput,
    },
    ConnectGamepad {
        window: InputWindowId,
        seat: InputSeatId,
        gamepad: u16,
    },
    ConnectSyntheticGamepad {
        window: InputWindowId,
        seat: InputSeatId,
        gamepad: u16,
    },
    DisconnectGamepad {
        connection: super::GamepadConnection,
    },
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
    GamepadButton {
        connection: super::GamepadConnection,
        button: u16,
        phase: ButtonTransitionPhase,
    },
    GamepadAxis {
        connection: super::GamepadConnection,
        axis: u16,
        value: AxisValue,
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
    pub gamepad_connection: Option<super::GamepadConnection>,
}

impl __InputDriverOutput {
    pub(super) fn new(admission: InputAdmission) -> Self {
        Self {
            admission,
            released_keys: Vec::new(),
            released_mouse_buttons: Vec::new(),
            gamepad_connection: None,
        }
    }
}
