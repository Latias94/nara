use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use nara_app::{
    AppExit, PluginError, PluginHook, PluginHookMutation, PluginInstantiationError,
    PluginPlanError, PluginPrepareError, RuntimeAdmissionError, RuntimeAdmissionReservation,
    RuntimeCandidateFailure, RuntimeCandidateRetirementState, RuntimeCloseCause,
    RuntimeCloseErrorDisposition, RuntimeCloseEvidence, RuntimeCloseParticipantPhase,
    RuntimeClosePolicy, RuntimeConstructionError, RuntimeConstructionFailure, RuntimeControl,
    RuntimeControlRequestResult, RuntimeControlStatus, RuntimeControlTicket, RuntimeFault,
    RuntimeFaultKind, RuntimeFaultReporter, RuntimeInstance, RuntimeObligationLedger,
    RuntimePublicationFailure, RuntimePublicationSlot, RuntimeRetirement, RuntimeState,
};
use nara_asset::{
    AssetEvents, AssetRecord, AssetServer, AssetSourceKind, AssetStates, Assets, Handle,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_ecs::{Mut, Resource, World};
use nara_gameplay::{GameplayCommandQueue, GameplayCommandSubmission};
use nara_image::ImageAsset;
use nara_reflect::{
    CatalogFingerprint, ComponentRegistry, ComponentRegistrySnapshot,
    validate_component_registry_authority,
};
use nara_scene::{SceneDocument, spawn_scene};

use super::{
    CompositionError, ProjectContentRevision, ProjectContentSnapshot, ProjectSettingsLineage,
    RuntimePlan, RuntimePlanError,
};

const PROJECT_MANIFEST: &str = "nara.toml";

mod action;

#[cfg(feature = "tooling")]
mod editor;

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub use action::{DesktopRun, DesktopRunIntent, DesktopRunOutcome, DesktopRunReport};
pub use action::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport};
#[cfg(feature = "tooling")]
pub use editor::{EditorProjectIntent, EditorProjectOpenError, EditorProjectSession};

struct RuntimeStartAttempt {
    owner: Arc<HostStartClaim>,
    epoch: u64,
    stamp: RuntimeStartStamp,
    inputs: Option<RuntimeStartInputs>,
}

struct RuntimeStartInputs {
    snapshot: ProjectContentSnapshot,
    plan: RuntimePlan,
    startup_scene: Option<Arc<SceneDocument>>,
    commands: Vec<GameplayCommandSubmission>,
    obligations: RuntimeObligationLedger,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RuntimeStartStamp {
    lineage: ProjectSettingsLineage,
    content_revision: ProjectContentRevision,
    schema_fingerprint: CatalogFingerprint,
}

impl RuntimeStartStamp {
    fn capture(snapshot: &ProjectContentSnapshot) -> Self {
        Self {
            lineage: snapshot.lineage(),
            content_revision: snapshot.revision(),
            schema_fingerprint: snapshot.schema_fingerprint(),
        }
    }

    fn matches(&self, snapshot: &ProjectContentSnapshot, plan: &RuntimePlan) -> bool {
        self.lineage == snapshot.lineage()
            && self.lineage == plan.lineage()
            && self.content_revision == snapshot.revision()
            && self.schema_fingerprint == snapshot.schema_fingerprint()
            && self.schema_fingerprint == plan.schema_validation().fingerprint()
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
}

enum ProjectHostSlot {
    Empty,
    Running(Box<PublishedProjectRuntime>),
    Cleaning {
        epoch: u64,
        owner: Box<CleanupOwner>,
    },
}

struct PublishedProjectRuntime {
    epoch: u64,
    runtime: RuntimePublicationSlot,
    _snapshot: ProjectContentSnapshot,
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

struct HostPublicationReservation<'host> {
    host: &'host mut ProjectHost,
    active: bool,
}

impl<'host> HostPublicationReservation<'host> {
    fn new(
        host: &'host mut ProjectHost,
        epoch: u64,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
    ) -> Self {
        debug_assert!(matches!(host.slot, ProjectHostSlot::Empty));
        host.slot = ProjectHostSlot::Running(Box::new(PublishedProjectRuntime {
            epoch,
            runtime: RuntimePublicationSlot::new(),
            _snapshot: snapshot,
            _plan: plan,
        }));
        Self { host, active: true }
    }

    fn destination(&mut self) -> &mut RuntimePublicationSlot {
        match &mut self.host.slot {
            ProjectHostSlot::Running(published) => &mut published.runtime,
            ProjectHostSlot::Empty | ProjectHostSlot::Cleaning { .. } => {
                unreachable!("an active publication reservation owns the publishing slot")
            }
        }
    }

    fn commit(mut self) {
        self.active = false;
    }
}

impl Drop for HostPublicationReservation<'_> {
    fn drop(&mut self) {
        if self.active {
            self.host.slot = ProjectHostSlot::Empty;
        }
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
    },
}

impl CleanupOwner {
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

    fn completion_failure_report(&self) -> DiagnosticReport {
        let mut diagnostics = DiagnosticReport::default();
        if let Some(evidence) = self.close_evidence()
            && close_evidence_has_terminal_failure(evidence)
        {
            diagnostics.extend(cleanup_failed_report(evidence));
        }
        if matches!(self, Self::Candidate(owner) if owner.fault().is_some()) {
            diagnostics.extend(runtime_faulted_report());
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

    fn running_runtime_generation(&self) -> Option<u64> {
        match &self.slot {
            ProjectHostSlot::Running(published) => Some(published.runtime().generation().get()),
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
        let expected = published._plan.schema_validation().snapshot();
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
        let expected = published._plan.schema_validation().snapshot().clone();
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        let drive = published.runtime_mut().drive(real_delta);
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        drive.map(|_| ()).map_err(|_| {
            HostFault::new(single_error(
                "project.run.drive-failed",
                "Project runtime drive failed",
            ))
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

    fn begin_start(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Result<RuntimeStartAttempt, HostFault> {
        self.begin_start_with_scene(snapshot, plan, None, commands)
    }

    fn begin_editor_start(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        startup_scene: SceneDocument,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Result<RuntimeStartAttempt, HostFault> {
        self.begin_start_with_scene(snapshot, plan, Some(Arc::new(startup_scene)), commands)
    }

    fn begin_start_with_scene(
        &mut self,
        snapshot: ProjectContentSnapshot,
        plan: RuntimePlan,
        startup_scene: Option<Arc<SceneDocument>>,
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
        if snapshot.schema_fingerprint() != plan.schema_validation().fingerprint() {
            return Err(HostFault::new(single_error(
                "project.run.schema-mismatch",
                "Project content and runtime plan schemas do not match",
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
        let stamp = RuntimeStartStamp::capture(&snapshot);
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
            }),
        })
    }

    fn complete_start(
        &mut self,
        attempt: &mut RuntimeStartAttempt,
    ) -> Result<DiagnosticReport, HostFault> {
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
        } = inputs;

        let mut candidate = match plan.plugin_plan().instantiate_runtime_candidate(
            reservation,
            obligations,
            self.cleanup_policy,
        ) {
            Ok(candidate) => candidate,
            Err(owner) => {
                let diagnostics = runtime_construction_failure_report(owner.error());
                self.install_start_cleanup(epoch, CleanupOwner::Construction(owner));
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
                    startup_scene.as_deref(),
                    commands,
                );
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
            self.install_start_cleanup(
                epoch,
                CleanupOwner::Candidate(candidate.begin_retirement()),
            );
            let diagnostics = match admission.as_ref().err().and_then(|error| error.fault()) {
                Some(fault) => runtime_scope_failure_report(fault),
                None => single_error(
                    "project.run.candidate-scope-failed",
                    "Project runtime candidate admission failed",
                ),
            };
            return Err(HostFault::new(diagnostics));
        };
        let diagnostics = match materialization {
            Ok(diagnostics) if admission.is_ok() => diagnostics,
            Err(fault) => {
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Candidate(candidate.begin_retirement()),
                );
                return Err(fault);
            }
            Ok(_) => {
                self.install_start_cleanup(
                    epoch,
                    CleanupOwner::Candidate(candidate.begin_retirement()),
                );
                return Err(HostFault::new(single_error(
                    "project.run.candidate-scope-failed",
                    "Project runtime candidate admission failed",
                )));
            }
        };

        #[cfg(test)]
        let publication_reporter = candidate.fault_reporter();
        let ready = match candidate.complete_startup() {
            Ok(ready) => ready,
            Err(owner) => {
                let diagnostics = runtime_startup_failure_report(owner.fault());
                self.install_start_cleanup(epoch, CleanupOwner::Startup(owner));
                return Err(HostFault::new(diagnostics));
            }
        };
        // `complete_start` has exclusive access to the Host, so a validated empty publication slot
        // cannot change between claim and this commit boundary.
        debug_assert!(matches!(self.slot, ProjectHostSlot::Empty));

        #[cfg(test)]
        inject_publication_fault_if_armed(&publication_reporter);

        let mut reservation = HostPublicationReservation::new(self, epoch, snapshot, plan);
        match ready.publish_into(reservation.destination()) {
            Ok(()) => reservation.commit(),
            Err(owner) => {
                drop(reservation);
                self.install_start_cleanup(epoch, CleanupOwner::Publication(owner));
                return Err(HostFault::new(single_error(
                    "project.run.publication-faulted",
                    "Project runtime faulted at the publication boundary",
                )));
            }
        }

        claim.finish();
        if diagnostics.has_errors() {
            unreachable!("scene admission rejects error diagnostics before publication");
        }
        Ok(diagnostics)
    }

    fn drive_one_fixed_tick(&mut self) -> Result<Option<AppExit>, HostFault> {
        let ProjectHostSlot::Running(published) = &mut self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is not available for a fixed tick",
            )));
        };

        let expected = published._plan.schema_validation().snapshot().clone();
        verify_published_runtime_registry(published.runtime().world(), &expected)?;
        let runtime = published.runtime_mut();
        if runtime.state() == RuntimeState::Running {
            let RuntimeControlRequestResult::Accepted(pause) =
                runtime.request_control(RuntimeControl::Pause)
            else {
                return Err(HostFault::new(single_error(
                    "project.run.pause-rejected",
                    "Project runtime rejected exact-step preparation",
                )));
            };
            runtime.drive(Duration::ZERO).map_err(|_| {
                HostFault::new(single_error(
                    "project.run.pause-failed",
                    "Project runtime could not enter exact-step mode",
                ))
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
        let outcome = runtime.drive(Duration::ZERO).map_err(|_| {
            HostFault::new(single_error(
                "project.run.drive-failed",
                "Project runtime fixed tick failed",
            ))
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

    fn capture_outcome<O>(&self) -> Result<O, HostFault>
    where
        O: Resource + Clone,
    {
        self.outcome_resource::<O>().cloned()
    }

    fn capture_outcome_if<O>(
        &self,
        predicate: &(dyn Fn(&O) -> bool + Send + 'static),
    ) -> Result<Option<O>, HostFault>
    where
        O: Resource + Clone,
    {
        let outcome = self.outcome_resource::<O>()?;
        Ok(predicate(outcome).then(|| outcome.clone()))
    }

    fn outcome_resource<O>(&self) -> Result<&O, HostFault>
    where
        O: Resource,
    {
        let ProjectHostSlot::Running(published) = &self.slot else {
            return Err(HostFault::new(single_error(
                "project.run.runtime-unavailable",
                "Project runtime is not available for outcome capture",
            )));
        };
        if published.runtime().fault().is_some() {
            return Err(HostFault::new(runtime_faulted_report()));
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
                if close_evidence_has_terminal_failure(published.runtime().close_evidence()) {
                    Err(HostFault::new(cleanup_failed_report(
                        published.runtime().close_evidence(),
                    )))
                } else if published.runtime().fault().is_some() {
                    Err(HostFault::new(runtime_faulted_report()))
                } else {
                    Ok(())
                }
            }
            RuntimeState::Stopping | RuntimeState::CloseIncomplete => {
                let epoch = published.epoch;
                let mut diagnostics =
                    cleanup_incomplete_report(published.runtime().close_evidence());
                if published.runtime().fault().is_some() {
                    diagnostics.extend(runtime_faulted_report());
                }
                if close_evidence_has_terminal_failure(published.runtime().close_evidence()) {
                    diagnostics.extend(single_error(
                        "project.run.cleanup-failed",
                        "Project runtime cleanup reported a terminal failure",
                    ));
                }
                self.install_cleanup(
                    epoch,
                    CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
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
            self.install_cleanup(
                epoch,
                CleanupOwner::Candidate(published.take_runtime().begin_retirement()),
            );
        } else {
            self.slot = slot;
        }
    }

    fn install_start_cleanup(&mut self, epoch: u64, owner: CleanupOwner) {
        self.install_cleanup(epoch, owner);
    }

    fn install_cleanup(&mut self, epoch: u64, owner: CleanupOwner) {
        debug_assert!(matches!(self.slot, ProjectHostSlot::Empty));
        self.slot = if owner.state() == RuntimeCandidateRetirementState::Retired {
            ProjectHostSlot::Empty
        } else {
            ProjectHostSlot::Cleaning {
                epoch,
                owner: Box::new(owner),
            }
        };
    }

    fn has_cleanup_owner(&self) -> bool {
        matches!(self.slot, ProjectHostSlot::Cleaning { .. })
    }

    fn drive_cleanup_once(&mut self) -> CleanupDriveOutcome {
        let slot = std::mem::replace(&mut self.slot, ProjectHostSlot::Empty);
        let ProjectHostSlot::Cleaning { epoch, mut owner } = slot else {
            self.slot = slot;
            return CleanupDriveOutcome::Complete {
                failed: false,
                diagnostics: DiagnosticReport::default(),
            };
        };
        let state = owner.drive();
        match state {
            RuntimeCandidateRetirementState::Retired => {
                let diagnostics = owner.completion_failure_report();
                CleanupDriveOutcome::Complete {
                    failed: diagnostics.has_errors(),
                    diagnostics,
                }
            }
            RuntimeCandidateRetirementState::Retiring => {
                self.slot = ProjectHostSlot::Cleaning { epoch, owner };
                CleanupDriveOutcome::Retiring
            }
            RuntimeCandidateRetirementState::RetirementIncomplete => {
                self.slot = ProjectHostSlot::Cleaning { epoch, owner };
                CleanupDriveOutcome::RetirementIncomplete
            }
        }
    }
}

fn materialize_project_runtime(
    world: &mut World,
    plan: &RuntimePlan,
    snapshot: &ProjectContentSnapshot,
    startup_scene: Option<&SceneDocument>,
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

    let scene = spawn_scene(
        world,
        plan.schema_validation().registry(),
        startup_scene.unwrap_or_else(|| snapshot.expanded_startup_scene()),
    );
    if scene.diagnostics.has_errors() || scene.instance.is_none() {
        return Err(HostFault::new(scene.diagnostics));
    }
    let diagnostics = scene.diagnostics;
    publish_snapshot_images(world, snapshot)?;
    let faults = world
        .get_resource::<RuntimeFaultReporter>()
        .cloned()
        .ok_or_else(|| {
            HostFault::new(single_error(
                "project.run.fault-reporter-missing",
                "Project runtime fault reporter is unavailable",
            ))
        })?;
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
    drop(queue);
    verify_runtime_registry(world, plan)?;
    Ok(diagnostics)
}

fn publish_snapshot_images(
    world: &mut World,
    snapshot: &ProjectContentSnapshot,
) -> Result<(), HostFault> {
    if snapshot.images().is_empty() {
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
            let image = snapshot.share_image_for_runtime(index).ok_or_else(|| {
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
                                Handle::new(handle.id()),
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
    )
}

fn verify_runtime_registry(world: &World, plan: &RuntimePlan) -> Result<(), HostFault> {
    verify_runtime_registry_snapshot(world, plan.schema_validation().snapshot())
}

fn verify_runtime_registry_snapshot(
    world: &World,
    expected: &ComponentRegistrySnapshot,
) -> Result<(), HostFault> {
    let registry = world.get_resource::<ComponentRegistry>().ok_or_else(|| {
        HostFault::new(single_error(
            "project.run.registry-missing",
            "Project runtime component registry is missing",
        ))
    })?;
    if !registry.shares_snapshot(expected) {
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
    Ok(())
}

fn verify_published_runtime_registry(
    world: &World,
    expected: &ComponentRegistrySnapshot,
) -> Result<(), HostFault> {
    let result = verify_runtime_registry_snapshot(world, expected);
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
        CompositionError::UnknownProductCapability { .. } => "unknown-product-capability",
        CompositionError::UncompiledProductCapability { .. } => "uncompiled-product-capability",
        CompositionError::UnrequestedProductCapability { .. } => "unrequested-product-capability",
        CompositionError::MissingSchemaProvider { .. } => "missing-schema-provider",
        CompositionError::DivergentSchemaProvider { .. } => "divergent-schema-provider",
        CompositionError::AmbiguousSchemaProviderOwner { .. } => "ambiguous-schema-provider-owner",
        CompositionError::SchemaProviderRejected { .. } => "schema-provider-rejected",
        CompositionError::SchemaProviderPanicked { .. } => "schema-provider-panicked",
        CompositionError::SchemaFreezeRejected { .. } => "schema-freeze-rejected",
        CompositionError::SchemaAuthorityPublicationFailed => "schema-authority-conflict",
    };
    let mut diagnostic = failure_diagnostic(
        "project.run.composition-invalid",
        "Project runtime composition could not be admitted",
        reason,
    );
    match error {
        CompositionError::UnknownProductCapability { plugin, capability } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "capability", capability.as_str());
        }
        CompositionError::UncompiledProductCapability { plugin, capability }
        | CompositionError::UnrequestedProductCapability { plugin, capability } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "capability", capability.as_str());
        }
        CompositionError::MissingSchemaProvider { provider }
        | CompositionError::DivergentSchemaProvider { provider }
        | CompositionError::AmbiguousSchemaProviderOwner { provider }
        | CompositionError::SchemaProviderRejected { provider, .. }
        | CompositionError::SchemaProviderPanicked { provider } => {
            diagnostic = attach_identifier(diagnostic, "provider", provider.as_str());
        }
        CompositionError::ProjectLineageMismatch
        | CompositionError::SchemaFreezeRejected { .. }
        | CompositionError::SchemaAuthorityPublicationFailed => {}
    }
    single_diagnostic(diagnostic)
}

fn plugin_plan_failure_report(error: &PluginPlanError) -> DiagnosticReport {
    let reason = match error {
        PluginPlanError::DeclarationPanicked => "declaration-panicked",
        PluginPlanError::GroupPanicked { .. } => "group-panicked",
        PluginPlanError::GroupCycle { .. } => "group-cycle",
        PluginPlanError::DivergentGroup { .. } => "divergent-group",
        PluginPlanError::SlotPluginMismatch { .. } => "slot-plugin-mismatch",
        PluginPlanError::MissingEditTarget => "missing-edit-target",
        PluginPlanError::AmbiguousEditTarget => "ambiguous-edit-target",
        PluginPlanError::RequiredSlotDisabled { .. } => "required-slot-disabled",
        PluginPlanError::DuplicateSlot { .. } => "duplicate-slot",
        PluginPlanError::DivergentSlotContract { .. } => "divergent-slot-contract",
        PluginPlanError::ActiveSlotDisabled { .. } => "active-slot-disabled",
        PluginPlanError::DuplicatePlugin { .. } => "duplicate-plugin",
        PluginPlanError::DivergentDefinition { .. } => "divergent-definition",
        PluginPlanError::MissingPlugin { .. } => "missing-plugin",
        PluginPlanError::MissingCapability { .. } => "missing-capability",
        PluginPlanError::MissingService { .. } => "missing-service",
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
        PluginPlanError::RequiredSlotDisabled { slot }
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
        PluginPlanError::MissingCapability { plugin, required } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "capability", required.as_str());
        }
        PluginPlanError::MissingService { plugin, required } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "service", required.as_str());
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

fn runtime_construction_failure_report(error: &RuntimeConstructionError) -> DiagnosticReport {
    match error {
        RuntimeConstructionError::Plugin(PluginInstantiationError::Prepare(error)) => {
            plugin_prepare_failure_report(error)
        }
        RuntimeConstructionError::Plugin(PluginInstantiationError::Plugin(error)) => {
            plugin_hook_failure_report(error)
        }
        RuntimeConstructionError::Admission(error) => admission_failure_report(error),
    }
}

fn plugin_prepare_failure_report(error: &PluginPrepareError) -> DiagnosticReport {
    let (reason, plugin, failure_code) = match error {
        PluginPrepareError::Failed { plugin, code } => ("failed", *plugin, Some(*code)),
        PluginPrepareError::Panicked { plugin } => ("panicked", *plugin, None),
    };
    let mut diagnostic = failure_diagnostic(
        "project.run.plugin-prepare-failed",
        "Project plugin preparation failed",
        reason,
    );
    diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
    if let Some(code) = failure_code {
        diagnostic = attach_identifier(diagnostic, "failure-code", code);
    }
    single_diagnostic(diagnostic)
}

fn plugin_hook_failure_report(error: &PluginError) -> DiagnosticReport {
    let mut diagnostic = failure_diagnostic(
        "project.run.plugin-hook-failed",
        "Project plugin lifecycle hook failed",
        plugin_error_reason(error),
    );
    match error {
        PluginError::HookMutationForbidden {
            plugin,
            hook,
            mutation,
        } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "hook", plugin_hook_id(*hook));
            diagnostic = attach_identifier(diagnostic, "mutation", plugin_mutation_id(*mutation));
        }
        PluginError::SetupFailed { plugin, .. }
        | PluginError::ComponentRegistrationFailed { plugin, .. } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            if let PluginError::ComponentRegistrationFailed { component, .. } = error {
                diagnostic = attach_identifier(diagnostic, "component", component);
            }
        }
        PluginError::HookPanicked { plugin, hook } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "hook", plugin_hook_id(*hook));
        }
        PluginError::CommittedPreflightRejected { plugin, source } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "cause", plugin_error_reason(source));
        }
        PluginError::MissingShutdownObligation { plugin, obligation }
        | PluginError::UndeclaredShutdownObligation { plugin, obligation }
        | PluginError::DuplicateShutdownObligation { plugin, obligation } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "obligation", obligation.as_str());
        }
        PluginError::DuplicateRuntimeCloseParticipant {
            plugin,
            obligation,
            participant_id,
        } => {
            diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            diagnostic = attach_identifier(diagnostic, "obligation", obligation.as_str());
            diagnostic = attach_identifier(diagnostic, "participant", participant_id.as_str());
        }
        PluginError::AppSealed
        | PluginError::RawBuiltInScheduleMutationForbidden
        | PluginError::ScheduleCompatibility(_)
        | PluginError::ShutdownObligationOutsideBuild
        | PluginError::LifecycleShutdown
        | PluginError::LifecyclePoisoned
        | PluginError::FinishReentered => {}
    }
    single_diagnostic(diagnostic)
}

fn plugin_error_reason(error: &PluginError) -> &'static str {
    match error {
        PluginError::HookMutationForbidden { .. } => "hook-mutation-forbidden",
        PluginError::AppSealed => "app-sealed",
        PluginError::RawBuiltInScheduleMutationForbidden => "raw-schedule-mutation-forbidden",
        PluginError::ScheduleCompatibility(_) => "schedule-compatibility",
        PluginError::SetupFailed { .. } => "setup-failed",
        PluginError::ComponentRegistrationFailed { .. } => "component-registration-failed",
        PluginError::HookPanicked { .. } => "hook-panicked",
        PluginError::CommittedPreflightRejected { .. } => "committed-preflight-rejected",
        PluginError::MissingShutdownObligation { .. } => "missing-shutdown-obligation",
        PluginError::UndeclaredShutdownObligation { .. } => "undeclared-shutdown-obligation",
        PluginError::DuplicateShutdownObligation { .. } => "duplicate-shutdown-obligation",
        PluginError::DuplicateRuntimeCloseParticipant { .. } => {
            "duplicate-runtime-close-participant"
        }
        PluginError::ShutdownObligationOutsideBuild => "shutdown-obligation-outside-build",
        PluginError::LifecycleShutdown => "lifecycle-shutdown",
        PluginError::LifecyclePoisoned => "lifecycle-poisoned",
        PluginError::FinishReentered => "finish-reentered",
    }
}

fn admission_failure_report(error: &RuntimeAdmissionError) -> DiagnosticReport {
    let reason = match error {
        RuntimeAdmissionError::AppStarted => "app-started",
        RuntimeAdmissionError::RawRunnerInstalled => "raw-runner-installed",
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

fn runtime_faulted_report() -> DiagnosticReport {
    single_error(
        "project.run.runtime-faulted",
        "Project runtime reported a fault before product completion",
    )
}

fn runtime_scope_failure_report(fault: &RuntimeFault) -> DiagnosticReport {
    if fault.kind() == RuntimeFaultKind::RuntimeAuthority {
        return runtime_authority_invalid_report();
    }
    runtime_faulted_report()
}

fn runtime_startup_failure_report(fault: &RuntimeFault) -> DiagnosticReport {
    if fault.kind() == RuntimeFaultKind::RuntimeAuthority {
        return runtime_authority_invalid_report();
    }
    single_error(
        "project.run.startup-failed",
        "Project runtime startup failed",
    )
}

fn runtime_authority_invalid_report() -> DiagnosticReport {
    single_error(
        "project.run.runtime-authority-invalid",
        "Project runtime authority is invalid",
    )
}

fn cleanup_incomplete_report(evidence: &RuntimeCloseEvidence) -> DiagnosticReport {
    let mut report = single_warning(
        "project.run.cleanup-incomplete",
        "Project runtime cleanup is incomplete",
    );
    append_close_cause_diagnostics(&mut report, evidence);
    report
}

fn cleanup_failed_report(evidence: &RuntimeCloseEvidence) -> DiagnosticReport {
    let mut report = single_error(
        "project.run.cleanup-failed",
        "Project runtime cleanup reported a terminal failure",
    );
    append_close_cause_diagnostics(&mut report, evidence);
    report
}

fn append_close_cause_diagnostics(report: &mut DiagnosticReport, evidence: &RuntimeCloseEvidence) {
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

fn close_evidence_has_terminal_failure(evidence: &RuntimeCloseEvidence) -> bool {
    evidence.causes().iter().any(|cause| {
        matches!(cause, RuntimeCloseCause::PluginShutdown)
            || matches!(
                cause,
                RuntimeCloseCause::ParticipantError {
                    disposition: RuntimeCloseErrorDisposition::Terminal,
                    ..
                }
            )
    })
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
        PluginHookMutation::RunnerSelection => "runner-selection",
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
