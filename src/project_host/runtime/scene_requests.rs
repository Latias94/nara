//! One hosted, worker-prepared Scene request with an explicit owner-thread commit.

mod api;
#[cfg(all(feature = "project-source", feature = "asset-watch"))]
pub(super) mod input_reload;
mod preparation;
#[cfg(feature = "project-source")]
mod source_updates;

#[cfg(feature = "project-source")]
pub use api::ProjectSceneSourceUpdateStatus;
pub use api::{
    PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestState,
    ProjectSceneSelection, ProjectScenes, activate_prepared_project_scene,
    commit_prepared_project_scene,
};
#[cfg(all(test, feature = "project-source", any(windows, target_os = "linux")))]
mod tests;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nara_app::tasks::TaskPools;
use nara_app::{
    App, PluginError, PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    RuntimeGeneration,
};
use nara_asset::AssetProductRef;
#[cfg(feature = "project-source")]
use nara_asset::AssetSourcePath;
use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticReport, ReportDiagnostic as Diagnostic, SafeSummary,
};
use nara_ecs::{Res, ResMut, Resource, World, bevy_ecs::system::SystemState, system::SystemParam};
#[cfg(feature = "project-source")]
use nara_project::ProjectPath;
pub use nara_scene::current_scene::requests::SceneRequestId as ProjectSceneRequestId;
use nara_scene::current_scene::requests::{SceneRequestError, SceneRequestSource, SceneRequests};
use nara_scene::{
    CompiledSceneDocument, SceneSpawnReport,
    advanced::{SceneProductOverlayWriter, SceneProductRetirement, SceneProductTransactionLimits},
};

use super::{content::PreparedSceneContent, content_residency::ProjectSceneResidency};
#[cfg(feature = "project-source")]
use crate::project_host::{
    ProjectContentErrorKind, ProjectContentLoader, ProjectSettingsCandidate, RuntimePlan,
};
use crate::{runtime_content::RuntimeContentBase, startup_scene::CurrentScene};
#[cfg(test)]
use preparation::Preparation;
use preparation::{PreparationFailure, start_preparation};

enum Context {
    #[cfg(feature = "project-source")]
    Source {
        candidate: ProjectSettingsCandidate,
        plan: RuntimePlan,
        loader: Arc<ProjectContentLoader>,
        input: Option<crate::project_content::ProjectInputContent>,
    },
    Processed {
        base: RuntimeContentBase,
    },
}

impl Context {
    #[cfg(feature = "project-source")]
    fn declares_source_path(&self, path: &ProjectPath) -> bool {
        let Self::Source { candidate, .. } = self else {
            return false;
        };
        let settings = candidate.settings();
        settings.startup.default_scene.as_ref() == Some(path)
            || settings.content.scenes.contains(path)
    }

    fn admits_request(&self, selection: &ProjectSceneSelection) -> bool {
        match (self, selection) {
            #[cfg(feature = "project-source")]
            (Self::Source { .. }, ProjectSceneSelection::Source(path)) => {
                self.declares_source_path(path)
            }
            // Product identity and declared-root membership need actual source discovery. The
            // worker admits that association or returns a rejection without owner-thread I/O.
            #[cfg(feature = "project-source")]
            (Self::Source { .. }, ProjectSceneSelection::Product(_)) => true,
            (Self::Processed { base }, ProjectSceneSelection::Product(product)) => {
                base.catalog().scenes().binary_search(product).is_ok()
            }
            #[cfg(feature = "project-source")]
            (Self::Processed { .. }, ProjectSceneSelection::Source(_)) => false,
        }
    }
}

struct Target {
    selection: ProjectSceneSelection,
    purpose: RequestPurpose,
}

#[derive(Clone)]
enum RequestPurpose {
    Travel,
    #[cfg(feature = "project-source")]
    AdditiveSource(AssetSourcePath),
    AdditiveProcessed,
}

impl RequestPurpose {
    fn is_additive(&self) -> bool {
        !matches!(self, Self::Travel)
    }
}

impl SceneRequestSource for Context {
    type Target = Target;
    type Content = PreparedSceneContent;
    type Error = ProjectSceneRequestError;
    type Failure = PreparationFailure;

    fn admits(&self, target: &Target) -> bool {
        target.purpose.is_additive() || self.admits_request(&target.selection)
    }

    fn matches_current(&self, current: &CurrentScene) -> bool {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source { .. } => current.source_content_stamp().is_some(),
            Self::Processed { base } => current
                .retained_source()
                .uses_hosted_catalog(base.catalog()),
        }
    }

    fn matches_prepared(&self, target: &Target, content: &PreparedSceneContent) -> bool {
        api::content_matches_admission(content, self, target)
    }
}

#[derive(Resource)]
struct RequestOwner {
    requests: SceneRequests<Context>,
    #[cfg(feature = "project-source")]
    source_updates: Option<source_updates::SourceUpdates>,
    #[cfg(all(feature = "project-source", feature = "asset-watch"))]
    input_reload: input_reload::InputReload,
    #[cfg(test)]
    gate: Option<preparation::TestGate>,
}

/// Installs only the original Host authority. Call during unpublished runtime admission.
#[cfg(feature = "project-source")]
pub(super) fn install_context(
    world: &mut World,
    candidate: ProjectSettingsCandidate,
    plan: RuntimePlan,
    loader: Arc<ProjectContentLoader>,
    input: Option<crate::project_content::ProjectInputContent>,
    source_closure: Option<crate::project_content::ProjectContentClosureReceipt>,
) -> Result<(), ProjectSceneRequestError> {
    if candidate.lineage() != plan.lineage() {
        return Err(ProjectSceneRequestError::Stale);
    }
    install_request_owner(
        world,
        Context::Source {
            candidate,
            plan,
            loader,
            input,
        },
        source_closure,
    )
}

/// Captures the original admitted processed base, without reconstructing its executable plan.
pub(super) fn install_runtime_context(
    world: &mut World,
    base: RuntimeContentBase,
) -> Result<(), ProjectSceneRequestError> {
    install_request_owner(
        world,
        Context::Processed { base },
        #[cfg(feature = "project-source")]
        None,
    )
}

fn install_request_owner(
    world: &mut World,
    context: Context,
    #[cfg(feature = "project-source")] source_closure: Option<
        crate::project_content::ProjectContentClosureReceipt,
    >,
) -> Result<(), ProjectSceneRequestError> {
    if world.contains_resource::<RequestOwner>() {
        return Err(ProjectSceneRequestError::Stale);
    }
    let closing = world
        .get_resource::<Closing>()
        .ok_or(ProjectSceneRequestError::Unavailable)?
        .0
        .clone();
    if closing.load(Ordering::Acquire) {
        return Err(ProjectSceneRequestError::Unavailable);
    }
    #[cfg(feature = "project-source")]
    let source_updates = if let Some(source_closure) = source_closure {
        let generation = world
            .get_resource::<RuntimeGeneration>()
            .copied()
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        let instance = crate::startup_scene::materialized_startup_scene_receipt(world)
            .ok_or(ProjectSceneRequestError::Unavailable)?;
        Some(source_updates::SourceUpdates::new(
            source_closure,
            generation,
            instance,
        ))
    } else {
        None
    };
    world.insert_resource(RequestOwner {
        requests: SceneRequests::new(context, closing),
        #[cfg(feature = "project-source")]
        source_updates,
        #[cfg(all(feature = "project-source", feature = "asset-watch"))]
        input_reload: input_reload::InputReload::default(),
        #[cfg(test)]
        gate: None,
    });
    Ok(())
}

#[cfg(all(test, feature = "project-source"))]
pub(super) fn hold_next_preparation(
    world: &mut World,
) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let (gate, control) = preparation::TestGate::new(preparation::TestPhase::AfterPreparation);
    let mut owner = world.resource_mut::<RequestOwner>();
    assert!(owner.gate.is_none(), "one preparation gate may be armed");
    owner.gate = Some(gate);
    control.into_channels()
}

/// The exclusive TaskUpdate poll is inert in runtimes without a project request owner.
pub(crate) fn poll_project_scene_requests(world: &mut World) {
    #[cfg(feature = "project-source")]
    let input_reload_pending = {
        #[cfg(feature = "asset-watch")]
        let pending = input_reload::poll(world);
        #[cfg(not(feature = "asset-watch"))]
        let pending = false;
        pending
    };
    #[cfg(feature = "project-source")]
    if !input_reload_pending
        && world.get_resource::<RequestOwner>().is_some_and(|owner| {
            !owner.requests.is_occupied()
                && !owner.requests.is_closing()
                && owner
                    .source_updates
                    .as_ref()
                    .is_some_and(|updates| updates.status().pending)
        })
        && !super::content_residency::has_active_project_scene_addition(world)
    {
        ProjectScenes::scope(world, |scenes| scenes.start_source_update());
    }
    if !world
        .get_resource::<RequestOwner>()
        .is_some_and(|owner| owner.requests.active_request().is_some())
    {
        return;
    }
    // Borrow the owner in place. Polling must never invoke resource lifecycle effects.
    let mut access = SystemState::<(
        Option<ResMut<RequestOwner>>,
        Option<Res<CurrentScene>>,
        Option<Res<RuntimeGeneration>>,
    )>::new(world);
    let (Some(owner), current, generation) = access
        .get_mut(world)
        .expect("Scene request resources do not conflict")
    else {
        return;
    };
    let owner = owner.into_inner();
    owner.requests.poll(
        generation.as_deref().copied(),
        current.as_deref(),
        PreparationFailure::unavailable,
        |_id, failure| {
            #[cfg(feature = "project-source")]
            if let Some(updates) = &mut owner.source_updates {
                updates.failed(_id, failure.error, failure.observations);
            }
            (failure.error, failure.diagnostics)
        },
    );
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        DiagnosticCode::new(code).expect("engine-owned diagnostic code is valid"),
        SafeSummary::new(summary).expect("static summary"),
    ));
    report
}

fn error_diagnostics(error: ProjectSceneRequestError) -> DiagnosticReport {
    rejected(error.code(), "Project Scene request was rejected").diagnostics
}

#[derive(Resource)]
struct Closing(Arc<AtomicBool>);

pub(crate) const PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.project.scene-requests");

pub(crate) fn register_close_participant(app: &mut App) -> Result<(), PluginError> {
    let closing = Arc::new(AtomicBool::new(false));
    app.register_plugin_runtime_close_participant(
        PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION,
        RuntimeCloseParticipantId::new("nara.project.scene-requests"),
        CloseParticipant(Arc::clone(&closing)),
    )?;
    app.insert_resource(Closing(closing))?;
    Ok(())
}

struct CloseParticipant(Arc<AtomicBool>);
impl RuntimeCloseParticipant for CloseParticipant {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.store(true, Ordering::Release);
        if let Some(owner) = context.get_resource_mut::<RequestOwner>().map_err(|_| {
            RuntimeCloseParticipantError::new("project.scene.close-owner-unavailable")
        })? {
            #[cfg(feature = "project-source")]
            let owner = {
                let owner = owner.into_inner();
                owner.source_updates = None;
                #[cfg(feature = "asset-watch")]
                owner.input_reload.cancel();
                owner
            };
            owner.requests.close();
        }
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Complete)
    }
}
