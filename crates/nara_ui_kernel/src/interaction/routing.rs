use super::{UiNonActivation, UiPointerInput, UiRoutePhase, UiRoutedEvent, UiRoutedEventKind};
use crate::{
    UiDispatchBatch, UiFrame, UiHitRegion, UiInteraction, UiKey, UiPoint, UiRootKey, UiRootStatus,
};

pub(super) fn hit_target(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    position: UiPoint,
) -> Result<Option<UiKey>, UiNonActivation> {
    let Some(region) = frame
        .hit_regions()
        .iter()
        .find(|region| region_contains(region, position))
    else {
        return Ok(None);
    };
    if let Some(reason) = target_rejection(frame, latest, modal_root, region.key()) {
        return Err(reason);
    }
    if !region.enabled() || !key_is_enabled(latest, region.key()) {
        return Err(UiNonActivation::TargetDisabled {
            target: region.key().clone(),
        });
    }
    Ok(Some(region.key().clone()))
}

fn region_contains(region: &UiHitRegion, point: UiPoint) -> bool {
    region.rect.contains(point) && region.clip_rect.is_none_or(|clip| clip.contains(point))
}

pub(super) fn target_rejection(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    target: &UiKey,
) -> Option<UiNonActivation> {
    match key_availability(frame, target) {
        KeyAvailability::Retired => {
            return Some(UiNonActivation::TargetRetired {
                target: target.clone(),
            });
        }
        KeyAvailability::Hidden => {
            return Some(UiNonActivation::TargetHidden {
                target: target.clone(),
            });
        }
        KeyAvailability::Active => {}
    }
    match key_availability(latest, target) {
        KeyAvailability::Retired => {
            return Some(UiNonActivation::TargetRetired {
                target: target.clone(),
            });
        }
        KeyAvailability::Hidden => {
            return Some(UiNonActivation::TargetHidden {
                target: target.clone(),
            });
        }
        KeyAvailability::Active => {}
    }
    modal_root
        .is_some_and(|root| target.root_key() != root)
        .then(|| UiNonActivation::TargetOutsideModal {
            target: target.clone(),
        })
}

pub(super) fn is_interactive_key(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    target: &UiKey,
) -> bool {
    target_rejection(frame, latest, modal_root, target).is_none()
        && frame
            .hit_regions()
            .iter()
            .any(|region| region.key() == target && region.enabled())
        && key_is_enabled(latest, target)
}

fn key_is_enabled(frame: &UiFrame, target: &UiKey) -> bool {
    frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(target))
        .is_some_and(|layout| layout.interaction().enabled())
}

pub(super) fn nearest_pointer_listener(
    frame: &UiFrame,
    latest: &UiFrame,
    modal_root: Option<&UiRootKey>,
    hit: &UiKey,
    interested: fn(UiInteraction) -> bool,
) -> Result<Option<UiKey>, UiNonActivation> {
    for target in route_path(frame, hit).into_iter().rev() {
        let Some(layout) = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&target))
        else {
            continue;
        };
        if !interested(layout.interaction()) {
            continue;
        }
        if let Some(reason) = target_rejection(frame, latest, modal_root, &target) {
            return Err(reason);
        }
        if !layout.interaction().enabled() || !key_is_enabled(latest, &target) {
            return Err(UiNonActivation::TargetDisabled { target });
        }
        return Ok(latest
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&target))
            .filter(|layout| interested(layout.interaction()))
            .map(|_| target));
    }
    Ok(None)
}

pub(super) fn active_modal_root(frame: &UiFrame) -> Option<UiRootKey> {
    frame.active_modal_root().cloned()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyAvailability {
    Active,
    Hidden,
    Retired,
}

pub(super) fn key_availability(frame: &UiFrame, key: &UiKey) -> KeyAvailability {
    if !root_is_published(frame, key.root_key()) {
        return KeyAvailability::Retired;
    }
    match frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(key))
    {
        Some(layout) if layout.visible() => KeyAvailability::Active,
        Some(_) => KeyAvailability::Hidden,
        None => KeyAvailability::Retired,
    }
}

fn root_is_published(frame: &UiFrame, root: &UiRootKey) -> bool {
    frame
        .root_outcomes()
        .iter()
        .any(|outcome| outcome.key() == root && outcome.status() == UiRootStatus::Published)
}

pub(super) fn route_event(
    frame: &UiFrame,
    target: &UiKey,
    kind: UiRoutedEventKind,
    batch: &mut UiDispatchBatch,
) {
    route_event_with_pointer(frame, target, kind, None, batch);
}

pub(super) fn route_pointer_event(
    frame: &UiFrame,
    target: &UiKey,
    kind: UiRoutedEventKind,
    input: UiPointerInput,
    batch: &mut UiDispatchBatch,
) {
    route_event_with_pointer(frame, target, kind, Some(input), batch);
}

fn route_event_with_pointer(
    frame: &UiFrame,
    target: &UiKey,
    kind: UiRoutedEventKind,
    input: Option<UiPointerInput>,
    batch: &mut UiDispatchBatch,
) {
    let path = route_path(frame, target);
    let ancestors = &path[..path.len().saturating_sub(1)];
    for current in ancestors {
        batch.push(
            UiRoutedEvent::new(
                target.clone(),
                current.clone(),
                UiRoutePhase::Capture,
                kind.clone(),
            )
            .with_pointer_input(input),
        );
    }
    batch.push(
        UiRoutedEvent::new(
            target.clone(),
            target.clone(),
            UiRoutePhase::Target,
            kind.clone(),
        )
        .with_pointer_input(input),
    );
    for current in ancestors.iter().rev() {
        batch.push(
            UiRoutedEvent::new(
                target.clone(),
                current.clone(),
                UiRoutePhase::Bubble,
                kind.clone(),
            )
            .with_pointer_input(input),
        );
    }
}

pub(super) fn route_path(frame: &UiFrame, target: &UiKey) -> Vec<UiKey> {
    let mut reversed = vec![target.clone()];
    let mut current = target;
    while let Some(parent) = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(current))
        .and_then(|layout| layout.route_parent())
    {
        if reversed.contains(parent) {
            break;
        }
        reversed.push(parent.clone());
        current = parent;
    }
    reversed.reverse();
    reversed
}
