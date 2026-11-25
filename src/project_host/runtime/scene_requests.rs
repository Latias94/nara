//! One hosted, worker-prepared Scene request with an explicit owner-thread commit.

mod api;
pub(crate) mod domains;
mod preparation;
#[cfg(feature = "project-source")]
mod source_updates;

#[cfg(feature = "project-source")]
pub use api::ProjectSceneSourceUpdateStatus;
pub use api::{
    PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestId,
    ProjectSceneRequestState, ProjectSceneSelection, ProjectScenes,
    activate_prepared_project_scene, begin_prepared_project_scene_travel,
    commit_prepared_project_scene,
};
#[cfg(all(test, feature = "project-source", any(windows, target_os = "linux")))]
mod tests;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
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
use nara_ecs::identity::SpawnedSceneInstance;
use nara_ecs::{Res, ResMut, Resource, World, system::SystemParam};
#[cfg(feature = "project-source")]
use nara_project::ProjectPath;
use nara_scene::{
    CompiledSceneDocument, SceneSpawnReport,
    advanced::{SceneProductOverlayWriter, SceneProductRetirement, SceneProductTransactionLimits},
};

use super::{content::PreparedSceneContent, content_residency::ProjectSceneResidency};
#[cfg(feature = "project-source")]
use crate::project_host::{
    ProjectContentErrorKind, ProjectContentLoader, ProjectSettingsCandidate, RuntimePlan,
};
use crate::{
    runtime_content::{RuntimeContentBase, RuntimeContentMemberKind},
    startup_scene::CurrentSceneOwner,
};
use domains::ManagedSceneTravelAttempt;
use preparation::Preparation;

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

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
    fn matches_current(&self, current: &CurrentSceneOwner) -> bool {
        match self {
            #[cfg(feature = "project-source")]
            Self::Source { .. } => current.is_project_backed(),
            Self::Processed { base } => current
                .hosted_runtime_base()
                .is_some_and(|current| base.same_base(current)),
        }
    }

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
                base.catalog().scenes().contains(product)
                    && base.member(*product).is_some_and(|member| {
                        matches!(member.kind(), RuntimeContentMemberKind::Scene { .. })
                    })
            }
            #[cfg(feature = "project-source")]
            (Self::Processed { .. }, ProjectSceneSelection::Source(_)) => false,
        }
    }
}

struct Admission {
    context: Arc<Context>,
    id: ProjectSceneRequestId,
    expected: SpawnedSceneInstance,
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

#[derive(Resource)]
struct RequestOwner {
    context: Arc<Context>,
    closing: Arc<AtomicBool>,
    slot: Option<Slot>,
    #[cfg(feature = "project-source")]
    source_updates: Option<source_updates::SourceUpdates>,
    #[cfg(test)]
    gate: Option<preparation::TestGate>,
}

enum Slot {
    Preparing {
        admission: Admission,
        preparation: Preparation,
        discard: Option<ProjectSceneRequestState>,
    },
    Ready {
        admission: Admission,
        content: PreparedSceneContent,
    },
    /// Queued or active managed travel. Once native retirement starts, this slot retains the exact
    /// candidate and attempt even if ordinary pre-commit admission facts change.
    Managed(ManagedTravelState),
    Terminal {
        id: ProjectSceneRequestId,
        state: ProjectSceneRequestState,
        diagnostics: DiagnosticReport,
    },
}

struct ManagedTravelState {
    admission: Admission,
    attempt: ManagedSceneTravelAttempt,
    prepared: Option<super::content_residency::StagedProjectSceneReplacement>,
    published: Option<SpawnedSceneInstance>,
    retry_requested: bool,
    native_started: bool,
    cancel_requested: bool,
    cleanup_result: Option<ManagedTravelCleanup>,
    queued_frame: u64,
    state: ProjectSceneRequestState,
    publication_diagnostics: DiagnosticReport,
    diagnostics: DiagnosticReport,
    #[cfg(feature = "project-source")]
    source_closure: Option<crate::project_content::ProjectContentClosureReceipt>,
}

struct ManagedTravelCleanup {
    state: ProjectSceneRequestState,
    diagnostics: DiagnosticReport,
}

impl ManagedTravelCleanup {
    fn cancelled() -> Self {
        Self {
            state: ProjectSceneRequestState::Cancelled,
            diagnostics: DiagnosticReport::default(),
        }
    }
}

impl Slot {
    fn id(&self) -> ProjectSceneRequestId {
        match self {
            Self::Preparing { admission, .. } | Self::Ready { admission, .. } => admission.id,
            Self::Managed(travel) => travel.admission.id,
            Self::Terminal { id, .. } => *id,
        }
    }

    fn admission(&self) -> Option<&Admission> {
        match self {
            Self::Preparing { admission, .. } | Self::Ready { admission, .. } => Some(admission),
            Self::Managed(travel) => Some(&travel.admission),
            Self::Terminal { .. } => None,
        }
    }

    fn state(&self) -> ProjectSceneRequestState {
        match self {
            Self::Preparing {
                discard: Some(_), ..
            } => ProjectSceneRequestState::Cancelling,
            Self::Preparing { .. } => ProjectSceneRequestState::Preparing,
            Self::Ready { .. } => ProjectSceneRequestState::Ready,
            Self::Managed(travel) if travel.cancel_requested => {
                ProjectSceneRequestState::Cancelling
            }
            Self::Managed(travel) => travel.state,
            Self::Terminal { state, .. } => *state,
        }
    }

    fn terminal(id: ProjectSceneRequestId, state: ProjectSceneRequestState) -> Self {
        let diagnostics = match state {
            ProjectSceneRequestState::Rejected(error) => error_diagnostics(error),
            _ => DiagnosticReport::default(),
        };
        Self::Terminal {
            id,
            state,
            diagnostics,
        }
    }
}

impl RequestOwner {
    fn request(
        &mut self,
        selection: ProjectSceneSelection,
        purpose: RequestPurpose,
        generation: RuntimeGeneration,
        expected: &SpawnedSceneInstance,
        pools: &TaskPools,
    ) -> Result<ProjectSceneRequestId, ProjectSceneRequestError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(ProjectSceneRequestError::Unavailable);
        }
        if self.slot.is_some() {
            return Err(ProjectSceneRequestError::Busy);
        }
        if !purpose.is_additive() && !self.context.admits_request(&selection) {
            return Err(ProjectSceneRequestError::UndeclaredScene);
        }
        let sequence = NEXT_REQUEST
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| ProjectSceneRequestError::IdExhausted)?;
        let id = ProjectSceneRequestId {
            generation,
            sequence,
        };
        let preparation = Preparation::start(
            Arc::clone(&self.context),
            selection.clone(),
            purpose.clone(),
            pools,
            #[cfg(test)]
            self.gate.take(),
        )?;
        self.slot = Some(Slot::Preparing {
            admission: Admission {
                context: Arc::clone(&self.context),
                id,
                expected: expected.clone(),
                selection,
                purpose,
            },
            preparation,
            discard: None,
        });
        Ok(id)
    }

    fn matches(
        &self,
        admission: &Admission,
        generation: Option<RuntimeGeneration>,
        current: Option<&CurrentSceneOwner>,
    ) -> bool {
        !self.closing.load(Ordering::Acquire)
            && generation == Some(admission.id.generation)
            && Arc::ptr_eq(&self.context, &admission.context)
            && current.is_some_and(|current| {
                current.receipt() == &admission.expected && self.context.matches_current(current)
            })
    }

    fn cancel(&mut self, request: ProjectSceneRequestId) -> bool {
        match self.slot.as_mut().filter(|slot| slot.id() == request) {
            Some(Slot::Preparing {
                preparation,
                discard,
                ..
            }) => {
                if discard.is_some() {
                    return false;
                }
                *discard = Some(ProjectSceneRequestState::Cancelled);
                preparation.cancel();
                true
            }
            Some(Slot::Ready { .. }) => {
                self.slot = Some(Slot::terminal(request, ProjectSceneRequestState::Cancelled));
                true
            }
            Some(Slot::Managed(travel))
                if travel.admission.id == request
                    && !travel.native_started
                    && travel.published.is_none()
                    && !travel.cancel_requested =>
            {
                travel.cancel_requested = true;
                travel
                    .cleanup_result
                    .get_or_insert_with(ManagedTravelCleanup::cancelled);
                true
            }
            _ => false,
        }
    }

    fn reject_stale(&mut self, request: ProjectSceneRequestId) {
        match self.slot.as_mut().filter(|slot| slot.id() == request) {
            Some(Slot::Preparing {
                preparation,
                discard,
                ..
            }) => {
                if discard.is_none() {
                    *discard = Some(ProjectSceneRequestState::Rejected(
                        ProjectSceneRequestError::Stale,
                    ));
                    preparation.cancel();
                }
            }
            Some(Slot::Ready { .. }) => {
                self.slot = Some(Slot::terminal(
                    request,
                    ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
                ));
            }
            _ => {}
        }
    }
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
        context: Arc::new(context),
        closing,
        slot: None,
        #[cfg(feature = "project-source")]
        source_updates,
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
    if world.get_resource::<RequestOwner>().is_some_and(|owner| {
        owner.slot.is_none()
            && !owner.closing.load(Ordering::Acquire)
            && owner
                .source_updates
                .as_ref()
                .is_some_and(|updates| updates.status().pending)
    }) && !super::content_residency::has_active_project_scene_addition(world)
    {
        ProjectScenes::scope(world, |scenes| scenes.start_source_update());
    }
    if world
        .get_resource::<RequestOwner>()
        .is_some_and(|owner| matches!(owner.slot, Some(Slot::Managed(_))))
    {
        drive_managed_travel(world);
        return;
    }
    let valid = {
        let Some(owner) = world.get_resource::<RequestOwner>() else {
            return;
        };
        owner
            .slot
            .as_ref()
            .and_then(Slot::admission)
            .is_none_or(|admission| {
                owner.matches(
                    admission,
                    world.get_resource::<RuntimeGeneration>().copied(),
                    world.get_resource::<CurrentSceneOwner>(),
                )
            })
    };
    // Poll changes only this owner's fields. Keep the resource in place, so observation never
    // invokes remove/reinsert lifecycle effects or needs another resource's mutable borrow.
    let mut owner = world.resource_mut::<RequestOwner>();
    let Some(mut slot) = owner.slot.take() else {
        return;
    };
    if let Slot::Preparing {
        admission,
        preparation,
        discard,
    } = &mut slot
    {
        if !valid && discard.is_none() {
            *discard = Some(ProjectSceneRequestState::Rejected(
                ProjectSceneRequestError::Stale,
            ));
            preparation.cancel();
        }
        if let Some(result) = preparation.poll() {
            let state = discard.take();
            let id = admission.id;
            let content_valid = result
                .as_ref()
                .is_ok_and(|content| api::content_matches_admission(content, admission));
            slot = match (state, result) {
                (Some(state), _) => Slot::terminal(id, state),
                (None, Ok(_)) if !content_valid => Slot::terminal(
                    id,
                    ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
                ),
                (None, Ok(content)) => {
                    let Slot::Preparing { admission, .. } = slot else {
                        unreachable!()
                    };
                    Slot::Ready { admission, content }
                }
                (None, Err(failure)) => {
                    #[cfg(feature = "project-source")]
                    if let Some(updates) = &mut owner.source_updates {
                        updates.failed(id, failure.error, failure.observations);
                    }
                    Slot::Terminal {
                        id,
                        state: ProjectSceneRequestState::Rejected(failure.error),
                        diagnostics: failure.diagnostics,
                    }
                }
            };
        }
    } else if !valid && !matches!(slot, Slot::Managed(_)) {
        slot = Slot::terminal(
            slot.id(),
            ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
        );
    }
    owner.slot = Some(slot);
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    rejected_with_code(
        DiagnosticCode::new(code).expect("engine-owned diagnostic code is valid"),
        summary,
    )
}

fn rejected_with_code(code: DiagnosticCode, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        code,
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
    app.insert_resource(Closing(closing))?
        .init_resource::<domains::ManagedSceneDomains>()?
        .add_simulation_condition(managed_scene_simulation_ready)?;
    Ok(())
}

fn managed_scene_simulation_ready(world: &World) -> bool {
    world
        .get_resource::<RequestOwner>()
        .is_none_or(|owner| !matches!(owner.slot, Some(Slot::Managed(_))))
}

fn drive_managed_travel(world: &mut World) {
    if crate::ecs::transaction::validate_resource_scope::<RequestOwner>(world).is_err() {
        reject_unscoped_managed_travel(world);
        return;
    }
    world.resource_scope(|world, mut owner: nara_ecs::Mut<RequestOwner>| {
        if owner.closing.load(Ordering::Acquire) {
            return;
        }
        let Some(Slot::Managed(mut travel)) = owner.slot.take() else {
            return;
        };
        let cleanup_result = if travel.cancel_requested
            && !travel.native_started
            && travel.published.is_none()
        {
            Some(
                travel
                    .cleanup_result
                    .take()
                    .unwrap_or_else(ManagedTravelCleanup::cancelled),
            )
        } else if travel.retry_requested
            && travel.cleanup_result.is_some()
            && !travel.native_started
            && travel.published.is_none()
        {
            travel.cleanup_result.take()
        } else {
            None
        };
        if let Some(cleanup_result) = cleanup_result {
            let candidate = travel.prepared.take().expect(if travel.cancel_requested {
                "unpublished managed travel retains its candidate"
            } else {
                "cleanup retry retains its unpublished candidate"
            });
            match candidate.discard(world) {
                Ok(()) => {
                    owner.slot = Some(Slot::Terminal {
                        id: travel.admission.id,
                        state: cleanup_result.state,
                        diagnostics: cleanup_result.diagnostics,
                    });
                }
                Err(candidate) => {
                    travel.diagnostics = cleanup_result.diagnostics.clone();
                    travel.diagnostics.extend(rejected(
                        "project.scene.candidate-cleanup-incomplete",
                        "Unpublished Scene candidate remains owned because lifecycle cleanup is unsafe",
                    ).diagnostics);
                    travel.prepared = Some(candidate);
                    travel.retry_requested = false;
                    travel.cancel_requested = false;
                    travel.cleanup_result = Some(cleanup_result);
                    travel.state = ProjectSceneRequestState::Degraded;
                    owner.slot = Some(Slot::Managed(travel));
                }
            }
            return;
        }
        if !travel.retry_requested
            || world
                .get_resource::<nara_app::RealTime>()
                .is_none_or(|time| time.frame <= travel.queued_frame)
        {
            owner.slot = Some(Slot::Managed(travel));
            return;
        }
        travel.retry_requested = false;
        // A retry replaces its native failure evidence while retaining accepted publication facts.
        travel.diagnostics = travel.publication_diagnostics.clone();
        let result = (|| {
            if world.get_resource::<RuntimeGeneration>().copied()
                != Some(travel.admission.id.generation)
                || !Arc::ptr_eq(&owner.context, &travel.admission.context)
            {
                return Err(rejected(
                    "project.scene.request-stale",
                    "Managed Scene travel lost its runtime authority",
                ));
            }
            if travel.published.is_none() {
                if !travel.native_started {
                    let candidate = travel
                        .prepared
                        .as_ref()
                        .expect("unpublished travel retains its candidate");
                    candidate.begin_native_retirement(world)?;
                    travel.native_started = true;
                }
                travel
                    .attempt
                    .retire(world, &travel.admission.expected)
                    .map_err(|error| {
                        rejected_with_code(
                            error.code(),
                            "Native Scene retirement failed; the attempt remains retained",
                        )
                    })?;
                let candidate = travel
                    .prepared
                    .take()
                    .expect("unpublished travel retains its candidate");
                let report = match candidate.commit(world) {
                    Ok(report) => report,
                    Err((candidate, report)) => {
                        travel.prepared = Some(candidate);
                        return Err(report);
                    }
                };
                travel.published = report.instance;
                #[cfg(feature = "project-source")]
                if let (Some(updates), Some(source_closure), Some(instance)) =
                    (&mut owner.source_updates, travel.source_closure.take(), travel.published.as_ref())
                {
                    updates.publish(source_closure, instance);
                }
                travel.publication_diagnostics = report.diagnostics;
                travel.diagnostics = travel.publication_diagnostics.clone();
            }
            travel
                .attempt
                .reconcile(
                    world,
                    travel
                        .published
                        .as_ref()
                        .expect("published travel retains its receipt"),
                )
                .map_err(|error| {
                    rejected_with_code(
                        error.code(),
                        "Native Scene reconciliation failed; the attempt remains retained",
                    )
                })?;
            Ok(())
        })();
        match result {
            Ok(()) => owner.slot = Some(Slot::Terminal {
                id: travel.admission.id,
                state: ProjectSceneRequestState::Committed,
                diagnostics: travel.diagnostics,
            }),
            Err(report) if !travel.native_started => {
                let candidate = travel
                    .prepared
                    .take()
                    .expect("unstarted travel retains its candidate");
                let cleanup_result = ManagedTravelCleanup {
                    state: ProjectSceneRequestState::Rejected(
                        ProjectSceneRequestError::CommitRejected,
                    ),
                    diagnostics: report.diagnostics,
                };
                match candidate.discard(world) {
                    Ok(()) => owner.slot = Some(Slot::Terminal {
                        id: travel.admission.id,
                        state: cleanup_result.state,
                        diagnostics: cleanup_result.diagnostics,
                    }),
                    Err(candidate) => {
                        travel.diagnostics = cleanup_result.diagnostics.clone();
                        travel.diagnostics.extend(rejected(
                            "project.scene.candidate-cleanup-incomplete",
                            "Unpublished Scene candidate remains owned because lifecycle cleanup is unsafe",
                        ).diagnostics);
                        travel.prepared = Some(candidate);
                        travel.retry_requested = false;
                        travel.cancel_requested = false;
                        travel.cleanup_result = Some(cleanup_result);
                        travel.state = ProjectSceneRequestState::Degraded;
                        owner.slot = Some(Slot::Managed(travel));
                    }
                }
            }
            Err(report) => {
                travel.diagnostics.extend(report.diagnostics);
                travel.state = ProjectSceneRequestState::Degraded;
                owner.slot = Some(Slot::Managed(travel));
            }
        }
    });
}

fn reject_unscoped_managed_travel(world: &mut World) {
    let Some(mut owner) = world.get_resource_mut::<RequestOwner>() else {
        return;
    };
    if owner.closing.load(Ordering::Acquire) {
        return;
    }
    let Some(Slot::Managed(mut travel)) = owner.slot.take() else {
        return;
    };
    let canceling_unpublished_candidate =
        travel.cancel_requested && !travel.native_started && travel.published.is_none();
    if !travel.retry_requested
        && travel.state == ProjectSceneRequestState::Degraded
        && !canceling_unpublished_candidate
    {
        owner.slot = Some(Slot::Managed(travel));
        return;
    }
    let (code, summary) = if canceling_unpublished_candidate {
        (
            "project.scene.candidate-cleanup-incomplete",
            "Unpublished Scene candidate remains owned because lifecycle cleanup is unsafe",
        )
    } else {
        (
            "project.scene.request-lifecycle-unsupported",
            "Scene request ownership cannot safely drive managed travel",
        )
    };
    travel
        .diagnostics
        .extend(rejected(code, summary).diagnostics);
    travel.retry_requested = false;
    travel.cancel_requested = false;
    if canceling_unpublished_candidate {
        travel
            .cleanup_result
            .get_or_insert_with(ManagedTravelCleanup::cancelled);
    }
    travel.state = ProjectSceneRequestState::Degraded;
    owner.slot = Some(Slot::Managed(travel));
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
                owner
            };
            if let Some(Slot::Preparing { preparation, .. }) = &owner.slot {
                preparation.cancel();
            }
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
