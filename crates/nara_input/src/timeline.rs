//! Bounded normalized Input ownership, observation and presentation-route publication.

#[path = "timeline/capacity.rs"]
mod capacity;
#[path = "timeline/owner.rs"]
mod owner;
#[path = "timeline/topology.rs"]
mod topology;
#[path = "timeline/types.rs"]
mod types;

use owner::HeldInput;
pub use owner::{
    InputEndpointClaim, InputRouteCommit, InputTimeline, PreparedInputRoutePublication,
    abort_presented_route, claim_input_endpoint, commit_presented_route, prepare_claimed_route,
    prepare_presented_route, prepared_route_invalidation, retire_claimed_endpoint,
    supersede_presented_route, unbind_claimed_endpoint,
};
#[doc(hidden)]
pub use types::{__InputDriverEvent, __InputDriverOutput};
pub use types::{
    ActionInputProvenance, InputAdmission, InputConfigurationError, InputConsumerCursors,
    InputEndpointRetirement, InputEventKind, InputEventOrigin, InputFrameObservations,
    InputGeometryGeneration, InputIngressError, InputInvalidationReason, InputLimits,
    InputPointerId, InputPresentationEpoch, InputPresentedRoute, InputRouteGeneration,
    InputRouteStamp, InputSeatId, InputSlotGeneration, InputSurfaceId, InputTimelineEntry,
    InputUiCursorError, InputUiDisposition, InputWindowEvent, InputWindowId, MAX_INPUT_ROUTES,
    MAX_INPUT_TIMELINE_ENTRIES,
};

#[cfg(test)]
#[path = "timeline/retention_tests.rs"]
mod retention_tests;
#[cfg(test)]
#[path = "timeline/tests.rs"]
mod tests;
