use std::sync::Arc;

use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    RuntimeGeneration,
};
use nara_ecs::{Resource, World, schedule::IntoScheduleConfigs};

use super::{
    OneShotPresentationCommit, UiPaintPacket, UiPresentationAttemptId,
    UiPresentationCapabilityGeneration, UiPresentationEpoch, UiPresentationOutcome,
    UiPresentationRuntimeGeneration, presentation::*,
};

#[cfg(feature = "wgpu")]
mod post_host;
mod surfaces;

#[cfg(feature = "wgpu")]
pub use post_host::UiPostHostConnection;

#[derive(Default, Resource)]
struct PresentationSequence(u64);

/// Compiles UI frames for each window and publishes input geometry after backend acceptance.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiPresentationBridgePlugin;

pub const UI_PRESENTATION_BRIDGE_PLUGIN_ID: PluginId = PluginId::new("nara.ui-presentation-bridge");
pub const UI_PRESENTATION_BRIDGE_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(UI_PRESENTATION_BRIDGE_PLUGIN_ID, PluginCategory::Render)
        .requires_plugins(&[nara_ui::UI_PLUGIN_ID, super::UI_RENDER_PLUGIN_ID]);

impl Plugin for UiPresentationBridgePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &UI_PRESENTATION_BRIDGE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<PresentationSequence>()?;
        app.add_systems(
            CoreStage::First,
            surfaces::synchronize_runtime_ui_surface_metrics
                .after(nara_window::WindowSet::ApplyEvents)
                .before(nara_input::InputSet::RouteUi),
        )?;
        app.add_systems(CoreStage::Queue, compile_runtime_ui_packets)?;
        app.add_systems(CoreStage::Cleanup, commit_runtime_ui_presentations)?;
        Ok(())
    }
}

fn compile_runtime_ui_packets(world: &mut World) {
    if backend_outcome_receiver(world)
        .is_none_or(|receiver| receiver.is_closing() || receiver.has_unsettled_backend_results())
    {
        return;
    }
    if let Err(error) = queue_runtime_ui_packets(world) {
        tracing::warn!(%error, "could not queue HUD presentation");
    }
}

fn queue_runtime_ui_packets(world: &mut World) -> Result<(), String> {
    // A backend that skipped the previous frame has no in-flight packet to protect.
    let skipped = replace_ui_backend_frame(world, Vec::new(), Vec::new())
        .map_err(|error| error.to_string())?;
    for submission in skipped {
        let _ = abort_ui_planned_packet(world, discard_prepared(submission));
    }

    surfaces::synchronize_runtime_ui_routes(world)?;
    let frames = nara_ui::ui_candidate_frames(world).map_err(|error| error.to_string())?;
    super::compile_ui_paint_packets(world, &frames).map_err(|error| error.to_string())?;
    let (planned, mut packets) = snapshot_ui_paint_packets(world)
        .map_err(|error| error.to_string())?
        .into_parts();
    let runtime_generation = world
        .get_resource::<RuntimeGeneration>()
        .map_or(1, |id| id.get());
    let runtime = UiPresentationRuntimeGeneration::new(runtime_generation)
        .expect("runtime generations are nonzero");
    let mut submissions = Vec::new();
    let mut sequence = world.resource_mut::<PresentationSequence>();
    for packet in planned {
        let frame = packet.frame();
        if !frames
            .iter()
            .any(|candidate| Arc::ptr_eq(candidate, &frame))
        {
            continue;
        }
        let value = sequence
            .0
            .checked_add(1)
            .ok_or("HUD presentation sequence exhausted")?;
        sequence.0 = value;
        let id =
            UiPresentationAttemptId::new(value).expect("increment issued a nonzero attempt ID");
        let epoch = UiPresentationEpoch::new(value).expect("increment issued a nonzero epoch");
        let slot = packet.identity().slot_generation();
        let capability = UiPresentationCapabilityGeneration::new(slot.get())
            .expect("surface slot generations are nonzero");
        replace_draw_packet(&mut packets, Arc::clone(&packet));
        submissions.push(prepare_backend_submission(
            id, packet, epoch, runtime, capability,
        ));
    }
    replace_ui_backend_frame(world, packets, submissions).map_err(|error| error.to_string())?;
    Ok(())
}

fn replace_draw_packet(packets: &mut Vec<Arc<UiPaintPacket>>, packet: Arc<UiPaintPacket>) {
    if let Some(previous) = packets
        .iter_mut()
        .find(|previous| previous.target() == packet.target())
    {
        *previous = packet;
    } else {
        packets.push(packet);
    }
}

fn commit_runtime_ui_presentations(world: &mut World) {
    if backend_outcome_receiver(world)
        .is_none_or(|receiver| receiver.is_closing() || receiver.pending_backend_count() != 0)
    {
        return;
    }
    let Ok(outcomes) = super::drain_ui_presentation_outcomes(world) else {
        return;
    };
    for outcome in outcomes {
        if let UiPresentationOutcome::Presented(commit) = outcome {
            commit_presented_frame(world, commit);
        } else {
            if let Some(attempt) = outcome.attempt() {
                let _ = abort_ui_planned_packet(world, attempt);
            }
            if let Some(message) = outcome.message() {
                tracing::debug!(%message, "HUD packet was not presented");
            }
        }
    }
}

fn commit_presented_frame(world: &mut World, commit: OneShotPresentationCommit) {
    let Some(accepted) = super::integration::try_commit_ui_backend_presentation(world, commit)
    else {
        return;
    };
    let epoch = accepted.identity().presentation_epoch().get();
    let _ =
        nara_ui::presentation::publish_presented_frame(world, &accepted.packet().frame(), epoch);
}
