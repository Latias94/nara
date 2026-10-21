//! One project-owned, worker-prepared Scene request with an explicit owner-thread commit.

mod api;
mod preparation;

pub use api::{
    PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestId,
    ProjectSceneRequestState, ProjectScenes, commit_prepared_project_scene,
};
#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use nara_app::{
    App, PluginError, PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    RuntimeGeneration,
};
use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, SafeSummary};
use nara_ecs::{Res, ResMut, Resource, World, system::SystemParam};
use nara_identity::{SpawnedSceneInstance, WorldEntityToken};
use nara_project::ProjectPath;
use nara_scene::{
    SceneDocument, SceneSpawnReport,
    advanced::{SceneProductOverlayWriter, SceneProductTransactionLimits},
};
use nara_tasks::TaskPools;

use crate::{
    project_host::{
        ProjectContentErrorKind, ProjectContentLoader, ProjectContentSnapshot,
        ProjectSettingsCandidate, RuntimePlan,
    },
    startup_scene::CurrentSceneOwner,
};
use preparation::Preparation;

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

struct Context {
    candidate: ProjectSettingsCandidate,
    plan: RuntimePlan,
    loader: Arc<ProjectContentLoader>,
}

struct Admission {
    context: Arc<Context>,
    id: ProjectSceneRequestId,
    expected: SpawnedSceneInstance,
    path: ProjectPath,
}

#[derive(Resource)]
struct RequestOwner {
    context: Arc<Context>,
    closing: Arc<AtomicBool>,
    slot: Option<Slot>,
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
        snapshot: ProjectContentSnapshot,
    },
    Terminal {
        id: ProjectSceneRequestId,
        state: ProjectSceneRequestState,
        diagnostics: DiagnosticReport,
    },
}

impl Slot {
    fn id(&self) -> ProjectSceneRequestId {
        match self {
            Self::Preparing { admission, .. } | Self::Ready { admission, .. } => admission.id,
            Self::Terminal { id, .. } => *id,
        }
    }

    fn admission(&self) -> Option<&Admission> {
        match self {
            Self::Preparing { admission, .. } | Self::Ready { admission, .. } => Some(admission),
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
        path: ProjectPath,
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
        let settings = self.context.candidate.settings();
        if settings.startup.default_scene.as_ref() != Some(&path)
            && !settings.content.scenes.contains(&path)
        {
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
            path.clone(),
            pools,
            #[cfg(test)]
            self.gate.take(),
        )?;
        self.slot = Some(Slot::Preparing {
            admission: Admission {
                context: Arc::clone(&self.context),
                id,
                expected: expected.clone(),
                path,
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
            && current.is_some_and(|current| current.receipt() == &admission.expected)
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
pub(super) fn install_context(
    world: &mut World,
    candidate: ProjectSettingsCandidate,
    plan: RuntimePlan,
    loader: Arc<ProjectContentLoader>,
) -> Result<(), ProjectSceneRequestError> {
    if world.contains_resource::<RequestOwner>() || candidate.lineage() != plan.lineage() {
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
    world.insert_resource(RequestOwner {
        context: Arc::new(Context {
            candidate,
            plan,
            loader,
        }),
        closing,
        slot: None,
        #[cfg(test)]
        gate: None,
    });
    Ok(())
}

#[cfg(test)]
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
            slot = match (state, result) {
                (Some(state), _) => Slot::terminal(id, state),
                (None, Ok(snapshot)) => {
                    let Slot::Preparing { admission, .. } = slot else {
                        unreachable!()
                    };
                    Slot::Ready {
                        admission,
                        snapshot,
                    }
                }
                (None, Err(failure)) => Slot::Terminal {
                    id,
                    state: ProjectSceneRequestState::Rejected(failure.error),
                    diagnostics: failure.diagnostics,
                },
            };
        }
    } else if !valid {
        slot = Slot::terminal(
            slot.id(),
            ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Stale),
        );
    }
    owner.slot = Some(slot);
}

fn rejected(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut report = SceneSpawnReport::default();
    report.diagnostics.push(Diagnostic::error(
        DiagnosticCode::new(code).expect("static code"),
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
        if let Some(owner) = context.world().get_resource::<RequestOwner>()
            && let Some(Slot::Preparing { preparation, .. }) = &owner.slot
        {
            preparation.cancel();
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
