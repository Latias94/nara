use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    app::{AppExitRequests, PluginCategory, PluginDeclaration, PluginError, PluginId},
    ecs::bevy_ecs::error::BevyError,
    fs::DirectoryCapability,
    prelude::{App, CoreStage, Plugin, Resource, World},
    project_host::{DesktopRun, DesktopRunIntent},
    render_wgpu::{WgpuBackendTelemetryConfig, WgpuRenderBackend},
    ui_render::advanced::latest_presented_ui_observation,
};
use nara_reference_game::{REFERENCE_DESKTOP_PLUGIN_ID, desktop_wave_recipe};

mod evidence;
mod measurement;

use evidence::ArmedCandidate;
pub(crate) use evidence::CandidateSmokeEvidence;
use measurement::{CandidateCompletionSample, CandidateMeasurement};

const CANDIDATE_SMOKE_DEADLINE: Duration = Duration::from_secs(30);
const CANDIDATE_SMOKE_PLUGIN_ID: PluginId = PluginId::new("reference-game.desktop-candidate-smoke");
const CANDIDATE_SMOKE_REQUIREMENTS: &[PluginId] = &[REFERENCE_DESKTOP_PLUGIN_ID];
const CANDIDATE_SMOKE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CANDIDATE_SMOKE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(CANDIDATE_SMOKE_REQUIREMENTS);

pub(crate) type CandidateSmokeEvidenceSink = Arc<Mutex<Option<CandidateSmokeEvidence>>>;

pub(crate) fn candidate_smoke_run(
    project_root: DirectoryCapability,
) -> (DesktopRun, CandidateSmokeEvidenceSink) {
    let evidence = Arc::new(Mutex::new(None));
    let recipe = candidate_smoke_recipe(Arc::clone(&evidence));
    (
        DesktopRun::new(
            project_root,
            DesktopRunIntent::new()
                .with_profile("desktop")
                .with_recipe(recipe),
        ),
        evidence,
    )
}

fn candidate_smoke_recipe(evidence: CandidateSmokeEvidenceSink) -> ProductRecipe {
    desktop_wave_recipe()
        .and_then(|recipe| {
            recipe.add_configured_plugin::<CandidateSmokePlugin, _, _>((), move |_| {
                CandidateSmokePlugin {
                    evidence: Arc::clone(&evidence),
                }
            })
        })
        .expect("the candidate smoke recipe is statically valid")
}

#[derive(Debug)]
struct CandidateSmokePlugin {
    evidence: CandidateSmokeEvidenceSink,
}

impl Plugin for CandidateSmokePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CANDIDATE_SMOKE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?
            .get_resource_mut::<WgpuRenderBackend>()
            .ok_or_else(|| candidate_setup_error("WGPU backend telemetry owner is not installed"))?
            .configure_telemetry(WgpuBackendTelemetryConfig::default());
        app.insert_resource(CandidateSmokeState {
            deadline: Instant::now() + CANDIDATE_SMOKE_DEADLINE,
            evidence: Arc::clone(&self.evidence),
            phase: CandidateSmokePhase::AwaitPresentedReady,
        })?
        .add_systems(CoreStage::Last, observe_candidate_frame)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct CandidateSmokeState {
    deadline: Instant,
    evidence: CandidateSmokeEvidenceSink,
    phase: CandidateSmokePhase,
}

#[derive(Debug)]
enum CandidateSmokePhase {
    AwaitPresentedReady,
    Measuring {
        armed: Box<ArmedCandidate>,
        measurement: CandidateMeasurement,
    },
}

fn observe_candidate_frame(world: &mut World) -> Result<(), BevyError> {
    if Instant::now() >= world.resource::<CandidateSmokeState>().deadline {
        return Err(BevyError::error(CandidateSmokeDeadlineExceeded));
    }
    world
        .resource::<WgpuRenderBackend>()
        .poll_telemetry_completions()
        .map_err(BevyError::error)?;
    let samples = drain_completion_samples(world)?;
    let newly_armed = register_latest_product_presentation(world)?;
    collect_completion_samples(world, samples, newly_armed)?;
    if seal_candidate_evidence(world)? {
        return Ok(());
    }
    Ok(())
}

fn register_latest_product_presentation(world: &mut World) -> Result<bool, BevyError> {
    let Some(presented) = latest_presented_ui_observation(world) else {
        return Ok(false);
    };
    let Some(candidate) = ArmedCandidate::capture(world, presented).map_err(measurement_error)?
    else {
        return Ok(false);
    };
    let identity = candidate.identity;
    let awaiting_ready = matches!(
        world.resource::<CandidateSmokeState>().phase,
        CandidateSmokePhase::AwaitPresentedReady
    );
    if awaiting_ready {
        let measurement = CandidateMeasurement::arm(identity);
        let state = &mut *world.resource_mut::<CandidateSmokeState>();
        state.phase = CandidateSmokePhase::Measuring {
            armed: Box::new(candidate),
            measurement,
        };
        return Ok(true);
    } else {
        let state = &mut *world.resource_mut::<CandidateSmokeState>();
        let CandidateSmokePhase::Measuring { measurement, .. } = &mut state.phase else {
            unreachable!("a candidate that is not awaiting readiness must be measuring");
        };
        measurement
            .admit_product_frame(identity)
            .map_err(measurement_error)?;
    }
    Ok(false)
}

fn drain_completion_samples(
    world: &mut World,
) -> Result<Vec<CandidateCompletionSample>, BevyError> {
    let (lost_samples, samples) = {
        let mut backend = world.resource_mut::<WgpuRenderBackend>();
        let lost_samples = backend.frame_completion_stats().lost_samples();
        let samples = backend
            .drain_frame_completion_samples()
            .into_iter()
            .map(|sample| {
                CandidateCompletionSample::new(
                    sample.device_epoch(),
                    sample.app_frame_index(),
                    sample.render_transaction_frame_index(),
                    sample.duration_ns(),
                )
            })
            .collect();
        (lost_samples, samples)
    };
    if lost_samples != 0 {
        return Err(measurement_error(
            "desktop candidate lost a GPU completion during product measurement",
        ));
    }
    Ok(samples)
}

fn collect_completion_samples(
    world: &mut World,
    samples: Vec<CandidateCompletionSample>,
    newly_armed: bool,
) -> Result<(), BevyError> {
    if matches!(
        world.resource::<CandidateSmokeState>().phase,
        CandidateSmokePhase::AwaitPresentedReady
    ) {
        return Ok(());
    }

    let (backend_instance_id, device_epoch) = {
        let backend = world.resource::<WgpuRenderBackend>();
        (backend.instance_id(), backend.device_epoch())
    };
    let state = &mut *world.resource_mut::<CandidateSmokeState>();
    let CandidateSmokePhase::Measuring { measurement, .. } = &mut state.phase else {
        unreachable!("the candidate phase was checked above");
    };
    let identity = measurement.identity();
    if backend_instance_id != identity.backend_instance_id()
        || device_epoch != identity.device_epoch()
    {
        return Err(measurement_error(
            "desktop candidate WGPU generation changed after measurement armed",
        ));
    }
    if newly_armed {
        measurement
            .observe_arm_boundary(samples)
            .map_err(measurement_error)?;
        return Ok(());
    }
    for sample in samples {
        measurement.observe(sample).map_err(measurement_error)?;
    }
    Ok(())
}

fn seal_candidate_evidence(world: &mut World) -> Result<bool, BevyError> {
    let seal_ready = {
        let state = world.resource::<CandidateSmokeState>();
        matches!(
            &state.phase,
            CandidateSmokePhase::Measuring { measurement, .. }
                if measurement.seal_input().is_some()
        )
    };
    if !seal_ready {
        return Ok(false);
    }
    let remaining = remaining_candidate_time(
        world.resource::<CandidateSmokeState>().deadline,
        Instant::now(),
    )
    .map_err(BevyError::error)?;
    world
        .resource::<WgpuRenderBackend>()
        .wait_for_telemetry_completions(remaining)
        .map_err(BevyError::error)?;
    if Instant::now() >= world.resource::<CandidateSmokeState>().deadline {
        return Err(BevyError::error(CandidateSmokeDeadlineExceeded));
    }
    drain_completion_samples(world)?;

    let (armed, presented_completion, samples, sink) = {
        let state = world.resource::<CandidateSmokeState>();
        let CandidateSmokePhase::Measuring { armed, measurement } = &state.phase else {
            unreachable!("a seal-ready candidate must be measuring");
        };
        let (completion, samples) = measurement
            .seal_input()
            .expect("seal readiness was checked above");
        (
            armed.as_ref().clone(),
            completion,
            samples.to_vec(),
            Arc::clone(&state.evidence),
        )
    };
    let evidence = CandidateSmokeEvidence::capture(
        world.resource::<WgpuRenderBackend>(),
        armed,
        presented_completion,
        &samples,
    )
    .map_err(measurement_error)?;
    *sink
        .lock()
        .map_err(|_| BevyError::error(CandidateSmokeEvidenceSinkPoisoned))? = Some(evidence);
    world.resource_mut::<AppExitRequests>().request_success();
    Ok(true)
}

fn remaining_candidate_time(
    deadline: Instant,
    now: Instant,
) -> Result<Duration, CandidateSmokeDeadlineExceeded> {
    deadline
        .checked_duration_since(now)
        .filter(|remaining| !remaining.is_zero())
        .ok_or(CandidateSmokeDeadlineExceeded)
}

fn measurement_error(message: &'static str) -> BevyError {
    BevyError::error(CandidateSmokeMeasurementIncomplete(message))
}

#[derive(Debug)]
struct CandidateSmokeDeadlineExceeded;

impl fmt::Display for CandidateSmokeDeadlineExceeded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("desktop candidate did not submit a bounded product frame")
    }
}

impl Error for CandidateSmokeDeadlineExceeded {}

#[derive(Debug)]
struct CandidateSmokeEvidenceSinkPoisoned;

impl fmt::Display for CandidateSmokeEvidenceSinkPoisoned {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("desktop candidate UI observation sink was poisoned")
    }
}

impl Error for CandidateSmokeEvidenceSinkPoisoned {}

#[derive(Debug)]
struct CandidateSmokeMeasurementIncomplete(&'static str);

impl fmt::Display for CandidateSmokeMeasurementIncomplete {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for CandidateSmokeMeasurementIncomplete {}

fn candidate_setup_error(message: impl Into<String>) -> PluginError {
    PluginError::SetupFailed {
        plugin: CANDIDATE_SMOKE_PLUGIN_ID,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::ecs::{
        World,
        bevy_ecs::system::{RunSystemError, RunSystemOnce},
    };

    #[test]
    fn candidate_smoke_is_an_ordinary_recipe_contribution() {
        let recipe = candidate_smoke_recipe(Arc::new(Mutex::new(None)));
        let plugin_ids = recipe
            .entries()
            .map(|entry| entry.plugin_id())
            .collect::<Vec<_>>();

        assert!(plugin_ids.contains(&REFERENCE_DESKTOP_PLUGIN_ID));
        assert!(plugin_ids.contains(&CANDIDATE_SMOKE_PLUGIN_ID));
    }

    #[test]
    fn expired_candidate_frame_fails_without_requesting_exit() {
        let mut world = World::new();
        world.insert_resource(WgpuRenderBackend::default());
        world.insert_resource(CandidateSmokeState {
            deadline: Instant::now() - Duration::from_millis(1),
            evidence: Arc::new(Mutex::new(None)),
            phase: CandidateSmokePhase::AwaitPresentedReady,
        });
        world.insert_resource(AppExitRequests::default());

        let result: Result<(), RunSystemError> = world.run_system_once(observe_candidate_frame);

        assert!(matches!(result, Err(RunSystemError::Failed(_))));
        assert_eq!(world.resource::<AppExitRequests>().requested(), None);
    }

    #[test]
    fn completion_wait_uses_only_the_remaining_candidate_budget() {
        let now = Instant::now();

        assert_eq!(
            remaining_candidate_time(now + Duration::from_millis(7), now).unwrap(),
            Duration::from_millis(7)
        );
        assert!(remaining_candidate_time(now, now).is_err());
        assert!(remaining_candidate_time(now, now + Duration::from_nanos(1)).is_err());
    }
}
