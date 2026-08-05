use super::{UiNonActivation, UiRoutePhase, UiRoutedEvent, UiRoutedEventKind};
use crate::{
    UiDispatchBatch, UiFrame, UiHitRegion, UiKey, UiLayer, UiPoint, UiRootKey, UiRootStatus,
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
    if !region.enabled() {
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
}

pub(super) fn active_modal_root(frame: &UiFrame) -> Option<UiRootKey> {
    frame
        .layouts()
        .iter()
        .rev()
        .find(|layout| {
            layout.layer() == UiLayer::Modal
                && layout.visible()
                && root_is_published(frame, layout.root_key())
        })
        .map(|layout| layout.root_key().clone())
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
    let path = route_path(frame, target);
    let ancestors = &path[..path.len().saturating_sub(1)];
    for current in ancestors {
        batch.push(UiRoutedEvent::new(
            target.clone(),
            current.clone(),
            UiRoutePhase::Capture,
            kind.clone(),
        ));
    }
    batch.push(UiRoutedEvent::new(
        target.clone(),
        target.clone(),
        UiRoutePhase::Target,
        kind.clone(),
    ));
    for current in ancestors.iter().rev() {
        batch.push(UiRoutedEvent::new(
            target.clone(),
            current.clone(),
            UiRoutePhase::Bubble,
            kind.clone(),
        ));
    }
}

fn route_path(frame: &UiFrame, target: &UiKey) -> Vec<UiKey> {
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
