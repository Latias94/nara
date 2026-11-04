use super::{UiButtonPhase, UiPointerButton, UiPointerId, UiSeatId};
use crate::UiPoint;

/// Modifier state captured by the adapter at the time of one input sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiModifiers {
    shift: bool,
    control: bool,
    alt: bool,
    meta: bool,
}

impl UiModifiers {
    #[must_use]
    pub const fn new(shift: bool, control: bool, alt: bool, meta: bool) -> Self {
        Self {
            shift,
            control,
            alt,
            meta,
        }
    }

    #[must_use]
    pub const fn shift(self) -> bool {
        self.shift
    }

    #[must_use]
    pub const fn control(self) -> bool {
        self.control
    }

    #[must_use]
    pub const fn alt(self) -> bool {
        self.alt
    }

    #[must_use]
    pub const fn meta(self) -> bool {
        self.meta
    }
}

/// A scroll displacement with the adapter's original unit preserved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiScrollDelta {
    /// Device line units. Conversion and sensitivity belong to the consuming component.
    Lines(UiPoint),
    /// Authored logical point units after the adapter applies the exact inverse content scale.
    Pixels(UiPoint),
}

impl UiScrollDelta {
    pub(super) fn is_finite(self) -> bool {
        match self {
            Self::Lines(delta) | Self::Pixels(delta) => delta.is_finite(),
        }
    }
}

/// The operation carried by one neutral pointer input sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiPointerInputKind {
    Moved,
    Button {
        button: UiPointerButton,
        phase: UiButtonPhase,
    },
    Scroll {
        delta: UiScrollDelta,
    },
    Cancelled,
}

/// One event-time pointer sample in authored logical coordinates.
///
/// Constructors are infallible. Dispatch rejects non-finite positions and scroll deltas before
/// any reconciliation or state mutation. A button without a position resolves only from the
/// same pointer's position in the exact presentation epoch. Cancellation has no position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPointerInput {
    seat: UiSeatId,
    pointer: UiPointerId,
    position: Option<UiPoint>,
    modifiers: UiModifiers,
    kind: UiPointerInputKind,
}

impl UiPointerInput {
    #[must_use]
    pub const fn moved(seat: UiSeatId, pointer: UiPointerId, position: UiPoint) -> Self {
        Self::new(seat, pointer, Some(position), UiPointerInputKind::Moved)
    }

    #[must_use]
    pub const fn button(
        seat: UiSeatId,
        pointer: UiPointerId,
        button: UiPointerButton,
        phase: UiButtonPhase,
        position: Option<UiPoint>,
    ) -> Self {
        Self::new(
            seat,
            pointer,
            position,
            UiPointerInputKind::Button { button, phase },
        )
    }

    #[must_use]
    pub const fn scroll(
        seat: UiSeatId,
        pointer: UiPointerId,
        position: UiPoint,
        delta: UiScrollDelta,
    ) -> Self {
        Self::new(
            seat,
            pointer,
            Some(position),
            UiPointerInputKind::Scroll { delta },
        )
    }

    #[must_use]
    pub const fn cancelled(seat: UiSeatId, pointer: UiPointerId) -> Self {
        Self::new(seat, pointer, None, UiPointerInputKind::Cancelled)
    }

    const fn new(
        seat: UiSeatId,
        pointer: UiPointerId,
        position: Option<UiPoint>,
        kind: UiPointerInputKind,
    ) -> Self {
        Self {
            seat,
            pointer,
            position,
            modifiers: UiModifiers::new(false, false, false, false),
            kind,
        }
    }

    #[must_use]
    pub const fn with_modifiers(mut self, modifiers: UiModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    #[must_use]
    pub const fn seat(self) -> UiSeatId {
        self.seat
    }

    #[must_use]
    pub const fn pointer(self) -> UiPointerId {
        self.pointer
    }

    #[must_use]
    pub const fn position(self) -> Option<UiPoint> {
        self.position
    }

    #[must_use]
    pub const fn modifiers(self) -> UiModifiers {
        self.modifiers
    }

    #[must_use]
    pub const fn kind(self) -> UiPointerInputKind {
        self.kind
    }

    pub(super) const fn with_resolved_position(mut self, position: UiPoint) -> Self {
        self.position = Some(position);
        self
    }
}
