use super::{
    UiNonActivation, UiPointerId, UiRoutedEventKind, UiSeatId,
    focus::{is_focusable_key, set_focus},
    routing::{hit_target, route_event, target_rejection},
    state::{PointerCapture, SurfaceState},
};
use crate::{UiDispatchBatch, UiFrame, UiKey};

pub(super) fn pointer_press(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
    frame: &UiFrame,
    latest: &UiFrame,
    presentation_epoch: u64,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let pointer_key = (seat, pointer);
    let pointer_state = state
        .pointers
        .get(&pointer_key)
        .expect("pointer preflight creates state");
    if let Some(target) = &pointer_state.captured {
        return Err(UiNonActivation::PointerAlreadyCaptured {
            target: target.target.clone(),
        });
    }
    let Some(position) = pointer_state.position.map(|position| position.point) else {
        return Err(UiNonActivation::MissingPointerPosition);
    };
    let target = match hit_target(frame, latest, state.modal_root.as_ref(), position) {
        Ok(Some(target)) => target,
        Ok(None) => {
            set_hover(state, seat, pointer, None, frame, batch);
            return Err(UiNonActivation::NoPointerTarget);
        }
        Err(reason) => {
            set_hover(state, seat, pointer, None, frame, batch);
            return Err(reason);
        }
    };
    set_hover(state, seat, pointer, Some(target.clone()), frame, batch);
    state
        .pointers
        .get_mut(&pointer_key)
        .expect("pointer preflight creates state")
        .captured = Some(PointerCapture {
        target: target.clone(),
        presentation_epoch,
    });
    if is_focusable_key(frame, latest, state.modal_root.as_ref(), &target) {
        set_focus(state, seat, Some(target.clone()), frame, batch);
    }
    route_event(frame, &target, UiRoutedEventKind::Pressed, batch);
    Ok(())
}

pub(super) fn pointer_release(
    state: &mut SurfaceState,
    seat: UiSeatId,
    pointer: UiPointerId,
    frame: &UiFrame,
    latest: &UiFrame,
    presentation_epoch: u64,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let pointer_key = (seat, pointer);
    let (captured, position) = {
        let pointer = state
            .pointers
            .get_mut(&pointer_key)
            .expect("pointer preflight creates state");
        (
            pointer.captured.take(),
            pointer.position.map(|position| position.point),
        )
    };
    let Some(capture) = captured else {
        return Err(UiNonActivation::NoPointerCapture);
    };
    let target = capture.target;
    if capture.presentation_epoch != presentation_epoch {
        route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
        return Ok(());
    }
    let Some(position) = position else {
        route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
        return Ok(());
    };
    if target_rejection(frame, latest, state.modal_root.as_ref(), &target).is_some() {
        route_event(frame, &target, UiRoutedEventKind::Cancelled, batch);
        return Ok(());
    }
    let hovered = hit_target(frame, latest, state.modal_root.as_ref(), position).unwrap_or(None);
    let inside = hovered.as_ref() == Some(&target);
    route_event(
        frame,
        &target,
        UiRoutedEventKind::Released { inside },
        batch,
    );
    set_hover(state, seat, pointer, hovered, frame, batch);
    Ok(())
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
