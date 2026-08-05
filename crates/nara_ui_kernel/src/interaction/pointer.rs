use super::{
    UiNonActivation, UiPointerId, UiRoutedEventKind, UiSeatId,
    focus::{is_focusable_key, set_focus},
    routing::{hit_target, route_event, target_rejection},
    state::SurfaceState,
};
use crate::{UiDispatchBatch, UiFrame, UiKey};

pub(super) fn pointer_press(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let pointer_key = (seat, pointer);
    let pointer_state = state
        .pointers
        .get(&pointer_key)
        .expect("pointer preflight creates state");
    if let Some(target) = &pointer_state.captured {
        batch.reject(UiNonActivation::PointerAlreadyCaptured {
            target: target.clone(),
        });
        return;
    }
    let Some(position) = pointer_state.position else {
        batch.reject(UiNonActivation::MissingPointerPosition);
        return;
    };
    let target = match hit_target(frame, latest, state.modal_root.as_ref(), position) {
        Ok(Some(target)) => target,
        Ok(None) => {
            set_hover(state, seat, pointer, None, frame, batch);
            batch.reject(UiNonActivation::NoPointerTarget);
            return;
        }
        Err(reason) => {
            set_hover(state, seat, pointer, None, frame, batch);
            batch.reject(reason);
            return;
        }
    };
    set_hover(state, seat, pointer, Some(target.clone()), frame, batch);
    state
        .pointers
        .get_mut(&pointer_key)
        .expect("pointer preflight creates state")
        .captured = Some(target.clone());
    if is_focusable_key(frame, latest, state.modal_root.as_ref(), &target) {
        set_focus(state, seat, Some(target.clone()), frame, batch);
    }
    route_event(frame, &target, UiRoutedEventKind::Pressed, batch);
}

pub(super) fn pointer_release(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let pointer_key = (seat, pointer);
    let (captured, position) = {
        let pointer = state
            .pointers
            .get_mut(&pointer_key)
            .expect("pointer preflight creates state");
        (pointer.captured.take(), pointer.position)
    };
    let Some(target) = captured else {
        batch.reject(UiNonActivation::NoPointerCapture);
        return;
    };
    let Some(position) = position else {
        route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
        batch.reject(UiNonActivation::MissingPointerPosition);
        return;
    };
    if let Some(reason) = target_rejection(frame, latest, state.modal_root.as_ref(), &target) {
        route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
        batch.reject(reason);
        return;
    }
    let hovered = match hit_target(frame, latest, state.modal_root.as_ref(), position) {
        Ok(hovered) => hovered,
        Err(reason) => {
            batch.reject(reason);
            None
        }
    };
    let inside = hovered.as_ref() == Some(&target);
    route_event(
        frame,
        &target,
        UiRoutedEventKind::Released { inside },
        batch,
    );
    set_hover(state, seat, pointer, hovered, frame, batch);
}

pub(super) fn set_hover(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
    next: Option<UiKey>,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let pointer_state = state
        .pointers
        .get_mut(&(seat, pointer))
        .expect("pointer preflight creates state");
    if pointer_state.hovered == next {
        return;
    }
    let previous = std::mem::replace(&mut pointer_state.hovered, next.clone());
    if let Some(previous) = previous {
        route_event(
            frame,
            &previous,
            UiRoutedEventKind::HoverChanged { hovered: false },
            batch,
        );
    }
    if let Some(next) = next {
        route_event(
            frame,
            &next,
            UiRoutedEventKind::HoverChanged { hovered: true },
            batch,
        );
    }
}
