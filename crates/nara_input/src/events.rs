use nara_core::Vec2;
use nara_ecs::{Resource, bevy_ecs::message::Message};

use crate::{GamepadInputKind, ImeEventKind, KeyCode, MouseWheelUnit, TouchPhase};

macro_rules! input_id {
    ($(#[$doc:meta])* $name:ident($inner:ty), $default:ident) => {
        $(#[$doc])*
        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name($inner);

        impl $name {
            pub const $default: Self = Self(0);

            #[must_use]
            pub const fn new(value: $inner) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }
    };
}

input_id!(/// A window identifier supplied by the native host or a headless producer.
          InputWindowId(u64), UNSPECIFIED);
input_id!(/// A local player or UI seat.
          InputSeatId(u32), PRIMARY);
input_id!(/// A pointer slot within a window and seat.
          InputPointerId(u32), PRIMARY);

impl InputWindowId {
    /// The first window in the standalone Winit runner.
    pub const PRIMARY: Self = Self(1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTransitionPhase {
    Pressed,
    Released,
}

/// A window-wide change. Focus loss and retirement release controls for every seat on the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputWindowEvent {
    Focused(bool),
    Resized { logical_size: Vec2 },
    ScaleFactorChanged { scale_factor: f64 },
    CloseRequested,
    SurfaceRetired,
}

/// A direct input message. Producers write these in arrival order before input projection.
/// Pointer positions and pixel wheel deltas use logical window coordinates.
#[derive(Debug, Clone, PartialEq, Message)]
pub struct InputEdge {
    pub window: InputWindowId,
    pub seat: InputSeatId,
    pub kind: InputEdgeKind,
}

impl InputEdge {
    #[must_use]
    pub const fn new(window: InputWindowId, seat: InputSeatId, kind: InputEdgeKind) -> Self {
        Self { window, seat, kind }
    }

    #[must_use]
    pub const fn primary(kind: InputEdgeKind) -> Self {
        Self::new(InputWindowId::UNSPECIFIED, InputSeatId::PRIMARY, kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEdgeKind {
    Key {
        key: KeyCode,
        phase: ButtonTransitionPhase,
    },
    PointerMoved {
        pointer: InputPointerId,
        position: Vec2,
    },
    PointerLeft {
        pointer: InputPointerId,
    },
    PointerButton {
        pointer: InputPointerId,
        button: MouseButton,
        phase: ButtonTransitionPhase,
        position: Option<Vec2>,
    },
    Wheel {
        pointer: InputPointerId,
        delta: Vec2,
        unit: MouseWheelUnit,
        phase: TouchPhase,
        position: Option<Vec2>,
    },
    Window(InputWindowEvent),
    Text(String),
    Ime(ImeEventKind),
    Touch {
        id: u64,
        phase: TouchPhase,
        position: Vec2,
        force: Option<f32>,
    },
    Gamepad {
        gamepad: u64,
        kind: GamepadInputKind,
    },
}

/// Projected edges for the current app frame, ordered across devices. Available from
/// [`crate::InputSet::RouteUi`] through frame cleanup. Copy any data needed beyond that frame.
#[derive(Debug, Default, Resource)]
pub struct InputFrame {
    pub(crate) edges: Vec<InputEdge>,
    pub(crate) consumed: Vec<bool>,
    pub(crate) projected: bool,
    pub(crate) resolved: bool,
}

impl InputFrame {
    #[must_use]
    pub fn edges(&self) -> &[InputEdge] {
        &self.edges
    }

    /// Whether this frame has already passed gameplay resolution.
    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        self.resolved
    }

    /// Prevents this frame-local edge from starting or updating a gameplay action. A release
    /// still ends gameplay input previously started by this control.
    pub fn consume(&mut self, index: usize) -> bool {
        let Some(consumed) = self.consumed.get_mut(index).filter(|_| !self.resolved) else {
            return false;
        };
        !std::mem::replace(consumed, true)
    }

    #[must_use]
    pub fn is_consumed(&self, index: usize) -> bool {
        self.consumed.get(index).copied().unwrap_or(false)
    }

    pub(crate) fn clear(&mut self) {
        self.edges.clear();
        self.consumed.clear();
        self.projected = false;
        self.resolved = false;
    }
}
