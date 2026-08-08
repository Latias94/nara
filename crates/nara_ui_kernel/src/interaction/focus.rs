use super::{
    UiActivationSource, UiNavigation, UiNonActivation, UiRoutedEventKind, UiSeatId,
    routing::{route_event, target_rejection},
    state::SurfaceState,
};
use crate::{UiDispatchBatch, UiFocusFact, UiFrame, UiKey, UiRect, UiRootKey};

pub(super) fn set_focus(
    state: &mut SurfaceState,
    seat: UiSeatId,
    next: Option<UiKey>,
    frame: &UiFrame,
    batch: &mut UiDispatchBatch,
) {
    let seat_state = state.seats.entry(seat).or_default();
    if seat_state.focused == next {
        return;
    }
    let previous = std::mem::replace(&mut seat_state.focused, next.clone());
    if let Some(previous) = previous {
        route_event(
            frame,
            &previous,
            UiRoutedEventKind::FocusChanged { focused: false },
            batch,
        );
    }
    if let Some(next) = next {
        route_event(
            frame,
            &next,
            UiRoutedEventKind::FocusChanged { focused: true },
            batch,
        );
    }
}

pub(super) fn navigate_focus(
    state: &mut SurfaceState,
    seat: UiSeatId,
    direction: UiNavigation,
    frame: &UiFrame,
    latest: &UiFrame,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    let candidates = focus_candidates(frame, latest, state.modal_root.as_ref());
    if candidates.is_empty() {
        return Err(UiNonActivation::NoFocusTarget);
    }
    let current = state
        .seats
        .get(&seat)
        .and_then(|seat| seat.focused.as_ref());
    let next = match direction {
        UiNavigation::Next | UiNavigation::Previous => sequential_focus(
            current,
            direction,
            frame,
            latest,
            state.modal_root.as_ref(),
            &candidates,
        ),
        UiNavigation::Up | UiNavigation::Down | UiNavigation::Left | UiNavigation::Right => current
            .and_then(|current| directional_focus(current, direction, frame, &candidates))
            .unwrap_or_else(|| candidates[0].key().clone()),
    };
    set_focus(state, seat, Some(next), frame, batch);
    Ok(())
}

fn sequential_focus(
    current: Option<&UiKey>,
    direction: UiNavigation,
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    candidates: &[&UiFocusFact],
) -> UiKey {
    let order = frame.focus_order();
    let current_index = current.and_then(|current| {
        order
            .iter()
            .position(|candidate| candidate.key() == current)
    });
    let next = match (direction, current_index) {
        (UiNavigation::Next, Some(index)) => order[index + 1..]
            .iter()
            .chain(order[..index].iter())
            .find(|candidate| is_focus_destination(frame, latest, modal_root, candidate)),
        (UiNavigation::Previous, Some(index)) => order[..index]
            .iter()
            .rev()
            .chain(order[index + 1..].iter().rev())
            .find(|candidate| is_focus_destination(frame, latest, modal_root, candidate)),
        _ => None,
    };
    next.or_else(|| match direction {
        UiNavigation::Previous => candidates.last().copied(),
        _ => candidates.first().copied(),
    })
    .expect("navigation requires at least one focus destination")
    .key()
    .clone()
}

fn directional_focus(
    current: &UiKey,
    direction: UiNavigation,
    frame: &UiFrame,
    candidates: &[&UiFocusFact],
) -> Option<UiKey> {
    let current_rect = frame
        .focus_order()
        .iter()
        .find(|candidate| candidate.key() == current)?
        .rect();
    let (current_x, current_y) = rect_center(current_rect);
    candidates
        .iter()
        .filter(|candidate| candidate.key() != current)
        .filter_map(|candidate| {
            let (x, y) = rect_center(candidate.rect());
            let (primary, secondary) = match direction {
                UiNavigation::Up => (current_y - y, (current_x - x).abs()),
                UiNavigation::Down => (y - current_y, (current_x - x).abs()),
                UiNavigation::Left => (current_x - x, (current_y - y).abs()),
                UiNavigation::Right => (x - current_x, (current_y - y).abs()),
                UiNavigation::Next | UiNavigation::Previous => return None,
            };
            (primary > 0.0).then_some((primary, secondary, candidate.key()))
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
                .then_with(|| left.2.cmp(right.2))
        })
        .map(|(_, _, key)| key.clone())
}

fn rect_center(rect: UiRect) -> (f32, f32) {
    (
        rect.origin().x() + rect.size().width() * 0.5,
        rect.origin().y() + rect.size().height() * 0.5,
    )
}

pub(super) fn activate_target(
    target: &UiKey,
    requested_action: Option<crate::UiActionId>,
    source: UiActivationSource,
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    if let Some(reason) = target_rejection(frame, latest, modal_root, target) {
        return Err(reason);
    }
    let Some(semantic) = frame
        .semantics()
        .iter()
        .find(|semantic| semantic.key() == target)
    else {
        return Err(UiNonActivation::NoSupportedAction {
            target: target.clone(),
        });
    };
    if !semantic.enabled() {
        return Err(UiNonActivation::TargetDisabled {
            target: target.clone(),
        });
    }
    let action = if let Some(action) = requested_action {
        if !semantic.actions().contains(&action) {
            return Err(UiNonActivation::ActionUnavailable {
                target: target.clone(),
                action,
            });
        }
        action
    } else {
        let Some(action) = semantic.actions().first().cloned() else {
            return Err(UiNonActivation::NoSupportedAction {
                target: target.clone(),
            });
        };
        action
    };
    route_event(
        frame,
        target,
        UiRoutedEventKind::ActivateRequested { action, source },
        batch,
    );
    Ok(())
}

pub(super) fn is_focusable_key(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    target: &UiKey,
) -> bool {
    target_rejection(frame, latest, modal_root, target).is_none()
        && frame.focus_order().iter().any(|fact| {
            fact.key() == target && is_focus_destination(frame, latest, modal_root, fact)
        })
}

pub(super) fn can_retain_focus(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    target: &UiKey,
) -> bool {
    target_rejection(frame, latest, modal_root, target).is_none()
        && frame.focus_order().iter().any(|fact| fact.key() == target)
}

fn focus_candidates<'a>(
    frame: &'a UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
) -> Vec<&'a UiFocusFact> {
    frame
        .focus_order()
        .iter()
        .filter(|fact| is_focus_destination(frame, latest, modal_root, fact))
        .collect()
}

fn is_focus_destination(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    fact: &UiFocusFact,
) -> bool {
    fact.enabled() && target_rejection(frame, latest, modal_root, fact.key()).is_none()
}

pub(super) fn first_focus(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
) -> Option<UiKey> {
    focus_candidates(frame, latest, modal_root)
        .first()
        .map(|fact| fact.key().clone())
}

pub(super) fn committed_focus(
    state: Option<&SurfaceState>,
    seat: UiSeatId,
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
) -> Option<UiKey> {
    state
        .and_then(|state| state.seats.get(&seat))
        .and_then(|seat| seat.focused.clone())
        .filter(|target| can_retain_focus(frame, latest, modal_root, target))
}
