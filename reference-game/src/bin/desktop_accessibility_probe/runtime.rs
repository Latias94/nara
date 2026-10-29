use std::{collections::VecDeque, sync::Arc, time::Duration, time::Instant};

use nara::{
    app::{
        AppExitRequests, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
        PluginError, PluginId, PluginShutdownContext, PluginShutdownObligationId,
        RuntimeTimeSettings,
    },
    ecs::{Mut, Resource, With, World},
    prelude::{App, CoreStage, Plugin},
    ui::{UiActionConsumerId, UiActionCursor, UiActionJournal, advanced::UiKey},
    ui_render::advanced::{PresentedUiObservation, latest_presented_ui_observation},
    window::{PrimaryWindow, Window},
};
use nara_reference_game::{
    REFERENCE_DESKTOP_PLUGIN_ID, REFERENCE_RETRY_ACTION, ReferenceHudProjection, WaveRetryStatus,
    WaveRunGeneration,
};

use super::evidence::{ProbeActionEvidence, ProbeEvidence, ProbePhase};

const PROBE_PLUGIN_ID: PluginId = PluginId::new("reference-game.desktop-accessibility-probe");
const PROBE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("reference-game.desktop-accessibility-probe", 1);
const PROBE_CONSUMER: &str = "reference-game.desktop-accessibility-probe";
const PRESENTED_HISTORY_LIMIT: usize = 3;
const PROBE_SHUTDOWN_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("reference-game.desktop-accessibility-probe.action-consumer");
const PROBE_REQUIREMENTS: &[PluginId] = &[REFERENCE_DESKTOP_PLUGIN_ID];
const PROBE_SHUTDOWN_OBLIGATIONS: &[PluginShutdownObligationId] = &[PROBE_SHUTDOWN_OBLIGATION];
const PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PROBE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(PROBE_REQUIREMENTS)
        .shutdown_obligations(PROBE_SHUTDOWN_OBLIGATIONS);

pub(crate) fn definition(evidence: Arc<ProbeEvidence>, timeout: Duration) -> PluginDefinition {
    PluginDefinition::infallible::<DesktopAccessibilityProbePlugin, _>(
        PROBE_DEFINITION_ID,
        b"reference-game-desktop-accessibility-probe-v3",
        move || DesktopAccessibilityProbePlugin {
            evidence: Arc::clone(&evidence),
            timeout,
        },
    )
}

#[derive(Debug)]
struct DesktopAccessibilityProbePlugin {
    evidence: Arc<ProbeEvidence>,
    timeout: Duration,
}

impl Plugin for DesktopAccessibilityProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let consumer = UiActionConsumerId::new(PROBE_CONSUMER).map_err(setup_error)?;
        if app.world().contains_resource::<ProbeUiActionCursor>() {
            return Err(setup_error("probe UI action consumer is already installed"));
        }
        let cursor = app
            .world_mut()?
            .get_resource_mut::<UiActionJournal>()
            .ok_or_else(|| setup_error("UI action journal is not installed"))?
            .register_consumer(consumer)
            .map_err(|error| setup_error(error.to_string()))?;
        app.register_plugin_shutdown_obligation(PROBE_SHUTDOWN_OBLIGATION)?;
        app.insert_resource(ProbeUiActionCursor(cursor))?
            .insert_resource(RuntimeProbeState {
                evidence: Arc::clone(&self.evidence),
                deadline: Instant::now()
                    .checked_add(self.timeout)
                    .unwrap_or_else(Instant::now),
                journal_high_water: 0,
                presented_history: VecDeque::new(),
            })?
            .add_systems(CoreStage::Last, observe_runtime_trajectory)?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        if !context.world().contains_resource::<ProbeUiActionCursor>() {
            return Ok(());
        }
        context.world_mut().resource_scope(
            |world, mut journal: Mut<UiActionJournal>| -> Result<(), PluginError> {
                let cursor = world
                    .get_resource::<ProbeUiActionCursor>()
                    .expect("validated probe UI action cursor remains installed");
                journal
                    .unregister_consumer(&cursor.0)
                    .map_err(|error| setup_error(error.to_string()))
            },
        )?;
        context.world_mut().remove_resource::<ProbeUiActionCursor>();
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct ProbeUiActionCursor(UiActionCursor);

#[derive(Debug, Resource)]
struct RuntimeProbeState {
    evidence: Arc<ProbeEvidence>,
    deadline: Instant,
    journal_high_water: u64,
    presented_history: VecDeque<PresentedUiObservation>,
}

impl RuntimeProbeState {
    fn remember_presented(&mut self, presented: PresentedUiObservation) {
        if self
            .presented_history
            .front()
            .is_some_and(|current| current.receipt() == presented.receipt())
        {
            return;
        }
        self.presented_history.push_front(presented);
        self.presented_history.truncate(PRESENTED_HISTORY_LIMIT);
    }

    fn presented_for_action(
        &self,
        action: &ProbeActionEvidence,
    ) -> Option<&PresentedUiObservation> {
        self.presented_history.iter().find(|presented| {
            let receipt = presented.receipt();
            receipt.surface_id().get() == action.surface_id
                && receipt.presentation_epoch() == action.presentation_epoch
                && receipt.frame_revision().get() == action.frame_revision
        })
    }
}

fn observe_runtime_trajectory(world: &mut World) {
    let latest_presented = latest_presented_ui_observation(world);
    if let Some(presented) = latest_presented.clone() {
        world
            .resource_mut::<RuntimeProbeState>()
            .remember_presented(presented);
    }
    let evidence = Arc::clone(&world.resource::<RuntimeProbeState>().evidence);
    let drain = match drain_retry_actions(world) {
        Ok(drain) => drain,
        Err(error) => {
            finish_with_failure(world, error);
            return;
        }
    };
    let journal_high_water = {
        let mut state = world.resource_mut::<RuntimeProbeState>();
        if let Some(last_sequence) = drain.last_sequence {
            state.journal_high_water = state.journal_high_water.max(last_sequence);
        }
        state.journal_high_water
    };
    let generation = world.resource::<WaveRunGeneration>().get();
    let (applied_generation, applied_ui_action_sequence, last_rejection) = {
        let retry = world.resource::<WaveRetryStatus>();
        (
            retry.applied_generation(),
            retry.applied_ui_action_sequence(),
            retry.last_rejection(),
        )
    };
    let projection = *world.resource::<ReferenceHudProjection>();
    let terminal_wgpu_frame = latest_presented
        .as_ref()
        .and_then(|presented| presented_product_wgpu_frame(presented).ok());

    if generation == 1
        && projection.terminal_visible
        && projection.retry_enabled
        && let Some(wgpu_frame) = terminal_wgpu_frame
    {
        match primary_window_title(world) {
            Ok(title) => evidence.observe_terminal_frame(&title, wgpu_frame),
            Err(error) => evidence.fail(error),
        }
    }

    let armed_this_tick = if evidence.arm_is_requested() {
        if generation != 1 || !projection.terminal_visible || !projection.retry_enabled {
            evidence.fail("runtime Retry arming lost the exact submitted terminal state");
            false
        } else {
            match latest_presented
                .as_ref()
                .ok_or_else(|| "the presented Retry observation is unavailable".to_owned())
                .and_then(armed_retry_target)
            {
                Ok((presentation_epoch, frame_revision, target)) => evidence
                    .arm_runtime(
                        journal_high_water,
                        presentation_epoch,
                        frame_revision,
                        target,
                    )
                    .is_ok(),
                Err(error) => {
                    evidence.fail(error);
                    false
                }
            }
        }
    } else {
        false
    };

    let had_retry_actions = !drain.retry_actions.is_empty();
    if !armed_this_tick {
        for action in drain.retry_actions {
            if let Err(error) = validate_presented_retry_action(world, &action) {
                evidence.fail(error);
                break;
            }
            if evidence.observe_action(action).is_err() {
                break;
            }
        }
    }

    if generation > 2 {
        evidence.fail(format!(
            "run generation advanced past the one-shot Retry proof: {generation}"
        ));
    } else if applied_generation == Some(generation) {
        if evidence
            .observe_retry_applied(generation, applied_ui_action_sequence)
            .is_ok()
            && let Some(presented) = latest_presented.as_ref()
        {
            match successor_presentation(presented) {
                Ok(Some((presentation_epoch, frame_revision, wgpu_frame))) => {
                    match evidence.observe_successor_presentation(
                        presentation_epoch,
                        frame_revision,
                        wgpu_frame,
                    ) {
                        Ok(true) => world.resource_mut::<RuntimeTimeSettings>().set_paused(true),
                        Ok(false) => {}
                        Err(error) => evidence.fail(error),
                    }
                }
                Ok(None) => {}
                Err(error) => evidence.fail(error),
            }
        }
    } else if evidence.has_observed_action() && last_rejection.is_some() {
        evidence.fail(format!(
            "product Retry was rejected: {:?}",
            world.resource::<WaveRetryStatus>()
        ));
    }

    if !had_retry_actions && evidence.phase() == ProbePhase::PlatformVerified {
        let _ = evidence.complete_after_journal_barrier(journal_high_water);
    }

    if !evidence.is_complete() && Instant::now() >= world.resource::<RuntimeProbeState>().deadline {
        evidence.fail("runtime timed out before the exact accessibility trajectory completed");
    }
    if evidence.should_stop() {
        world.resource_mut::<AppExitRequests>().request_exit();
    }
}

fn armed_retry_target(presented: &PresentedUiObservation) -> Result<(u64, u64, UiKey), String> {
    let _ = presented_product_wgpu_frame(presented)?;
    let receipt = presented.receipt();
    let frame = presented.frame();
    let mut targets = frame.semantics().iter().filter(|node| {
        node.enabled()
            && node
                .actions()
                .iter()
                .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
    });
    let target = targets
        .next()
        .map(|node| node.key().clone())
        .ok_or_else(|| {
            "the submitted terminal frame has no enabled Retry semantic target".to_owned()
        })?;
    if targets.next().is_some() {
        return Err(
            "the submitted terminal frame has multiple enabled Retry semantic targets".to_owned(),
        );
    }
    Ok((
        receipt.presentation_epoch(),
        receipt.frame_revision().get(),
        target,
    ))
}

fn successor_presentation(
    presented: &PresentedUiObservation,
) -> Result<Option<(u64, u64, u64)>, String> {
    let wgpu_frame = presented_product_wgpu_frame(presented)?;
    let frame = presented.frame();
    if frame.semantics().iter().any(|node| {
        node.actions()
            .iter()
            .any(|action| action.as_str() == REFERENCE_RETRY_ACTION)
    }) {
        return Ok(None);
    }
    Ok(Some((
        presented.receipt().presentation_epoch(),
        presented.receipt().frame_revision().get(),
        wgpu_frame,
    )))
}

fn presented_product_wgpu_frame(presented: &PresentedUiObservation) -> Result<u64, String> {
    let receipt = presented.receipt();
    let frame = presented.frame();
    if receipt.surface_id().get() != nara_reference_game::REFERENCE_UI_SURFACE_ID
        || frame.environment().surface_id() != receipt.surface_id()
        || frame.revision() != receipt.frame_revision()
        || !presented.submission().matches_receipt(receipt)
    {
        return Err("the presented UI frame does not match its exact receipt".to_owned());
    }
    let wgpu = presented
        .wgpu()
        .ok_or_else(|| "the presented UI observation lacks WGPU submission facts".to_owned())?;
    if wgpu.primitive_count() == 0
        || wgpu.glyph_primitive_count() == 0
        || wgpu.glyph_page_count() == 0
        || wgpu.submitted_quad_count() == 0
        || wgpu.submitted_glyph_quad_count() == 0
        || wgpu.referenced_texture_binding_count() == 0
    {
        return Err("the presented UI observation has an incomplete WGPU payload".to_owned());
    }
    Ok(wgpu.render_transaction_frame_index())
}

fn primary_window_title(world: &mut World) -> Result<String, String> {
    let mut query = world.query_filtered::<&Window, With<PrimaryWindow>>();
    let mut windows = query.iter(world);
    let title = windows
        .next()
        .map(|window| window.title.clone())
        .ok_or_else(|| "the primary product window is unavailable".to_owned())?;
    if windows.next().is_some() {
        return Err("multiple primary product windows are installed".to_owned());
    }
    Ok(title)
}

struct JournalDrain {
    retry_actions: Vec<ProbeActionEvidence>,
    last_sequence: Option<u64>,
}

fn drain_retry_actions(world: &mut World) -> Result<JournalDrain, String> {
    world.resource_scope(
        |world, mut journal: Mut<UiActionJournal>| -> Result<JournalDrain, String> {
            let cursor = world
                .get_resource::<ProbeUiActionCursor>()
                .ok_or_else(|| "probe UI action cursor disappeared".to_owned())?;
            let pending = journal
                .pending(&cursor.0)
                .map_err(|error| format!("failed to read probe UI actions: {error}"))?;
            let actions = pending
                .iter()
                .filter(|event| event.action().as_str() == REFERENCE_RETRY_ACTION)
                .map(|event| ProbeActionEvidence {
                    sequence: event.sequence().get(),
                    input_sequence: event.input_sequence(),
                    surface_id: event.surface_id().get(),
                    presentation_epoch: event.presentation_epoch(),
                    frame_revision: event.frame_revision(),
                    target: event.target().clone(),
                    action: event.action().clone(),
                    source: event.source(),
                })
                .collect();
            let last_sequence = pending.last().map(|event| event.sequence());
            if let Some(last_sequence) = last_sequence {
                journal
                    .acknowledge(&cursor.0, last_sequence)
                    .map_err(|error| format!("failed to acknowledge probe UI actions: {error}"))?;
            }
            Ok(JournalDrain {
                retry_actions: actions,
                last_sequence: last_sequence.map(|sequence| sequence.get()),
            })
        },
    )
}

fn validate_presented_retry_action(
    world: &World,
    action: &ProbeActionEvidence,
) -> Result<(), String> {
    let presented = world
        .resource::<RuntimeProbeState>()
        .presented_for_action(action)
        .ok_or_else(|| {
            format!(
                "the UIA Retry action has no retained owner-produced presentation: epoch={}, frame={}",
                action.presentation_epoch, action.frame_revision,
            )
        })?;
    let frame = presented.frame();
    let mut targets = frame.semantics().iter().filter(|node| {
        node.key() == &action.target
            && node.enabled()
            && node
                .actions()
                .iter()
                .any(|candidate| candidate == &action.action)
    });
    if targets.next().is_none() || targets.next().is_some() {
        return Err(
            "the exact presented receipt does not contain one enabled Retry semantic target"
                .to_owned(),
        );
    }
    Ok(())
}

fn finish_with_failure(world: &mut World, message: impl Into<String>) {
    let evidence = Arc::clone(&world.resource::<RuntimeProbeState>().evidence);
    evidence.fail(message);
    world.resource_mut::<AppExitRequests>().request_exit();
}

fn setup_error(message: impl ToString) -> PluginError {
    PluginError::SetupFailed {
        plugin: PROBE_PLUGIN_ID,
        message: message.to_string(),
    }
}
