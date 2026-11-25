use std::{collections::HashSet, sync::Arc};

use crate::hud::{
    UiPaintPacket, UiPresentationAttemptId, UiPresentationAttemptIdentity,
    UiPresentationBackendFaultDomain, UiPresentationRuntimeGeneration,
};
use nara_app::{App, CoreStage, Plugin, PluginError, RuntimeGeneration};
use nara_ecs::{World, schedule::IntoScheduleConfigs};
use nara_ui::{UiPresentationReceipt, UiSurfaceId};

mod attempts;
mod close;
mod observation;
#[cfg(feature = "wgpu")]
mod post_host;
mod surfaces;

use attempts::{UiPresentationAttempt, UiPresentationAttemptLedger};
#[cfg(feature = "wgpu")]
pub use observation::PresentedUiWgpuFacts;
pub use observation::{PresentedUiObservation, latest_presented_ui_observation};
#[cfg(feature = "wgpu")]
pub use post_host::UiPostHostConnection;

#[derive(Debug, Default, nara_ecs::Resource)]
struct UiPresentationBridgeStats {
    last_admitted_packets: u32,
    last_rejected_packets: u32,
    presented_packets: u64,
    invalidated_presentations: u64,
    deferred_packets: u64,
    content_rejections: u64,
    backend_faults: u64,
    aborted_packets: u64,
    superseded_packets: u64,
    stale_backend_acceptances: u64,
    last_presented_receipt: Option<UiPresentationReceipt>,
    last_error: Option<String>,
}

/// Connects runtime UI surfaces and presented input to the renderer's HUD packet exchange.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiPresentationBridgePlugin;

pub const UI_PRESENTATION_BRIDGE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.ui-presentation-bridge");
const UI_PRESENTATION_BRIDGE_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] =
    &[nara_ui::UI_PLUGIN_ID, crate::hud::UI_RENDER_PLUGIN_ID];
const UI_PRESENTATION_BRIDGE_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("presentation-ui")];
pub const UI_PRESENTATION_BRIDGE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        UI_PRESENTATION_BRIDGE_PLUGIN_ID,
        nara_app::PluginCategory::Render,
    )
    .requires_plugins(UI_PRESENTATION_BRIDGE_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(UI_PRESENTATION_BRIDGE_PRODUCT_REQUIREMENTS)
    .shutdown_obligations(&[close::UI_PRESENTATION_CLOSE_OBLIGATION]);

impl Plugin for UiPresentationBridgePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_PRESENTATION_BRIDGE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<UiPresentationBridgeStats>()?;
        app.init_resource::<UiPresentationAttemptLedger>()?;
        app.init_resource::<observation::PresentedUiObservationStore>()?;
        close::install(app)?;
        #[cfg(feature = "wgpu")]
        post_host::install(app)?;
        app.add_systems(
            CoreStage::First,
            surfaces::synchronize_runtime_ui_surface_metrics
                .after(nara_window::WindowSet::ApplyEvents)
                .before(nara_input::InputSet::RouteUi),
        )?;
        app.add_systems(CoreStage::Queue, compile_runtime_ui_packets)?;
        // CPU backends may return during the managed frame. Native WGPU results return later
        // and use the post-Host connection immediately, before another frame can run.
        app.add_systems(CoreStage::Cleanup, commit_runtime_ui_presentations)?;
        Ok(())
    }
}

fn compile_runtime_ui_packets(world: &mut World) {
    if world.resource::<UiPresentationAttemptLedger>().is_closing() {
        return;
    }
    world.resource_mut::<UiPresentationBridgeStats>().last_error = None;
    if let Err(error) = surfaces::synchronize_runtime_ui_routes(world) {
        record_bridge_error(world, error);
        return;
    }
    if let Err(error) = clear_unconsumed_backend_frame(world) {
        record_bridge_error(world, error);
        return;
    }
    if world
        .resource::<UiPresentationAttemptLedger>()
        .active()
        .any(UiPresentationAttempt::is_submitted)
    {
        let Some(receiver) = crate::hud::presentation::backend_outcome_receiver(world) else {
            record_bridge_error(
                world,
                "the original backend outcome owner is unavailable".to_owned(),
            );
            return;
        };
        if receiver.has_unsettled_backend_results() {
            return;
        }
        // The transport has already returned and its result was drained. A delayed one-shot
        // acceptance no longer pins capture admission; supersede its remaining original UI
        // authority before preparing the successor, so a later old commit fails closed.
        let stranded = world
            .resource::<UiPresentationAttemptLedger>()
            .active()
            .filter(|attempt| attempt.is_submitted())
            .map(UiPresentationAttempt::attempt_id)
            .collect::<Vec<_>>();
        abort_prepared_attempts(world, &stranded, AttemptTermination::Aborted);
    }
    let frames = match nara_ui::ui_candidate_frames(world) {
        Ok(frames) => frames,
        Err(error) => {
            record_bridge_error(world, error.to_string());
            return;
        }
    };
    let (mut submissions, mut submission_ids, retry_surfaces) = prepare_deferred_retries(world);
    let compile_frames = (!retry_surfaces.is_empty()).then(|| {
        frames
            .iter()
            .filter(|frame| !retry_surfaces.contains(&frame.environment().surface_id()))
            .cloned()
            .collect::<Vec<_>>()
    });
    let frames_to_compile = compile_frames.as_deref().unwrap_or(&frames);
    let stats = match crate::hud::compile_ui_paint_packets(world, frames_to_compile) {
        Ok(stats) => stats,
        Err(error) => {
            abort_prepared_attempts(world, &submission_ids, AttemptTermination::Aborted);
            record_bridge_error(world, error.to_string());
            return;
        }
    };
    let snapshots = match crate::hud::presentation::snapshot_ui_paint_packets(world) {
        Ok(snapshots) => snapshots,
        Err(error) => {
            abort_prepared_attempts(world, &submission_ids, AttemptTermination::Aborted);
            record_bridge_error(world, error.to_string());
            return;
        }
    };
    let (planned_packets, mut draw_packets) = snapshots.into_parts();
    for submission in &submissions {
        upsert_draw_packet(&mut draw_packets, Arc::clone(submission.packet()));
    }

    let runtime_generation = presentation_runtime_generation(world);
    for packet in planned_packets {
        let surface_id = packet.identity().surface_id();
        if retry_surfaces.contains(&surface_id) {
            continue;
        }
        let packet_frame = packet.frame();
        if !frames
            .binary_search_by_key(&surface_id, |frame| frame.environment().surface_id())
            .ok()
            .and_then(|index| frames.get(index))
            .is_some_and(|latest| Arc::ptr_eq(latest, &packet_frame))
        {
            continue;
        }
        match prepare_backend_attempt(world, Arc::clone(&packet), runtime_generation) {
            Ok(submission) => {
                submission_ids.push(submission.identity().admission_id());
                upsert_draw_packet(&mut draw_packets, Arc::clone(&packet));
                submissions.push(submission);
            }
            Err(error) => {
                let attempt = crate::hud::presentation::presentation_attempt(&packet);
                let _ = crate::hud::presentation::abort_ui_planned_packet(world, attempt);
                record_bridge_error(world, error);
            }
        }
    }

    match crate::hud::presentation::replace_ui_backend_frame(world, draw_packets, submissions) {
        Ok(unconsumed) => {
            abort_unconsumed_backend_submissions(world, unconsumed);
            let transition_error = {
                let mut ledger = world.resource_mut::<UiPresentationAttemptLedger>();
                submission_ids
                    .iter()
                    .copied()
                    .find_map(|attempt_id| ledger.mark_submitted(attempt_id).err())
            };
            if let Some(error) = transition_error {
                let cleanup_error = clear_unconsumed_backend_frame(world).err();
                let message = cleanup_error.map_or_else(
                    || error.to_string(),
                    |cleanup| format!("{error}; backend-frame cleanup also failed: {cleanup}"),
                );
                record_bridge_error(world, message);
                return;
            }
            let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
            bridge.last_admitted_packets = stats.admitted_packets;
            bridge.last_rejected_packets = stats.rejected_packets;
        }
        Err(error) => {
            abort_prepared_attempts(world, &submission_ids, AttemptTermination::Aborted);
            record_bridge_error(world, error.to_string());
        }
    }
}

fn presentation_runtime_generation(world: &World) -> UiPresentationRuntimeGeneration {
    let generation = world
        .get_resource::<RuntimeGeneration>()
        .map_or(1, |generation| generation.get());
    UiPresentationRuntimeGeneration::new(generation)
        .expect("managed and standalone runtime generations are non-zero")
}

fn prepare_backend_attempt(
    world: &mut World,
    packet: Arc<UiPaintPacket>,
    runtime_generation: UiPresentationRuntimeGeneration,
) -> Result<crate::hud::presentation::UiPreparedBackendSubmission, String> {
    let surface_id = packet.identity().surface_id();
    let reservation = world
        .resource_mut::<UiPresentationAttemptLedger>()
        .reserve(surface_id, runtime_generation)
        .map_err(|error| error.to_string())?;
    let authority = nara_ui::presentation::prepare_ui_presentation(
        world,
        packet.frame(),
        reservation.presentation_epoch.get(),
    )
    .map_err(|error| error.to_string())?;
    let submission = crate::hud::presentation::prepare_backend_submission(
        reservation.attempt_id,
        packet,
        reservation.presentation_epoch,
        reservation.runtime_generation,
        reservation.capability_generation,
    );
    let identity = submission.identity();
    if !identity.matches_receipt(authority.receipt()) {
        nara_ui::presentation::abort_prepared_ui_presentation(world, authority);
        let _ = crate::hud::presentation::discard_prepared(submission);
        return Err(format!(
            "UI presentation attempt {} does not match its pre-submit receipt authority",
            identity.admission_id().get()
        ));
    }
    let insert_error = world
        .resource_mut::<UiPresentationAttemptLedger>()
        .try_insert_prepared(identity, authority)
        .err();
    if let Some(insert_error) = insert_error {
        let (error, authority) = insert_error.into_parts();
        nara_ui::presentation::abort_prepared_ui_presentation(world, authority);
        let _ = crate::hud::presentation::discard_prepared(submission);
        return Err(error.to_string());
    }
    Ok(submission)
}

fn prepare_deferred_retries(
    world: &mut World,
) -> (
    Vec<crate::hud::presentation::UiPreparedBackendSubmission>,
    Vec<UiPresentationAttemptId>,
    HashSet<UiSurfaceId>,
) {
    let deferred_ids = world
        .resource::<UiPresentationAttemptLedger>()
        .deferred_ids();
    let mut submissions = Vec::with_capacity(deferred_ids.len());
    let mut submission_ids = Vec::with_capacity(deferred_ids.len());
    let mut surfaces = HashSet::with_capacity(deferred_ids.len());
    for attempt_id in deferred_ids {
        let Some((identity, ui, backend)) = world
            .resource_mut::<UiPresentationAttemptLedger>()
            .take_deferred(attempt_id)
        else {
            record_bridge_error(
                world,
                format!(
                    "deferred UI presentation attempt {} disappeared before retry",
                    attempt_id.get()
                ),
            );
            continue;
        };
        let surface_id = identity.packet().surface_id();
        match nara_ui::presentation::retry_deferred_ui_presentation(world, ui) {
            Ok(nara_ui::presentation::UiDeferredPresentationRetry::Prepared(authority)) => {
                let authority = *authority;
                let submission = crate::hud::presentation::retry_deferred(backend);
                if submission.identity() != identity {
                    let discarded_attempt = crate::hud::presentation::discard_prepared(submission);
                    nara_ui::presentation::abort_prepared_ui_presentation(world, authority);
                    let _ = crate::hud::presentation::abort_ui_planned_packet(
                        world,
                        crate::hud::presentation::submitted_attempt(identity),
                    );
                    world
                        .resource_mut::<UiPresentationBridgeStats>()
                        .aborted_packets += 1;
                    record_bridge_error(
                        world,
                        format!(
                            "deferred UI attempt {} changed backend identity to {}",
                            attempt_id.get(),
                            discarded_attempt.admission_id().map_or(0, |id| id.get())
                        ),
                    );
                    continue;
                }
                let insert_error = world
                    .resource_mut::<UiPresentationAttemptLedger>()
                    .try_insert_prepared(identity, authority)
                    .err();
                if let Some(insert_error) = insert_error {
                    let (error, authority) = insert_error.into_parts();
                    nara_ui::presentation::abort_prepared_ui_presentation(world, authority);
                    let discarded_attempt = crate::hud::presentation::discard_prepared(submission);
                    world
                        .resource_mut::<UiPresentationBridgeStats>()
                        .aborted_packets += 1;
                    record_bridge_error(world, error.to_string());
                    let _ =
                        crate::hud::presentation::abort_ui_planned_packet(world, discarded_attempt);
                    continue;
                }
                surfaces.insert(surface_id);
                submission_ids.push(attempt_id);
                submissions.push(submission);
            }
            Ok(nara_ui::presentation::UiDeferredPresentationRetry::Superseded) => {
                let attempt = crate::hud::presentation::discard_prepared(
                    crate::hud::presentation::retry_deferred(backend),
                );
                let _ = crate::hud::presentation::abort_ui_planned_packet(world, attempt);
                world
                    .resource_mut::<UiPresentationBridgeStats>()
                    .superseded_packets += 1;
            }
            Err(error) => {
                let attempt = crate::hud::presentation::discard_prepared(
                    crate::hud::presentation::retry_deferred(backend),
                );
                let _ = crate::hud::presentation::abort_ui_planned_packet(world, attempt);
                world
                    .resource_mut::<UiPresentationBridgeStats>()
                    .aborted_packets += 1;
                record_bridge_error(world, error.to_string());
            }
        }
    }
    (submissions, submission_ids, surfaces)
}

fn clear_unconsumed_backend_frame(world: &mut World) -> Result<(), String> {
    let unconsumed =
        crate::hud::presentation::replace_ui_backend_frame(world, Vec::new(), Vec::new())
            .map_err(|error| error.to_string())?;
    abort_unconsumed_backend_submissions(world, unconsumed);
    Ok(())
}

fn abort_unconsumed_backend_submissions(
    world: &mut World,
    submissions: Vec<crate::hud::presentation::UiPreparedBackendSubmission>,
) {
    for submission in submissions {
        let attempt = crate::hud::presentation::discard_prepared(submission);
        if let Some(attempt_id) = attempt.admission_id() {
            terminate_attempt(world, attempt_id, AttemptTermination::Aborted);
        }
    }
}

fn abort_prepared_attempts(
    world: &mut World,
    attempt_ids: &[UiPresentationAttemptId],
    termination: AttemptTermination,
) {
    for attempt_id in attempt_ids.iter().copied() {
        terminate_attempt(world, attempt_id, termination);
    }
}

fn abort_surface_attempts(world: &mut World, surface_id: UiSurfaceId) {
    let attempt_id = world
        .resource::<UiPresentationAttemptLedger>()
        .attempt_id_for_surface(surface_id);
    if let Some(attempt_id) = attempt_id {
        terminate_attempt(world, attempt_id, AttemptTermination::Aborted);
    }
}

fn upsert_draw_packet(packets: &mut Vec<Arc<UiPaintPacket>>, packet: Arc<UiPaintPacket>) {
    if let Some(existing) = packets
        .iter_mut()
        .find(|existing| existing.target() == packet.target())
    {
        *existing = packet;
    } else {
        packets.push(packet);
    }
}

fn commit_runtime_ui_presentations(world: &mut World) {
    let Some(ledger) = world.get_resource::<UiPresentationAttemptLedger>() else {
        record_bridge_error(
            world,
            "the UI presentation attempt owner is unavailable".to_owned(),
        );
        return;
    };
    if ledger.is_closing()
        || crate::hud::presentation::backend_outcome_receiver(world)
            .is_none_or(|receiver| receiver.is_closing())
    {
        return;
    }
    let outcomes = match crate::hud::drain_ui_presentation_outcomes(world) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            record_bridge_error(world, error.to_string());
            return;
        }
    };
    let mut device_fault_generations = HashSet::new();
    for outcome in outcomes {
        match outcome {
            crate::hud::UiPresentationOutcome::Presented(commit) => {
                commit_presented_attempt(world, commit);
            }
            crate::hud::UiPresentationOutcome::ContentRejected { attempt, message } => {
                terminate_outcome_attempt(world, attempt, AttemptTermination::Aborted);
                let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                bridge.content_rejections += 1;
                bridge.last_error = Some(message);
            }
            crate::hud::UiPresentationOutcome::Deferred { retry, message } => {
                defer_submitted_attempt(world, retry);
                let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                bridge.deferred_packets += 1;
                bridge.last_error = Some(message);
            }
            crate::hud::UiPresentationOutcome::BackendFault {
                attempt,
                backend_generation,
                domain,
                message,
            } => {
                if domain == UiPresentationBackendFaultDomain::Device
                    && let Some(generation) = backend_generation
                {
                    device_fault_generations.insert(generation);
                }
                if let Some(attempt) = attempt {
                    terminate_outcome_attempt(world, attempt, AttemptTermination::Aborted);
                }
                let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
                bridge.backend_faults += 1;
                bridge.last_error = Some(message);
            }
            crate::hud::UiPresentationOutcome::Aborted { attempt, .. } => {
                terminate_outcome_attempt(world, attempt, AttemptTermination::Aborted);
            }
            crate::hud::UiPresentationOutcome::Superseded { attempt, .. } => {
                terminate_outcome_attempt(world, attempt, AttemptTermination::Superseded);
            }
        }
    }
    for generation in device_fault_generations {
        let deferred = world
            .resource::<UiPresentationAttemptLedger>()
            .deferred_ids_for_backend(generation);
        abort_prepared_attempts(world, &deferred, AttemptTermination::Aborted);
    }
}

fn commit_presented_attempt(world: &mut World, commit: crate::hud::OneShotPresentationCommit) {
    let identity = commit.submission();
    let attempt_id = identity.admission_id();
    let Some(authority) = world
        .resource_mut::<UiPresentationAttemptLedger>()
        .take_submitted(identity, commit.packet())
    else {
        let _ = crate::hud::presentation::discard_ui_backend_acceptance(commit);
        let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
        bridge.stale_backend_acceptances += 1;
        bridge.last_error = Some(format!(
            "backend acceptance for stale UI attempt {} was discarded",
            attempt_id.get()
        ));
        return;
    };
    let accepted = crate::hud::presentation::commit_ui_backend_presentation(world, commit);
    let completion = nara_ui::presentation::commit_prepared_ui_presentation(world, authority);
    let receipt = match completion {
        nara_ui::presentation::UiPresentationCommit::Committed(receipt) => receipt,
        nara_ui::presentation::UiPresentationCommit::Invalidated { reason } => {
            let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
            bridge.presented_packets += 1;
            bridge.invalidated_presentations += 1;
            bridge.last_error = Some(format!(
                "UI attempt {} was accepted by the backend after presentation invalidation: {reason:?}",
                attempt_id.get(),
            ));
            return;
        }
    };
    let staged_observation = observation::stage_presented_ui_observation(world, &accepted);
    {
        let mut bridge = world.resource_mut::<UiPresentationBridgeStats>();
        bridge.presented_packets += 1;
        bridge.last_presented_receipt = Some(receipt);
    }
    match staged_observation {
        Ok(staged) => world
            .resource_mut::<observation::PresentedUiObservationStore>()
            .publish(staged.commit(receipt)),
        Err(error) => record_bridge_error(world, error),
    }
}

fn defer_submitted_attempt(
    world: &mut World,
    retry: crate::hud::presentation::UiDeferredBackendPresentation,
) {
    let identity = retry.identity().submission();
    let Some(authority) = world
        .resource_mut::<UiPresentationAttemptLedger>()
        .take_submitted(identity, retry.packet())
    else {
        let _ = crate::hud::presentation::discard_prepared(
            crate::hud::presentation::retry_deferred(retry),
        );
        return;
    };
    let deferred = nara_ui::presentation::defer_prepared_ui_presentation(world, authority);
    let insert_error = world
        .resource_mut::<UiPresentationAttemptLedger>()
        .try_insert_deferred(identity, deferred, retry)
        .err();
    if let Some(insert_error) = insert_error {
        let (error, (deferred, retry)) = insert_error.into_parts();
        nara_ui::presentation::supersede_deferred_ui_presentation(deferred);
        let attempt = crate::hud::presentation::discard_prepared(
            crate::hud::presentation::retry_deferred(retry),
        );
        let _ = crate::hud::presentation::abort_ui_planned_packet(world, attempt);
        record_bridge_error(world, error.to_string());
    }
}

fn terminate_outcome_attempt(
    world: &mut World,
    attempt: UiPresentationAttemptIdentity,
    termination: AttemptTermination,
) {
    if let Some(attempt_id) = attempt.admission_id() {
        terminate_attempt(world, attempt_id, termination);
    }
}

fn terminate_attempt(
    world: &mut World,
    attempt_id: UiPresentationAttemptId,
    termination: AttemptTermination,
) {
    let Some(attempt) = world
        .resource_mut::<UiPresentationAttemptLedger>()
        .take(attempt_id)
    else {
        return;
    };
    let attempt_identity = crate::hud::presentation::submitted_attempt(attempt.identity());
    match attempt {
        UiPresentationAttempt::Deferred { ui, backend, .. } => {
            nara_ui::presentation::supersede_deferred_ui_presentation(ui);
            let _ = crate::hud::presentation::discard_prepared(
                crate::hud::presentation::retry_deferred(backend),
            );
        }
        UiPresentationAttempt::Prepared { authority, .. }
        | UiPresentationAttempt::Submitted { authority, .. } => match termination {
            AttemptTermination::Aborted => {
                nara_ui::presentation::abort_prepared_ui_presentation(world, authority);
            }
            AttemptTermination::Superseded => {
                nara_ui::presentation::supersede_prepared_ui_presentation(world, authority);
            }
        },
    }
    let _ = crate::hud::presentation::abort_ui_planned_packet(world, attempt_identity);
    if let Some(mut bridge) = world.get_resource_mut::<UiPresentationBridgeStats>() {
        match termination {
            AttemptTermination::Aborted => bridge.aborted_packets += 1,
            AttemptTermination::Superseded => bridge.superseded_packets += 1,
        }
    }
}

#[derive(Clone, Copy)]
enum AttemptTermination {
    Aborted,
    Superseded,
}

fn record_bridge_error(world: &mut World, message: String) {
    if let Some(mut stats) = world.get_resource_mut::<UiPresentationBridgeStats>() {
        stats.last_error = Some(message);
    }
}

#[cfg(test)]
mod tests;
