use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use nara_app::{
    App, AppExitRequests, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
    PluginDefinitionId, PluginError, PluginId, PluginProductCapability, RuntimeGeneration,
};
use nara_ecs::{Res, ResMut, Resource};
use nara_fs::DirectoryCapability;
use nara_render_wgpu::{WGPU_RENDER_PLUGIN_ID, WgpuRenderBackend};

use super::super::{
    ProductSupervisedDesktopReadyAdvance, ProductSupervisedDesktopReadyPublication,
    ProductSupervisedDesktopSession, StartupContentClosureReceipt,
    SupervisedDesktopProductIdentity, SupervisedDesktopRuntimeClosureFacts,
    SupervisedDesktopWireError,
    composition::{RuntimeHostPolicyContribution, runtime_host_policy_contribution},
};

pub(super) const DEFAULT_FIRST_PRESENT_TIMEOUT: Duration = Duration::from_secs(10);

const DESKTOP_FIRST_PRESENT_PLUGIN_ID: PluginId =
    PluginId::new("nara.project-host.desktop-first-present-verification");
const DESKTOP_FIRST_PRESENT_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.project-host.desktop-first-present-verification", 1);
const DESKTOP_FIRST_PRESENT_PLUGIN_REQUIREMENTS: &[PluginId] = &[WGPU_RENDER_PLUGIN_ID];
const DESKTOP_FIRST_PRESENT_PRODUCT_REQUIREMENTS: &[PluginProductCapability] =
    &[PluginProductCapability::new("render-wgpu")];
const DESKTOP_FIRST_PRESENT_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(DESKTOP_FIRST_PRESENT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(DESKTOP_FIRST_PRESENT_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(DESKTOP_FIRST_PRESENT_PRODUCT_REQUIREMENTS);

/// Product-level proof that one checked startup closure reached the real desktop presentation path.
///
/// The receipt retains the content authority instead of copying its manifest, schema, and member
/// facts. The WGPU receipt is process-local and proves submit plus present for one exact live
/// backend transaction; neither receipt is serializable evidence by itself.
#[derive(Clone)]
pub struct DesktopPresentationVerificationReceipt {
    startup_content_closure: StartupContentClosureReceipt,
    presented_frame: nara_render_wgpu::__private::WgpuPresentedFrameReceipt,
}

impl DesktopPresentationVerificationReceipt {
    pub(super) const fn new(
        startup_content_closure: StartupContentClosureReceipt,
        presented_frame: nara_render_wgpu::__private::WgpuPresentedFrameReceipt,
    ) -> Self {
        Self {
            startup_content_closure,
            presented_frame,
        }
    }

    #[must_use]
    pub const fn startup_content_closure(&self) -> &StartupContentClosureReceipt {
        &self.startup_content_closure
    }
}

impl fmt::Debug for DesktopPresentationVerificationReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopPresentationVerificationReceipt")
            .field("startup_content_closure", &self.startup_content_closure)
            .field("presented_frame", &self.presented_frame)
            .finish()
    }
}

#[derive(Debug)]
pub(super) enum DesktopFirstPresentRequest {
    VerificationCanary,
    SupervisedActivation {
        session: ProductSupervisedDesktopSession,
        product_identity: SupervisedDesktopProductIdentity,
    },
}

impl DesktopFirstPresentRequest {
    const fn mode_tag(&self) -> u8 {
        match self {
            Self::VerificationCanary => 0,
            Self::SupervisedActivation { .. } => 1,
        }
    }

    pub(super) const fn label(&self) -> &'static str {
        match self {
            Self::VerificationCanary => "verification-canary",
            Self::SupervisedActivation { .. } => "supervised-activation",
        }
    }

    pub(super) fn is_bound_to_project_root(&self, project_root: &DirectoryCapability) -> bool {
        match self {
            Self::VerificationCanary => true,
            Self::SupervisedActivation { session, .. } => {
                session.is_bound_to_project_root(project_root)
            }
        }
    }
}

pub(super) struct DesktopFirstPresentVerification {
    shared: Arc<Mutex<DesktopFirstPresentState>>,
    mode_tag: u8,
}

impl DesktopFirstPresentVerification {
    pub(super) fn new(request: DesktopFirstPresentRequest) -> Self {
        let mode_tag = request.mode_tag();
        Self {
            shared: Arc::new(Mutex::new(DesktopFirstPresentState::Unarmed(request))),
            mode_tag,
        }
    }

    pub(super) fn host_policy(&self) -> RuntimeHostPolicyContribution {
        let shared = Arc::clone(&self.shared);
        let mut definition_config = [0_u8; 17];
        definition_config[0] = self.mode_tag;
        definition_config[1..]
            .copy_from_slice(&DEFAULT_FIRST_PRESENT_TIMEOUT.as_nanos().to_le_bytes());
        runtime_host_policy_contribution(
            PluginDefinition::infallible::<DesktopFirstPresentPlugin, _>(
                DESKTOP_FIRST_PRESENT_DEFINITION_ID,
                definition_config,
                move || DesktopFirstPresentPlugin {
                    shared: Arc::clone(&shared),
                },
            ),
        )
    }

    pub(super) fn arm(
        &self,
        startup_content_closure: &StartupContentClosureReceipt,
    ) -> Result<(), SupervisedDesktopWireError> {
        let deadline = Instant::now()
            .checked_add(DEFAULT_FIRST_PRESENT_TIMEOUT)
            .expect("the bounded first-present timeout fits in Instant");
        let mut state = lock(&self.shared);
        let current = std::mem::replace(&mut *state, DesktopFirstPresentState::Transitioning);
        let DesktopFirstPresentState::Unarmed(request) = current else {
            *state = current;
            return Err(SupervisedDesktopWireError::InvalidField(
                "first-present activation state",
            ));
        };
        let behavior = match request {
            DesktopFirstPresentRequest::VerificationCanary => {
                DesktopFirstPresentBehavior::ExitAfterAcceptedPresentation
            }
            DesktopFirstPresentRequest::SupervisedActivation {
                session,
                product_identity,
            } => {
                let runtime_closure = match runtime_closure_facts(startup_content_closure) {
                    Ok(runtime_closure) => runtime_closure,
                    Err(error) => {
                        *state = DesktopFirstPresentState::Failed(
                            DesktopFirstPresentFailure::RuntimeClosure,
                        );
                        return Err(error);
                    }
                };
                DesktopFirstPresentBehavior::PublishSupervisedReady {
                    session,
                    product_identity,
                    runtime_closure,
                }
            }
        };
        *state = DesktopFirstPresentState::Awaiting { deadline, behavior };
        Ok(())
    }

    pub(super) fn outcome(&self) -> DesktopFirstPresentOutcome {
        match &*lock(&self.shared) {
            DesktopFirstPresentState::Unarmed(_) => DesktopFirstPresentOutcome::Unarmed,
            DesktopFirstPresentState::Awaiting { .. } => DesktopFirstPresentOutcome::Awaiting,
            DesktopFirstPresentState::Publishing { .. } => DesktopFirstPresentOutcome::Publishing,
            DesktopFirstPresentState::Accepted(receipt) => {
                DesktopFirstPresentOutcome::Accepted(*receipt)
            }
            DesktopFirstPresentState::TimedOut => DesktopFirstPresentOutcome::TimedOut,
            DesktopFirstPresentState::Failed(failure) => {
                DesktopFirstPresentOutcome::Failed(*failure)
            }
            DesktopFirstPresentState::Transitioning => {
                DesktopFirstPresentOutcome::Failed(DesktopFirstPresentFailure::ReadyPublication)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DesktopFirstPresentFailure {
    RuntimeClosure,
    ReadyPublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DesktopFirstPresentOutcome {
    Unarmed,
    Awaiting,
    Publishing,
    Accepted(nara_render_wgpu::__private::WgpuPresentedFrameReceipt),
    TimedOut,
    Failed(DesktopFirstPresentFailure),
}

enum DesktopFirstPresentBehavior {
    ExitAfterAcceptedPresentation,
    PublishSupervisedReady {
        session: ProductSupervisedDesktopSession,
        product_identity: SupervisedDesktopProductIdentity,
        runtime_closure: SupervisedDesktopRuntimeClosureFacts,
    },
}

enum DesktopFirstPresentState {
    Unarmed(DesktopFirstPresentRequest),
    Awaiting {
        deadline: Instant,
        behavior: DesktopFirstPresentBehavior,
    },
    Publishing {
        presented: nara_render_wgpu::__private::WgpuPresentedFrameReceipt,
        publication: ProductSupervisedDesktopReadyPublication,
    },
    Accepted(nara_render_wgpu::__private::WgpuPresentedFrameReceipt),
    TimedOut,
    Failed(DesktopFirstPresentFailure),
    Transitioning,
}

fn runtime_closure_facts(
    receipt: &StartupContentClosureReceipt,
) -> Result<SupervisedDesktopRuntimeClosureFacts, SupervisedDesktopWireError> {
    let authoring_bundle_digest =
        receipt
            .runtime_schema_bundle_digest()
            .ok_or(SupervisedDesktopWireError::InvalidField(
                "runtime authoring bundle digest",
            ))?;
    SupervisedDesktopRuntimeClosureFacts::new(
        receipt.manifest_digest(),
        authoring_bundle_digest,
        receipt.schema_fingerprint().to_hex(),
        receipt.content_digest(),
        receipt.closure_plan_digest(),
    )
}

#[derive(Resource)]
struct DesktopFirstPresentResource {
    shared: Arc<Mutex<DesktopFirstPresentState>>,
}

struct DesktopFirstPresentPlugin {
    shared: Arc<Mutex<DesktopFirstPresentState>>,
}

impl Plugin for DesktopFirstPresentPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DESKTOP_FIRST_PRESENT_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(DesktopFirstPresentResource {
            shared: Arc::clone(&self.shared),
        })?;
        app.add_systems(CoreStage::Last, verify_first_present)?;
        Ok(())
    }
}

fn verify_first_present(
    runtime_generation: Res<RuntimeGeneration>,
    backend: Res<WgpuRenderBackend>,
    verification: Res<DesktopFirstPresentResource>,
    mut exit_requests: ResMut<AppExitRequests>,
) {
    let presented = nara_render_wgpu::__private::presented_frame_this_transaction(&backend);
    let action = advance(
        &verification.shared,
        *runtime_generation,
        presented,
        Instant::now(),
    );
    match action {
        DesktopFirstPresentAction::None => {}
        DesktopFirstPresentAction::RequestSuccess if exit_requests.requested().is_none() => {
            exit_requests.request_success();
        }
        DesktopFirstPresentAction::RequestExit if exit_requests.requested().is_none() => {
            exit_requests.request_exit();
        }
        DesktopFirstPresentAction::RequestSuccess | DesktopFirstPresentAction::RequestExit => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesktopFirstPresentAction {
    None,
    RequestSuccess,
    RequestExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesktopFirstPresentDecision<T> {
    Await,
    Accept(T),
    TimedOut,
}

fn first_present_decision<T>(
    deadline: Instant,
    now: Instant,
    exact_presentation: Option<T>,
) -> DesktopFirstPresentDecision<T> {
    if now >= deadline {
        DesktopFirstPresentDecision::TimedOut
    } else if let Some(presentation) = exact_presentation {
        DesktopFirstPresentDecision::Accept(presentation)
    } else {
        DesktopFirstPresentDecision::Await
    }
}

fn advance(
    shared: &Arc<Mutex<DesktopFirstPresentState>>,
    runtime_generation: RuntimeGeneration,
    presented: Option<nara_render_wgpu::__private::WgpuPresentedFrameReceipt>,
    now: Instant,
) -> DesktopFirstPresentAction {
    let mut state = lock(shared);
    let current = std::mem::replace(&mut *state, DesktopFirstPresentState::Transitioning);
    let (next, action) = match current {
        DesktopFirstPresentState::Awaiting { deadline, behavior } => {
            let exact_presentation =
                presented.filter(|presented| presented.runtime_generation() == runtime_generation);
            match first_present_decision(deadline, now, exact_presentation) {
                DesktopFirstPresentDecision::TimedOut => (
                    DesktopFirstPresentState::TimedOut,
                    DesktopFirstPresentAction::RequestExit,
                ),
                DesktopFirstPresentDecision::Accept(presented) => match behavior {
                    DesktopFirstPresentBehavior::ExitAfterAcceptedPresentation => (
                        DesktopFirstPresentState::Accepted(presented),
                        DesktopFirstPresentAction::RequestSuccess,
                    ),
                    DesktopFirstPresentBehavior::PublishSupervisedReady {
                        session,
                        product_identity,
                        runtime_closure,
                    } => match session.begin_ready(product_identity, runtime_closure, now) {
                        Ok(mut publication) => match publication.advance(now) {
                            Ok(ProductSupervisedDesktopReadyAdvance::Published) => (
                                DesktopFirstPresentState::Accepted(presented),
                                DesktopFirstPresentAction::None,
                            ),
                            Ok(ProductSupervisedDesktopReadyAdvance::Pending) => (
                                DesktopFirstPresentState::Publishing {
                                    presented,
                                    publication,
                                },
                                DesktopFirstPresentAction::None,
                            ),
                            Err(_) => (
                                DesktopFirstPresentState::Failed(
                                    DesktopFirstPresentFailure::ReadyPublication,
                                ),
                                DesktopFirstPresentAction::RequestExit,
                            ),
                        },
                        Err(_) => (
                            DesktopFirstPresentState::Failed(
                                DesktopFirstPresentFailure::ReadyPublication,
                            ),
                            DesktopFirstPresentAction::RequestExit,
                        ),
                    },
                },
                DesktopFirstPresentDecision::Await => (
                    DesktopFirstPresentState::Awaiting { deadline, behavior },
                    DesktopFirstPresentAction::None,
                ),
            }
        }
        DesktopFirstPresentState::Publishing {
            presented,
            mut publication,
        } => match publication.advance(now) {
            Ok(ProductSupervisedDesktopReadyAdvance::Published) => (
                DesktopFirstPresentState::Accepted(presented),
                DesktopFirstPresentAction::None,
            ),
            Ok(ProductSupervisedDesktopReadyAdvance::Pending) => (
                DesktopFirstPresentState::Publishing {
                    presented,
                    publication,
                },
                DesktopFirstPresentAction::None,
            ),
            Err(_) => (
                DesktopFirstPresentState::Failed(DesktopFirstPresentFailure::ReadyPublication),
                DesktopFirstPresentAction::RequestExit,
            ),
        },
        other => (other, DesktopFirstPresentAction::None),
    };
    *state = next;
    action
}

fn lock(shared: &Arc<Mutex<DesktopFirstPresentState>>) -> MutexGuard<'_, DesktopFirstPresentState> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use nara_app::{
        App, RuntimeAdmissionReservation, RuntimeCandidateRetirementState, RuntimeClosePolicy,
        RuntimeObligationLedger,
    };

    use super::*;

    fn runtime_generation() -> RuntimeGeneration {
        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                App::new().seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let runtime = candidate.complete_startup().unwrap().promote();
        let generation = runtime.generation();
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
        generation
    }

    #[test]
    fn timeout_requests_normal_exit_without_forging_acceptance() {
        let shared = Arc::new(Mutex::new(DesktopFirstPresentState::Awaiting {
            deadline: Instant::now(),
            behavior: DesktopFirstPresentBehavior::ExitAfterAcceptedPresentation,
        }));

        assert_eq!(
            advance(&shared, runtime_generation(), None, Instant::now()),
            DesktopFirstPresentAction::RequestExit
        );
        assert!(matches!(
            &*lock(&shared),
            DesktopFirstPresentState::TimedOut
        ));
        assert_eq!(
            advance(&shared, runtime_generation(), None, Instant::now()),
            DesktopFirstPresentAction::None
        );
    }

    #[test]
    fn presentation_at_first_present_deadline_is_rejected() {
        let deadline = Instant::now();

        assert_eq!(
            first_present_decision(deadline, deadline, Some(())),
            DesktopFirstPresentDecision::TimedOut
        );
    }

    #[test]
    fn presentation_after_first_present_deadline_is_rejected() {
        let deadline = Instant::now();
        let after_deadline = deadline
            .checked_add(Duration::from_nanos(1))
            .expect("one nanosecond fits in Instant");

        assert_eq!(
            first_present_decision(deadline, after_deadline, Some(())),
            DesktopFirstPresentDecision::TimedOut
        );
    }

    #[test]
    fn unarmed_verification_cannot_request_exit() {
        let shared = Arc::new(Mutex::new(DesktopFirstPresentState::Unarmed(
            DesktopFirstPresentRequest::VerificationCanary,
        )));

        assert_eq!(
            advance(&shared, runtime_generation(), None, Instant::now()),
            DesktopFirstPresentAction::None
        );
        assert!(matches!(
            &*lock(&shared),
            DesktopFirstPresentState::Unarmed(_)
        ));
    }

    #[test]
    fn supervised_activation_has_a_distinct_non_canary_mode() {
        let canary = DesktopFirstPresentRequest::VerificationCanary;

        assert_eq!(canary.mode_tag(), 0);
        assert_eq!(canary.label(), "verification-canary");
    }
}
