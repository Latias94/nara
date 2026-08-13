use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    app::{AppExitRequests, PluginCategory, PluginDeclaration, PluginError, PluginId},
    ecs::error::BevyError,
    fs::DirectoryCapability,
    image::PreparedImageResource,
    prelude::{App, CoreStage, Plugin, Resource, World},
    project_host::{DesktopRun, DesktopRunIntent},
    render::{PreparedRenderResources, RenderFrame},
    render_wgpu::{WgpuBackendTelemetryConfig, WgpuRenderBackend},
    sprite_render::SpriteBatches,
};
use nara_reference_game::{
    REFERENCE_DESKTOP_PLUGIN_ID, ReferenceUiObservation, desktop_wave_recipe,
};

use crate::desktop_support::{resident_product_glyphs, submitted_product_frame};

const CANDIDATE_SMOKE_DEADLINE: Duration = Duration::from_secs(30);
const CANDIDATE_SMOKE_WARMUP_SAMPLES: usize = 4;
const CANDIDATE_SMOKE_PLUGIN_ID: PluginId = PluginId::new("reference-game.desktop-candidate-smoke");
const CANDIDATE_SMOKE_REQUIREMENTS: &[PluginId] = &[REFERENCE_DESKTOP_PLUGIN_ID];
const CANDIDATE_SMOKE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CANDIDATE_SMOKE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(CANDIDATE_SMOKE_REQUIREMENTS);

pub(crate) type CandidateSmokeEvidenceSink = Arc<Mutex<Option<CandidateSmokeEvidence>>>;

#[derive(Clone, Debug)]
pub(crate) struct CandidateSmokeEvidence {
    ui: ReferenceUiObservation,
    backend: CandidateBackendObservation,
}

impl CandidateSmokeEvidence {
    pub(crate) const fn ui(&self) -> &ReferenceUiObservation {
        &self.ui
    }

    pub(crate) fn backend_json_line(&self) -> String {
        self.backend.to_json_line()
    }
}

#[derive(Clone, Debug)]
struct CandidateBackendObservation {
    frame_index: u64,
    device_epoch: u64,
    completion_sample_loss: u64,
    completion_samples: Vec<CandidateCompletionSample>,
    ui_packet_count: u64,
    ui_primitives: u64,
    ui_glyph_primitives: u64,
    ui_glyph_pages: u64,
    uploaded_glyph_pages: u32,
    reused_glyph_pages: u32,
    current_texture_bytes: u64,
    peak_texture_bytes: u64,
    peak_instance_buffer_bytes: u64,
    peak_total_bytes: u64,
}

#[derive(Clone, Copy, Debug)]
struct CandidateCompletionSample {
    frame_index: u64,
    duration_ns: u64,
}

impl CandidateBackendObservation {
    fn capture(world: &mut World) -> Result<Self, BevyError> {
        world
            .resource::<WgpuRenderBackend>()
            .wait_for_telemetry_completions()
            .map_err(BevyError::error)?;
        let mut backend = world.resource_mut::<WgpuRenderBackend>();
        let samples = backend.drain_frame_completion_samples();
        let completion = backend.frame_completion_stats();
        let resources = backend.gpu_resource_stats();
        let transaction = backend.frame_transaction_stats();
        let textures = backend.texture_stats();
        if samples.len() <= CANDIDATE_SMOKE_WARMUP_SAMPLES {
            return Err(BevyError::error(CandidateSmokeMeasurementIncomplete(
                "desktop candidate did not retain enough warmed completion samples",
            )));
        }
        if completion.lost_samples() != 0 {
            return Err(BevyError::error(CandidateSmokeMeasurementIncomplete(
                "desktop candidate lost GPU completion samples",
            )));
        }
        if !resources.tracking_enabled()
            || resources.pending_retirements() != 0
            || resources.lost_retirements() != 0
        {
            return Err(BevyError::error(CandidateSmokeMeasurementIncomplete(
                "desktop candidate GPU resource retirement was incomplete",
            )));
        }
        let frame_index = transaction
            .render_transaction_frame_index()
            .ok_or_else(|| {
            BevyError::error(CandidateSmokeMeasurementIncomplete(
                "desktop candidate transaction had no frame identity",
            ))
        })?;
        let device_epoch = backend.device_epoch();
        let completion_samples = samples
            .into_iter()
            .skip(CANDIDATE_SMOKE_WARMUP_SAMPLES)
            .map(|sample| {
                if sample.device_epoch() != device_epoch {
                    return Err(BevyError::error(CandidateSmokeMeasurementIncomplete(
                        "desktop candidate retained a stale completion sample",
                    )));
                }
                Ok(CandidateCompletionSample {
                    frame_index: sample.render_transaction_frame_index(),
                    duration_ns: sample.duration_ns(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !completion_samples
            .iter()
            .any(|sample| sample.frame_index == frame_index)
        {
            return Err(BevyError::error(CandidateSmokeMeasurementIncomplete(
                "desktop candidate did not complete the exact observed UI packet frame",
            )));
        }

        Ok(Self {
            frame_index,
            device_epoch,
            completion_sample_loss: completion.lost_samples(),
            completion_samples,
            ui_packet_count: transaction.ui_packet_count(),
            ui_primitives: transaction.ui_primitives(),
            ui_glyph_primitives: transaction.ui_glyph_primitives(),
            ui_glyph_pages: transaction.ui_glyph_pages(),
            uploaded_glyph_pages: textures.uploaded_glyph_pages,
            reused_glyph_pages: textures.reused_glyph_pages,
            current_texture_bytes: resources.current_texture_bytes(),
            peak_texture_bytes: resources.peak_texture_bytes(),
            peak_instance_buffer_bytes: resources.peak_instance_buffer_bytes(),
            peak_total_bytes: resources.peak_total_bytes(),
        })
    }

    fn to_json_line(&self) -> String {
        let samples = self
            .completion_samples
            .iter()
            .map(|sample| {
                format!(
                    "{{\"frame_index\":{},\"duration_ns\":{}}}",
                    sample.frame_index, sample.duration_ns
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            concat!(
                "{{\"schema\":\"nara-reference-game.ui-performance-v1\",",
                "\"frame_index\":{},\"device_epoch\":{},",
                "\"warmup_samples\":{},\"completion_sample_loss\":{},",
                "\"completion_samples\":[{}],\"ui_packet_count\":{},",
                "\"ui_primitives\":{},\"ui_glyph_primitives\":{},",
                "\"ui_glyph_pages\":{},\"uploaded_glyph_pages\":{},",
                "\"reused_glyph_pages\":{},\"current_texture_bytes\":{},",
                "\"peak_texture_bytes\":{},\"peak_instance_buffer_bytes\":{},",
                "\"peak_total_bytes\":{}}}\n"
            ),
            self.frame_index,
            self.device_epoch,
            CANDIDATE_SMOKE_WARMUP_SAMPLES,
            self.completion_sample_loss,
            samples,
            self.ui_packet_count,
            self.ui_primitives,
            self.ui_glyph_primitives,
            self.ui_glyph_pages,
            self.uploaded_glyph_pages,
            self.reused_glyph_pages,
            self.current_texture_bytes,
            self.peak_texture_bytes,
            self.peak_instance_buffer_bytes,
            self.peak_total_bytes,
        )
    }
}

fn submitted_observed_product_ui_frame(
    frame: &RenderFrame,
    backend: &WgpuRenderBackend,
    observation: &ReferenceUiObservation,
) -> bool {
    let transaction = backend.frame_transaction_stats();
    let Some(identity) = transaction.ui_packet_identity() else {
        return false;
    };
    let Some(presentation) = observation.presentation() else {
        return false;
    };
    submitted_product_frame(frame, backend)
        && resident_product_glyphs(backend)
        && identity.surface_id().get() == observation.surface_id
        && identity.frame_revision().get() == presentation.frame_revision
        && identity.source_revision().get() == presentation.source_revision
        && identity.slot_generation().get() == presentation.slot_generation
        && identity.route_revision().get() == presentation.route_revision
        && identity.geometry_generation().get() == presentation.geometry_generation
        && identity.presentation_revision().get() == presentation.presentation_revision
        && identity.theme_revision().get() == presentation.theme_revision
        && identity.font_revision().get() == presentation.font_revision
}

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
        })?
        .add_systems(CoreStage::Cleanup, observe_candidate_frame)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct CandidateSmokeState {
    deadline: Instant,
    evidence: CandidateSmokeEvidenceSink,
}

fn observe_candidate_frame(world: &mut World) -> Result<(), BevyError> {
    let observation = ReferenceUiObservation::from_world(world);
    let product_frame_ready = !world
        .resource::<PreparedRenderResources<PreparedImageResource>>()
        .is_empty()
        && world.get_resource::<SpriteBatches>().is_some_and(|batches| {
            batches
                .as_slice()
                .iter()
                .any(|batch| batch.image.is_some())
        })
        && observation.as_ref().is_some_and(|observation| {
            submitted_observed_product_ui_frame(
                world.resource::<RenderFrame>(),
                world.resource::<WgpuRenderBackend>(),
                observation,
            )
        });
    let state = world.resource::<CandidateSmokeState>();
    let deadline = state.deadline;
    let sink = Arc::clone(&state.evidence);
    if product_frame_ready && let Some(observation) = observation {
        let backend = CandidateBackendObservation::capture(world)?;
        *sink
            .lock()
            .map_err(|_| BevyError::error(CandidateSmokeEvidenceSinkPoisoned))? =
            Some(CandidateSmokeEvidence {
                ui: observation,
                backend,
            });
        world.resource_mut::<AppExitRequests>().request_success();
    } else if Instant::now() >= deadline {
        return Err(BevyError::error(CandidateSmokeDeadlineExceeded));
    }
    Ok(())
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
        system::{RunSystemError, RunSystemOnce},
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
        world.insert_resource(PreparedRenderResources::<PreparedImageResource>::default());
        world.insert_resource(RenderFrame::default());
        world.insert_resource(WgpuRenderBackend::default());
        world.insert_resource(CandidateSmokeState {
            deadline: Instant::now() - Duration::from_millis(1),
            evidence: Arc::new(Mutex::new(None)),
        });
        world.insert_resource(AppExitRequests::default());

        let result: Result<(), RunSystemError> = world.run_system_once(observe_candidate_frame);

        assert!(matches!(result, Err(RunSystemError::Failed(_))));
        assert_eq!(world.resource::<AppExitRequests>().requested(), None);
    }
}
