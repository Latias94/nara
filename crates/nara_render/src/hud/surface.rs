use std::collections::HashMap;

use nara_ecs::{World, bevy_ecs::error::Result as SystemResult};
use nara_ui::{
    UiPrimarySurfaceMetrics, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, UiSurfaceSlot,
    UiSurfaceSuspensionReason,
};
use nara_window::{Window, WindowId, WindowResolution};

/// Synchronizes HUD surfaces after window events and before routing the next input frame.
pub fn sync_surface_metrics(world: &mut World) -> SystemResult {
    let Ok(slots) = nara_ui::ui_surface_slot_snapshots(world) else {
        return Ok(());
    };
    let primary = world.get_resource::<UiPrimarySurfaceMetrics>().copied();
    let windows = world
        .query::<&Window>()
        .iter(world)
        .map(|window| (window.id, window.resolution))
        .collect::<HashMap<_, _>>();

    for snapshot in slots {
        let slot = snapshot.slot();
        let id = slot.surface_id();
        let window = WindowId::new(slot.endpoint().window().get());
        let transform = windows
            .get(&window)
            .ok_or(UiSurfaceSuspensionReason::EndpointUnavailable)
            .and_then(|resolution| surface_transform(slot, *resolution, primary));
        match transform {
            Ok(transform) if !snapshot.availability().is_active() => {
                nara_ui::restore_ui_surface(world, id, slot.endpoint(), transform)?;
            }
            Ok(transform) if slot.transform() != transform => {
                nara_ui::update_ui_surface_slot(world, id, slot.endpoint(), transform)?;
            }
            Err(reason) if snapshot.availability().is_active() => {
                nara_ui::suspend_ui_surface(world, id, reason)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn surface_transform(
    slot: UiSurfaceSlot,
    resolution: WindowResolution,
    primary: Option<UiPrimarySurfaceMetrics>,
) -> Result<UiSurfaceContentTransform, UiSurfaceSuspensionReason> {
    use UiSurfaceSuspensionReason::{InvalidScale, ZeroExtent};

    if resolution.is_zero_sized() {
        return Err(ZeroExtent);
    }
    let size = UiSize::new(
        resolution.physical_width as f32,
        resolution.physical_height as f32,
    )
    .ok_or(InvalidScale)?;
    let insets = resolution.safe_area_insets;
    let safe_area = UiSafeAreaInsets::new(insets.left, insets.top, insets.right, insets.bottom)
        .ok_or(InvalidScale)?;
    let scale = resolution.scale_factor as f32;
    if let Some(primary) = primary.filter(|metrics| metrics.surface_id() == slot.surface_id()) {
        return primary
            .target_transform(size, safe_area, scale)
            .ok_or(InvalidScale);
    }
    UiSurfaceContentTransform::new(
        slot.transform().authored_logical_size(),
        size,
        safe_area,
        scale,
    )
    .map_err(|_| InvalidScale)
}
