use std::{collections::BTreeMap, mem};

use nara_app::{App, CoreStage, PluginError};
use nara_ecs::{Resource, World, schedule::IntoScheduleConfigs};
use nara_ui::{
    UiPresentationReceipt, UiSeatId, UiSurfaceAvailability, UiSurfaceId, UiSurfaceSlotSnapshot,
};
use nara_winit::__private::{
    WinitAccessibilityPlatformUpdate, drain_winit_accessibility_inputs,
    install_winit_accessibility_bridge, publish_winit_accessibility_update,
};

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    install_winit_accessibility_bridge(app)?;
    app.init_resource::<PlatformSurfaceMirror>()?;
    app.add_systems(
        CoreStage::First,
        synchronize_platform_surface_lifetimes
            .after(super::surfaces::synchronize_runtime_ui_surface_metrics)
            .before(nara_input::__private::InputSet::RouteUi),
    )?;
    app.add_systems(
        CoreStage::First,
        dispatch_platform_accessibility_inputs
            .after(synchronize_platform_surface_lifetimes)
            .before(nara_input::__private::InputSet::RouteUi),
    )?;
    Ok(())
}

pub(super) fn publish_presented(
    world: &mut World,
    receipt: UiPresentationReceipt,
) -> Result<(), String> {
    let slot = nara_ui::active_ui_surface_slots(world)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|slot| slot.surface_id() == receipt.surface_id())
        .ok_or_else(|| {
            format!(
                "presented UI surface {} has no active platform slot",
                receipt.surface_id().get()
            )
        })?;
    let snapshot =
        nara_ui::ui_semantic_snapshot(world, receipt, UiSeatId::new(slot.endpoint().seat().get()))
            .map_err(|error| error.to_string())?;
    publish_winit_accessibility_update(
        world,
        WinitAccessibilityPlatformUpdate::Present {
            window_id: nara_window::WindowId::new(slot.endpoint().window().get()),
            snapshot,
        },
    )
    .map_err(|error| error.to_string())
}

fn publish_invalidation(
    world: &mut World,
    previous: UiSurfaceSlotSnapshot,
    successor_slot_generation: u64,
) -> Result<(), String> {
    let slot = previous.slot();
    publish_winit_accessibility_update(
        world,
        WinitAccessibilityPlatformUpdate::Invalidate {
            window_id: nara_window::WindowId::new(slot.endpoint().window().get()),
            surface_id: slot.surface_id(),
            successor_slot_generation,
        },
    )
    .map_err(|error| error.to_string())
}

fn publish_detachment(
    world: &mut World,
    previous: UiSurfaceSlotSnapshot,
    successor_slot_generation: u64,
) -> Result<(), String> {
    let slot = previous.slot();
    publish_winit_accessibility_update(
        world,
        WinitAccessibilityPlatformUpdate::Detach {
            window_id: nara_window::WindowId::new(slot.endpoint().window().get()),
            surface_id: slot.surface_id(),
            successor_slot_generation,
        },
    )
    .map_err(|error| error.to_string())
}

fn publish_suspension(world: &mut World, snapshot: UiSurfaceSlotSnapshot) -> Result<(), String> {
    let slot = snapshot.slot();
    publish_winit_accessibility_update(
        world,
        WinitAccessibilityPlatformUpdate::Suspend {
            window_id: nara_window::WindowId::new(slot.endpoint().window().get()),
            surface_id: slot.surface_id(),
            slot_generation: slot.slot_generation(),
        },
    )
    .map_err(|error| error.to_string())
}

fn publish_retirement(world: &mut World, previous: UiSurfaceSlotSnapshot) -> Result<(), String> {
    let slot = previous.slot();
    let retirement = nara_ui::__private::retired_ui_surface_token(world, slot.surface_id())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "UI surface {} disappeared without an exact retirement token",
                slot.surface_id().get()
            )
        })?;
    publish_winit_accessibility_update(
        world,
        WinitAccessibilityPlatformUpdate::Retire {
            window_id: nara_window::WindowId::new(slot.endpoint().window().get()),
            retirement,
        },
    )
    .map_err(|error| error.to_string())
}

#[derive(Debug, Default, Resource)]
struct PlatformSurfaceMirror {
    slots: BTreeMap<UiSurfaceId, UiSurfaceSlotSnapshot>,
}

pub(super) fn synchronize_platform_surface_lifetimes(world: &mut World) {
    let snapshots = match nara_ui::ui_surface_slot_snapshots(world) {
        Ok(snapshots) => snapshots,
        Err(error) => {
            super::record_bridge_error(world, error.to_string());
            return;
        }
    };
    let current = snapshots
        .into_iter()
        .map(|snapshot| (snapshot.slot().surface_id(), snapshot))
        .collect::<BTreeMap<_, _>>();
    let previous = mem::take(&mut world.resource_mut::<PlatformSurfaceMirror>().slots);
    let mut committed = current.clone();

    for (surface_id, previous_snapshot) in &previous {
        let result = match current.get(surface_id).copied() {
            Some(snapshot)
                if snapshot.availability() == UiSurfaceAvailability::Active
                    && snapshot.slot().slot_generation()
                        != previous_snapshot.slot().slot_generation()
                    && snapshot.slot().endpoint().window()
                        != previous_snapshot.slot().endpoint().window() =>
            {
                publish_detachment(world, *previous_snapshot, snapshot.slot().slot_generation())
            }
            Some(snapshot)
                if snapshot.availability() == UiSurfaceAvailability::Active
                    && snapshot.slot().slot_generation()
                        != previous_snapshot.slot().slot_generation() =>
            {
                publish_invalidation(world, *previous_snapshot, snapshot.slot().slot_generation())
            }
            Some(snapshot)
                if matches!(snapshot.availability(), UiSurfaceAvailability::Suspended(_))
                    && snapshot.slot().slot_generation()
                        != previous_snapshot.slot().slot_generation() =>
            {
                publish_suspension(world, snapshot)
            }
            Some(_) => Ok(()),
            None => publish_retirement(world, *previous_snapshot),
        };
        if let Err(error) = result {
            super::record_bridge_error(world, error);
            committed.insert(*surface_id, *previous_snapshot);
        }
    }

    world.resource_mut::<PlatformSurfaceMirror>().slots = committed;
}

fn dispatch_platform_accessibility_inputs(world: &mut World) {
    let (inputs, rejected) = drain_winit_accessibility_inputs(world).into_parts();
    if rejected != 0 {
        super::record_bridge_error(
            world,
            format!("rejected {rejected} native accessibility inputs at the bounded ingress"),
        );
    }
    for input in inputs {
        let _ = nara_ui::dispatch_ui_accessibility_input(world, input);
    }
}
