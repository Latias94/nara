use std::sync::Arc;

use crate::hud::presentation::{
    UiBackendOutcomeReceiver, UiPresentationPlannerClaim, backend_outcome_receiver,
    presentation_planner_claim,
};
use nara_app::{
    App, PluginError, PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_ecs::{Resource, World};

use super::{
    AttemptTermination, UI_PRESENTATION_BRIDGE_PLUGIN_ID, UiPresentationAttempt,
    UiPresentationAttemptLedger, clear_unconsumed_backend_frame, terminate_attempt,
};

pub(super) const UI_PRESENTATION_CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.ui-presentation.logical-close");
const UI_PRESENTATION_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.ui-presentation.logical-close");

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    let receiver =
        backend_outcome_receiver(app.world()).ok_or_else(|| PluginError::SetupFailed {
            plugin: UI_PRESENTATION_BRIDGE_PLUGIN_ID,
            message: "the UI presentation bridge requires its original backend outcome receiver"
                .to_owned(),
        })?;
    let planner =
        presentation_planner_claim(app.world()).ok_or_else(|| PluginError::SetupFailed {
            plugin: UI_PRESENTATION_BRIDGE_PLUGIN_ID,
            message: "the UI presentation bridge requires its original planner owner".to_owned(),
        })?;
    let owner = app
        .world()
        .resource::<UiPresentationAttemptLedger>()
        .owner();
    app.insert_resource(UiPresentationCloseAuthority {
        owner: Arc::clone(&owner),
    })?;
    app.register_plugin_runtime_close_participant(
        UI_PRESENTATION_CLOSE_OBLIGATION,
        UI_PRESENTATION_CLOSE_PARTICIPANT,
        UiPresentationClose {
            input: UiPresentationCloseInput {
                owner,
                receiver,
                planner,
            },
        },
    )?;
    Ok(())
}

/// The registered close consumer retains original result custody independently of World ingress.
struct UiPresentationClose {
    input: UiPresentationCloseInput,
}

impl UiPresentationClose {
    fn drive(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        context
            .with_world_mut(|world| close_original_presentations(world, &self.input))
            .map_err(RuntimeCloseParticipantError::retryable)
    }
}

impl RuntimeCloseParticipant for UiPresentationClose {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.drive(context)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.drive(context)
    }
}

#[derive(Resource)]
struct UiPresentationCloseAuthority {
    owner: Arc<()>,
}

struct UiPresentationCloseInput {
    owner: Arc<()>,
    receiver: UiBackendOutcomeReceiver,
    planner: UiPresentationPlannerClaim,
}

fn close_original_presentations(
    world: &mut World,
    input: &UiPresentationCloseInput,
) -> Result<RuntimeCloseProgress, &'static str> {
    input.receiver.begin_close();
    preflight_original_authority(world, input)?;
    world
        .resource_mut::<UiPresentationAttemptLedger>()
        .begin_close();
    clear_unconsumed_backend_frame(world)
        .map_err(|_| "ui-presentation-backend-queue-unavailable")?;
    while let Some(attempt_id) = world
        .resource::<UiPresentationAttemptLedger>()
        .first_attempt_id()
    {
        terminate_attempt(world, attempt_id, AttemptTermination::Aborted);
    }
    if input.receiver.pending_backend_count() != 0 {
        return Ok(RuntimeCloseProgress::Pending);
    }
    // Backend results do not grant interaction after logical close. Their original UI/Input
    // authorities were terminalized above; neither a Presented result nor Deferred can revive it.
    let outcomes = input
        .receiver
        .drain()
        .map_err(|_| "ui-presentation-original-outcomes-unavailable")?;
    drop(outcomes);
    Ok(RuntimeCloseProgress::Complete)
}

fn preflight_original_authority(
    world: &World,
    input: &UiPresentationCloseInput,
) -> Result<(), &'static str> {
    let authority = world
        .get_resource::<UiPresentationCloseAuthority>()
        .ok_or("ui-presentation-close-port-unavailable")?;
    let ledger = world
        .get_resource::<UiPresentationAttemptLedger>()
        .ok_or("ui-presentation-attempt-owner-unavailable")?;
    if !Arc::ptr_eq(&authority.owner, &input.owner) || !ledger.is_owned_by(&input.owner) {
        return Err("ui-presentation-attempt-owner-replaced");
    }
    if !input.planner.is_current(world) {
        return Err("ui-presentation-original-planner-unavailable");
    }
    for attempt in ledger.active() {
        if let UiPresentationAttempt::Prepared { authority, .. }
        | UiPresentationAttempt::Submitted { authority, .. } = attempt
            && !nara_ui::presentation::can_abort_prepared_ui_presentation(world, authority)
        {
            return Err("ui-presentation-original-input-unavailable");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
