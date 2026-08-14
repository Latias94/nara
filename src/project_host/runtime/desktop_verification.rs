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
use nara_render_wgpu::{WGPU_RENDER_PLUGIN_ID, WgpuRenderBackend};

use super::super::{
    StartupContentClosureReceipt,
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

pub(super) struct DesktopFirstPresentVerification {
    shared: Arc<Mutex<DesktopFirstPresentState>>,
}

impl DesktopFirstPresentVerification {
    pub(super) fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(DesktopFirstPresentState::Unarmed)),
        }
    }

    pub(super) fn host_policy(&self) -> RuntimeHostPolicyContribution {
        let shared = Arc::clone(&self.shared);
        let timeout_nanos = DEFAULT_FIRST_PRESENT_TIMEOUT.as_nanos().to_le_bytes();
        runtime_host_policy_contribution(
            PluginDefinition::infallible::<DesktopFirstPresentPlugin, _>(
                DESKTOP_FIRST_PRESENT_DEFINITION_ID,
                timeout_nanos,
                move || DesktopFirstPresentPlugin {
                    shared: Arc::clone(&shared),
                },
            ),
        )
    }

    pub(super) fn arm(&self) {
        let deadline = Instant::now()
            .checked_add(DEFAULT_FIRST_PRESENT_TIMEOUT)
            .expect("the bounded first-present timeout fits in Instant");
        let mut state = lock(&self.shared);
        debug_assert!(matches!(*state, DesktopFirstPresentState::Unarmed));
        *state = DesktopFirstPresentState::Awaiting { deadline };
    }

    pub(super) fn outcome(&self) -> DesktopFirstPresentOutcome {
        match *lock(&self.shared) {
            DesktopFirstPresentState::Unarmed => DesktopFirstPresentOutcome::Unarmed,
            DesktopFirstPresentState::Awaiting { .. } => DesktopFirstPresentOutcome::Awaiting,
            DesktopFirstPresentState::Accepted(receipt) => {
                DesktopFirstPresentOutcome::Accepted(receipt)
            }
            DesktopFirstPresentState::TimedOut => DesktopFirstPresentOutcome::TimedOut,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DesktopFirstPresentOutcome {
    Unarmed,
    Awaiting,
    Accepted(nara_render_wgpu::__private::WgpuPresentedFrameReceipt),
    TimedOut,
}

#[derive(Clone, Copy)]
enum DesktopFirstPresentState {
    Unarmed,
    Awaiting { deadline: Instant },
    Accepted(nara_render_wgpu::__private::WgpuPresentedFrameReceipt),
    TimedOut,
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

fn advance(
    shared: &Arc<Mutex<DesktopFirstPresentState>>,
    runtime_generation: RuntimeGeneration,
    presented: Option<nara_render_wgpu::__private::WgpuPresentedFrameReceipt>,
    now: Instant,
) -> DesktopFirstPresentAction {
    let mut state = lock(shared);
    let DesktopFirstPresentState::Awaiting { deadline } = *state else {
        return DesktopFirstPresentAction::None;
    };
    if let Some(presented) = presented
        && presented.runtime_generation() == runtime_generation
    {
        *state = DesktopFirstPresentState::Accepted(presented);
        return DesktopFirstPresentAction::RequestSuccess;
    }
    if now >= deadline {
        *state = DesktopFirstPresentState::TimedOut;
        return DesktopFirstPresentAction::RequestExit;
    }
    DesktopFirstPresentAction::None
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
        }));

        assert_eq!(
            advance(&shared, runtime_generation(), None, Instant::now()),
            DesktopFirstPresentAction::RequestExit
        );
        assert!(matches!(*lock(&shared), DesktopFirstPresentState::TimedOut));
        assert_eq!(
            advance(&shared, runtime_generation(), None, Instant::now()),
            DesktopFirstPresentAction::None
        );
    }

    #[test]
    fn unarmed_verification_cannot_request_exit() {
        let shared = Arc::new(Mutex::new(DesktopFirstPresentState::Unarmed));

        assert_eq!(
            advance(&shared, runtime_generation(), None, Instant::now()),
            DesktopFirstPresentAction::None
        );
        assert!(matches!(*lock(&shared), DesktopFirstPresentState::Unarmed));
    }
}
