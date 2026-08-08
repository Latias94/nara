use nara_ecs::World;
use nara_input::InputTimeline;
#[cfg(test)]
use nara_ui_kernel::UiRouteStamp;
use nara_ui_kernel::{UiAccessibilityInput, UiAccessibilityRequest, UiInputEvent};

use crate::runtime::UiRuntimeState;

mod journal;
mod timeline;
mod transaction;

pub use journal::{
    MAX_UI_ACTION_CONSUMERS, MAX_UI_ACTION_JOURNAL_ENTRIES, UiActionAdmissionError,
    UiActionConsumerError, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionJournal,
    UiActionSequence,
};
pub use transaction::{
    UiInputProjectionIssue, UiRouteDisposition, UiRouteRejection, UiRouteRejectionLog,
    UiRouteRejectionRecord,
};

/// Dispatches one receipt-stamped accessibility request through the same atomic UI transaction as
/// normalized pointer, keyboard, and gamepad input. Platform adapters do not receive an input
/// timeline sequence; the request is intentionally owned by the presented receipt instead.
pub fn dispatch_ui_accessibility_input(
    world: &mut World,
    input: UiAccessibilityInput,
) -> UiRouteDisposition {
    let route = input.route();
    let seat = input.seat();
    let event = match input.request() {
        UiAccessibilityRequest::FocusTarget { target } => UiInputEvent::FocusTarget {
            seat,
            target: target.clone(),
        },
        UiAccessibilityRequest::SemanticAction { target, action } => UiInputEvent::SemanticAction {
            seat,
            target: target.clone(),
            action: action.clone(),
        },
    };
    if !world.contains_resource::<UiRuntimeState>() || !world.contains_resource::<UiActionJournal>()
    {
        let rejection = UiRouteRejection::RuntimeUnavailable;
        record_rejection(world, None, rejection.clone());
        return UiRouteDisposition::RejectedClosed(rejection);
    }
    if world
        .resource::<UiRuntimeState>()
        .surface(route.surface_id())
        .is_some_and(|surface| surface.slot().endpoint().seat().get() != seat.get())
    {
        let rejection = UiRouteRejection::SeatOwnerMismatch;
        record_rejection(world, None, rejection.clone());
        return UiRouteDisposition::RejectedClosed(rejection);
    }
    let disposition = transaction::execute(world, route, event, None);
    if let UiRouteDisposition::RejectedClosed(rejection) = &disposition {
        record_rejection(world, None, rejection.clone());
    }
    disposition
}

use timeline::UiTimelineProjection;

pub(crate) fn route_timeline_ui_input(world: &mut World) {
    let entries = world
        .resource::<InputTimeline>()
        .pending_ui_entries()
        .copied()
        .collect::<Vec<_>>();
    for entry in entries {
        let disposition = route_timeline_entry(world, entry);
        if let UiRouteDisposition::RejectedClosed(rejection) = disposition {
            record_rejection(world, Some(entry.sequence()), rejection);
        }
    }
}

fn route_timeline_entry(
    world: &mut World,
    entry: nara_input::InputTimelineEntry,
) -> UiRouteDisposition {
    let projection = {
        let state = world.resource::<UiRuntimeState>();
        timeline::project_timeline_entry(state, entry)
    };
    match projection {
        UiTimelineProjection::PassThrough => UiRouteDisposition::PassThrough,
        UiTimelineProjection::Rejected(rejection) => UiRouteDisposition::RejectedClosed(
            transaction::reject_timeline_entry(world, entry.sequence(), rejection),
        ),
        UiTimelineProjection::Routed { route, input } => {
            transaction::execute(world, route, input, Some(entry.sequence()))
        }
    }
}

#[cfg(test)]
pub(crate) fn dispatch_presented_ui_input(
    world: &mut World,
    route: UiRouteStamp,
    input: UiInputEvent,
) -> UiRouteDisposition {
    if !world.contains_resource::<UiRuntimeState>() || !world.contains_resource::<UiActionJournal>()
    {
        let rejection = UiRouteRejection::RuntimeUnavailable;
        record_rejection(world, None, rejection.clone());
        return UiRouteDisposition::RejectedClosed(rejection);
    }
    let disposition = transaction::execute(world, route, input, None);
    if let UiRouteDisposition::RejectedClosed(rejection) = &disposition {
        record_rejection(world, None, rejection.clone());
    }
    disposition
}

fn record_rejection(world: &mut World, input_sequence: Option<u64>, rejection: UiRouteRejection) {
    if let Some(mut log) = world.get_resource_mut::<UiRouteRejectionLog>() {
        log.record(input_sequence, rejection);
    }
}
