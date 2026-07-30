use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{
        AppExitRequests, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
        PluginError, PluginId,
    },
    fs::DirectoryCapability,
    image::PreparedImageResource,
    prelude::{App, CoreStage, Plugin, Res, ResMut, Resource},
    project_host::DesktopRun,
    render::{PreparedRenderResources, RenderFrame},
    render_wgpu::WgpuRenderBackend,
    sprite_render::SpriteBatches,
};
use nara_reference_game::{
    REFERENCE_DESKTOP_PLUGIN_ID, ReferenceDesktopPlugin, wave_desktop_intent,
};

use crate::desktop_support::submitted_product_frame;

const CANDIDATE_SMOKE_DEADLINE: Duration = Duration::from_secs(30);
const CANDIDATE_SMOKE_PLUGIN_ID: PluginId = PluginId::new("reference-game.desktop-candidate-smoke");
const CANDIDATE_SMOKE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("reference-game.desktop-candidate-smoke", 1);
const CANDIDATE_SMOKE_REQUIREMENTS: &[PluginId] = &[REFERENCE_DESKTOP_PLUGIN_ID];
const CANDIDATE_SMOKE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CANDIDATE_SMOKE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(CANDIDATE_SMOKE_REQUIREMENTS);

#[derive(Debug, Default)]
pub(crate) struct CandidateSmokeEvidence {
    completed: Arc<AtomicBool>,
}

impl CandidateSmokeEvidence {
    pub(crate) fn completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }
}

pub(crate) fn candidate_smoke_run(
    project_root: DirectoryCapability,
) -> (DesktopRun, Option<CandidateSmokeEvidence>) {
    let evidence = CandidateSmokeEvidence::default();
    let plugin_evidence = Arc::clone(&evidence.completed);
    let probe = PluginDefinition::infallible::<CandidateSmokePlugin, _>(
        CANDIDATE_SMOKE_DEFINITION_ID,
        b"reference-game-desktop-candidate-smoke-v1",
        move || CandidateSmokePlugin {
            completed: Arc::clone(&plugin_evidence),
        },
    );
    let intent = wave_desktop_intent().insert_after::<ReferenceDesktopPlugin>(probe);
    (DesktopRun::new(project_root, intent), Some(evidence))
}

#[derive(Debug)]
struct CandidateSmokePlugin {
    completed: Arc<AtomicBool>,
}

impl Plugin for CandidateSmokePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CANDIDATE_SMOKE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(CandidateSmokeState {
            completed: Arc::clone(&self.completed),
            deadline: Instant::now() + CANDIDATE_SMOKE_DEADLINE,
        })?
        .add_systems(CoreStage::Cleanup, observe_candidate_frame)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct CandidateSmokeState {
    completed: Arc<AtomicBool>,
    deadline: Instant,
}

fn observe_candidate_frame(
    prepared: Res<PreparedRenderResources<PreparedImageResource>>,
    batches: Res<SpriteBatches>,
    frame: Res<RenderFrame>,
    backend: Res<WgpuRenderBackend>,
    state: Res<CandidateSmokeState>,
    mut exit: ResMut<AppExitRequests>,
) {
    let product_frame_ready = !prepared.is_empty()
        && batches
            .as_slice()
            .iter()
            .any(|batch| batch.material.image.is_some())
        && submitted_product_frame(&frame, &backend);
    if product_frame_ready {
        state.completed.store(true, Ordering::SeqCst);
        exit.request_exit();
    } else if Instant::now() >= state.deadline {
        exit.request_exit();
    }
}
