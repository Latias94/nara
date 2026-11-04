use super::routing::{KeyAvailability, active_modal_root, key_availability, route_path};
use crate::{UiFrame, UiKey};

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
        && previous
            .layouts()
            .iter()
            .filter_map(|layout| {
                layout.key().map(|key| {
                    let input = layout.interaction();
                    (
                        key,
                        layout.route_parent(),
                        layout.visible(),
                        input.pointer_motion(),
                        input.scroll(),
                        input.pointer_buttons(),
                        input.capture_continuity(),
                    )
                })
            })
            .eq(latest.layouts().iter().filter_map(|layout| {
                layout.key().map(|key| {
                    let input = layout.interaction();
                    (
                        key,
                        layout.route_parent(),
                        layout.visible(),
                        input.pointer_motion(),
                        input.scroll(),
                        input.pointer_buttons(),
                        input.capture_continuity(),
                    )
                })
            }))
        && previous
            .hit_regions()
            .iter()
            .map(|region| (region.key(), region.rect(), region.clip_rect))
            .eq(latest
                .hit_regions()
                .iter()
                .map(|region| (region.key(), region.rect(), region.clip_rect)))
        && active_modal_root(previous) == active_modal_root(latest)
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
        if key_availability(previous, key) != KeyAvailability::Active
            || key_availability(latest, key) != KeyAvailability::Active
        {
            return false;
        }
        let previous = previous
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(key));
        let latest = latest
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(key));
        previous.zip(latest).is_some_and(|(previous, latest)| {
            previous.route_parent() == latest.route_parent()
                && previous.layer() == latest.layer()
                && previous.local_z() == latest.local_z()
                && previous.rect() == latest.rect()
                && previous.physical_rect() == latest.physical_rect()
                && previous.clip_rect() == latest.clip_rect()
                && previous.visible() == latest.visible()
                && previous.visible()
        })
    })
}
