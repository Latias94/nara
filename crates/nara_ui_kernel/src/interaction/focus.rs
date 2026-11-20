use super::{
    UiActivationSource, UiNavigation, UiNonActivation, UiRoutedEventKind, UiSeatId,
    routing::{published_layout, route_event, surface_target_rejection},
    state::SurfaceState,
};
use crate::{UiDispatchBatch, UiFocusFact, UiFrame, UiKey, UiRect, UiRootKey, UiRouteStamp};

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
    state.mark_focus_changed();
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

pub(super) fn set_input_focus(
    state: &mut SurfaceState,
    seat: UiSeatId,
    target: UiKey,
    frame: &UiFrame,
    route: UiRouteStamp,
    batch: &mut UiDispatchBatch,
) -> Result<(), UiNonActivation> {
    if state.focus_is_stale(route)
        && state
            .seats
            .get(&seat)
            .and_then(|seat| seat.focused.as_ref())
            != Some(&target)
    {
        return Err(UiNonActivation::StaleFocus);
    }
    set_focus(state, seat, Some(target), frame, batch);
    Ok(())
}

pub(super) fn navigate_focus(
    state: &mut SurfaceState,
    seat: UiSeatId,
    direction: UiNavigation,
    frame: &UiFrame,
    latest: &UiFrame,
    route: UiRouteStamp,
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
        UiNavigation::Up | UiNavigation::Down | UiNavigation::Left | UiNavigation::Right => {
            if let Some(current) = current {
                if state.focus_is_stale(route) {
                    return Err(UiNonActivation::StaleFocus);
                }
                let origin = focus_fact(frame, current).ok_or(UiNonActivation::NoFocusTarget)?;
                let Some(next) = directional_focus(origin, direction, &candidates) else {
                    return Ok(());
                };
                next
            } else {
                candidates[0].key().clone()
            }
        }
    };
    set_input_focus(state, seat, next, frame, route, batch)
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
    origin: &UiFocusFact,
    direction: UiNavigation,
    candidates: &[&UiFocusFact],
) -> Option<UiKey> {
    let current_rect = origin.visible_rect();
    let (current_x, current_y) = rect_center(current_rect);
    let horizontal = matches!(direction, UiNavigation::Left | UiNavigation::Right);
    let cross_range = |rect: UiRect| {
        if horizontal {
            (rect.origin().y(), rect.max().y())
        } else {
            (rect.origin().x(), rect.max().x())
        }
    };
    let (current_min, current_max) = cross_range(current_rect);
    candidates
        .iter()
        .filter(|candidate| candidate.key() != origin.key())
        .filter_map(|candidate| {
            let rect = candidate.visible_rect();
            let (x, y) = rect_center(rect);
            let dx = x - current_x;
            let dy = y - current_y;
            let forward = match direction {
                UiNavigation::Up => -dy,
                UiNavigation::Down => dy,
                UiNavigation::Left => -dx,
                UiNavigation::Right => dx,
                UiNavigation::Next | UiNavigation::Previous => return None,
            };
            if forward <= 0.0 {
                return None;
            }
            let (min, max) = cross_range(rect);
            let aligned = min < current_max && current_min < max;
            Some((!aligned, dx * dx + dy * dy, candidate.key()))
        })
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
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
    if let Some(reason) = surface_target_rejection(frame, latest, modal_root, target) {
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
    let latest_disabled = !std::ptr::eq(frame, latest)
        && published_layout(latest, target).is_some_and(|layout| {
            let interaction = layout.interaction();
            interaction.requires_key() && !interaction.enabled()
        });
    if !semantic.enabled() || latest_disabled {
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
    focus_fact(frame, target)
        .is_some_and(|fact| is_focus_destination(frame, latest, modal_root, fact))
}

pub(super) fn can_retain_focus(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    target: &UiKey,
) -> bool {
    modal_root.is_none_or(|root| target.root_key() == root)
        && focus_fact(frame, target).is_some()
        && (std::ptr::eq(frame, latest) || focus_fact(latest, target).is_some())
}

pub(super) fn focus_fact<'a>(frame: &'a UiFrame, target: &UiKey) -> Option<&'a UiFocusFact> {
    frame.focus_order().iter().find(|fact| fact.key() == target)
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
    fact.enabled()
        && modal_root.is_none_or(|root| fact.key().root_key() == root)
        && (std::ptr::eq(frame, latest)
            || focus_fact(latest, fact.key()).is_some_and(UiFocusFact::enabled))
}

pub(super) fn first_focus(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
) -> Option<UiKey> {
    frame
        .focus_order()
        .iter()
        .find(|fact| is_focus_destination(frame, latest, modal_root, fact))
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
