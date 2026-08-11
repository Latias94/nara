use std::{sync::Arc, time::Duration, time::Instant};

use nara::{
    app::{
        AppExitRequests, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
        PluginError, PluginId, PluginShutdownContext, PluginShutdownObligationId,
    },
    ecs::{Mut, Resource, With, World},
    prelude::{App, CoreStage, Plugin},
    render::RenderFrame,
    render_wgpu::WgpuRenderBackend,
    ui::{
        UiActionConsumerId, UiActionCursor, UiActionJournal, UiSurfaceId, advanced::UiKey,
        latest_ui_frame,
    },
    window::{PrimaryWindow, Window},
};
use nara_reference_game::{
    REFERENCE_DESKTOP_PLUGIN_ID, REFERENCE_RETRY_ACTION, ReferenceHudProjection, WaveRetryStatus,
    WaveRunGeneration,
};

use crate::desktop_support::{resident_product_glyphs, submitted_product_frame};

use super::evidence::{ProbeActionEvidence, ProbeEvidence, ProbePhase};

const PROBE_PLUGIN_ID: PluginId = PluginId::new("reference-game.desktop-accessibility-probe");
const PROBE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("reference-game.desktop-accessibility-probe", 1);
const PROBE_CONSUMER: &str = "reference-game.desktop-accessibility-probe";
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
}

fn observe_runtime_trajectory(world: &mut World) {
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
    let retry = *world.resource::<WaveRetryStatus>();
    let projection = *world.resource::<ReferenceHudProjection>();
    let frame = *world.resource::<RenderFrame>();
    let backend = world.resource::<WgpuRenderBackend>();
    let submitted = submitted_product_frame(&frame, backend);
    let ui_glyphs_resident = resident_product_glyphs(backend);

    if generation == 1
        && projection.terminal_visible
        && projection.retry_enabled
        && submitted
        && ui_glyphs_resident
    {
        match primary_window_title(world) {
            Ok(title) => evidence.observe_terminal_frame(&title, frame.index),
            Err(error) => evidence.fail(error),
        }
    }

    let armed_this_tick = if evidence.arm_is_requested() {
        if generation != 1
            || !projection.terminal_visible
            || !projection.retry_enabled
            || !submitted
            || !ui_glyphs_resident
        {
            evidence.fail("runtime Retry arming lost the exact submitted terminal state");
            false
        } else {
            match armed_retry_target(world) {
                Ok((frame_revision, target)) => evidence
                    .arm_runtime(journal_high_water, frame_revision, target)
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
            if evidence.observe_action(action).is_err() {
                break;
            }
        }
    }

    if generation > 2 {
        evidence.fail(format!(
            "run generation advanced past the one-shot Retry proof: {generation}"
        ));
    } else if retry.applied_generation() == Some(generation) {
        if evidence
            .observe_retry_applied(generation, retry.applied_ui_action_sequence())
            .is_ok()
            && submitted
        {
            let _ = evidence.observe_successor_frame(frame.index);
        }
    } else if evidence.has_observed_action() && retry.last_rejection().is_some() {
        evidence.fail(format!("product Retry was rejected: {retry:?}"));
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

fn armed_retry_target(world: &World) -> Result<(u64, UiKey), String> {
    let surface_id = UiSurfaceId::new(nara_reference_game::REFERENCE_UI_SURFACE_ID)
        .ok_or_else(|| "reference UI surface identity is invalid".to_owned())?;
    let frame = latest_ui_frame(world, surface_id)
        .map_err(|error| format!("failed to read the current Retry frame: {error}"))?
        .ok_or_else(|| "the current Retry frame is unavailable".to_owned())?;
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
    Ok((frame.revision().get(), target))
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
