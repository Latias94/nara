use std::collections::BTreeMap;

use super::{
    UiInputEvent, UiPointerId, UiRoutedEventKind, UiSeatId,
    focus::{first_focus, is_focusable_key, set_focus},
    routing::{active_modal_root, is_interactive_key, route_event, target_rejection},
    session::UiDispatchError,
};
use crate::{UiDispatchBatch, UiFrame, UiKey, UiPoint, UiRootKey};

const MAX_SEATS_PER_SURFACE: usize = 8;
const MAX_POINTERS_PER_SURFACE: usize = 32;

#[derive(Clone, Debug, Default)]
pub(super) struct PointerState {
    pub(super) position: Option<UiPoint>,
    pub(super) hovered: Option<UiKey>,
    pub(super) captured: Option<UiKey>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SeatState {
    pub(super) focused: Option<UiKey>,
    pub(super) restore_after_modal: Option<UiKey>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SurfaceState {
    pub(super) invalidated_epoch: Option<u64>,
    pub(super) modal_root: Option<UiRootKey>,
    pub(super) seats: BTreeMap<UiSeatId, SeatState>,
    pub(super) pointers: BTreeMap<(UiSeatId, UiPointerId), PointerState>,
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
        if let Some(target) = captured
            && !is_interactive_key(frame, latest, state.modal_root.as_ref(), &target)
        {
            route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
            if let Some(reason) =
                target_rejection(frame, latest, state.modal_root.as_ref(), &target)
            {
                batch.reject(reason);
            }
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
        let focused = state.seats.get(&seat).and_then(|seat| seat.focused.clone());
        if focused.as_ref().is_some_and(|target| {
            !is_focusable_key(frame, latest, state.modal_root.as_ref(), target)
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
    if let Some(target) = pointer_state.captured.take() {
        route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
    }
    if let Some(target) = pointer_state.hovered.take() {
        route_event(
            frame,
            &target,
            UiRoutedEventKind::HoverChanged { hovered: false },
            batch,
        );
    }
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
