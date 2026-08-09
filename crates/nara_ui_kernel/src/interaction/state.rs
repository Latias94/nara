use std::collections::BTreeMap;

use super::{
    UiInputEvent, UiPointerId, UiRoutedEventKind, UiSeatId,
    focus::{can_retain_focus, first_focus, is_focusable_key, set_focus},
    routing::{active_modal_root, is_interactive_key, route_event},
    session::UiDispatchError,
};
use crate::{
    UiDispatchBatch, UiFrame, UiKey, UiPoint, UiRevision, UiRootKey, UiRouteStamp, UiSurfaceId,
};

const MAX_SEATS_PER_SURFACE: usize = 1;
const MAX_POINTERS_PER_SURFACE: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UiRouteLineage {
    surface_id: UiSurfaceId,
    slot_generation: UiRevision,
}

impl UiRouteLineage {
    const fn from_route(route: UiRouteStamp) -> Self {
        Self {
            surface_id: route.surface_id(),
            slot_generation: route.slot_generation(),
        }
    }

    fn matches(self, route: UiRouteStamp) -> bool {
        self.surface_id == route.surface_id() && self.slot_generation == route.slot_generation()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UiGeometryInvalidation {
    lineage: UiRouteLineage,
    presentation_epoch: u64,
}

impl UiGeometryInvalidation {
    const fn from_route(route: UiRouteStamp) -> Self {
        Self {
            lineage: UiRouteLineage::from_route(route),
            presentation_epoch: route.presentation_epoch(),
        }
    }

    fn blocks(self, route: UiRouteStamp) -> bool {
        self.lineage.matches(route) && route.presentation_epoch() <= self.presentation_epoch
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PointerState {
    pub(super) position: Option<PointerPosition>,
    pub(super) hovered: Option<UiKey>,
    pub(super) captured: Option<PointerCapture>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PointerPosition {
    pub(super) point: UiPoint,
    pub(super) presentation_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PointerCapture {
    pub(super) target: UiKey,
    pub(super) presentation_epoch: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SeatState {
    pub(super) focused: Option<UiKey>,
    pub(super) restore_after_modal: Option<UiKey>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceState {
    pub(super) presentation_epoch: Option<u64>,
    pub(super) presented_route: Option<UiRouteStamp>,
    /// The latest geometry invalidation, scoped to its Surface Slot lineage.
    ///
    /// Presentation epochs are only ordered within one slot lifetime. Keeping the lineage with
    /// the invalidation prevents an epoch restart on a successor slot from inheriting the
    /// predecessor's stale-geometry barrier.
    invalidated_geometry: Option<UiGeometryInvalidation>,
    pub(super) modal_root: Option<UiRootKey>,
    pub(super) seats: BTreeMap<UiSeatId, SeatState>,
    pub(super) pointers: BTreeMap<(UiSeatId, UiPointerId), PointerState>,
}

impl SurfaceState {
    pub(super) fn invalidate_geometry(&mut self, route: UiRouteStamp) {
        let invalidation = UiGeometryInvalidation::from_route(route);
        let replace = match self.invalidated_geometry {
            None => true,
            Some(previous) if previous.lineage != invalidation.lineage => true,
            Some(previous) => invalidation.presentation_epoch > previous.presentation_epoch,
        };
        if replace {
            self.invalidated_geometry = Some(invalidation);
        }
    }

    pub(super) fn geometry_is_invalidated_for(&self, route: UiRouteStamp) -> bool {
        self.invalidated_geometry
            .is_some_and(|invalidated| invalidated.blocks(route))
    }

    pub(super) fn has_pointer_facts(&self) -> bool {
        self.pointers.values().any(|pointer| {
            pointer.position.is_some() || pointer.hovered.is_some() || pointer.captured.is_some()
        })
    }
}

pub(super) fn preflight_seat(
    state: &mut SurfaceState,
    seat: UiSeatId,
) -> Result<(), UiDispatchError> {
    if !state.seats.contains_key(&seat) && state.seats.len() == MAX_SEATS_PER_SURFACE {
        return Err(UiDispatchError::SeatLimitExceeded {
            maximum: MAX_SEATS_PER_SURFACE,
        });
    }
    state.seats.entry(seat).or_default();
    Ok(())
}

pub(super) fn preflight_pointer(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
) -> Result<(), UiDispatchError> {
    preflight_seat(state, seat)?;
    let key = (seat, pointer);
    if !state.pointers.contains_key(&key) && state.pointers.len() == MAX_POINTERS_PER_SURFACE {
        return Err(UiDispatchError::PointerLimitExceeded {
            maximum: MAX_POINTERS_PER_SURFACE,
        });
    }
    state.pointers.entry(key).or_default();
    Ok(())
}

pub(super) fn preflight_input_identity(
    state: &mut SurfaceState,
    input: &UiInputEvent,
) -> Result<(), UiDispatchError> {
    match input {
        UiInputEvent::PointerMoved { seat, pointer, .. }
        | UiInputEvent::PointerButton { seat, pointer, .. } => {
            preflight_pointer(state, *seat, *pointer)
        }
        UiInputEvent::Navigate { seat, .. }
        | UiInputEvent::ActivateFocused { seat, .. }
        | UiInputEvent::FocusTarget { seat, .. }
        | UiInputEvent::SemanticAction { seat, .. } => preflight_seat(state, *seat),
        UiInputEvent::PointerCancelled { .. }
        | UiInputEvent::SeatCancelled { .. }
        | UiInputEvent::GeometryInvalidated
        | UiInputEvent::SurfaceCancelled => Ok(()),
    }
}

pub(super) fn cancel_invalidated_pointer_interactions(
    state: &mut SurfaceState,
    invalidated: &[UiKey],
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    if invalidated.is_empty() {
        return;
    }

    let pointer_keys = state.pointers.keys().copied().collect::<Vec<_>>();
    for pointer_key in pointer_keys {
        let (captured, hovered) = state
            .pointers
            .get(&pointer_key)
            .map(|pointer| (pointer.captured.clone(), pointer.hovered.clone()))
            .unwrap_or_default();
        let captured_is_retired = captured
            .as_ref()
            .is_some_and(|capture| invalidated.contains(&capture.target));
        let hovered_is_retired = hovered
            .as_ref()
            .is_some_and(|target| invalidated.contains(target));
        if captured_is_retired || hovered_is_retired {
            cancel_pointer(state, pointer_key.0, pointer_key.1, frame, batch, false);
        }
    }
}

pub(super) fn reconcile_presented_pointer_state(
    state: &mut SurfaceState,
    previous: Option<(&UiFrame, u64)>,
    latest: &UiFrame,
    latest_epoch: Option<u64>,
    invalidated: &[UiKey],
    batch: &mut UiDispatchBatch,
) {
    let Some((previous, previous_epoch)) = previous else {
        cancel_all_pointers(state, latest, batch);
        return;
    };
    if !pointer_routing_is_equivalent(previous, latest) {
        cancel_all_pointers(state, latest, batch);
        return;
    }

    let pointer_keys = state.pointers.keys().copied().collect::<Vec<_>>();
    for (seat, pointer) in pointer_keys {
        let generation_mismatch = state.pointers.get(&(seat, pointer)).is_some_and(|pointer| {
            pointer
                .captured
                .as_ref()
                .is_some_and(|capture| capture.presentation_epoch != previous_epoch)
                || pointer
                    .position
                    .is_some_and(|position| position.presentation_epoch != previous_epoch)
                || (pointer.hovered.is_some() && pointer.position.is_none())
        });
        if generation_mismatch {
            cancel_pointer(state, seat, pointer, latest, batch, false);
        }
    }

    cancel_invalidated_pointer_interactions(state, invalidated, latest, batch);
    if let Some(latest_epoch) = latest_epoch {
        for pointer in state.pointers.values_mut() {
            if let Some(capture) = pointer.captured.as_mut() {
                capture.presentation_epoch = latest_epoch;
            }
            if let Some(position) = pointer.position.as_mut() {
                position.presentation_epoch = latest_epoch;
            }
        }
    }
}

fn pointer_routing_is_equivalent(previous: &UiFrame, latest: &UiFrame) -> bool {
    previous.environment().slot_generation() == latest.environment().slot_generation()
        && previous.environment().route_revision() == latest.environment().route_revision()
        && previous.environment().geometry_generation()
            == latest.environment().geometry_generation()
        && previous.layouts() == latest.layouts()
        && previous.hit_regions() == latest.hit_regions()
        && active_modal_root(previous) == active_modal_root(latest)
}

pub(super) fn reconcile_pointer_input_generation(
    state: &mut SurfaceState,
    input: &UiInputEvent,
    presentation_epoch: u64,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let pointer_key = match input {
        UiInputEvent::PointerMoved { seat, pointer, .. }
        | UiInputEvent::PointerButton { seat, pointer, .. } => (*seat, *pointer),
        _ => return,
    };
    let generation_changed = state.pointers.get(&pointer_key).is_some_and(|pointer| {
        pointer
            .captured
            .as_ref()
            .is_some_and(|capture| capture.presentation_epoch != presentation_epoch)
            || pointer
                .position
                .is_some_and(|position| position.presentation_epoch != presentation_epoch)
            || (pointer.hovered.is_some() && pointer.position.is_none())
    });
    if !generation_changed {
        return;
    }

    let Some(pointer) = state.pointers.get_mut(&pointer_key) else {
        return;
    };
    let captured = pointer.captured.take();
    let hovered = pointer.hovered.take();
    pointer.position = None;
    if let Some(capture) = captured {
        route_event(frame, &capture.target, UiRoutedEventKind::Cancelled, batch);
    }
    if let Some(target) = hovered {
        route_event(
            frame,
            &target,
            UiRoutedEventKind::HoverChanged { hovered: false },
            batch,
        );
    }
}

pub(super) fn reconcile_surface(
    state: &mut SurfaceState,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let next_modal = active_modal_root(frame);
    if state.modal_root != next_modal {
        reconcile_modal(state, next_modal.clone(), frame, latest, batch);
        state.modal_root = next_modal;
    }

    let pointer_keys = state.pointers.keys().copied().collect::<Vec<_>>();
    for (seat, pointer) in pointer_keys {
        let (hovered, captured) = state
            .pointers
            .get(&(seat, pointer))
            .map(|pointer| (pointer.hovered.clone(), pointer.captured.clone()))
            .unwrap_or_default();
        if let Some(capture) = captured
            && !is_interactive_key(frame, latest, state.modal_root.as_ref(), &capture.target)
        {
            route_event(frame, &capture.target, UiRoutedEventKind::Cancelled, batch);
            if let Some(pointer) = state.pointers.get_mut(&(seat, pointer)) {
                pointer.captured = None;
            }
        }
        if let Some(target) = hovered
            && !is_interactive_key(frame, latest, state.modal_root.as_ref(), &target)
        {
            route_event(
                frame,
                &target,
                UiRoutedEventKind::HoverChanged { hovered: false },
                batch,
            );
            if let Some(pointer) = state.pointers.get_mut(&(seat, pointer)) {
                pointer.hovered = None;
            }
        }
    }

    let seats = state.seats.keys().copied().collect::<Vec<_>>();
    for seat in seats {
        let restore_is_retired = state
            .seats
            .get(&seat)
            .and_then(|seat| seat.restore_after_modal.as_ref())
            .is_some_and(|target| !can_retain_focus(frame, latest, None, target));
        if restore_is_retired && let Some(seat) = state.seats.get_mut(&seat) {
            seat.restore_after_modal = None;
        }
        let focused = state.seats.get(&seat).and_then(|seat| seat.focused.clone());
        if focused.as_ref().is_some_and(|target| {
            !can_retain_focus(frame, latest, state.modal_root.as_ref(), target)
        }) {
            let next = first_focus(frame, latest, state.modal_root.as_ref());
            set_focus(state, seat, next, frame, batch);
        }
    }
}

fn reconcile_modal(
    state: &mut SurfaceState,
    next_modal: Option<UiRootKey>,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let seats = state.seats.keys().copied().collect::<Vec<_>>();
    for seat in seats {
        let current = state.seats.get(&seat).and_then(|seat| seat.focused.clone());
        let next = match (&state.modal_root, &next_modal) {
            (None, Some(modal)) => {
                if current
                    .as_ref()
                    .is_some_and(|focused| focused.root_key() != modal)
                    && let Some(seat_state) = state.seats.get_mut(&seat)
                {
                    seat_state.restore_after_modal = current.clone();
                }
                current
                    .filter(|focused| focused.root_key() == modal)
                    .filter(|focused| is_focusable_key(frame, latest, Some(modal), focused))
                    .or_else(|| first_focus(frame, latest, Some(modal)))
            }
            (Some(_), None) => {
                let restore = state
                    .seats
                    .get_mut(&seat)
                    .and_then(|seat| seat.restore_after_modal.take());
                restore
                    .filter(|target| is_focusable_key(frame, latest, None, target))
                    .or_else(|| first_focus(frame, latest, None))
            }
            (Some(_), Some(modal)) => current
                .filter(|focused| focused.root_key() == modal)
                .filter(|focused| is_focusable_key(frame, latest, Some(modal), focused))
                .or_else(|| first_focus(frame, latest, Some(modal))),
            (None, None) => current,
        };
        set_focus(state, seat, next, frame, batch);
    }
}

pub(super) fn cancel_pointer(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
    remove: bool,
) {
    let Some(mut pointer_state) = state.pointers.remove(&(seat, pointer)) else {
        return;
    };
    if let Some(capture) = pointer_state.captured.take() {
        route_event(frame, &capture.target, UiRoutedEventKind::Cancelled, batch);
    }
    if let Some(target) = pointer_state.hovered.take() {
        route_event(
            frame,
            &target,
            UiRoutedEventKind::HoverChanged { hovered: false },
            batch,
        );
    }
    pointer_state.position = None;
    if !remove {
        state.pointers.insert((seat, pointer), pointer_state);
    }
}

pub(super) fn cancel_seat(
    state: &mut SurfaceState,
    seat: UiSeatId,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let pointers = state
        .pointers
        .keys()
        .filter_map(|(pointer_seat, pointer)| (*pointer_seat == seat).then_some(*pointer))
        .collect::<Vec<_>>();
    for pointer in pointers {
        cancel_pointer(state, seat, pointer, frame, batch, true);
    }
    if let Some(mut seat_state) = state.seats.remove(&seat)
        && let Some(focused) = seat_state.focused.take()
    {
        route_event(
            frame,
            &focused,
            UiRoutedEventKind::FocusChanged { focused: false },
            batch,
        );
    }
}

pub(super) fn cancel_all_pointers(
    state: &mut SurfaceState,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let pointers = state.pointers.keys().copied().collect::<Vec<_>>();
    for (seat, pointer) in pointers {
        cancel_pointer(state, seat, pointer, frame, batch, false);
    }
}

pub(super) fn cancel_surface(
    state: &mut SurfaceState,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    cancel_all_pointers(state, frame, batch);
    let seats = state.seats.keys().copied().collect::<Vec<_>>();
    for seat in seats {
        cancel_seat(state, seat, frame, batch);
    }
}
