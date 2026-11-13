//! Finite source-game code iteration over the existing Cargo and process owners.
//!
//! The incumbent remains the sole ordinary command target while a candidate is prepared. Probe
//! execution is isolated and discarded; adoption starts at the captured state in another fresh
//! process. This game-owned Host does not extend supervised desktop Play or serialize native state.

#[cfg(feature = "desktop")]
pub mod presentation;
mod process;
pub mod protocol;
mod source;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    fs::ContentDigest,
    project_tooling::{
        cargo_build::{
            CapturedCargoExecutable, CargoBuildFailure, CargoBuildRequest, build_executable,
        },
        process::SupervisedProcessCleanupReceipt,
    },
};

use crate::resume::{ResumeState, encode_resume};
use process::{Executable, Process, Store};
pub use protocol::{DesktopPresentation, DevelopmentPlacement, InputActivation};
use protocol::{MAX_STEP_COMMANDS, Observation, SemanticCommand};
use source::SourceFence;

const MAX_REBUILDS: usize = 8;
const MAX_PROBE_STEPS: usize = 64;
const MAX_EXECUTABLE_BYTES: usize = 1024 * 1024 * 1024;
const CLEANUP_OWNER_BUDGET: Duration = Duration::from_secs(10);

/// A bounded semantic probe. Every step compares the complete expected game state.
#[derive(Debug, Clone)]
pub struct ProbeStep {
    pub commands: Vec<SemanticCommand>,
    pub expected: ResumeState,
}

/// Host-local generation observations. Neither the PID nor these numbers grant process control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveGeneration {
    pub sequence: u64,
    pub pid: u32,
    pub executable: ContentDigest,
    pub provenance: ContentDigest,
}

/// Nominal permission to adopt or cancel the exact candidate retained by its originating Host.
#[derive(Debug, Clone)]
pub struct PreparedRebuild {
    identity: Arc<()>,
    generation: ActiveGeneration,
    presentation: Option<DesktopPresentation>,
}

impl PreparedRebuild {
    #[must_use]
    pub const fn generation(&self) -> ActiveGeneration {
        self.generation
    }

    #[must_use]
    pub fn presentation(&self) -> Option<&DesktopPresentation> {
        self.presentation.as_ref()
    }
}

/// Native activation follows route adoption and preserves its own actual error/result.
#[derive(Debug, Clone)]
pub struct ActivationOutcome {
    pub active: ActiveGeneration,
    pub activation: InputActivation,
    pub error: Option<String>,
}

/// Completion of the original native owners, separately from a successful execution-target change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetirementStatus {
    Complete,
    Pending,
}

/// Adoption has committed even when predecessor cleanup needs another bounded drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdoptionOutcome {
    pub active: ActiveGeneration,
    pub predecessor: ActiveGeneration,
    pub retirement: RetirementStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetirementRole {
    Probe,
    Candidate,
    Predecessor,
    Active,
}

/// Actual native cleanup and the separate game/termination facts for one exact retained process.
#[derive(Debug, Clone, Copy)]
pub struct RetirementReport {
    pub generation: ActiveGeneration,
    pub role: RetirementRole,
    pub native: SupervisedProcessCleanupReceipt,
    pub game_closed: bool,
    pub terminated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevelopmentStage {
    State,
    Build,
    Start,
    Capture,
    Restore,
    Probe,
    Adopt,
    Cleanup,
}

#[derive(Debug, thiserror::Error)]
#[error("game development {stage:?} failed: {message}")]
pub struct DevelopmentError {
    pub stage: DevelopmentStage,
    pub message: String,
}

impl DevelopmentError {
    fn at(stage: DevelopmentStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

struct LiveGame {
    process: Process,
    generation: ActiveGeneration,
}

struct Candidate {
    ticket: PreparedRebuild,
    live: LiveGame,
    basis: ResumeState,
    source: SourceFence,
}

struct Retiring {
    live: LiveGame,
    role: RetirementRole,
}

/// A concrete development Host for the source Two-Level Game's explicit execution placement.
///
/// The caller grants trust in an explicit source project and existing temporary parent. Builds
/// use an explicit public Cargo request; this Host never edits source or discovers dependencies.
/// The finite session admits at most eight rebuild attempts, 64 probe steps per attempt and 1 GiB
/// of cumulative captured executable bytes. These are logical allowances, not RSS guarantees.
/// All children use the existing contained native-process owner. Call `close` until complete;
/// dropping a live Host is fallback cleanup and is never evidence of graceful retirement.
pub struct DevelopmentHost {
    project: PathBuf,
    placement: DevelopmentPlacement,
    store: Store,
    active: Option<LiveGame>,
    candidate: Option<Candidate>,
    retiring: Vec<Retiring>,
    reports: Vec<RetirementReport>,
    last_build_failure: Option<CargoBuildFailure>,
    next_generation: u64,
    rebuilds: usize,
    captured_bytes: usize,
    closing: bool,
    closed: bool,
    cleanup_error: Option<String>,
}

impl DevelopmentHost {
    pub fn new(project: &Path, work_parent: &Path) -> Result<Self, DevelopmentError> {
        Self::with_placement(project, work_parent, DevelopmentPlacement::SourceHeadless)
    }

    pub fn with_placement(
        project: &Path,
        work_parent: &Path,
        placement: DevelopmentPlacement,
    ) -> Result<Self, DevelopmentError> {
        if placement == DevelopmentPlacement::SourceDesktop && !cfg!(windows) {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "source desktop development is currently qualified only on Windows",
            ));
        }
        if !project.is_absolute() || !work_parent.is_absolute() {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "project and work parent must be absolute",
            ));
        }
        let project = project
            .canonicalize()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::State, error.to_string()))?;
        if !project.join("nara.toml").is_file() {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "the explicit game project is unavailable",
            ));
        }
        let work_parent = work_parent
            .canonicalize()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::State, error.to_string()))?;
        if work_parent.starts_with(&project) {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "development storage must be outside the source project",
            ));
        }
        let store = Store::new(&work_parent)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::State, error))?;
        Ok(Self {
            project,
            placement,
            store,
            active: None,
            candidate: None,
            retiring: Vec::new(),
            reports: Vec::new(),
            last_build_failure: None,
            next_generation: 1,
            rebuilds: 0,
            captured_bytes: 0,
            closing: false,
            closed: false,
            cleanup_error: None,
        })
    }

    pub fn start(
        &mut self,
        captured: CapturedCargoExecutable,
    ) -> Result<Observation, DevelopmentError> {
        self.ready_for_work()?;
        if self.active.is_some() {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "the Host already has an incumbent",
            ));
        }
        let executable = self.install(captured)?;
        let mut live = self.launch(executable)?;
        let observed = match live.process.observation() {
            Ok(observed) => observed,
            Err(error) => {
                self.retire(live, RetirementRole::Candidate);
                return Err(DevelopmentError::at(DevelopmentStage::Start, error));
            }
        };
        self.active = Some(live);
        Ok(observed)
    }

    #[must_use]
    pub fn active(&self) -> Option<ActiveGeneration> {
        self.active.as_ref().map(|live| live.generation)
    }

    #[must_use]
    pub const fn placement(&self) -> DevelopmentPlacement {
        self.placement
    }

    /// Freezes native ingress through the active child's original owner before handoff capture.
    /// A refusal preserves the adopted route; successful isolation requires explicit activation
    /// to restore ordinary native input, including after a later build or candidate refusal.
    pub fn isolate_active_input(&mut self) -> Result<Observation, DevelopmentError> {
        self.active_mut()?
            .process
            .isolate_input()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::State, error))
    }

    /// Reveals and activates the already adopted child. A failure does not undo route adoption.
    pub fn activate_active(&mut self) -> Result<ActivationOutcome, DevelopmentError> {
        if self.placement != DevelopmentPlacement::SourceDesktop {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "native activation requires the explicit desktop placement",
            ));
        }
        let live = self.active_mut()?;
        let (activation, error) = live
            .process
            .activate_input()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::State, error))?;
        Ok(ActivationOutcome {
            active: live.generation,
            activation,
            error,
        })
    }

    pub fn confirm_active_presentation(&mut self) -> Result<DesktopPresentation, DevelopmentError> {
        if self.placement != DevelopmentPlacement::SourceDesktop {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "native presentation requires the explicit desktop placement",
            ));
        }
        let live = self.active_mut()?;
        let state = live
            .process
            .capture()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Capture, error))?;
        qualify_presentation(&mut live.process, &state)
    }

    #[must_use]
    pub fn last_build_failure(&self) -> Option<&CargoBuildFailure> {
        self.last_build_failure.as_ref()
    }

    #[must_use]
    pub fn retirements(&self) -> &[RetirementReport] {
        &self.reports
    }

    #[must_use]
    pub fn cleanup_error(&self) -> Option<&str> {
        self.cleanup_error.as_deref()
    }

    pub fn capture(&mut self) -> Result<ResumeState, DevelopmentError> {
        self.active_mut()?
            .process
            .capture()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Capture, error))
    }

    /// Drives only the adopted incumbent. A prepared candidate is never an ordinary input target.
    /// Changing the incumbent invalidates its candidate's adoption basis until cancellation.
    pub fn step(
        &mut self,
        commands: Vec<SemanticCommand>,
    ) -> Result<Observation, DevelopmentError> {
        validate_commands(&commands)?;
        self.active_mut()?
            .process
            .step(commands)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::State, error))
    }

    /// Builds, restores and probes a candidate while preserving the incumbent.
    ///
    /// A successful probe process is physically retired. A separate fresh process then restores
    /// the original capture and becomes the exact prepared candidate. Probe state is never adopted.
    pub fn prepare_rebuild(
        &mut self,
        request: &CargoBuildRequest,
        expected_initial: &ResumeState,
        probes: &[ProbeStep],
    ) -> Result<PreparedRebuild, DevelopmentError> {
        self.ready_for_work()?;
        if self.candidate.is_some() || self.active.is_none() {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "rebuild needs one incumbent and no existing candidate",
            ));
        }
        if request.workspace() != self.project
            || !eligible_selection(request.selection(), self.placement)
        {
            return Err(DevelopmentError::at(
                DevelopmentStage::Build,
                "the request must select this source game's exact declared continuity placement",
            ));
        }
        if probes.is_empty() || probes.len() > MAX_PROBE_STEPS || self.rebuilds >= MAX_REBUILDS {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "finite rebuild or probe allowance was exceeded",
            ));
        }
        for probe in probes {
            validate_commands(&probe.commands)?;
            probe.expected.validate().map_err(|error| {
                DevelopmentError::at(DevelopmentStage::Probe, error.to_string())
            })?;
        }
        if self.placement == DevelopmentPlacement::SourceDesktop {
            self.isolate_active_input()?;
        }
        let basis = self.capture()?;
        if !exact_state(&basis, expected_initial)? {
            return Err(DevelopmentError::at(
                DevelopmentStage::Capture,
                "the supplied probe does not describe the current incumbent capture",
            ));
        }
        let mut source = SourceFence::capture(&self.project)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Build, error))?;
        self.rebuilds += 1;
        self.last_build_failure = None;
        let captured = match build_executable(request, |_| Ok(())) {
            Ok(captured) => captured,
            Err(failure) => {
                let message = failure.to_string();
                self.last_build_failure = Some(failure);
                return Err(DevelopmentError::at(DevelopmentStage::Build, message));
            }
        };
        source
            .bind_build(&captured)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Build, error))?;
        let executable = self.install(captured)?;
        let mut probe = self.launch(Arc::clone(&executable))?;
        let qualification = (|| {
            restore_exact(&mut probe.process, &basis)?;
            if self.placement == DevelopmentPlacement::SourceDesktop {
                qualify_presentation(&mut probe.process, &basis)?;
            }
            for step in probes {
                probe
                    .process
                    .step(step.commands.clone())
                    .map_err(|error| DevelopmentError::at(DevelopmentStage::Probe, error))?;
                let actual = probe
                    .process
                    .capture()
                    .map_err(|error| DevelopmentError::at(DevelopmentStage::Probe, error))?;
                if !exact_state(&actual, &step.expected)? {
                    return Err(DevelopmentError::at(
                        DevelopmentStage::Probe,
                        "the candidate did not match the complete expected game state",
                    ));
                }
                if self.placement == DevelopmentPlacement::SourceDesktop {
                    qualify_presentation(&mut probe.process, &actual)?;
                }
            }
            Ok(())
        })();
        let probe_generation = probe.generation;
        self.retire(probe, RetirementRole::Probe);
        qualification?;
        let probe_retirement = self
            .reports
            .iter()
            .find(|report| report.generation == probe_generation);
        if !probe_retirement.is_some_and(clean_game_retirement) {
            return Err(DevelopmentError::at(
                DevelopmentStage::Cleanup,
                "the qualified probe has not completed normal game and native retirement",
            ));
        }
        source
            .verify()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Restore, error))?;
        let mut candidate = self.launch(executable)?;
        let restored = restore_exact(&mut candidate.process, &basis).and_then(|()| {
            source
                .verify()
                .map_err(|error| DevelopmentError::at(DevelopmentStage::Restore, error))?;
            if !exact_state(&self.capture()?, &basis)? {
                return Err(DevelopmentError::at(
                    DevelopmentStage::Adopt,
                    "the incumbent changed while its candidate was prepared",
                ));
            }
            Ok(())
        });
        if let Err(error) = restored {
            self.retire(candidate, RetirementRole::Candidate);
            return Err(error);
        }
        let presentation = if self.placement == DevelopmentPlacement::SourceDesktop {
            match qualify_presentation(&mut candidate.process, &basis) {
                Ok(presentation) => Some(presentation),
                Err(error) => {
                    self.retire(candidate, RetirementRole::Candidate);
                    return Err(error);
                }
            }
        } else {
            None
        };
        let ticket = PreparedRebuild {
            identity: Arc::new(()),
            generation: candidate.generation,
            presentation,
        };
        self.candidate = Some(Candidate {
            ticket: ticket.clone(),
            live: candidate,
            basis,
            source,
        });
        Ok(ticket)
    }

    /// Switches the sole command route only after both original states are revalidated.
    /// Once switched, predecessor cleanup remains a separate pending outcome and cannot roll back
    /// the adopted game or authorize a second active input route.
    pub fn adopt(&mut self, ticket: &PreparedRebuild) -> Result<AdoptionOutcome, DevelopmentError> {
        self.check_ticket(ticket)?;
        if self.placement == DevelopmentPlacement::SourceDesktop {
            self.isolate_active_input()?;
        }
        self.candidate
            .as_ref()
            .unwrap()
            .source
            .verify()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Adopt, error))?;
        let basis = self.candidate.as_ref().unwrap().basis.clone();
        if !exact_state(&self.capture()?, &basis)? {
            return Err(DevelopmentError::at(
                DevelopmentStage::Adopt,
                "the incumbent no longer matches the candidate capture",
            ));
        }
        let candidate = self.candidate.as_mut().unwrap();
        if !exact_state(
            &candidate
                .live
                .process
                .capture()
                .map_err(|error| DevelopmentError::at(DevelopmentStage::Adopt, error))?,
            &basis,
        )? {
            return Err(DevelopmentError::at(
                DevelopmentStage::Adopt,
                "the candidate no longer matches its restored capture",
            ));
        }
        if self.placement == DevelopmentPlacement::SourceDesktop {
            qualify_presentation(&mut candidate.live.process, &basis)?;
        }
        candidate
            .source
            .verify()
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Adopt, error))?;
        let candidate = self.candidate.take().unwrap();
        let predecessor = self.active.replace(candidate.live).unwrap();
        let prior_generation = predecessor.generation;
        let active = self.active().unwrap();
        self.retiring.push(Retiring {
            live: predecessor,
            role: RetirementRole::Predecessor,
        });
        Ok(AdoptionOutcome {
            active,
            predecessor: prior_generation,
            retirement: self.retirement_status(),
        })
    }

    pub fn cancel(
        &mut self,
        ticket: &PreparedRebuild,
    ) -> Result<RetirementStatus, DevelopmentError> {
        self.check_ticket(ticket)?;
        let candidate = self.candidate.take().unwrap();
        self.retire(candidate.live, RetirementRole::Candidate);
        Ok(self.retirement_status())
    }

    /// Retries the original physical owners; it never selects a new process by an observation ID.
    /// Each retained child receives its own ten-second cleanup drive, so a pending predecessor
    /// cannot consume the active child's opportunity to close. A call may span several drives.
    pub fn retry_retirement(&mut self) -> RetirementStatus {
        self.cleanup_error = None;
        let mut remaining = Vec::new();
        for mut retiring in self.retiring.drain(..) {
            let deadline = Instant::now() + CLEANUP_OWNER_BUDGET;
            match retiring.live.process.close(deadline) {
                Ok(retired) => self.reports.push(RetirementReport {
                    generation: retiring.live.generation,
                    role: retiring.role,
                    native: retired.native,
                    game_closed: retired.game_closed,
                    terminated: retired.terminated,
                }),
                Err(error) => {
                    self.cleanup_error = Some(error);
                    remaining.push(retiring);
                }
            }
        }
        self.retiring = remaining;
        if let Some(CargoBuildFailure::CleanupIncomplete(custody)) =
            self.last_build_failure.as_mut()
            && let Err(error) = custody.retry_cleanup()
        {
            self.cleanup_error = Some(error.to_string());
        }
        self.retirement_status()
    }

    /// Fences new operations and retains every original owner until native and storage cleanup.
    pub fn close(&mut self) -> RetirementStatus {
        if self.closed {
            return RetirementStatus::Complete;
        }
        self.closing = true;
        if let Some(candidate) = self.candidate.take() {
            self.retiring.push(Retiring {
                live: candidate.live,
                role: RetirementRole::Candidate,
            });
        }
        if let Some(active) = self.active.take() {
            self.retiring.push(Retiring {
                live: active,
                role: RetirementRole::Active,
            });
        }
        if self.retry_retirement() == RetirementStatus::Pending {
            return RetirementStatus::Pending;
        }
        match self.store.close() {
            Ok(()) => {
                self.closed = true;
                RetirementStatus::Complete
            }
            Err(error) => {
                self.cleanup_error = Some(error);
                RetirementStatus::Pending
            }
        }
    }

    fn retirement_status(&self) -> RetirementStatus {
        if self.retiring.is_empty() && !self.build_cleanup_pending() {
            RetirementStatus::Complete
        } else {
            RetirementStatus::Pending
        }
    }

    fn build_cleanup_pending(&self) -> bool {
        matches!(self.last_build_failure.as_ref(), Some(CargoBuildFailure::CleanupIncomplete(custody)) if custody.process_id().is_some())
    }

    fn ready_for_work(&self) -> Result<(), DevelopmentError> {
        if self.closing || self.closed || self.retirement_status() == RetirementStatus::Pending {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "the Host is closing or retains unfinished physical cleanup",
            ));
        }
        Ok(())
    }

    fn active_mut(&mut self) -> Result<&mut LiveGame, DevelopmentError> {
        if self.closing || self.closed {
            return Err(DevelopmentError::at(
                DevelopmentStage::State,
                "the Host is closing or closed",
            ));
        }
        self.active.as_mut().ok_or_else(|| {
            DevelopmentError::at(DevelopmentStage::State, "the Host has no adopted game")
        })
    }

    fn check_ticket(&self, ticket: &PreparedRebuild) -> Result<(), DevelopmentError> {
        self.ready_for_work()?;
        if !self
            .candidate
            .as_ref()
            .is_some_and(|candidate| Arc::ptr_eq(&candidate.ticket.identity, &ticket.identity))
        {
            return Err(DevelopmentError::at(
                DevelopmentStage::Adopt,
                "the prepared candidate is foreign, cancelled or already consumed",
            ));
        }
        Ok(())
    }

    fn install(
        &mut self,
        captured: CapturedCargoExecutable,
    ) -> Result<Arc<Executable>, DevelopmentError> {
        if !eligible_selection(captured.provenance().selection(), self.placement)
            || captured.provenance().selection().target_triple()
                != captured.provenance().toolchain().host_target()
            || !eligible_features(captured.provenance().resolved_features(), self.placement)
        {
            return Err(DevelopmentError::at(
                DevelopmentStage::Build,
                "only the exact native game product and its declared placement features are supported",
            ));
        }
        let total = self
            .captured_bytes
            .checked_add(captured.bytes().len())
            .filter(|total| *total <= MAX_EXECUTABLE_BYTES)
            .ok_or_else(|| {
                DevelopmentError::at(
                    DevelopmentStage::Build,
                    "the finite captured executable allowance was exceeded",
                )
            })?;
        let installed = Executable::install(&mut self.store, captured)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Start, error))?;
        self.captured_bytes = total;
        Ok(installed)
    }

    fn launch(&mut self, executable: Arc<Executable>) -> Result<LiveGame, DevelopmentError> {
        let sequence = self.next_generation;
        self.next_generation = sequence.checked_add(1).ok_or_else(|| {
            DevelopmentError::at(
                DevelopmentStage::State,
                "generation observation is exhausted",
            )
        })?;
        let digest = executable.captured().digest();
        let provenance = executable.captured().provenance().digest();
        match Process::start(&self.project, &mut self.store, executable, self.placement) {
            Ok(process) => {
                let generation = ActiveGeneration {
                    sequence,
                    pid: process.pid(),
                    executable: digest,
                    provenance,
                };
                Ok(LiveGame {
                    process,
                    generation,
                })
            }
            Err(mut failure) => {
                if let Some(process) = failure.process.take() {
                    let generation = ActiveGeneration {
                        sequence,
                        pid: process.pid(),
                        executable: digest,
                        provenance,
                    };
                    self.retire(
                        LiveGame {
                            process,
                            generation,
                        },
                        RetirementRole::Candidate,
                    );
                }
                Err(DevelopmentError::at(
                    DevelopmentStage::Start,
                    failure.message,
                ))
            }
        }
    }

    fn retire(&mut self, live: LiveGame, role: RetirementRole) {
        self.retiring.push(Retiring { live, role });
        self.retry_retirement();
    }
}

impl Drop for DevelopmentHost {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        if self.close() == RetirementStatus::Pending {
            let retained = std::mem::take(&mut self.retiring);
            let build = self.last_build_failure.take();
            std::mem::forget((retained, build));
            eprintln!(
                "game development Host still owns incomplete cleanup: {:?}",
                self.cleanup_error
            );
        }
    }
}

fn eligible_selection(
    selection: &nara::project_workflow::cargo::CargoBuildSelection,
    placement: DevelopmentPlacement,
) -> bool {
    selection.package() == "two-level-game"
        && selection.target() == "continuity"
        && !selection.default_features()
        && eligible_features(selection.features(), placement)
}

fn eligible_features(features: &[String], placement: DevelopmentPlacement) -> bool {
    match placement {
        DevelopmentPlacement::SourceHeadless => features.is_empty(),
        DevelopmentPlacement::SourceDesktop => features == ["desktop"],
    }
}

fn qualify_presentation(
    process: &mut Process,
    state: &ResumeState,
) -> Result<DesktopPresentation, DevelopmentError> {
    let expected = ContentDigest::of_bytes(
        &encode_resume(state)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Capture, error.to_string()))?,
    );
    let receipt = process
        .confirm_presentation()
        .map_err(|error| DevelopmentError::at(DevelopmentStage::Restore, error))?;
    let current = process
        .observation()
        .map_err(|error| DevelopmentError::at(DevelopmentStage::Restore, error))?;
    if receipt.state_digest != *expected.as_bytes()
        || receipt.runtime_generation != current.runtime_generation
        || receipt.app_frame == 0
        || receipt.render_frame == 0
        || receipt.surface_generation == 0
        || receipt.backend_instance == 0
        || receipt.device_epoch == 0
    {
        return Err(DevelopmentError::at(
            DevelopmentStage::Restore,
            "desktop presentation does not match the original child's exact canonical state",
        ));
    }
    Ok(receipt)
}

fn restore_exact(process: &mut Process, basis: &ResumeState) -> Result<(), DevelopmentError> {
    process
        .restore(basis)
        .map_err(|error| DevelopmentError::at(DevelopmentStage::Restore, error))?;
    let actual = process
        .capture()
        .map_err(|error| DevelopmentError::at(DevelopmentStage::Restore, error))?;
    if !exact_state(&actual, basis)? {
        return Err(DevelopmentError::at(
            DevelopmentStage::Restore,
            "the complete restored state differs from the original capture",
        ));
    }
    Ok(())
}

fn exact_state(actual: &ResumeState, expected: &ResumeState) -> Result<bool, DevelopmentError> {
    let encode = |state: &ResumeState| {
        encode_resume(state)
            .map_err(|error| DevelopmentError::at(DevelopmentStage::Capture, error.to_string()))
    };
    Ok(encode(actual)? == encode(expected)?)
}

fn validate_commands(commands: &[SemanticCommand]) -> Result<(), DevelopmentError> {
    if commands.len() > MAX_STEP_COMMANDS
        || commands.iter().any(
            |command| matches!(command, SemanticCommand::Move { direction, .. } if *direction >= 4),
        )
    {
        return Err(DevelopmentError::at(
            DevelopmentStage::State,
            "the finite semantic command batch is invalid",
        ));
    }
    Ok(())
}

fn clean_game_retirement(report: &RetirementReport) -> bool {
    report.game_closed
        && !report.terminated
        && report.native.exit().success()
        && report.native.containment_empty()
        && report.native.readers_stopped()
        && report.native.output_drained()
}
