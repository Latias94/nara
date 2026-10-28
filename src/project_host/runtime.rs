use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub(crate) use super::composition::{RuntimeHostPolicyContribution, RuntimePluginOverlay};
use super::{
    CompositionError, ProjectContentRevision, ProjectContentSnapshot, ProjectSettingsLineage,
    RuntimePlan, RuntimePlanError,
};
use crate::startup_scene::{SceneSource, materialize_startup_scene};
use nara_app::{
    AppExit, PluginFailureReport, PluginHook, PluginHookMutation, PluginPlanError,
    RuntimeAdmissionError, RuntimeAdmissionReservation, RuntimeCandidateFailure,
    RuntimeCandidateRetirementState, RuntimeCloseCause, RuntimeCloseErrorDisposition,
    RuntimeCloseEvidence, RuntimeCloseParticipantPhase, RuntimeClosePolicy,
    RuntimeConstructionFailure, RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus,
    RuntimeControlTicket, RuntimeFault, RuntimeFaultDetail, RuntimeFaultKind, RuntimeFaultReporter,
    RuntimeGeneration, RuntimeInstance, RuntimeObligationLedger, RuntimePublicationFailure,
    RuntimePublicationSlot, RuntimeRetirement, RuntimeState,
};
use nara_asset::{
    AssetEvents, AssetRecord, AssetServer, AssetSourceKind, AssetStates, Assets, SourceHash,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_ecs::{Mut, Resource, World};
use nara_gameplay::{GameplayCommandQueue, GameplayCommandSubmission};
use nara_image::ImageAsset;
use nara_reflect::{
    SchemaCompositionFingerprint, component_registry, validate_component_registry_authority,
};
use nara_scene::{
    SceneComponentBindings, scene_component_bindings, validate_scene_component_bindings_authority,
};
use nara_tasks::TaskShutdownReport;
use nara_tilemap::TileSet;
use plugin_diagnostics::{append_plugin_shutdown_diagnostics, runtime_construction_failure_report};

const PROJECT_MANIFEST: &str = "nara.toml";

mod action;
mod content_residency;
mod scene_requests;
pub use scene_requests::domains::{
    ManagedSceneDomain, ManagedSceneDomainAttempt, ManagedSceneDomainError, ManagedSceneDomainId,
    ManagedSceneDomainPhase, ManagedSceneDomainPreparation, ManagedSceneDomains,
};
pub(crate) use scene_requests::{
    PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION, poll_project_scene_requests,
    register_close_participant as register_project_scene_close_participant,
};
pub use scene_requests::{
    PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestId,
    ProjectSceneRequestState, ProjectScenes, begin_prepared_project_scene_travel,
    commit_prepared_project_scene,
};

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
mod desktop_verification;

#[cfg(feature = "tooling")]
mod editor;
#[cfg(feature = "tooling")]
pub use editor::{
    EditorExtensionDocumentHostError, EditorSchemaOwnerAdoptionError,
    EditorSchemaOwnerAdoptionFailure, EditorSchemaOwnerAdoptionId,
    EditorSchemaOwnerAdoptionOutcome, EditorSchemaOwnerAdoptionPhase,
    EditorSchemaOwnerAdoptionView, EditorSourceAssetDomain, EditorSourceAssetImporters,
    EditorSourceToolCommand, EditorSourceToolDefinition, EditorSourceToolInstallation,
    EditorSourceToolInstallationError, EditorSourceToolInstallationInspection,
    EditorSourceToolInstallationOrphan, EditorSourceToolInstallationOutcome,
    EditorSourceToolInstallationPlan, EditorSourceToolInstallationReceipt,
    EditorSourceToolInstalledMember, EditorSourceToolManagedError, EditorSourceToolManagedHost,
    EditorSourceToolManagedHostDigests, EditorSourceToolReleaseError,
    EditorSourceToolReleaseMember, EditorSourceToolReleaseMemberKind,
    EditorSourceToolReleaseRequirement, PreparedEditorSourceToolRelease,
    PreparedEditorSourceToolSelection,
};

mod plugin_diagnostics;

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub use action::{DesktopRun, DesktopRunIntent, DesktopRunOutcome, DesktopRunReport};
pub use action::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub use desktop_verification::DesktopPresentationVerificationReceipt;
#[cfg(all(feature = "tooling", feature = "presentation-ui"))]
pub use editor::EditorUiPreviewCandidate;
#[cfg(all(feature = "tooling", feature = "presentation-2d"))]
pub use editor::{
    EditorAcceptedPreview, EditorPresentationError, EditorPresentationImage, EditorPresentedToken,
    EditorPreviewPresentation,
};
#[cfg(feature = "tooling")]
#[doc(hidden)]
pub use editor::{
    EditorEditPreparationError, EditorProjectCloseProgress, EditorProjectCloseReceipt,
    EditorProjectOpenAttempt, EditorProjectOpenError, EditorProjectOpenFailure,
    EditorProjectOpenFailureImpact, EditorProjectOpenFailureSource, EditorProjectOpenHost,
    EditorProjectOpenPhase, EditorProjectOpenProgress, EditorProjectOpenTasksCloseReport,
    EditorProjectSession, SupervisedDesktopSourceAdmissionError, SupervisedDesktopSourceAuthority,
    TrustedCompiledEditorProjectIntent, admit_supervised_desktop_source_receipt,
};
#[cfg(all(
    feature = "tooling",
    any(feature = "presentation-2d", feature = "presentation-ui")
))]
pub use editor::{
    EditorGameInputBatch, EditorGameInputEnvelope, EditorGameInputEvent, EditorGameStartActivation,
    EditorGameStartRequest, EditorGameViewBridgeFailure, EditorGameViewCandidate,
    EditorGameViewCandidateIdentity, EditorGameViewCandidateParts, EditorGameViewCompletion,
    EditorGameViewCompletionDisposition,
};
#[cfg(all(feature = "tooling", feature = "presentation-2d"))]
pub use editor::{EditorPreviewCandidate, EditorPreviewContentStamp, EditorPreviewHit};
#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
pub use editor::{EditorUiAuthoringEditError, EditorUiAuthoringFrame};

struct RuntimeStartAttempt {
    owner: Arc<HostStartClaim>,
    epoch: u64,
    stamp: RuntimeStartStamp,
    inputs: Option<RuntimeStartInputs>,
}

struct PreparedRuntimeStart {
    claim: ActiveStartClaim,
    snapshot: ProjectContentSnapshot,
    plan: RuntimePlan,
    runtime: RuntimePublicationSlot,
    diagnostics: DiagnosticReport,
}

impl PreparedRuntimeStart {
    fn generation(&self) -> RuntimeGeneration {
        self.runtime
            .runtime()
            .expect("a prepared runtime start owns one inactive publication")
            .generation()
    }
}

struct RuntimeStartInputs {
    snapshot: ProjectContentSnapshot,
    plan: RuntimePlan,
    startup_scene: SceneSource,
    commands: Vec<GameplayCommandSubmission>,
    obligations: RuntimeObligationLedger,
    scene_preparation: Option<(
        super::ProjectSettingsCandidate,
        Arc<crate::project_content::ProjectContentLoader>,
    )>,
}

struct RuntimeStartStamp {
    lineage: ProjectSettingsLineage,
    content_revision: ProjectContentRevision,
    schema_fingerprint: SchemaCompositionFingerprint,
    bindings: SceneComponentBindings,
}

impl RuntimeStartStamp {
    fn capture(snapshot: &ProjectContentSnapshot, plan: &RuntimePlan) -> Self {
        Self {
            lineage: snapshot.lineage(),
            content_revision: snapshot.revision(),
            schema_fingerprint: snapshot.schema_fingerprint(),
            bindings: plan.schema_validation().bindings().clone(),
        }
    }

    fn matches(&self, snapshot: &ProjectContentSnapshot, plan: &RuntimePlan) -> bool {
        self.lineage == snapshot.lineage()
            && self.lineage == plan.lineage()
            && self.content_revision == snapshot.revision()
            && self.schema_fingerprint == snapshot.schema_fingerprint()
            && self.schema_fingerprint == plan.schema_validation().composition_fingerprint()
            && self
                .bindings
                .same_definitions(plan.schema_validation().bindings())
            && snapshot.shares_component_bindings(&self.bindings)
            && snapshot.matches_asset_domains(plan)
    }
}

struct HostStartClaim {
    active_epoch: AtomicU64,
}

impl HostStartClaim {
    const NONE: u64 = 0;

    const fn new() -> Self {
        Self {
            active_epoch: AtomicU64::new(Self::NONE),
        }
    }

    fn try_begin(&self, epoch: u64) -> bool {
        debug_assert_ne!(epoch, Self::NONE);
        self.active_epoch
            .compare_exchange(Self::NONE, epoch, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn is_active(&self, epoch: u64) -> bool {
        self.active_epoch.load(Ordering::Acquire) == epoch
    }

    fn finish(&self, epoch: u64) -> bool {
        self.active_epoch
            .compare_exchange(epoch, Self::NONE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn any_active(&self) -> bool {
        self.active_epoch.load(Ordering::Acquire) != Self::NONE
    }
}

struct ActiveStartClaim {
    owner: Arc<HostStartClaim>,
    epoch: u64,
    active: bool,
}

impl ActiveStartClaim {
    fn new(owner: Arc<HostStartClaim>, epoch: u64) -> Self {
        Self {
            owner,
            epoch,
            active: true,
        }
    }

    fn finish(mut self) {
        let released = self.owner.finish(self.epoch);
        debug_assert!(released, "the Host must own the completed start claim");
        self.active = false;
    }

    const fn epoch(&self) -> u64 {
        self.epoch
    }
}

impl Drop for ActiveStartClaim {
    fn drop(&mut self) {
        if self.active {
            let _ = self.owner.finish(self.epoch);
        }
    }
}

impl RuntimeStartAttempt {
    fn take_for(&mut self, host: &ProjectHost) -> Result<RuntimeStartInputs, HostFault> {
        let valid_owner = Arc::ptr_eq(&self.owner, &host.start_claim);
        let valid_epoch = self.owner.is_active(self.epoch);
        let valid_slot = matches!(host.slot, ProjectHostSlot::Empty);
        let valid_stamp = self
            .inputs
            .as_ref()
            .is_some_and(|inputs| self.stamp.matches(&inputs.snapshot, &inputs.plan));
        if !valid_owner || !valid_epoch || !valid_slot || !valid_stamp {
            return Err(HostFault::new(single_error(
                "project.run.stale-start",
                "Project run start identity is stale",
            )));
        }
        self.inputs.take().ok_or_else(|| {
            HostFault::new(single_error(
                "project.run.stale-start",
                "Project run start identity is stale",
            ))
        })
    }
}

impl Drop for RuntimeStartAttempt {
    fn drop(&mut self) {
        if self.inputs.is_some() {
            // No integration code can register into this private ledger before `take_for`.
            // Dropping an unclaimed attempt therefore releases an empty reservation ledger.
            let _ = self.owner.finish(self.epoch);
        }
    }
}

struct ProjectHost {
    start_claim: Arc<HostStartClaim>,
    next_epoch: u64,
    cleanup_policy: RuntimeClosePolicy,
    slot: ProjectHostSlot,
    // A Runtime retains one sticky fault; delivering it does not imply a cleanup failure.
    delivered_runtime_fault: Option<RuntimeGeneration>,
}

enum ProjectHostSlot {
    Empty,
    Running(Box<PublishedProjectRuntime>),
    Cleaning {
        epoch: u64,
        owner: Box<CleanupOwner>,
        task_report_required: bool,
    },
}

struct PublishedProjectRuntime {
    epoch: u64,
    runtime: RuntimePublicationSlot,
    startup_admission_revision: ProjectContentRevision,
    _plan: RuntimePlan,
}

impl PublishedProjectRuntime {
    fn runtime(&self) -> &RuntimeInstance {
        self.runtime
            .runtime()
            .expect("a visible project runtime slot is fully published")
    }

    fn runtime_mut(&mut self) -> &mut RuntimeInstance {
        self.runtime
            .runtime_mut()
            .expect("a visible project runtime slot is fully published")
    }

    fn take_runtime(&mut self) -> RuntimeInstance {
        self.runtime
            .take()
            .expect("a visible project runtime slot is fully published")
    }
}

enum CleanupOwner {
    Construction(RuntimeConstructionFailure),
    Candidate(RuntimeRetirement),
    Publication(RuntimePublicationFailure),
    Startup(RuntimeCandidateFailure),
}

enum CleanupDriveOutcome {
    Retiring,
    RetirementIncomplete,
    Complete {
        failed: bool,
        diagnostics: DiagnosticReport,
        observation: Option<ProjectRuntimeCloseObservation>,
    },
}

/// Concrete product observations captured before a physically retired World is released.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectRuntimeCloseObservation {
    epoch: u64,
    task_shutdown: Option<TaskShutdownReport>,
    close_evidence: Option<RuntimeCloseEvidence>,
    task_report_required: bool,
}

impl ProjectRuntimeCloseObservation {
    fn capture(
        epoch: u64,
        world: Option<&World>,
        close_evidence: Option<&RuntimeCloseEvidence>,
        task_report_required: bool,
    ) -> Self {
        Self {
            epoch,
            task_shutdown: world
                .and_then(|world| world.get_resource::<TaskShutdownReport>())
                .cloned(),
            close_evidence: close_evidence.cloned(),
            task_report_required,
        }
    }

    fn task_report_invalid(&self) -> bool {
        self.task_shutdown
            .as_ref()
            .map_or(self.task_report_required, |report| !report.is_complete())
    }

    fn has_cleanup_failure(&self) -> bool {
        self.task_report_invalid()
            || self
                .task_shutdown
                .as_ref()
                .is_some_and(TaskShutdownReport::has_cleanup_failure)
            || self
                .close_evidence
                .as_ref()
                .is_some_and(RuntimeCloseEvidence::has_terminal_failure)
    }

    fn completed_tasks(&self) -> Option<&TaskShutdownReport> {
        self.task_shutdown
            .as_ref()
            .filter(|report| report.is_complete())
    }
}

impl CleanupOwner {
    fn world(&self) -> Option<&World> {
        match self {
            Self::Construction(owner) => owner.world(),
            Self::Candidate(owner) => Some(owner.world()),
            Self::Publication(owner) => Some(owner.world()),
            Self::Startup(owner) => Some(owner.world()),
        }
    }

    fn close_observation(
        &self,
        epoch: u64,
        task_report_required: bool,
    ) -> ProjectRuntimeCloseObservation {
        ProjectRuntimeCloseObservation::capture(
            epoch,
            self.world(),
            self.close_evidence(),
            task_report_required,
        )
    }

    fn state(&self) -> RuntimeCandidateRetirementState {
        match self {
            Self::Construction(owner) => owner.retirement_state(),
            Self::Candidate(owner) => owner.retirement_state(),
            Self::Publication(owner) => owner.retirement_state(),
            Self::Startup(owner) => owner.retirement_state(),
        }
    }

    fn drive(&mut self) -> RuntimeCandidateRetirementState {
        match self {
            Self::Construction(owner) => owner.drive_retirement(),
            Self::Candidate(owner) => owner.drive_retirement(),
            Self::Publication(owner) => owner.drive_retirement(),
            Self::Startup(owner) => owner.drive_retirement(),
        }
    }

    fn close_evidence(&self) -> Option<&RuntimeCloseEvidence> {
        match self {
            Self::Construction(owner) => owner.close_evidence(),
            Self::Candidate(owner) => Some(owner.close_evidence()),
            Self::Publication(owner) => Some(owner.close_evidence()),
            Self::Startup(owner) => Some(owner.close_evidence()),
        }
    }

    fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        match self {
            Self::Construction(owner) => owner.plugin_failure_report(),
            Self::Candidate(owner) => owner.plugin_failure_report(),
            Self::Publication(owner) => owner.plugin_failure_report(),
            Self::Startup(owner) => owner.plugin_failure_report(),
        }
    }

    fn completion_failure_report(
        &self,
        delivered_runtime_fault: Option<RuntimeGeneration>,
    ) -> DiagnosticReport {
        let mut diagnostics = DiagnosticReport::default();
        if let Some(evidence) = self.close_evidence()
            && evidence.has_terminal_failure()
        {
            diagnostics.extend(cleanup_failed_report(
                evidence,
                self.plugin_failure_report(),
            ));
        }
        if let Self::Candidate(owner) = self
            && Some(owner.generation()) != delivered_runtime_fault
            && let Some(fault) = owner.fault()
        {
            diagnostics.extend(runtime_scope_failure_report(fault));
        }
        diagnostics
    }
}

impl ProjectHost {
    fn new(cleanup_policy: RuntimeClosePolicy) -> Self {
        Self {
            start_claim: Arc::new(HostStartClaim::new()),
            next_epoch: 1,
            cleanup_policy,
            slot: ProjectHostSlot::Empty,
            delivered_runtime_fault: None,
        }
    }

    fn running_runtime_mut(&mut self) -> Option<&mut RuntimeInstance> {
        match &mut self.slot {
            ProjectHostSlot::Running(published) => Some(published.runtime_mut()),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    fn running_runtime_state(&self) -> Option<RuntimeState> {
        match &self.slot {
            ProjectHostSlot::Running(published) => Some(published.runtime().state()),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    fn running_runtime_close_evidence(&self) -> Option<RuntimeCloseEvidence> {
        match &self.slot {
            ProjectHostSlot::Running(published) => {
                Some(published.runtime().close_evidence().clone())
            }
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
    fn running_runtime_fault(&self) -> Option<&RuntimeFault> {
        match &self.slot {
            ProjectHostSlot::Running(published) => published.runtime().fault(),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
    fn acknowledge_running_runtime_fault(&mut self) {
        if let ProjectHostSlot::Running(published) = &self.slot
            && published.runtime().fault().is_some()
        {
            self.delivered_runtime_fault = Some(published.runtime().generation());
        }
    }

    fn running_runtime_close_failure_report(&self) -> Option<DiagnosticReport> {
        match &self.slot {
            ProjectHostSlot::Running(published)
                if published.runtime().close_evidence().has_terminal_failure() =>
            {
                Some(cleanup_failed_report(
                    published.runtime().close_evidence(),
                    published.runtime().plugin_failure_report(),
                ))
            }
            ProjectHostSlot::Empty
            | ProjectHostSlot::Cleaning { .. }
            | ProjectHostSlot::Running(_) => None,
        }
    }

    fn running_runtime_has_terminal_close_failure(&self) -> bool {
        match &self.slot {
            ProjectHostSlot::Running(published) => {
                published.runtime().close_evidence().has_terminal_failure()
            }
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => false,
        }
    }

    #[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
    fn running_runtime_plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        match &self.slot {
            ProjectHostSlot::Running(published) => published.runtime().plugin_failure_report(),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    fn running_runtime_generation(&self) -> Option<u64> {
        match &self.slot {
            ProjectHostSlot::Running(published) => Some(published.runtime().generation().get()),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    fn running_content_revision(&self) -> Option<ProjectContentRevision> {
        match &self.slot {
            ProjectHostSlot::Running(published) => Some(published.startup_admission_revision),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    fn with_running_world<R>(&self, operation: impl FnOnce(&World) -> R) -> Result<R, HostFault> {
        let ProjectHostSlot::Running(published) = &self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is unavailable for observation",
            )));
        };
        let world = published.runtime().world();
        let expected = published._plan.schema_validation().bindings();
        verify_published_runtime_registry(world, expected)?;
        let output = operation(world);
        verify_published_runtime_registry(world, expected)?;
        Ok(output)
    }

    fn request_runtime_control(
        &mut self,
        control: RuntimeControl,
    ) -> Result<RuntimeControlTicket, HostFault> {
        let runtime = self.running_runtime_mut().ok_or_else(|| {
            HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is unavailable for control",
            ))
        })?;
        match runtime.request_control(control) {
            RuntimeControlRequestResult::Accepted(ticket) => Ok(ticket),
            RuntimeControlRequestResult::Rejected(_) => Err(HostFault::new(single_error(
                "project.run.control-rejected",
                "Project runtime rejected the control request",
            ))),
        }
    }

    fn runtime_control_status(&self, ticket: RuntimeControlTicket) -> Option<RuntimeControlStatus> {
        match &self.slot {
            ProjectHostSlot::Running(published) => published.runtime().control_status(ticket),
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => None,
        }
    }

    fn drive_running_runtime(&mut self, real_delta: Duration) -> Result<(), HostFault> {
        let ProjectHostSlot::Running(published) = &mut self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is unavailable for driving",
            )));
        };
        let expected = published._plan.schema_validation().bindings().clone();
        let generation = published.runtime().generation();
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        let drive = published.runtime_mut().drive(real_delta);
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        drive.map(|_| ()).map_err(|error| {
            self.delivered_runtime_fault = Some(generation);
            HostFault::new(runtime_scope_failure_report(error.fault()))
        })
    }

    fn release_stopped_runtime(&mut self) -> bool {
        if self.running_runtime_state() != Some(RuntimeState::Stopped) {
            return false;
        }
        let slot = std::mem::replace(&mut self.slot, ProjectHostSlot::Empty);
        let ProjectHostSlot::Running(mut published) = slot else {
            unreachable!("a stopped runtime remains in the running Host slot");
        };
        drop(published.take_runtime());
        true
    }

    fn stopped_close_observation(&self) -> Option<ProjectRuntimeCloseObservation> {
        let ProjectHostSlot::Running(published) = &self.slot else {
            return None;
        };
        (published.runtime().state() == RuntimeState::Stopped).then(|| {
            ProjectRuntimeCloseObservation::capture(
                published.epoch,
                Some(published.runtime().world()),
                Some(published.runtime().close_evidence()),
                project_requires_task_report(&published._plan),
            )
        })
    }

    fn current_task_shutdown(&self) -> Option<TaskShutdownReport> {
        let world = match &self.slot {
            // A caller-held prepared activation still owns its Runtime outside this slot.
            ProjectHostSlot::Empty => {
                return self
                    .start_claim
                    .any_active()
                    .then(TaskShutdownReport::default);
            }
            ProjectHostSlot::Running(published) => Some(published.runtime().world()),
            ProjectHostSlot::Cleaning { owner, .. } => owner.world(),
        };
        Some(
            world
                .and_then(|world| world.get_resource::<TaskShutdownReport>())
                .cloned()
                .unwrap_or_default(),
        )
    }

    fn has_runtime_custody(&self) -> bool {
        !matches!(self.slot, ProjectHostSlot::Empty) || self.start_claim.any_active()
    }

    fn begin_start(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Result<RuntimeStartAttempt, HostFault> {
        let startup_scene = snapshot.scene_source();
        self.begin_start_with_scene(snapshot, plan, startup_scene, commands)
    }

    fn begin_project_start(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        candidate: super::ProjectSettingsCandidate,
        loader: Arc<crate::project_content::ProjectContentLoader>,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Result<RuntimeStartAttempt, HostFault> {
        let mut attempt = self.begin_start(snapshot, plan, commands)?;
        attempt
            .inputs
            .as_mut()
            .expect("a fresh start owns its inputs")
            .scene_preparation = Some((candidate, loader));
        Ok(attempt)
    }

    fn begin_editor_start(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        startup_scene: SceneSource,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Result<RuntimeStartAttempt, HostFault> {
        self.begin_start_with_scene(snapshot, plan, startup_scene, commands)
    }

    fn begin_start_with_scene(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        startup_scene: SceneSource,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Result<RuntimeStartAttempt, HostFault> {
        if !matches!(self.slot, ProjectHostSlot::Empty) || self.start_claim.any_active() {
            return Err(HostFault::new(single_error(
                "project.run.busy",
                "Project Host already owns an active run",
            )));
        }
        if snapshot.lineage() != plan.lineage() {
            return Err(HostFault::new(single_error(
                "project.run.lineage-mismatch",
                "Project content and runtime plan lineages do not match",
            )));
        }
        if snapshot.schema_fingerprint() != plan.schema_validation().composition_fingerprint()
            || !snapshot.shares_component_bindings(plan.schema_validation().bindings())
            || !snapshot.matches_asset_domains(&plan)
        {
            return Err(HostFault::new(single_error(
                "project.run.schema-mismatch",
                "Project content and runtime plan schemas do not match",
            )));
        }
        if !snapshot.has_runtime_schema_agreement() {
            return Err(HostFault::new(single_error(
                "project.run.runtime-schema-agreement-missing",
                "Project content was not admitted against the selected runtime schema owners",
            )));
        }
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.checked_add(1).ok_or_else(|| {
            HostFault::new(single_error(
                "project.run.epoch-exhausted",
                "Project Host run identities are exhausted",
            ))
        })?;
        if !self.start_claim.try_begin(epoch) {
            return Err(HostFault::new(single_error(
                "project.run.busy",
                "Project Host already owns an active run",
            )));
        }
        let stamp = RuntimeStartStamp::capture(&snapshot, &plan);
        Ok(RuntimeStartAttempt {
            owner: Arc::clone(&self.start_claim),
            epoch,
            stamp,
            inputs: Some(RuntimeStartInputs {
                snapshot,
                plan,
                startup_scene,
                commands,
                obligations: RuntimeObligationLedger::new(),
                scene_preparation: None,
            }),
        })
    }

    fn prepare_start(
        &mut self,
        attempt: &mut RuntimeStartAttempt,
    ) -> Result<PreparedRuntimeStart, HostFault> {
        let epoch = attempt.epoch;
        let reservation = RuntimeAdmissionReservation::try_acquire().map_err(|_| {
            HostFault::new(single_error(
                "project.run.runtime-capacity-exhausted",
                "Managed runtime capacity is exhausted",
            ))
        })?;
        let inputs = attempt.take_for(self)?;
        let claim = ActiveStartClaim::new(Arc::clone(&self.start_claim), epoch);
        let RuntimeStartInputs {
            snapshot,
            plan,
            startup_scene,
            commands,
            obligations,
            scene_preparation,
        } = inputs;
        let task_report_required = project_requires_task_report(&plan);

        let mut candidate = match plan.plugin_plan().instantiate_runtime_candidate(
            reservation,
            obligations,
            self.cleanup_policy,
        ) {
            Ok(candidate) => candidate,
            Err(owner) => {
                let mut diagnostics =
                    runtime_construction_failure_report(owner.error(), owner.plugin_failure());
                append_plugin_shutdown_diagnostics(&mut diagnostics, owner.plugin_failure_report());
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Construction(owner),
                    task_report_required,
                );
                return Err(HostFault::new(diagnostics));
            }
        };

        let admission_plan = plan.clone();
        let admission_snapshot = snapshot.clone();
        let materialization = Arc::new(OnceLock::new());
        let command_result = Arc::clone(&materialization);
        let admission = candidate.with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                let value = materialize_project_runtime(
                    world,
                    &admission_plan,
                    &admission_snapshot,
                    startup_scene,
                    commands,
                )
                .and_then(|diagnostics| {
                    if let Some((candidate, loader)) = scene_preparation {
                        scene_requests::install_context(
                            world,
                            candidate,
                            admission_plan.clone(),
                            loader,
                        )
                        .map_err(|_| {
                            HostFault::new(single_error(
                                "project.scene.context-install-failed",
                                "Project Scene preparation context could not be installed",
                            ))
                        })?;
                    }
                    Ok(diagnostics)
                });
                assert!(
                    command_result.set(value).is_ok(),
                    "one candidate admission command publishes one result"
                );
            });
        });
        let materialization = Arc::try_unwrap(materialization)
            .ok()
            .and_then(OnceLock::into_inner);
        let Some(materialization) = materialization else {
            let admission_fault = match &admission {
                Ok(outcome) => outcome.post_operation_fault.as_ref(),
                Err(error) => error.fault(),
            };
            let diagnostics = match admission_fault {
                Some(fault) => {
                    self.delivered_runtime_fault = Some(candidate.generation());
                    runtime_scope_failure_report(fault)
                }
                None => single_error(
                    "project.run.candidate-scope-failed",
                    "Project runtime candidate admission failed",
                ),
            };
            self.install_start_cleanup(
                epoch,
                CleanupOwner::Candidate(candidate.begin_retirement()),
                task_report_required,
            );
            return Err(HostFault::new(diagnostics));
        };
        let diagnostics = match materialization {
            Ok(diagnostics)
                if admission
                    .as_ref()
                    .is_ok_and(|outcome| outcome.post_operation_fault.is_none()) =>
            {
                diagnostics
            }
            Err(fault) => {
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Candidate(candidate.begin_retirement()),
                    task_report_required,
                );
                return Err(fault);
            }
            Ok(_) => {
                let diagnostics = match &admission {
                    Ok(outcome) => outcome.post_operation_fault.as_ref(),
                    Err(error) => error.fault(),
                }
                .map(|fault| {
                    self.delivered_runtime_fault = Some(candidate.generation());
                    runtime_scope_failure_report(fault)
                })
                .unwrap_or_else(|| {
                    single_error(
                        "project.run.candidate-scope-failed",
                        "Project runtime candidate admission failed",
                    )
                });
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Candidate(candidate.begin_retirement()),
                    task_report_required,
                );
                return Err(HostFault::new(diagnostics));
            }
        };

        #[cfg(test)]
        let publication_reporter = candidate.fault_reporter();
        let ready = match candidate.complete_startup() {
            Ok(ready) => ready,
            Err(owner) => {
                let diagnostics = runtime_startup_failure_report(owner.fault());
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Startup(owner),
                    task_report_required,
                );
                return Err(HostFault::new(diagnostics));
            }
        };
        // `prepare_start` has exclusive access to the Host, and the active start claim prevents a
        // second start from changing the empty Host slot before coordinated publication.
        debug_assert!(matches!(self.slot, ProjectHostSlot::Empty));

        #[cfg(test)]
        inject_publication_fault_if_armed(&publication_reporter);

        let mut runtime = RuntimePublicationSlot::new();
        match ready.publish_into(&mut runtime) {
            Ok(()) => {}
            Err(owner) => {
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Publication(owner),
                    task_report_required,
                );
                return Err(HostFault::new(single_error(
                    "project.run.publication-faulted",
                    "Project runtime faulted at the publication boundary",
                )));
            }
        }

        if diagnostics.has_errors() {
            unreachable!("scene admission rejects error diagnostics before publication");
        }
        Ok(PreparedRuntimeStart {
            claim,
            snapshot,
            plan,
            runtime,
            diagnostics,
        })
    }

    fn complete_start(
        &mut self,
        attempt: &mut RuntimeStartAttempt,
    ) -> Result<DiagnosticReport, HostFault> {
        let prepared = self.prepare_start(attempt)?;
        Ok(self.commit_prepared_start(prepared))
    }

    fn commit_prepared_start(&mut self, prepared: PreparedRuntimeStart) -> DiagnosticReport {
        let PreparedRuntimeStart {
            claim,
            snapshot,
            plan,
            runtime,
            diagnostics,
        } = prepared;
        let epoch = claim.epoch();
        debug_assert!(matches!(self.slot, ProjectHostSlot::Empty));
        debug_assert!(claim.owner.is_active(epoch));
        self.slot = ProjectHostSlot::Running(Box::new(PublishedProjectRuntime {
            epoch,
            runtime,
            // Freshness compares the admitted startup, even after a legal current-Scene change.
            // Live sources and published Images retain their own decoded generation instead.
            startup_admission_revision: snapshot.revision(),
            _plan: plan,
        }));
        claim.finish();
        diagnostics
    }

    fn abort_prepared_start(&mut self, mut prepared: PreparedRuntimeStart) {
        let epoch = prepared.claim.epoch();
        let task_report_required = project_requires_task_report(&prepared.plan);
        let runtime = prepared
            .runtime
            .take()
            .expect("a prepared runtime start owns one inactive publication");
        drop(prepared);
        self.install_start_cleanup(
            epoch,
            CleanupOwner::Candidate(runtime.begin_retirement()),
            task_report_required,
        );
    }

    fn drive_one_frame(&mut self, real_delta: Duration) -> Result<Option<AppExit>, HostFault> {
        let ProjectHostSlot::Running(published) = &mut self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is not available for a frame",
            )));
        };
        let expected = published._plan.schema_validation().bindings().clone();
        let generation = published.runtime().generation();
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        let outcome = published.runtime_mut().drive(real_delta).map_err(|error| {
            self.delivered_runtime_fault = Some(generation);
            HostFault::new(runtime_scope_failure_report(error.fault()))
        })?;
        let frame = outcome.frame().ok_or_else(|| {
            HostFault::new(single_error(
                "project.run.frame-unavailable",
                "Project runtime did not execute a managed frame",
            ))
        })?;
        let exit = frame.exit;
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        Ok(exit)
    }

    fn drive_one_fixed_tick(&mut self) -> Result<Option<AppExit>, HostFault> {
        let ProjectHostSlot::Running(published) = &mut self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is not available for a fixed tick",
            )));
        };

        let expected = published._plan.schema_validation().bindings().clone();
        let generation = published.runtime().generation();
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        let runtime = published.runtime_mut();
        if let Some(fault) = runtime.fault() {
            self.delivered_runtime_fault = Some(generation);
            return Err(HostFault::new(runtime_scope_failure_report(fault)));
        }
        if runtime.state() == RuntimeState::Running {
            let RuntimeControlRequestResult::Accepted(pause) =
                runtime.request_control(RuntimeControl::Pause)
            else {
                return Err(HostFault::new(single_error(
                    "project.run.pause-rejected",
                    "Project runtime rejected exact-step preparation",
                )));
            };
            runtime.drive(Duration::ZERO).map_err(|error| {
                self.delivered_runtime_fault = Some(generation);
                HostFault::new(runtime_scope_failure_report(error.fault()))
            })?;
            if runtime.state() != RuntimeState::Paused
                || runtime.control_status(pause) != Some(RuntimeControlStatus::Applied)
            {
                return Err(HostFault::new(single_error(
                    "project.run.pause-failed",
                    "Project runtime could not enter exact-step mode",
                )));
            }
        }
        if runtime.state() != RuntimeState::Paused {
            return Err(HostFault::new(single_error(
                "project.run.exact-step-unavailable",
                "Project runtime is not available for exact fixed stepping",
            )));
        }

        let RuntimeControlRequestResult::Accepted(step) =
            runtime.request_control(RuntimeControl::StepFixedTick)
        else {
            return Err(HostFault::new(single_error(
                "project.run.exact-step-rejected",
                "Project runtime rejected an exact fixed step",
            )));
        };
        let outcome = runtime.drive(Duration::ZERO).map_err(|error| {
            self.delivered_runtime_fault = Some(generation);
            HostFault::new(runtime_scope_failure_report(error.fault()))
        })?;
        let actual = outcome.frame().map_or(0, |frame| frame.status.fixed_steps);
        if actual != 1
            || runtime.state() != RuntimeState::Paused
            || runtime.control_status(step) != Some(RuntimeControlStatus::Applied)
        {
            return Err(HostFault::new(single_error_with_u64(
                "project.run.fixed-step-mismatch",
                "Project runtime did not execute exactly one fixed tick",
                "actual",
                u64::from(actual),
            )));
        }
        let exit = outcome.frame().and_then(|frame| frame.exit);
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        Ok(exit)
    }

    fn capture_outcome<O>(&mut self) -> Result<O, HostFault>
    where
        O: Resource + Clone,
    {
        self.outcome_resource::<O>().cloned()
    }

    fn capture_outcome_if<O>(
        &mut self,
        predicate: &(dyn Fn(&O) -> bool + Send + 'static),
    ) -> Result<Option<O>, HostFault>
    where
        O: Resource + Clone,
    {
        let outcome = self.outcome_resource::<O>()?;
        Ok(predicate(outcome).then(|| outcome.clone()))
    }

    fn outcome_resource<O>(&mut self) -> Result<&O, HostFault>
    where
        O: Resource,
    {
        let ProjectHostSlot::Running(published) = &self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is not available for outcome capture",
            )));
        };
        if let Some(fault) = published.runtime().fault() {
            self.delivered_runtime_fault = Some(published.runtime().generation());
            return Err(HostFault::new(runtime_scope_failure_report(fault)));
        }
        published
            .runtime()
            .world()
            .get_resource::<O>()
            .ok_or_else(|| {
                HostFault::new(single_error(
                    "project.run.outcome-missing",
                    "Project runtime did not publish its typed outcome",
                ))
            })
    }

    fn stop_running(&mut self) -> Result<(), HostFault> {
        let slot = std::mem::replace(&mut self.slot, ProjectHostSlot::Empty);
        let ProjectHostSlot::Running(mut published) = slot else {
            self.slot = slot;
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is not available for bounded stop",
            )));
        };
        let task_report_required = project_requires_task_report(&published._plan);
        if matches!(
            published
                .runtime_mut()
                .request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Rejected(_)
        ) {
            let epoch = published.epoch;
            self.install_cleanup(
                epoch,
                CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
                task_report_required,
            );
            return Err(HostFault::new(single_error(
                "project.run.stop-rejected",
                "Project runtime rejected bounded stop",
            )));
        }

        // RuntimeInstance applies Stop on one drive and polls close participants on the next. Do
        // exactly those two bounded transitions; unfinished work is retained for a later caller
        // drive instead of spinning the Host thread until a wall-clock deadline.
        for _ in 0..2 {
            if published.runtime_mut().drive(Duration::ZERO).is_err() {
                let epoch = published.epoch;
                self.install_cleanup(
                    epoch,
                    CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
                    task_report_required,
                );
                return Err(HostFault::new(single_error(
                    "project.run.stop-drive-failed",
                    "Project runtime stop drive failed",
                )));
            }
            if matches!(
                published.runtime().state(),
                RuntimeState::Stopped | RuntimeState::CloseIncomplete
            ) {
                break;
            }
        }

        match published.runtime().state() {
            RuntimeState::Stopped => {
                let mut diagnostics = DiagnosticReport::default();
                if published.runtime().close_evidence().has_terminal_failure() {
                    diagnostics.extend(cleanup_failed_report(
                        published.runtime().close_evidence(),
                        published.runtime().plugin_failure_report(),
                    ));
                }
                if let Some(fault) = published.runtime().fault()
                    && self.delivered_runtime_fault != Some(published.runtime().generation())
                {
                    self.delivered_runtime_fault = Some(published.runtime().generation());
                    diagnostics.extend(runtime_scope_failure_report(fault));
                }
                if diagnostics.has_errors() {
                    Err(HostFault::new(diagnostics))
                } else {
                    Ok(())
                }
            }
            RuntimeState::Stopping | RuntimeState::CloseIncomplete => {
                let epoch = published.epoch;
                let mut diagnostics = cleanup_incomplete_report(
                    published.runtime().close_evidence(),
                    published.runtime().plugin_failure_report(),
                );
                if let Some(fault) = published.runtime().fault()
                    && self.delivered_runtime_fault != Some(published.runtime().generation())
                {
                    self.delivered_runtime_fault = Some(published.runtime().generation());
                    diagnostics.extend(runtime_scope_failure_report(fault));
                }
                if published.runtime().close_evidence().has_terminal_failure() {
                    diagnostics.extend(single_error(
                        "project.run.cleanup-failed",
                        "Project runtime cleanup reported a terminal failure",
                    ));
                }
                self.install_cleanup(
                    epoch,
                    CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
                    task_report_required,
                );
                Err(HostFault::new(diagnostics))
            }
            RuntimeState::Running
            | RuntimeState::Paused
            | RuntimeState::Faulted
            | RuntimeState::Stepping => {
                let epoch = published.epoch;
                self.install_cleanup(
                    epoch,
                    CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
                    task_report_required,
                );
                Err(HostFault::new(single_error(
                    "project.run.stop-drive-failed",
                    "Project runtime stop drive failed",
                )))
            }
        }
    }

    fn retire_running(&mut self) {
        let slot = std::mem::replace(&mut self.slot, ProjectHostSlot::Empty);
        if let ProjectHostSlot::Running(mut published) = slot {
            let epoch = published.epoch;
            let task_report_required = project_requires_task_report(&published._plan);
            self.install_cleanup(
                epoch,
                CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
                task_report_required,
            );
        } else {
            self.slot = slot;
        }
    }

    fn install_start_cleanup(
        &mut self,
        epoch: u64,
        owner: CleanupOwner,
        task_report_required: bool,
    ) {
        self.install_cleanup(epoch, owner, task_report_required);
    }

    fn install_cleanup(&mut self, epoch: u64, owner: CleanupOwner, task_report_required: bool) {
        debug_assert!(matches!(self.slot, ProjectHostSlot::Empty));
        let task_report_required = match &owner {
            // Construction can fail before acquisition, or after a later hook removes the facade.
            // Capture the actual obligation now, before polling removes completed participants.
            CleanupOwner::Construction(owner) => owner.close_evidence().is_some_and(|evidence| {
                evidence.pending_participants().iter().any(|pending| {
                    pending.participant() == nara_tasks::TASK_POOLS_CLOSE_PARTICIPANT
                })
            }),
            _ => task_report_required,
        };
        // Even immediate completion has observations that its consumer must receive once.
        self.slot = ProjectHostSlot::Cleaning {
            epoch,
            owner: Box::new(owner),
            task_report_required,
        };
    }

    fn has_cleanup_owner(&self) -> bool {
        matches!(self.slot, ProjectHostSlot::Cleaning { .. })
    }

    fn drive_cleanup_once(&mut self) -> CleanupDriveOutcome {
        let slot = std::mem::replace(&mut self.slot, ProjectHostSlot::Empty);
        let ProjectHostSlot::Cleaning {
            epoch,
            mut owner,
            task_report_required,
        } = slot
        else {
            self.slot = slot;
            return CleanupDriveOutcome::Complete {
                failed: false,
                diagnostics: DiagnosticReport::default(),
                observation: None,
            };
        };
        let state = if owner.state() == RuntimeCandidateRetirementState::Retired {
            RuntimeCandidateRetirementState::Retired
        } else {
            owner.drive()
        };
        match state {
            RuntimeCandidateRetirementState::Retired => {
                let observation = owner.close_observation(epoch, task_report_required);
                let mut diagnostics = owner.completion_failure_report(self.delivered_runtime_fault);
                if observation.task_report_invalid() {
                    diagnostics.extend(single_error(
                        "project.run.task-shutdown-report-invalid",
                        "Project runtime completed cleanup without its required complete task report",
                    ));
                }
                CleanupDriveOutcome::Complete {
                    failed: diagnostics.has_errors(),
                    diagnostics,
                    observation: Some(observation),
                }
            }
            RuntimeCandidateRetirementState::Retiring => {
                self.slot = ProjectHostSlot::Cleaning {
                    epoch,
                    owner,
                    task_report_required,
                };
                CleanupDriveOutcome::Retiring
            }
            RuntimeCandidateRetirementState::RetirementIncomplete => {
                self.slot = ProjectHostSlot::Cleaning {
                    epoch,
                    owner,
                    task_report_required,
                };
                CleanupDriveOutcome::RetirementIncomplete
            }
        }
    }
}

fn project_requires_task_report(plan: &RuntimePlan) -> bool {
    plan.plugin_plan()
        .entries()
        .iter()
        .any(|entry| entry.plugin_id() == nara_tasks::TASK_PLUGIN_ID)
}

fn materialize_project_runtime(
    world: &mut World,
    plan: &RuntimePlan,
    snapshot: &ProjectContentSnapshot,
    startup_scene: SceneSource,
    commands: Vec<GameplayCommandSubmission>,
) -> Result<DiagnosticReport, HostFault> {
    verify_runtime_registry(world, plan)?;
    if !world.contains_resource::<GameplayCommandQueue>() {
        return Err(HostFault::new(single_error(
            "project.run.command-queue-missing",
            "Project runtime has no semantic command queue",
        )));
    }
    world.insert_resource(plan.settings().runtime.runtime_time_settings());
    world.insert_resource(plan.settings().runtime.fixed_time());

    let diagnostics = materialize_startup_scene(world, startup_scene)
        .map_err(|error| HostFault::new(error.diagnostics().clone()))?;
    content_residency::publish_startup(world, snapshot)?;
    let faults = world
        .get_resource::<RuntimeFaultReporter>()
        .cloned()
        .ok_or_else(|| {
            HostFault::new(single_error(
                "project.run.fault-reporter-missing",
                "Project runtime fault reporter is unavailable",
            ))
        })?;
    {
        let mut queue = world
            .get_resource_mut::<GameplayCommandQueue>()
            .expect("the queue presence was checked before scene allocation");
        for command in commands {
            queue.submit(command).map_err(|_| {
                faults.report(RuntimeFault::engine(
                    RuntimeFaultKind::GameplayLifecycle,
                    "nara.project-host.command-submit",
                ));
                HostFault::new(single_error(
                    "project.run.command-rejected",
                    "Project semantic control input was rejected",
                ))
            })?;
        }
    }
    verify_runtime_registry(world, plan)?;
    Ok(diagnostics)
}

fn publish_snapshot_assets(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    publish_snapshot_assets_with_database(world, snapshot, snapshot.asset_database())
}

fn publish_snapshot_assets_with_database(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
    database: &nara_asset::ProjectAssetDatabase,
) -> Result<(), HostFault> {
    if snapshot.images().is_empty() && snapshot.tilesets().is_empty() {
        return Ok(());
    }
    if !world.contains_resource::<AssetServer>()
        || !world.contains_resource::<Assets<ImageAsset>>()
        || !world.contains_resource::<AssetStates>()
        || !world.contains_resource::<AssetEvents>()
    {
        return Err(HostFault::new(single_error(
            "project.run.image-resources-missing",
            "Project runtime image resources are unavailable",
        )));
    }
    if !snapshot.tilesets().is_empty() && !world.contains_resource::<Assets<TileSet>>() {
        return Err(HostFault::new(single_error(
            "project.run.tileset-resources-missing",
            "Project runtime TileSet resources are unavailable",
        )));
    }

    let mut publications = Vec::with_capacity(snapshot.images().len());
    {
        let mut server = world.resource_mut::<AssetServer>();
        for (index, content) in snapshot.images().iter().enumerate() {
            let source = content.image().source();
            let record = AssetRecord::new(
                source.stable_id(),
                source.path().clone(),
                AssetSourceKind::Image,
            );
            let handle = server.reserve_record::<ImageAsset>(&record).map_err(|_| {
                HostFault::new(single_error(
                    "project.run.image-identity-failed",
                    "Project runtime image identity could not be reserved",
                ))
            })?;
            let image = snapshot.share_image_for_consumer(index).ok_or_else(|| {
                HostFault::new(single_error(
                    "project.run.image-snapshot-invalid",
                    "Project runtime image snapshot is inconsistent",
                ))
            })?;
            publications.push((
                handle,
                image,
                source.source_hash(),
                source.artifact().key().digest(),
            ));
        }
    }

    world.resource_scope(
        |world, mut images: Mut<'_, Assets<ImageAsset>>| -> Result<(), HostFault> {
            world.resource_scope(
                |world, mut states: Mut<'_, AssetStates>| -> Result<(), HostFault> {
                    let mut events = world.resource_mut::<AssetEvents>();
                    for (handle, image, source_hash, import_hash) in publications {
                        images
                            .commit_loaded(
                                handle,
                                image,
                                &mut states,
                                &mut events,
                                Some(source_hash),
                                Some(import_hash),
                            )
                            .map_err(|_| {
                                HostFault::new(single_error(
                                    "project.run.image-publication-failed",
                                    "Project runtime image publication failed",
                                ))
                            })?;
                    }
                    Ok(())
                },
            )
        },
    )?;
    world.insert_resource(database.clone());
    publish_snapshot_tilesets(world, snapshot)?;
    Ok(())
}

fn publish_snapshot_tilesets(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    if snapshot.tilesets().is_empty() {
        return Ok(());
    }
    if !world.contains_resource::<nara_tilemap::TileSetAssets>() {
        return Err(HostFault::new(single_error(
            "project.run.tileset-owner-missing",
            "Project runtime TileSet publication owner is unavailable",
        )));
    }
    let mut access =
        nara_ecs::bevy_ecs::system::SystemState::<nara_tilemap::TileSetContent>::new(world);
    let mut tilesets = access.get_mut(world).map_err(|_| {
        HostFault::new(single_error(
            "project.run.tileset-resources-missing",
            "Project runtime TileSet resources are unavailable",
        ))
    })?;
    let mut handles = Vec::with_capacity(snapshot.tilesets().len());
    for content in snapshot.tilesets() {
        let record = AssetRecord::new(
            content.stable_id(),
            content.path().clone(),
            AssetSourceKind::Other(String::from("tileset")),
        );
        let handle = tilesets
            .request(
                record,
                content.source().clone(),
                SourceHash::from_digest(*content.source_hash().as_bytes()),
            )
            .map_err(|_| {
                HostFault::new(single_error(
                    "project.run.tileset-request-failed",
                    "Project runtime TileSet source could not be admitted",
                ))
            })?;
        handles.push(handle);
    }
    tilesets.reconcile();
    if handles.into_iter().any(|handle| {
        tilesets.error(handle).is_some()
            || !tilesets
                .get(handle)
                .is_some_and(|tileset| tileset.image_content().is_some())
    }) {
        return Err(HostFault::new(single_error(
            "project.run.tileset-publication-failed",
            "Project runtime TileSet publication failed",
        )));
    }
    Ok(())
}

fn verify_runtime_registry(world: &World, plan: &RuntimePlan) -> Result<(), HostFault> {
    verify_runtime_component_bindings(world, plan.schema_validation().bindings())
}

fn verify_runtime_component_bindings(
    world: &World,
    expected: &SceneComponentBindings,
) -> Result<(), HostFault> {
    let registry = component_registry(world).ok_or_else(|| {
        HostFault::new(single_error(
            "project.run.registry-missing",
            "Project runtime component registry is missing",
        ))
    })?;
    if !registry.shares_snapshot(expected.native_snapshot()) {
        return Err(HostFault::new(single_error(
            "project.run.registry-mismatch",
            "Project runtime component registry does not match its admitted behavior snapshot",
        )));
    }
    validate_component_registry_authority(world).map_err(|_| {
        HostFault::new(single_error(
            "project.run.registry-authority-invalid",
            "Project runtime component registry authority is invalid",
        ))
    })?;
    let bindings = scene_component_bindings(world).ok_or_else(|| {
        HostFault::new(single_error(
            "project.run.component-bindings-missing",
            "Project runtime component bindings are missing",
        ))
    })?;
    if !bindings.same_definitions(expected) {
        return Err(HostFault::new(single_error(
            "project.run.component-bindings-mismatch",
            "Project runtime component bindings do not match their admitted definitions",
        )));
    }
    validate_scene_component_bindings_authority(world).map_err(|_| {
        HostFault::new(single_error(
            "project.run.component-bindings-authority-invalid",
            "Project runtime component bindings authority is invalid",
        ))
    })?;
    Ok(())
}

fn verify_published_runtime_registry(
    world: &World,
    expected: &SceneComponentBindings,
) -> Result<(), HostFault> {
    let result = verify_runtime_component_bindings(world, expected);
    if result.is_err()
        && let Some(reporter) = world.get_resource::<RuntimeFaultReporter>()
    {
        reporter.report(RuntimeFault::engine(
            RuntimeFaultKind::RuntimeAuthority,
            "nara.project-host.component-registry-authority",
        ));
    }
    result
}

#[derive(Debug)]
struct HostFault {
    diagnostics: DiagnosticReport,
}

impl HostFault {
    const fn new(diagnostics: DiagnosticReport) -> Self {
        Self { diagnostics }
    }
}

fn runtime_plan_failure_report(error: &RuntimePlanError) -> DiagnosticReport {
    match error {
        RuntimePlanError::Composition(error) => composition_failure_report(error),
        RuntimePlanError::PluginPlan(error) => plugin_plan_failure_report(error),
    }
}

fn composition_failure_report(error: &CompositionError) -> DiagnosticReport {
    let reason = match error {
        CompositionError::ProjectLineageMismatch => "project-lineage-mismatch",
        CompositionError::UnsupportedDefaultConfiguration { .. } => {
            "unsupported-default-configuration"
        }
        CompositionError::AdvancedProbeProvidesSchema { .. } => "advanced-probe-provides-schema",
        CompositionError::UnknownProductCapability { .. } => "unknown-product-capability",
        CompositionError::UncompiledProductCapability { .. } => "uncompiled-product-capability",
        CompositionError::UnrequestedProductCapability { .. } => "unrequested-product-capability",
        CompositionError::UnrequestedProductCapabilityAlternatives { .. } => {
            "unrequested-product-capability-alternatives"
        }
        CompositionError::MissingSchemaProvider { .. } => "missing-schema-provider",
        CompositionError::DivergentSchemaProvider { .. } => "divergent-schema-provider",
        CompositionError::DivergentSchemaOwner { .. } => "divergent-schema-owner",
        CompositionError::ConflictingSchemaOwnerClaim { .. } => "conflicting-schema-owner-claim",
        CompositionError::SchemaProviderRejected { .. } => "schema-provider-rejected",
        CompositionError::SchemaProviderPanicked { .. } => "schema-provider-panicked",
        CompositionError::SchemaFreezeRejected { .. } => "schema-freeze-rejected",
        CompositionError::DataContributionNotSelected { .. } => "data-contribution-not-selected",
        CompositionError::SceneBindingsRejected { .. } => "scene-bindings-rejected",
        CompositionError::SchemaAuthorityPublicationFailed => "schema-authority-conflict",
    };
    let mut diagnostic = failure_diagnostic(
        "project.run.composition-invalid",
        "Project runtime composition could not be admitted",
        reason,
    );
    match error {
        CompositionError::AdvancedProbeProvidesSchema { plugin }
        | CompositionError::DataContributionNotSelected { plugin }
        | CompositionError::UnsupportedDefaultConfiguration { plugin } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
        }
        CompositionError::UnknownProductCapability { plugin, capability } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "capability", capability.as_str());
        }
        CompositionError::UncompiledProductCapability { plugin, capability }
        | CompositionError::UnrequestedProductCapability { plugin, capability } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "capability", capability.as_str());
        }
        CompositionError::UnrequestedProductCapabilityAlternatives {
            plugin,
            alternatives,
        } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(
                diagnostic,
                "alternatives",
                &normalized_product_capability_alternatives(*alternatives),
            );
        }
        CompositionError::MissingSchemaProvider { provider }
        | CompositionError::DivergentSchemaProvider { provider }
        | CompositionError::SchemaProviderRejected { provider, .. }
        | CompositionError::SchemaProviderPanicked { provider } => {
            diagnostic = attach_identifier(diagnostic, "provider", provider.as_str());
        }
        CompositionError::DivergentSchemaOwner { owner } => {
            diagnostic = attach_identifier(diagnostic, "owner", owner.as_str());
        }
        CompositionError::ConflictingSchemaOwnerClaim {
            component_id,
            first_owner,
            second_owner,
        } => {
            diagnostic = attach_identifier(diagnostic, "component", component_id.as_str());
            diagnostic = attach_identifier(diagnostic, "first_owner", first_owner.as_str());
            diagnostic = attach_identifier(diagnostic, "second_owner", second_owner.as_str());
        }
        CompositionError::ProjectLineageMismatch
        | CompositionError::SchemaFreezeRejected { .. }
        | CompositionError::SceneBindingsRejected { .. }
        | CompositionError::SchemaAuthorityPublicationFailed => {}
    }
    single_diagnostic(diagnostic)
}

fn normalized_product_capability_alternatives(
    alternatives: nara_project::ProductCapabilitySet,
) -> String {
    alternatives
        .iter()
        .map(nara_project::ProductCapability::as_str)
        .collect::<Vec<_>>()
        .join("/")
}

fn plugin_plan_failure_report(error: &PluginPlanError) -> DiagnosticReport {
    let reason = match error {
        PluginPlanError::DeclarationPanicked => "declaration-panicked",
        PluginPlanError::GroupPanicked { .. } => "group-panicked",
        PluginPlanError::GroupCycle { .. } => "group-cycle",
        PluginPlanError::DivergentGroup { .. } => "divergent-group",
        PluginPlanError::SlotPluginMismatch { .. } => "slot-plugin-mismatch",
        PluginPlanError::MissingEditTarget => "missing-edit-target",
        PluginPlanError::MissingSlotEditTarget { .. } => "missing-slot-edit-target",
        PluginPlanError::AmbiguousEditTarget => "ambiguous-edit-target",
        PluginPlanError::RequiredSlotDisabled { .. } => "required-slot-disabled",
        PluginPlanError::DuplicateSlot { .. } => "duplicate-slot",
        PluginPlanError::DivergentSlotContract { .. } => "divergent-slot-contract",
        PluginPlanError::ActiveSlotDisabled { .. } => "active-slot-disabled",
        PluginPlanError::DuplicatePlugin { .. } => "duplicate-plugin",
        PluginPlanError::DivergentDefinition { .. } => "divergent-definition",
        PluginPlanError::MissingPlugin { .. } => "missing-plugin",
        PluginPlanError::MissingSchemaProvider { .. } => "missing-schema-provider",
        PluginPlanError::Conflict { .. } => "plugin-conflict",
        PluginPlanError::OrderingCycle { .. } => "ordering-cycle",
        PluginPlanError::ImmutablePrefix => "immutable-prefix",
    };
    let mut diagnostic = failure_diagnostic(
        "project.run.plugin-plan-invalid",
        "Project plugin plan could not be resolved",
        reason,
    );
    match error {
        PluginPlanError::GroupPanicked { group } | PluginPlanError::DivergentGroup { group } => {
            diagnostic = attach_identifier(diagnostic, "group", group.as_str());
        }
        PluginPlanError::GroupCycle { chain } => {
            if let Some(group) = chain.first() {
                diagnostic = attach_identifier(diagnostic, "first-group", group.as_str());
            }
            diagnostic = attach_u64(diagnostic, "group-count", chain.len());
        }
        PluginPlanError::SlotPluginMismatch {
            slot,
            expected,
            actual,
        } => {
            diagnostic = attach_identifier(diagnostic, "slot", slot.as_str());
            diagnostic = attach_identifier(diagnostic, "expected-plugin", expected.as_str());
            diagnostic = attach_identifier(diagnostic, "actual-plugin", actual.as_str());
        }
        PluginPlanError::MissingSlotEditTarget { slot }
        | PluginPlanError::RequiredSlotDisabled { slot }
        | PluginPlanError::DivergentSlotContract { slot }
        | PluginPlanError::ActiveSlotDisabled { slot } => {
            diagnostic = attach_identifier(diagnostic, "slot", slot.as_str());
        }
        PluginPlanError::DuplicateSlot {
            slot,
            first,
            duplicate,
        } => {
            diagnostic = attach_identifier(diagnostic, "slot", slot.as_str());
            diagnostic = attach_identifier(diagnostic, "first-plugin", first.as_str());
            diagnostic = attach_identifier(diagnostic, "duplicate-plugin", duplicate.as_str());
        }
        PluginPlanError::DuplicatePlugin { plugin }
        | PluginPlanError::DivergentDefinition { plugin } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
        }
        PluginPlanError::MissingPlugin { plugin, required }
        | PluginPlanError::Conflict {
            plugin,
            conflict: required,
        } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "related-plugin", required.as_str());
        }
        PluginPlanError::MissingSchemaProvider { plugin, required } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "provider", required.as_str());
        }
        PluginPlanError::OrderingCycle { plugins } => {
            if let Some(plugin) = plugins.first() {
                diagnostic = attach_identifier(diagnostic, "first-plugin", plugin.as_str());
            }
            diagnostic = attach_u64(diagnostic, "plugin-count", plugins.len());
        }
        PluginPlanError::DeclarationPanicked
        | PluginPlanError::MissingEditTarget
        | PluginPlanError::AmbiguousEditTarget
        | PluginPlanError::ImmutablePrefix => {}
    }
    single_diagnostic(diagnostic)
}

fn runtime_plan_selected_report(plan: &RuntimePlan) -> DiagnosticReport {
    let fingerprint = format!("{:?}", plan.plugin_plan().fingerprint());
    let fingerprint = PublicDiagnosticIdentifier::new(&fingerprint)
        .expect("a plugin plan fingerprint is a valid public identifier");
    let diagnostic = Diagnostic::info(
        diagnostic_code("project.run.plan-selected"),
        safe_summary("Project runtime plan was selected"),
    )
    .try_with_field(DiagnosticField::public_identifier(
        field_key("plugin-plan-fingerprint"),
        fingerprint,
    ))
    .expect("the selected-plan diagnostic has one bounded field");
    single_diagnostic(diagnostic)
}

fn admission_failure_report(error: &RuntimeAdmissionError) -> DiagnosticReport {
    let reason = match error {
        RuntimeAdmissionError::AppNotReady { .. } => "app-not-ready",
        RuntimeAdmissionError::GenerationExhausted => "generation-exhausted",
        RuntimeAdmissionError::Obligation(_) => "obligation-invalid",
    };
    single_diagnostic(failure_diagnostic(
        "project.run.admission-failed",
        "Project runtime candidate admission failed",
        reason,
    ))
}

fn runtime_scope_failure_report(fault: &RuntimeFault) -> DiagnosticReport {
    let (code, summary) = if fault.kind() == RuntimeFaultKind::RuntimeAuthority {
        (
            "project.run.runtime-authority-invalid",
            "Project runtime authority is invalid",
        )
    } else {
        (
            "project.run.runtime-faulted",
            "Project runtime reported a fault before product completion",
        )
    };
    runtime_fault_report(fault, code, summary)
}

fn runtime_fault_report(
    fault: &RuntimeFault,
    code: &'static str,
    summary: &'static str,
) -> DiagnosticReport {
    let diagnostic = runtime_fault_diagnostic(fault.detail(), code, summary);
    let diagnostic = attach_identifier(
        diagnostic,
        "fault-kind",
        runtime_fault_kind_id(fault.kind()),
    );
    let diagnostic = attach_identifier(diagnostic, "fault-source", fault.source());
    single_diagnostic(diagnostic)
}

pub(super) const fn runtime_fault_kind_id(kind: RuntimeFaultKind) -> &'static str {
    match kind {
        RuntimeFaultKind::AppFrame => "app-frame",
        RuntimeFaultKind::FaultReporterAuthority => "fault-reporter-authority",
        RuntimeFaultKind::RuntimeAuthority => "runtime-authority",
        RuntimeFaultKind::System => "system",
        RuntimeFaultKind::RunCondition => "run-condition",
        RuntimeFaultKind::Command => "command",
        RuntimeFaultKind::Observer => "observer",
        RuntimeFaultKind::GameplayLifecycle => "gameplay-lifecycle",
        RuntimeFaultKind::LocalIntentLoss => "local-intent-loss",
        RuntimeFaultKind::RequiredTask => "required-task",
        RuntimeFaultKind::RequiredService => "required-service",
    }
}

fn runtime_startup_failure_report(fault: &RuntimeFault) -> DiagnosticReport {
    if fault.kind() == RuntimeFaultKind::RuntimeAuthority {
        return runtime_authority_invalid_report();
    }
    let diagnostic = runtime_fault_diagnostic(
        fault.detail(),
        "project.run.startup-failed",
        "Project runtime startup failed",
    );
    let diagnostic = attach_identifier(
        diagnostic,
        "fault-kind",
        runtime_fault_kind_id(fault.kind()),
    );
    let diagnostic = attach_identifier(diagnostic, "fault-source", fault.source());
    single_diagnostic(diagnostic)
}

fn runtime_fault_diagnostic(
    detail: Option<RuntimeFaultDetail>,
    fallback_code: &'static str,
    fallback_summary: &'static str,
) -> Diagnostic {
    let Some(detail) = detail else {
        return Diagnostic::error(
            diagnostic_code(fallback_code),
            safe_summary(fallback_summary),
        );
    };
    let Ok(code) = DiagnosticCode::new(detail.code()) else {
        return Diagnostic::error(
            diagnostic_code(fallback_code),
            safe_summary(fallback_summary),
        );
    };
    let Ok(summary) = SafeSummary::new(detail.summary()) else {
        return Diagnostic::error(
            diagnostic_code(fallback_code),
            safe_summary(fallback_summary),
        );
    };
    let Ok(origin) = PublicDiagnosticIdentifier::new(detail.origin()) else {
        return Diagnostic::error(
            diagnostic_code(fallback_code),
            safe_summary(fallback_summary),
        );
    };
    Diagnostic::error(code, summary)
        .try_with_field(DiagnosticField::public_identifier(
            field_key("fault-origin"),
            origin,
        ))
        .expect("one classified runtime fault has one bounded origin")
}

fn runtime_authority_invalid_report() -> DiagnosticReport {
    single_error(
        "project.run.runtime-authority-invalid",
        "Project runtime authority is invalid",
    )
}

fn cleanup_incomplete_report(
    evidence: &RuntimeCloseEvidence,
    plugins: Option<&PluginFailureReport>,
) -> DiagnosticReport {
    let mut report = single_warning(
        "project.run.cleanup-incomplete",
        "Project runtime cleanup is incomplete",
    );
    append_close_cause_diagnostics(&mut report, evidence);
    append_plugin_shutdown_diagnostics(&mut report, plugins);
    report
}

fn cleanup_failed_report(
    evidence: &RuntimeCloseEvidence,
    plugins: Option<&PluginFailureReport>,
) -> DiagnosticReport {
    let mut report = single_error(
        "project.run.cleanup-failed",
        "Project runtime cleanup reported a terminal failure",
    );
    append_close_cause_diagnostics(&mut report, evidence);
    append_plugin_shutdown_diagnostics(&mut report, plugins);
    report
}

fn append_close_cause_diagnostics(report: &mut DiagnosticReport, evidence: &RuntimeCloseEvidence) {
    for pending in evidence.pending_participants() {
        let diagnostic = Diagnostic::warning(
            diagnostic_code("project.run.cleanup-participant-pending"),
            safe_summary("Project runtime cleanup participant remains unfinished"),
        );
        let diagnostic =
            attach_identifier(diagnostic, "participant", pending.participant().as_str());
        report.push(attach_identifier(
            diagnostic,
            "phase",
            close_phase_id(pending.phase()),
        ));
    }
    for cause in evidence.causes() {
        let diagnostic = match cause {
            RuntimeCloseCause::PluginShutdown => Diagnostic::error(
                diagnostic_code("project.run.cleanup-plugin-shutdown"),
                safe_summary("Project plugin shutdown failed"),
            ),
            RuntimeCloseCause::ParticipantError {
                participant,
                phase,
                code,
                disposition,
            } => {
                let diagnostic = match disposition {
                    RuntimeCloseErrorDisposition::Retryable => Diagnostic::warning(
                        diagnostic_code("project.run.cleanup-participant-error"),
                        safe_summary("Project runtime cleanup participant can be retried"),
                    ),
                    RuntimeCloseErrorDisposition::Terminal => Diagnostic::error(
                        diagnostic_code("project.run.cleanup-participant-error"),
                        safe_summary("Project runtime cleanup participant failed terminally"),
                    ),
                };
                let diagnostic = attach_identifier(diagnostic, "participant", participant.as_str());
                let diagnostic = attach_identifier(diagnostic, "phase", close_phase_id(*phase));
                let diagnostic = attach_identifier(diagnostic, "failure-code", code);
                attach_identifier(
                    diagnostic,
                    "disposition",
                    close_disposition_id(*disposition),
                )
            }
            RuntimeCloseCause::DeadlineExceeded => Diagnostic::warning(
                diagnostic_code("project.run.cleanup-deadline-exceeded"),
                safe_summary("Project runtime cleanup exceeded its bounded deadline"),
            ),
        };
        report.push(diagnostic);
    }
}

fn failure_diagnostic(
    code: &'static str,
    summary: &'static str,
    reason: &'static str,
) -> Diagnostic {
    attach_identifier(
        Diagnostic::error(diagnostic_code(code), safe_summary(summary)),
        "reason",
        reason,
    )
}

fn attach_identifier(diagnostic: Diagnostic, key: &'static str, value: &str) -> Diagnostic {
    let field = PublicDiagnosticIdentifier::new(value).map_or_else(
        |_| DiagnosticField::sensitive(field_key(key)),
        |value| DiagnosticField::public_identifier(field_key(key), value),
    );
    diagnostic
        .try_with_field(field)
        .expect("project Host diagnostics use unique bounded fields")
}

fn attach_u64(diagnostic: Diagnostic, key: &'static str, value: usize) -> Diagnostic {
    diagnostic
        .try_with_field(DiagnosticField::public_u64(
            field_key(key),
            u64::try_from(value).unwrap_or(u64::MAX),
        ))
        .expect("project Host diagnostics use unique bounded fields")
}

const fn plugin_hook_id(hook: PluginHook) -> &'static str {
    match hook {
        PluginHook::Preflight => "preflight",
        PluginHook::Build => "build",
        PluginHook::Finish => "finish",
        PluginHook::Shutdown => "shutdown",
    }
}

const fn plugin_mutation_id(mutation: PluginHookMutation) -> &'static str {
    match mutation {
        PluginHookMutation::PluginMembership => "plugin-membership",
    }
}

const fn close_phase_id(phase: RuntimeCloseParticipantPhase) -> &'static str {
    match phase {
        RuntimeCloseParticipantPhase::Begin => "begin",
        RuntimeCloseParticipantPhase::Poll => "poll",
    }
}

const fn close_disposition_id(disposition: RuntimeCloseErrorDisposition) -> &'static str {
    match disposition {
        RuntimeCloseErrorDisposition::Retryable => "retryable",
        RuntimeCloseErrorDisposition::Terminal => "terminal",
    }
}

fn single_error(code: &'static str, summary: &'static str) -> DiagnosticReport {
    single_diagnostic(Diagnostic::error(
        diagnostic_code(code),
        safe_summary(summary),
    ))
}

fn single_warning(code: &'static str, summary: &'static str) -> DiagnosticReport {
    single_diagnostic(Diagnostic::warning(
        diagnostic_code(code),
        safe_summary(summary),
    ))
}

fn single_warning_with_project_relative(
    code: &'static str,
    summary: &'static str,
    key: &'static str,
    value: &str,
) -> DiagnosticReport {
    let Ok(field) = DiagnosticField::project_relative(field_key(key), value) else {
        return single_warning(code, summary);
    };
    let Ok(diagnostic) =
        Diagnostic::warning(diagnostic_code(code), safe_summary(summary)).try_with_field(field)
    else {
        return single_warning(code, summary);
    };
    single_diagnostic(diagnostic)
}

fn single_error_with_u64(
    code: &'static str,
    summary: &'static str,
    key: &'static str,
    value: u64,
) -> DiagnosticReport {
    let diagnostic = Diagnostic::error(diagnostic_code(code), safe_summary(summary))
        .try_with_field(DiagnosticField::public_u64(field_key(key), value))
        .expect("project Host diagnostics use unique bounded fields");
    single_diagnostic(diagnostic)
}

fn single_diagnostic(diagnostic: Diagnostic) -> DiagnosticReport {
    let mut report = DiagnosticReport::default();
    report.push(diagnostic);
    report
}

fn diagnostic_code(code: &'static str) -> DiagnosticCode {
    DiagnosticCode::new(code).expect("project Host diagnostic codes are engine-owned")
}

fn safe_summary(summary: &'static str) -> SafeSummary {
    SafeSummary::new(summary).expect("project Host summaries are engine-owned")
}

fn field_key(key: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(key).expect("project Host diagnostic field keys are engine-owned")
}

#[cfg(test)]
thread_local! {
    static PUBLICATION_FAULT_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn arm_publication_fault_for_test() {
    PUBLICATION_FAULT_ARMED.with(|armed| armed.set(true));
}

#[cfg(test)]
fn inject_publication_fault_if_armed(reporter: &nara_app::RuntimeFaultReporter) {
    PUBLICATION_FAULT_ARMED.with(|armed| {
        if armed.replace(false) {
            reporter.report(nara_app::RuntimeFault::engine(
                nara_app::RuntimeFaultKind::RequiredService,
                "nara.test.project-publication",
            ));
        }
    });
}

#[cfg(all(test, feature = "serde", feature = "runtime-2d"))]
mod tests;

#[cfg(test)]
mod component_binding_admission_tests {
    use nara_app::{App, CoreStage};
    use nara_reflect::{
        ComponentRegistryPlugin, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
        PersistentComponentSchemaOwnerId,
    };

    use super::*;

    fn owner() -> ComponentSchemaOwnerSnapshot {
        ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("test.host-data").unwrap(),
            ComponentSchemaCatalog::default(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn host_requires_exact_data_owner_basis_alongside_the_same_native_snapshot() {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            nara_scene::SceneComponentBindingsPlugin::new([owner()]),
        ))
        .unwrap()
        .add_systems(CoreStage::Update, |world: &mut World| {
            let installed = scene_component_bindings(world).unwrap();
            let shared = SceneComponentBindings::new(
                installed.native_snapshot().clone(),
                installed.data_owners().iter().cloned(),
            )
            .unwrap();
            verify_runtime_component_bindings(world, &shared).unwrap();
            let foreign =
                SceneComponentBindings::new(installed.native_snapshot().clone(), [owner()])
                    .unwrap();
            let omitted =
                SceneComponentBindings::new(installed.native_snapshot().clone(), []).unwrap();
            for selection in [foreign, omitted] {
                let failure = verify_runtime_component_bindings(world, &selection).unwrap_err();
                assert!(failure.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "project.run.component-bindings-mismatch"
                }));
            }
            verify_runtime_component_bindings(world, installed).unwrap();
        })
        .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let mut retirement = runtime.begin_retirement();
        assert_eq!(
            retirement.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
    }
}
