use std::collections::BTreeMap;

use super::routing::{active_modal_root, published_layout, route_path};
use crate::{UiFrame, UiHitRegion, UiKey, UiLayoutFact, UiPoint, UiRect};

fn environment_is_equivalent(previous: &UiFrame, latest: &UiFrame) -> bool {
    let previous = previous.environment();
    let latest = latest.environment();
    previous.surface_id() == latest.surface_id()
        && previous.slot_generation() == latest.slot_generation()
        && previous.route_revision() == latest.route_revision()
        && previous.geometry_generation() == latest.geometry_generation()
        && previous.content_transform() == latest.content_transform()
        && previous.theme_revision() == latest.theme_revision()
        && previous.font_revision() == latest.font_revision()
        && previous.snap_policy() == latest.snap_policy()
}

pub(super) fn pointer_routing_is_equivalent(previous: &UiFrame, latest: &UiFrame) -> bool {
    environment_is_equivalent(previous, latest)
        && active_modal_root(previous) == active_modal_root(latest)
        && previous
            .hit_regions()
            .iter()
            .map(hit_geometry)
            .eq(latest.hit_regions().iter().map(hit_geometry))
        && propagation_is_equivalent(previous, latest)
}

fn hit_geometry(region: &UiHitRegion) -> (&UiKey, UiRect, Option<UiRect>, UiPoint, UiPoint) {
    (
        region.key(),
        region.rect(),
        region.clip_rect,
        region.minimum,
        region.maximum,
    )
}

fn propagation_is_equivalent(previous: &UiFrame, latest: &UiFrame) -> bool {
    if previous.hit_regions().is_empty() {
        return true;
    }
    let mut remaining_layouts = keyed_layouts(previous);
    let latest_layouts = keyed_layouts(latest);
    for region in previous.hit_regions() {
        let mut current = Some(region.key());
        while let Some(key) = current {
            let Some(previous) = remaining_layouts.remove(key) else {
                break;
            };
            let Some(latest) = latest_layouts.get(key) else {
                return false;
            };
            let previous_input = previous.interaction();
            let latest_input = latest.interaction();
            if previous.route_parent() != latest.route_parent()
                || previous.visible() != latest.visible()
                || previous_input.pointer_motion() != latest_input.pointer_motion()
                || previous_input.scroll() != latest_input.scroll()
                || previous_input.pointer_buttons() != latest_input.pointer_buttons()
                || previous_input.capture_continuity() != latest_input.capture_continuity()
            {
                return false;
            }
            current = previous.route_parent();
        }
    }
    true
}

fn keyed_layouts(frame: &UiFrame) -> BTreeMap<&UiKey, &UiLayoutFact> {
    frame
        .layouts()
        .iter()
        .filter_map(|layout| layout.key().map(|key| (key, layout)))
        .collect()
}

pub(super) fn capture_declarations_are_equivalent(
    previous: &UiFrame,
    latest: &UiFrame,
    target: &UiKey,
) -> bool {
    let path = route_path(previous, target);
    path == route_path(latest, target)
        && path.iter().all(|key| {
            let previous = previous
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(key));
            let latest = latest
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(key));
            previous
                .zip(latest)
                .is_some_and(|(previous, latest)| previous.interaction() == latest.interaction())
        })
}

pub(super) fn target_geometry_is_equivalent(
    previous: &UiFrame,
    latest: &UiFrame,
    target: &UiKey,
) -> bool {
    if !environment_is_equivalent(previous, latest)
        || active_modal_root(previous) != active_modal_root(latest)
        || !capture_declarations_are_equivalent(previous, latest, target)
    {
        return false;
    }
    let previous_hit = previous
        .hit_regions()
        .iter()
        .find(|region| region.key() == target);
    let latest_hit = latest
        .hit_regions()
        .iter()
        .find(|region| region.key() == target);
    if !previous_hit
        .zip(latest_hit)
        .is_some_and(|(previous, latest)| {
            previous == latest && previous.enabled() && latest.enabled()
        })
    {
        return false;
    }
    route_path(previous, target).iter().all(|key| {
        let previous = published_layout(previous, key);
        let latest = published_layout(latest, key);
        previous.zip(latest).is_some_and(|(previous, latest)| {
            previous.visible()
                && latest.visible()
                && previous.route_parent() == latest.route_parent()
                && previous.layer() == latest.layer()
                && previous.local_z() == latest.local_z()
                && previous.rect() == latest.rect()
                && previous.physical_rect() == latest.physical_rect()
                && previous.clip_rect() == latest.clip_rect()
        })
    })
}
