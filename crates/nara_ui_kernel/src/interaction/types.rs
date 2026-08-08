use crate::{UiActionId, UiFrameRevision, UiKey, UiPoint};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiSeatId(u32);

impl UiSeatId {
    pub const PRIMARY: Self = Self(0);

    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiPointerId(u32);

impl UiPointerId {
    pub const PRIMARY: Self = Self(0);

    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiButtonPhase {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiNavigation {
    Next,
    Previous,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiActivationSource {
    Pointer,
    Keyboard,
    Gamepad,
    Accessibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiRoutePhase {
    Capture,
    Target,
    Bubble,
}

/// One renderer- and platform-neutral interaction event mapped by an owning adapter.
#[derive(Clone, Debug, PartialEq)]
pub enum UiInputEvent {
    PointerMoved {
        seat: UiSeatId,
        pointer: UiPointerId,
        position: UiPoint,
    },
    PointerButton {
        seat: UiSeatId,
        pointer: UiPointerId,
        button: UiPointerButton,
        phase: UiButtonPhase,
        position: Option<UiPoint>,
    },
    PointerCancelled {
        seat: UiSeatId,
        pointer: UiPointerId,
    },
    Navigate {
        seat: UiSeatId,
        direction: UiNavigation,
    },
    ActivateFocused {
        seat: UiSeatId,
        source: UiActivationSource,
    },
    FocusTarget {
        seat: UiSeatId,
        target: UiKey,
    },
    SemanticAction {
        seat: UiSeatId,
        target: UiKey,
        action: UiActionId,
    },
    SeatCancelled {
        seat: UiSeatId,
    },
    GeometryInvalidated,
    SurfaceCancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRoutedEventKind {
    HoverChanged {
        hovered: bool,
    },
    Pressed,
    Released {
        inside: bool,
    },
    Cancelled,
    FocusChanged {
        focused: bool,
    },
    ActivateRequested {
        action: UiActionId,
        source: UiActivationSource,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiRoutedEvent {
    target: UiKey,
    current_target: UiKey,
    phase: UiRoutePhase,
    kind: UiRoutedEventKind,
}

impl UiRoutedEvent {
    pub(crate) const fn new(
        target: UiKey,
        current_target: UiKey,
        phase: UiRoutePhase,
        kind: UiRoutedEventKind,
    ) -> Self {
        Self {
            target,
            current_target,
            phase,
            kind,
        }
    }

    #[must_use]
    pub const fn target(&self) -> &UiKey {
        &self.target
    }

    #[must_use]
    pub const fn current_target(&self) -> &UiKey {
        &self.current_target
    }

    #[must_use]
    pub const fn phase(&self) -> UiRoutePhase {
        self.phase
    }

    #[must_use]
    pub const fn kind(&self) -> &UiRoutedEventKind {
        &self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiNonActivation {
    NoPointerTarget,
    NoPointerCapture,
    NoFocusTarget,
    MissingPointerPosition,
    UnsupportedPointerButton,
    PointerAlreadyCaptured { target: UiKey },
    StaleGeometry,
    TargetRetired { target: UiKey },
    TargetHidden { target: UiKey },
    TargetOutsideModal { target: UiKey },
    TargetDisabled { target: UiKey },
    NoSupportedAction { target: UiKey },
    ActionUnavailable { target: UiKey, action: UiActionId },
}

/// Events committed by one consumed normalized input transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDispatchBatch {
    frame_revision: UiFrameRevision,
    presentation_epoch: u64,
    reconciliation: bool,
    events: Vec<UiRoutedEvent>,
}

impl UiDispatchBatch {
    pub(crate) const fn new(frame_revision: UiFrameRevision, presentation_epoch: u64) -> Self {
        Self {
            frame_revision,
            presentation_epoch,
            reconciliation: false,
            events: Vec::new(),
        }
    }

    pub(crate) const fn reconciliation(
        frame_revision: UiFrameRevision,
        presentation_epoch: u64,
    ) -> Self {
        Self {
            frame_revision,
            presentation_epoch,
            reconciliation: true,
            events: Vec::new(),
        }
    }

    #[must_use]
    pub const fn frame_revision(&self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.presentation_epoch
    }

    /// Returns whether this batch owns presentation reconciliation rather than event-time input.
    #[must_use]
    pub const fn is_reconciliation(&self) -> bool {
        self.reconciliation
    }

    #[must_use]
    pub fn events(&self) -> &[UiRoutedEvent] {
        &self.events
    }

    pub(crate) fn push(&mut self, event: UiRoutedEvent) {
        self.events.push(event);
    }
}

/// Closed result of routing one normalized event against an exact presented session.
///
/// A rejected result cannot carry routed events or a candidate session, so callers cannot
/// accidentally commit interaction state while reporting `RejectedClosed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiDispatchOutcome {
    Consumed(UiDispatchBatch),
    Rejected(UiNonActivation),
}

impl UiDispatchOutcome {
    #[must_use]
    pub const fn consumed(&self) -> Option<&UiDispatchBatch> {
        match self {
            Self::Consumed(batch) => Some(batch),
            Self::Rejected(_) => None,
        }
    }

    #[must_use]
    pub const fn rejection(&self) -> Option<&UiNonActivation> {
        match self {
            Self::Consumed(_) => None,
            Self::Rejected(reason) => Some(reason),
        }
    }
}
