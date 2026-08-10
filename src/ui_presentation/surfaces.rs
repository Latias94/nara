use std::collections::BTreeMap;

use nara_ecs::World;
use nara_ui::{
    UiSurfaceAvailability, UiSurfaceContentTransform, UiSurfaceSlot, UiSurfaceSlotSnapshot,
    UiSurfaceSuspensionReason,
};

pub(super) fn synchronize_runtime_ui_surface_metrics(world: &mut World) {
    let windows = {
        let mut query = world.query::<&nara_window::Window>();
        query
            .iter(world)
            .map(|window| (window.id, window.resolution))
            .collect::<BTreeMap<_, _>>()
    };
    let snapshots = match nara_ui::ui_surface_slot_snapshots(world) {
        Ok(snapshots) => snapshots,
        Err(error) => {
            super::record_bridge_error(world, error.to_string());
            return;
        }
    };

    for snapshot in snapshots {
        let action = classify_surface_action(snapshot, &windows);
        let Some(action) = action else {
            continue;
        };
        super::abort_surface_attempts(world, snapshot.slot().surface_id());
        let result = match action {
            UiSurfaceLifecycleAction::Suspend(reason) => {
                nara_ui::suspend_ui_surface(world, snapshot.slot().surface_id(), reason).map(|_| ())
            }
            UiSurfaceLifecycleAction::Update(transform) => nara_ui::update_ui_surface_slot(
                world,
                snapshot.slot().surface_id(),
                snapshot.slot().endpoint(),
                transform,
            )
            .map(|_| ()),
            UiSurfaceLifecycleAction::Restore(transform) => nara_ui::restore_ui_surface(
                world,
                snapshot.slot().surface_id(),
                snapshot.slot().endpoint(),
                transform,
            )
            .map(|_| ()),
        };
        if let Err(error) = result {
            super::record_bridge_error(world, error.to_string());
        }
    }
}

pub(super) fn synchronize_runtime_ui_routes(world: &mut World) -> Result<(), String> {
    let slots = nara_ui::active_ui_surface_slots(world).map_err(|error| error.to_string())?;
    let routes = slots
        .into_iter()
        .map(|slot| {
            nara_ui_render::__private::UiPresentationRoute::new(
                slot.surface_id(),
                nara_window::WindowId::new(slot.endpoint().window().get()),
            )
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    nara_ui_render::__private::replace_ui_presentation_routes(world, routes)
        .map_err(|error| error.to_string())
}

fn classify_surface_action(
    snapshot: UiSurfaceSlotSnapshot,
    windows: &BTreeMap<nara_window::WindowId, nara_window::WindowResolution>,
) -> Option<UiSurfaceLifecycleAction> {
    let slot = snapshot.slot();
    let window_id = nara_window::WindowId::new(slot.endpoint().window().get());
    let Some(resolution) = windows.get(&window_id).copied() else {
        return snapshot
            .availability()
            .is_active()
            .then_some(UiSurfaceLifecycleAction::Suspend(
                UiSurfaceSuspensionReason::EndpointUnavailable,
            ));
    };
    match surface_transform(slot, resolution) {
        Err(reason) => snapshot
            .availability()
            .is_active()
            .then_some(UiSurfaceLifecycleAction::Suspend(reason)),
        Ok(transform) => match snapshot.availability() {
            UiSurfaceAvailability::Active if slot.transform() == transform => None,
            UiSurfaceAvailability::Active => Some(UiSurfaceLifecycleAction::Update(transform)),
            UiSurfaceAvailability::Suspended(_) => {
                Some(UiSurfaceLifecycleAction::Restore(transform))
            }
        },
    }
}

fn surface_transform(
    slot: UiSurfaceSlot,
    resolution: nara_window::WindowResolution,
) -> Result<UiSurfaceContentTransform, UiSurfaceSuspensionReason> {
    if resolution.is_zero_sized() {
        return Err(UiSurfaceSuspensionReason::ZeroExtent);
    }
    if !resolution.scale_factor.is_finite() || resolution.scale_factor <= 0.0 {
        return Err(UiSurfaceSuspensionReason::InvalidScale);
    }
    let Some(physical_size) = nara_ui::UiSize::new(
        resolution.physical_width as f32,
        resolution.physical_height as f32,
    ) else {
        return Err(UiSurfaceSuspensionReason::InvalidScale);
    };
    let insets = resolution.safe_area_insets;
    let Some(safe_area) =
        nara_ui::UiSafeAreaInsets::new(insets.left, insets.top, insets.right, insets.bottom)
    else {
        return Err(UiSurfaceSuspensionReason::InvalidScale);
    };
    UiSurfaceContentTransform::new(
        slot.transform().authored_logical_size(),
        physical_size,
        safe_area,
        resolution.scale_factor as f32,
    )
    .map_err(|_| UiSurfaceSuspensionReason::InvalidScale)
}

enum UiSurfaceLifecycleAction {
    Suspend(UiSurfaceSuspensionReason),
    Update(UiSurfaceContentTransform),
    Restore(UiSurfaceContentTransform),
}

#[cfg(test)]
mod tests {
    use nara_input::{InputPointerId, InputSeatId, InputWindowId};
    use nara_ui::{UiSafeAreaInsets, UiSize, UiSurfaceEndpoint, UiSurfaceId};

    use super::*;

    fn test_slot() -> UiSurfaceSlot {
        UiSurfaceSlot::new(
            UiSurfaceId::new(7).expect("test surface should be non-zero"),
            UiSurfaceEndpoint::new(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(
                UiSize::new(960.0, 540.0).expect("test authored size should be valid"),
                UiSize::new(1280.0, 720.0).expect("test physical size should be valid"),
                UiSafeAreaInsets::default(),
                1.0,
            )
            .expect("test transform should be valid"),
        )
        .expect("test surface slot should be valid")
    }

    #[test]
    fn physical_targets_preserve_design_space_and_letterbox_non_widescreen_windows() {
        let slot = test_slot();
        let transform = surface_transform(slot, nara_window::WindowResolution::new(1_024, 768))
            .expect("a 4:3 target should produce a letterboxed transform");
        assert!((transform.content_scale_factor() - (1024.0 / 960.0)).abs() < 1.0e-6);
        assert!((transform.physical_content_origin().y() - 96.0).abs() < 1.0e-6);
        assert!((transform.physical_content_origin().x()).abs() < 1.0e-6);
        assert!((transform.physical_content_size().width() - 1024.0).abs() < 1.0e-6);
        assert!((transform.physical_content_size().height() - 576.0).abs() < 1.0e-6);
    }

    #[test]
    fn unavailable_targets_have_typed_suspension_reasons() {
        let slot = test_slot();
        assert!(matches!(
            surface_transform(slot, nara_window::WindowResolution::new(0, 720)),
            Err(UiSurfaceSuspensionReason::ZeroExtent)
        ));
        assert!(matches!(
            surface_transform(
                slot,
                nara_window::WindowResolution::new(1_280, 720).with_scale_factor(0.0),
            ),
            Err(UiSurfaceSuspensionReason::InvalidScale)
        ));
    }
}
