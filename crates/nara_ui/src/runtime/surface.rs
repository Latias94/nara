use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use thiserror::Error;

use crate::kernel::{
    UiEnvironment, UiEnvironmentError, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId,
};

/// Window, input seat and pointer used by a HUD surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSurfaceEndpoint {
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
}

impl UiSurfaceEndpoint {
    pub const fn new(window: InputWindowId, seat: InputSeatId, pointer: InputPointerId) -> Self {
        Self {
            window,
            seat,
            pointer,
        }
    }

    pub const fn window(self) -> InputWindowId {
        self.window
    }

    pub const fn seat(self) -> InputSeatId {
        self.seat
    }

    pub const fn pointer(self) -> InputPointerId {
        self.pointer
    }
}

/// Host-installed surface and its exact coordinate transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceSlot {
    surface_id: UiSurfaceId,
    endpoint: UiSurfaceEndpoint,
    transform: UiSurfaceContentTransform,
    generation: u64,
}

impl UiSurfaceSlot {
    pub fn new(
        surface_id: UiSurfaceId,
        endpoint: UiSurfaceEndpoint,
        transform: UiSurfaceContentTransform,
    ) -> Result<Self, UiSurfaceSlotError> {
        let slot = Self {
            surface_id,
            endpoint,
            transform,
            generation: 1,
        };
        slot.environment(1)?;
        Ok(slot)
    }

    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    pub const fn endpoint(self) -> UiSurfaceEndpoint {
        self.endpoint
    }

    pub const fn transform(self) -> UiSurfaceContentTransform {
        self.transform
    }

    pub const fn slot_generation(self) -> u64 {
        self.generation
    }

    pub const fn route_generation(self) -> u64 {
        self.generation
    }

    pub const fn geometry_generation(self) -> u64 {
        self.generation
    }

    pub const fn presentation_generation(self) -> u64 {
        self.generation
    }

    pub const fn font_generation(self) -> u64 {
        1
    }

    pub(super) fn environment(
        self,
        theme_revision: u64,
    ) -> Result<UiEnvironment, UiEnvironmentError> {
        UiEnvironment::for_surface(
            self.surface_id,
            self.transform,
            UiSurfaceGenerations::new(
                self.generation,
                self.generation,
                self.generation,
                self.generation,
            )?,
            theme_revision,
            self.font_generation(),
        )
    }

    pub(super) fn advance(
        self,
        endpoint: UiSurfaceEndpoint,
        transform: UiSurfaceContentTransform,
    ) -> Result<Self, UiSurfaceSlotError> {
        Ok(Self {
            endpoint,
            transform,
            generation: self.next_generation()?,
            ..self
        })
    }

    pub(super) fn next_generation(self) -> Result<u64, UiSurfaceSlotError> {
        self.generation
            .checked_add(1)
            .ok_or(UiSurfaceSlotError::GenerationExhausted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSurfaceSuspensionReason {
    ZeroExtent,
    EndpointUnavailable,
    InvalidScale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSurfaceAvailability {
    Active,
    Suspended(UiSurfaceSuspensionReason),
}

impl UiSurfaceAvailability {
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    pub const fn suspension_reason(self) -> Option<UiSurfaceSuspensionReason> {
        match self {
            Self::Active => None,
            Self::Suspended(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceSlotSnapshot {
    pub(super) slot: UiSurfaceSlot,
    pub(super) availability: UiSurfaceAvailability,
}

impl UiSurfaceSlotSnapshot {
    pub const fn slot(self) -> UiSurfaceSlot {
        self.slot
    }

    pub const fn availability(self) -> UiSurfaceAvailability {
        self.availability
    }

    pub const fn active_slot(self) -> Option<UiSurfaceSlot> {
        if self.availability.is_active() {
            Some(self.slot)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceSuspension {
    pub(super) previous_slot_generation: u64,
    pub(super) snapshot: UiSurfaceSlotSnapshot,
}

impl UiSurfaceSuspension {
    pub const fn previous_slot_generation(self) -> u64 {
        self.previous_slot_generation
    }

    pub const fn snapshot(self) -> UiSurfaceSlotSnapshot {
        self.snapshot
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceRetirement {
    pub(super) slot: UiSurfaceSlot,
    pub(super) retirement_generation: u64,
}

impl UiSurfaceRetirement {
    pub const fn surface_id(self) -> UiSurfaceId {
        self.slot.surface_id()
    }

    pub const fn endpoint(self) -> UiSurfaceEndpoint {
        self.slot.endpoint()
    }

    pub const fn slot_generation(self) -> u64 {
        self.slot.slot_generation()
    }

    pub const fn retirement_generation(self) -> u64 {
        self.retirement_generation
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiSurfaceSlotError {
    #[error("UI surface generation was exhausted")]
    GenerationExhausted,
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
}
