use super::{
    UiButtonPhase, UiInputEvent, UiNonActivation, UiPointerButton, UiPointerId, UiPointerInput,
    UiPointerInputKind, UiRoutedEventKind, UiSeatId,
    focus::{is_focusable_key, set_focus},
    routing::{
        hit_target, nearest_pointer_listener, route_event, route_pointer_event, target_rejection,
    },
    state::{PointerCapture, SurfaceState},
};
use crate::{UiDispatchBatch, UiFrame, UiInteraction, UiKey};

pub(super) fn pointer_button_is_unsupported(
    state: &SurfaceState,
    input: &UiInputEvent,
    frame: &UiFrame,
    epoch: u64,
) -> bool {
    let UiInputEvent::Pointer(input) = input else {
        return false;
    };
    let UiPointerInputKind::Button { button, phase } = input.kind() else {
        return false;
    };
    let pointer = state.pointers.get(&(input.seat(), input.pointer()));
    if pointer
        .and_then(|pointer| pointer.captured.as_ref())
        .is_some()
    {
        return false;
    }
    if matches!(button, UiPointerButton::Other(_)) {
        return true;
    }
    if phase == UiButtonPhase::Released {
        return false;
    }
    let position = input.position().or_else(|| {
        pointer
            .and_then(|pointer| pointer.position)
            .filter(|position| position.presentation_epoch == epoch)
            .map(|position| position.point)
    });
    position
        .and_then(|position| {
            frame.hit_regions().iter().find(|region| {
                region.rect().contains(position)
                    && region.clip_rect.is_none_or(|clip| clip.contains(position))
            })
        })
        .and_then(|region| {
            frame
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(region.key()))
        })
        .is_some_and(|layout| !layout.interaction().pointer_buttons().contains(button))
}

pub(super) fn pointer_move(
    state: &mut SurfaceState,
    input: UiPointerInput,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let seat = input.seat();
    let pointer = input.pointer();
    let position = input
        .position()
        .ok_or(UiNonActivation::MissingPointerPosition)?;
    let captured = state
        .pointers
        .get(&(seat, pointer))
        .and_then(|pointer| pointer.captured.as_ref())
        .map(|capture| capture.target.clone());
    let hovered = match hit_target(frame, latest, state.modal_root.as_ref(), position) {
        Ok(hovered) => hovered,
        Err(_) if captured.is_some() => None,
        Err(reason) => return Err(reason),
    };
    let target = if let Some(target) = captured {
        frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&target))
            .filter(|layout| layout.interaction().pointer_motion())
            .map(|_| target)
    } else if let Some(hit) = &hovered {
        nearest_pointer_listener(
            frame,
            latest,
            state.modal_root.as_ref(),
            hit,
            UiInteraction::pointer_motion,
        )?
    } else {
        None
    };
    set_hover(state, seat, pointer, hovered, frame, batch);
    if let Some(target) = target {
        route_pointer_event(
            frame,
            &target,
            UiRoutedEventKind::PointerMoved,
            input,
            batch,
        );
    }
    Ok(())
}

pub(super) fn pointer_scroll(
    state: &mut SurfaceState,
    input: UiPointerInput,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let position = input
        .position()
        .ok_or(UiNonActivation::MissingPointerPosition)?;
    let hit = hit_target(frame, latest, state.modal_root.as_ref(), position)?
        .ok_or(UiNonActivation::NoPointerTarget)?;
    let target = nearest_pointer_listener(
        frame,
        latest,
        state.modal_root.as_ref(),
        &hit,
        UiInteraction::scroll,
    )?
    .ok_or(UiNonActivation::NoScrollTarget)?;
    route_pointer_event(frame, &target, UiRoutedEventKind::Scrolled, input, batch);
    Ok(())
}

pub(super) fn pointer_press(
    state: &mut SurfaceState,
    input: UiPointerInput,
    frame: &UiFrame,
    latest: &UiFrame,
    presentation_epoch: u64,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let seat = input.seat();
    let pointer = input.pointer();
    let UiPointerInputKind::Button { button, .. } = input.kind() else {
        unreachable!("pointer press receives only button input");
    };
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
    let accepts = |frame: &UiFrame| {
        frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&target))
            .is_some_and(|layout| layout.interaction().pointer_buttons().contains(button))
    };
    if !accepts(frame) || !accepts(latest) {
        return Err(UiNonActivation::UnsupportedPointerButton);
    }
    set_hover(state, seat, pointer, Some(target.clone()), frame, batch);
    state
        .pointers
        .get_mut(&pointer_key)
        .expect("pointer preflight creates state")
        .captured = Some(PointerCapture {
        target: target.clone(),
        presentation_epoch,
        button,
    });
    if is_focusable_key(frame, latest, state.modal_root.as_ref(), &target) {
        set_focus(state, seat, Some(target.clone()), frame, batch);
    }
    route_pointer_event(frame, &target, UiRoutedEventKind::Pressed, input, batch);
    Ok(())
}

pub(super) fn pointer_release(
    state: &mut SurfaceState,
    input: UiPointerInput,
    frame: &UiFrame,
    latest: &UiFrame,
    presentation_epoch: u64,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let seat = input.seat();
    let pointer = input.pointer();
    let UiPointerInputKind::Button { button, .. } = input.kind() else {
        unreachable!("pointer release receives only button input");
    };
    let pointer_key = (seat, pointer);
    let (captured, position) = {
        let pointer = state
            .pointers
            .get_mut(&pointer_key)
            .expect("pointer preflight creates state");
        if let Some(capture) = &pointer.captured
            && capture.button != button
        {
            return Err(UiNonActivation::PointerButtonMismatch {
                captured: capture.button,
                received: button,
            });
        }
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
    route_pointer_event(
        frame,
        &target,
        UiRoutedEventKind::Released { inside },
        input,
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
