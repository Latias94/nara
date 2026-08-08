use nara_input::{InputPointerId, InputRouteStamp, InputSeatId, InputWindowId};
use nara_ui_kernel::{
    UiEnvironment, UiEnvironmentError, UiSurfaceContentTransform, UiSurfaceGenerations,
    UiSurfaceId, UiSurfaceRetirementToken,
};
use thiserror::Error;

const INITIAL_SURFACE_GENERATION: u64 = 1;

/// Host-owned platform endpoint for one UI surface lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSurfaceEndpoint {
    window: InputWindowId,
    seat: InputSeatId,
    pointer: InputPointerId,
}

impl UiSurfaceEndpoint {
    #[must_use]
    pub const fn new(window: InputWindowId, seat: InputSeatId, pointer: InputPointerId) -> Self {
        Self {
            window,
            seat,
            pointer,
        }
    }

    #[must_use]
    pub const fn window(self) -> InputWindowId {
        self.window
    }

    #[must_use]
    pub const fn seat(self) -> InputSeatId {
        self.seat
    }

    #[must_use]
    pub const fn pointer(self) -> InputPointerId {
        self.pointer
    }
}

/// One immutable Host-installed UI surface generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceSlot {
    surface_id: UiSurfaceId,
    endpoint: UiSurfaceEndpoint,
    transform: UiSurfaceContentTransform,
    slot_generation: u64,
    route_generation: u64,
    geometry_generation: u64,
    presentation_generation: u64,
    font_generation: u64,
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
            slot_generation: INITIAL_SURFACE_GENERATION,
            route_generation: INITIAL_SURFACE_GENERATION,
            geometry_generation: INITIAL_SURFACE_GENERATION,
            presentation_generation: INITIAL_SURFACE_GENERATION,
            font_generation: INITIAL_SURFACE_GENERATION,
        };
        slot.environment(INITIAL_SURFACE_GENERATION)?;
        Ok(slot)
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn endpoint(self) -> UiSurfaceEndpoint {
        self.endpoint
    }

    #[must_use]
    pub const fn transform(self) -> UiSurfaceContentTransform {
        self.transform
    }

    #[must_use]
    pub const fn slot_generation(self) -> u64 {
        self.slot_generation
    }

    #[must_use]
    pub const fn route_generation(self) -> u64 {
        self.route_generation
    }

    #[must_use]
    pub const fn geometry_generation(self) -> u64 {
        self.geometry_generation
    }

    #[must_use]
    pub const fn presentation_generation(self) -> u64 {
        self.presentation_generation
    }

    #[must_use]
    pub const fn font_generation(self) -> u64 {
        self.font_generation
    }

    pub(super) fn generations(self) -> Result<UiSurfaceGenerations, UiEnvironmentError> {
        UiSurfaceGenerations::new(
            self.slot_generation,
            self.route_generation,
            self.geometry_generation,
            self.presentation_generation,
        )
    }

    pub(super) fn environment(
        self,
        theme_generation: u64,
    ) -> Result<UiEnvironment, UiEnvironmentError> {
        UiEnvironment::for_surface(
            self.surface_id,
            self.transform,
            self.generations()?,
            theme_generation,
            self.font_generation,
        )
    }

    pub(super) fn successor(
        self,
        endpoint: UiSurfaceEndpoint,
        transform: UiSurfaceContentTransform,
    ) -> Result<Self, UiSurfaceSlotError> {
        let endpoint_changed = self.endpoint != endpoint;
        let transform_changed = self.transform != transform;
        if !endpoint_changed && !transform_changed {
            return Ok(self);
        }

        let slot_generation = advance_generation(self.slot_generation, "slot_generation")?;
        let route_generation = advance_generation(self.route_generation, "route_generation")?;
        let presentation_generation =
            advance_generation(self.presentation_generation, "presentation_generation")?;
        let geometry_generation = if transform_changed {
            advance_generation(self.geometry_generation, "geometry_generation")?
        } else {
            self.geometry_generation
        };
        Ok(Self {
            surface_id: self.surface_id,
            endpoint,
            transform,
            slot_generation,
            route_generation,
            geometry_generation,
            presentation_generation,
            font_generation: self.font_generation,
        })
    }

    pub(super) fn next_retirement_generation(self) -> Result<u64, UiSurfaceSlotError> {
        advance_generation(self.slot_generation, "retirement_generation")
    }
}

/// Exact bounded tombstone returned when one Surface Slot lifetime is retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSurfaceRetirement {
    endpoint: UiSurfaceEndpoint,
    token: UiSurfaceRetirementToken,
    input_route: Option<InputRouteStamp>,
}

impl UiSurfaceRetirement {
    pub(super) const fn new(
        endpoint: UiSurfaceEndpoint,
        token: UiSurfaceRetirementToken,
        input_route: Option<InputRouteStamp>,
    ) -> Self {
        Self {
            endpoint,
            token,
            input_route,
        }
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.token.surface_id()
    }

    #[must_use]
    pub const fn endpoint(self) -> UiSurfaceEndpoint {
        self.endpoint
    }

    #[must_use]
    pub const fn slot_generation(self) -> u64 {
        self.token.slot_generation().get()
    }

    #[must_use]
    pub const fn retirement_generation(self) -> u64 {
        self.token.retirement_generation().get()
    }

    #[must_use]
    pub const fn token(self) -> UiSurfaceRetirementToken {
        self.token
    }

    pub(super) const fn input_route(self) -> Option<InputRouteStamp> {
        self.input_route
    }
}

fn advance_generation(current: u64, field: &'static str) -> Result<u64, UiSurfaceSlotError> {
    current
        .checked_add(1)
        .ok_or(UiSurfaceSlotError::GenerationExhausted { field })
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiSurfaceSlotError {
    #[error("UI surface generation `{field}` was exhausted")]
    GenerationExhausted { field: &'static str },
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
}
