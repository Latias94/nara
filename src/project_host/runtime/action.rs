use std::{fmt, num::NonZeroU32, time::Duration};

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_app::{AppExit, AppRunError, RuntimeCloseCause, RuntimeFaultKind, RuntimeState};
use nara_app::{RuntimeCloseEvidence, RuntimeClosePolicy};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::Resource;
use nara_fs::DirectoryCapability;
use nara_gameplay::GameplayCommandSubmission;
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_winit::{WinitControlFlow, WinitRunner};

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use super::super::{ProductSupervisedDesktopSession, SupervisedDesktopProductIdentity};
use super::super::{
    StartupContentClosureReceipt,
    check::{
        ProjectSourceCheckError, check_project_source_with_host_policy,
        check_project_source_with_overlay,
    },
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use super::desktop_verification::{
    DesktopFirstPresentFailure, DesktopFirstPresentOutcome, DesktopFirstPresentRequest,
    DesktopFirstPresentVerification, DesktopPresentationVerificationReceipt,
};
use super::{
    CleanupDriveOutcome, HostFault, ProjectHost, RuntimeHostPolicyContribution,
    RuntimePluginOverlay, RuntimeStartAttempt, runtime_plan_failure_report,
    runtime_plan_selected_report, single_error,
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use super::{
    append_plugin_shutdown_diagnostics, attach_identifier, runtime_fault_diagnostic,
    runtime_fault_kind_id, single_diagnostic,
};
use crate::ProductRecipe;

type TerminalPredicate<O> = Box<dyn Fn(&O) -> bool + Send + 'static>;

#[derive(Debug, Clone, Copy)]
enum HeadlessDrive {
    ExactTicks(NonZeroU32),
    Frames {
        count: NonZeroU32,
        real_delta: Duration,
    },
}

impl HeadlessDrive {
    fn count(self) -> u32 {
        match self {
            Self::ExactTicks(count) | Self::Frames { count, .. } => count.get(),
        }
    }

    fn limit_report(self) -> DiagnosticReport {
        match self {
            Self::ExactTicks(_) => single_error(
                "project.run.tick-limit",
                "Headless project run reached its fixed-tick limit",
            ),
            Self::Frames { .. } => single_error(
                "project.run.frame-limit",
                "Headless project run reached its frame limit",
            ),
        }
    }
}

/// File-backed headless run intent for one concrete product action.
///
/// The recipe is the complete Rust product-composition input. Candidate, publication, and cleanup
/// choreography remain private to the root Host.
pub struct HeadlessRunIntent<O> {
    profile: Option<String>,
    drive: HeadlessDrive,
    cleanup_policy: RuntimeClosePolicy,
    recipe: ProductRecipe,
    terminal_predicate: Option<TerminalPredicate<O>>,
}

impl<O> fmt::Debug for HeadlessRunIntent<O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeadlessRunIntent")
            .field("profile_present", &self.profile.is_some())
            .field("drive", &self.drive)
            .field("cleanup_policy", &self.cleanup_policy)
            .field("recipe", &self.recipe)
            .field(
                "stops_on_terminal_outcome",
                &self.terminal_predicate.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl<O> HeadlessRunIntent<O> {
    /// Runs exactly this many fixed transactions, without normal gameplay frame stages.
    ///
    /// Entering exact-step mode first runs one paused maintenance frame.
    /// Use [`Self::frames`] when gameplay requires local input collection, background result
    /// integration, variable updates or other normal frame work.
    #[must_use]
    pub fn new(fixed_ticks: NonZeroU32) -> Self {
        Self {
            profile: None,
            drive: HeadlessDrive::ExactTicks(fixed_ticks),
            cleanup_policy: RuntimeClosePolicy::default(),
            recipe: ProductRecipe::new(),
            terminal_predicate: None,
        }
    }

    /// Runs complete managed frames with the supplied real elapsed time per frame.
    ///
    /// Runtime time scale, clamping and fixed catch-up policies apply normally. Each frame may
    /// execute zero, one or multiple fixed ticks; zero real delta still runs frame maintenance.
    /// Neither the frame count nor the delta bounds operating-system I/O or wall-clock duration.
    #[must_use]
    pub fn frames(count: NonZeroU32, real_delta: Duration) -> Self {
        Self {
            drive: HeadlessDrive::Frames { count, real_delta },
            ..Self::new(count)
        }
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    #[must_use]
    pub fn with_cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.cleanup_policy = RuntimeClosePolicy::new(timeout);
        self
    }

    /// Uses one pure product recipe for this project run.
    #[must_use]
    pub fn with_recipe(mut self, recipe: ProductRecipe) -> Self {
        self.recipe = recipe;
        self
    }

    /// Stops after the first complete tick or frame whose typed outcome matches `predicate` or that
    /// publishes an application exit request.
    ///
    /// The selected tick/frame count remains the hard maximum. Reaching it without a match fails
    /// with `project.run.tick-limit` or `project.run.frame-limit` and drives ordinary shutdown.
    #[must_use]
    pub fn stop_when(mut self, predicate: impl Fn(&O) -> bool + Send + 'static) -> Self {
        self.terminal_predicate = Some(Box::new(predicate));
        self
    }
}

/// Product outcome of the latest bounded run drive.
#[derive(Debug, Clone, PartialEq)]
pub enum HeadlessRunOutcome<O> {
    Completed(O),
    Failed,
    CleanupIncomplete,
}

/// One immutable observation of a bounded product action.
#[derive(Debug, Clone)]
pub struct HeadlessRunReport<O> {
    outcome: HeadlessRunOutcome<O>,
    diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
}

impl<O> HeadlessRunReport<O> {
    #[must_use]
    pub const fn outcome(&self) -> &HeadlessRunOutcome<O> {
        &self.outcome
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Returns the exact startup source closure once project checking succeeded.
    ///
    /// Runtime startup, execution, shutdown, or cleanup may still have failed. `None` means the
    /// manifest, product composition, or startup content check itself did not publish a closure.
    #[must_use]
    pub const fn startup_content_closure(&self) -> Option<&StartupContentClosureReceipt> {
        self.startup_content_closure.as_ref()
    }

    #[must_use]
    pub fn into_outcome(self) -> HeadlessRunOutcome<O> {
        self.outcome
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
/// File-backed desktop run intent for the concrete first-party Winit product path.
pub struct DesktopRunIntent {
    profile: Option<String>,
    cleanup_policy: RuntimeClosePolicy,
    recipe: ProductRecipe,
    runner: WinitRunner,
    first_present: Option<DesktopFirstPresentRequest>,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl fmt::Debug for DesktopRunIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopRunIntent")
            .field("profile_present", &self.profile.is_some())
            .field("cleanup_policy", &self.cleanup_policy)
            .field("recipe", &self.recipe)
            .field("runner", &self.runner)
            .field(
                "first_present_mode",
                &self
                    .first_present
                    .as_ref()
                    .map(DesktopFirstPresentRequest::label),
            )
            .finish_non_exhaustive()
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl Default for DesktopRunIntent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl DesktopRunIntent {
    #[must_use]
    pub fn new() -> Self {
        Self {
            profile: None,
            cleanup_policy: RuntimeClosePolicy::default(),
            recipe: ProductRecipe::new(),
            runner: WinitRunner::default(),
            first_present: None,
        }
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    #[must_use]
    pub fn with_cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.cleanup_policy = RuntimeClosePolicy::new(timeout);
        self
    }

    /// Uses one pure product recipe for this desktop run.
    #[must_use]
    pub fn with_recipe(mut self, recipe: ProductRecipe) -> Self {
        self.recipe = recipe;
        self
    }

    #[must_use]
    pub fn with_control_flow(mut self, control_flow: WinitControlFlow) -> Self {
        self.runner = WinitRunner::new(control_flow);
        self
    }

    /// Requires one exact WGPU surface presentation before the desktop run can succeed.
    ///
    /// The fixed product timeout drives normal App and Host shutdown when presentation never
    /// arrives. The generated starter uses this mode for bounded verification; interactive runs
    /// remain unchanged.
    #[must_use]
    pub fn with_first_present_verification(mut self) -> Self {
        self.first_present = Some(DesktopFirstPresentRequest::VerificationCanary);
        self
    }

    /// Publishes one supervised READY attestation after Runtime startup and the first exact
    /// presentation, then keeps the interactive desktop product running.
    #[doc(hidden)]
    #[must_use]
    pub fn with_supervised_desktop_activation(
        mut self,
        session: ProductSupervisedDesktopSession,
        product_identity: SupervisedDesktopProductIdentity,
    ) -> Self {
        self.first_present = Some(DesktopFirstPresentRequest::SupervisedActivation {
            session,
            product_identity,
        });
        self
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRunOutcome {
    Completed(AppExit),
    Failed,
    CleanupIncomplete,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
#[derive(Debug, Clone)]
pub struct DesktopRunReport {
    outcome: DesktopRunOutcome,
    diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
    presentation_verification: Option<DesktopPresentationVerificationReceipt>,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl DesktopRunReport {
    #[must_use]
    pub const fn outcome(&self) -> DesktopRunOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Returns the exact startup source closure once project checking succeeded.
    ///
    /// Runtime startup, presentation, shutdown, or cleanup may still have failed. `None` means the
    /// manifest, product composition, or startup content check itself did not publish a closure.
    #[must_use]
    pub const fn startup_content_closure(&self) -> Option<&StartupContentClosureReceipt> {
        self.startup_content_closure.as_ref()
    }

    /// Returns the first-party desktop presentation proof when verification was requested and
    /// accepted before shutdown.
    #[must_use]
    pub const fn presentation_verification(
        &self,
    ) -> Option<&DesktopPresentationVerificationReceipt> {
        self.presentation_verification.as_ref()
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub struct DesktopRun {
    state: DesktopRunState,
    diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
    presentation_verification: Option<DesktopPresentationVerificationReceipt>,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl fmt::Debug for DesktopRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopRun")
            .field("state", &self.state.label())
            .field("diagnostics", &self.diagnostics)
            .field(
                "startup_content_closure_present",
                &self.startup_content_closure.is_some(),
            )
            .field(
                "presentation_verification_present",
                &self.presentation_verification.is_some(),
            )
            .finish_non_exhaustive()
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl DesktopRun {
    /// Creates the ordinary desktop product action from one recipe.
    #[must_use]
    pub fn from_recipe(project_root: DirectoryCapability, recipe: ProductRecipe) -> Self {
        Self::new(project_root, DesktopRunIntent::new().with_recipe(recipe))
    }

    #[must_use]
    pub fn new(project_root: DirectoryCapability, intent: DesktopRunIntent) -> Self {
        Self::new_inner(project_root, intent, None)
    }

    fn new_inner(
        project_root: DirectoryCapability,
        intent: DesktopRunIntent,
        overlay: Option<RuntimePluginOverlay>,
    ) -> Self {
        Self {
            state: DesktopRunState::Fresh(DesktopRunInputs {
                project_root,
                intent,
                overlay,
            }),
            diagnostics: DiagnosticReport::default(),
            startup_content_closure: None,
            presentation_verification: None,
        }
    }

    pub(crate) fn new_with_overlay(
        project_root: DirectoryCapability,
        intent: DesktopRunIntent,
        overlay: RuntimePluginOverlay,
    ) -> Self {
        Self::new_inner(project_root, intent, Some(overlay))
    }

    /// Runs the event loop once, or advances only retained cleanup after an incomplete close.
    pub fn execute(&mut self) -> DesktopRunReport {
        let state = std::mem::replace(&mut self.state, DesktopRunState::Executing);
        self.state = match state {
            DesktopRunState::Fresh(inputs) => self.execute_fresh(inputs),
            DesktopRunState::Cleaning { hosts, terminal } => self.drive_cleanup(hosts, terminal),
            DesktopRunState::Completed(exit) => DesktopRunState::Completed(exit),
            DesktopRunState::Failed => DesktopRunState::Failed,
            DesktopRunState::Executing => {
                self.diagnostics.extend(single_error(
                    "project.desktop.reentered",
                    "Desktop project run was reentered",
                ));
                DesktopRunState::Failed
            }
        };
        self.report()
    }

    fn drive_cleanup(
        &mut self,
        hosts: Vec<ProjectHost>,
        terminal: PendingDesktopTerminal,
    ) -> DesktopRunState {
        let mut incomplete = Vec::with_capacity(hosts.len());
        let mut cleanup_failed = false;
        for mut host in hosts {
            match host.drive_cleanup_once() {
                CleanupDriveOutcome::Complete {
                    failed,
                    diagnostics,
                } => {
                    cleanup_failed |= failed;
                    self.diagnostics.extend(diagnostics);
                }
                CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete => {
                    incomplete.push(host);
                }
            }
        }
        if !incomplete.is_empty() {
            DesktopRunState::Cleaning {
                hosts: incomplete,
                terminal,
            }
        } else if cleanup_failed {
            DesktopRunState::Failed
        } else {
            terminal.finish()
        }
    }

    fn execute_fresh(&mut self, inputs: DesktopRunInputs) -> DesktopRunState {
        let DesktopRunInputs {
            project_root,
            intent,
            overlay,
        } = inputs;
        let DesktopRunIntent {
            profile,
            cleanup_policy,
            recipe,
            runner,
            first_present,
        } = intent;
        if first_present
            .as_ref()
            .is_some_and(|request| !request.is_bound_to_project_root(&project_root))
        {
            self.diagnostics.extend(single_error(
                "project.desktop.supervised-project-root-mismatch",
                "Desktop supervised activation was verified against another project capability",
            ));
            return DesktopRunState::Failed;
        }
        let verification = first_present.map(DesktopFirstPresentVerification::new);
        let host_policy = verification
            .as_ref()
            .map(DesktopFirstPresentVerification::host_policy);
        let PreparedProjectStart {
            mut host,
            mut attempt,
            plan_diagnostics,
            startup_content_closure,
        } = match prepare_project_start(
            project_root,
            profile,
            cleanup_policy,
            recipe,
            overlay,
            host_policy,
            Vec::new(),
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.startup_content_closure = error.startup_content_closure;
                self.diagnostics.extend(error.fault.diagnostics);
                return DesktopRunState::Failed;
            }
        };
        self.startup_content_closure = Some(startup_content_closure);
        self.diagnostics.extend(plan_diagnostics);
        match host.complete_start(&mut attempt) {
            Ok(diagnostics) => {
                self.diagnostics.extend(diagnostics);
            }
            Err(fault) => {
                self.diagnostics.extend(fault.diagnostics);
                return desktop_state_after_failure(host);
            }
        }
        if let Some(verification) = verification.as_ref() {
            let startup_content_closure = self
                .startup_content_closure
                .as_ref()
                .expect("desktop activation starts only after project checking succeeds");
            if verification.arm(startup_content_closure).is_err() {
                self.diagnostics.extend(single_error(
                    "project.desktop.supervised-runtime-closure-invalid",
                    "Desktop supervised activation could not derive its strict Runtime closure",
                ));
                host.retire_running();
                return desktop_state_after_failure(host);
            }
        }

        let run_result = {
            let runtime = host
                .running_runtime_mut()
                .expect("a completed desktop start publishes one runtime");
            runner.run(runtime)
        };
        let runtime_state = host
            .running_runtime_state()
            .expect("the desktop Host retains its published runtime until terminal lowering");
        let close_evidence = host
            .running_runtime_close_evidence()
            .expect("the desktop Host retains close evidence until terminal lowering");
        let runtime_faulted = host.running_runtime_has_fault();
        let mut terminal = desktop_terminal_after_runner(
            run_result,
            runtime_state,
            &close_evidence,
            runtime_faulted,
            &mut self.diagnostics,
        );
        if let Some(report) = host.running_runtime_close_failure_report() {
            self.diagnostics.extend(report);
        } else {
            append_plugin_shutdown_diagnostics(
                &mut self.diagnostics,
                host.running_runtime_plugin_failure_report(),
            );
        }
        if let Some(verification) = verification {
            match verification.outcome() {
                DesktopFirstPresentOutcome::Accepted(presented_frame) => {
                    let startup_content_closure = self
                        .startup_content_closure
                        .clone()
                        .expect("desktop verification starts only after project checking succeeds");
                    self.presentation_verification =
                        Some(DesktopPresentationVerificationReceipt::new(
                            startup_content_closure,
                            presented_frame,
                        ));
                }
                DesktopFirstPresentOutcome::TimedOut => {
                    self.diagnostics.extend(single_error(
                        "project.desktop.presentation-timeout",
                        "Desktop first presentation exceeded its bounded deadline",
                    ));
                    terminal = PendingDesktopTerminal::Failed;
                }
                DesktopFirstPresentOutcome::Awaiting => {
                    self.diagnostics.extend(single_error(
                        "project.desktop.presentation-missing",
                        "Desktop runner exited before the first presentation was verified",
                    ));
                    terminal = PendingDesktopTerminal::Failed;
                }
                DesktopFirstPresentOutcome::Unarmed => {
                    self.diagnostics.extend(single_error(
                        "project.desktop.presentation-unarmed",
                        "Desktop first-presentation verification was not armed",
                    ));
                    terminal = PendingDesktopTerminal::Failed;
                }
                DesktopFirstPresentOutcome::Publishing => {
                    self.diagnostics.extend(single_error(
                        "project.desktop.supervised-ready-incomplete",
                        "Desktop runner exited before supervised READY publication completed",
                    ));
                    terminal = PendingDesktopTerminal::Failed;
                }
                DesktopFirstPresentOutcome::Failed(DesktopFirstPresentFailure::RuntimeClosure) => {
                    self.diagnostics.extend(single_error(
                        "project.desktop.supervised-runtime-closure-invalid",
                        "Desktop supervised activation did not publish a strict Runtime closure",
                    ));
                    terminal = PendingDesktopTerminal::Failed;
                }
                DesktopFirstPresentOutcome::Failed(
                    DesktopFirstPresentFailure::ReadyPublication,
                ) => {
                    self.diagnostics.extend(single_error(
                        "project.desktop.supervised-ready-failed",
                        "Desktop supervised READY publication failed closed",
                    ));
                    terminal = PendingDesktopTerminal::Failed;
                }
            }
        }
        host.retire_running();
        let mut cleanup_hosts = Vec::with_capacity(1);
        if host.has_cleanup_owner() {
            cleanup_hosts.push(host);
        }
        if cleanup_hosts.is_empty() {
            terminal.finish()
        } else {
            DesktopRunState::Cleaning {
                hosts: cleanup_hosts,
                terminal,
            }
        }
    }

    pub(crate) fn incomplete_cleanup_diagnostics(&self) -> DiagnosticReport {
        let mut diagnostics = DiagnosticReport::default();
        if let DesktopRunState::Cleaning { hosts, .. } = &self.state {
            for host in hosts {
                if let super::ProjectHostSlot::Cleaning { owner, .. } = &host.slot {
                    let empty = RuntimeCloseEvidence::default();
                    diagnostics.extend(super::cleanup_incomplete_report(
                        owner.close_evidence().unwrap_or(&empty),
                        owner.plugin_failure_report(),
                    ));
                }
            }
        }
        diagnostics
    }

    fn report(&self) -> DesktopRunReport {
        let outcome = match &self.state {
            DesktopRunState::Completed(exit) => DesktopRunOutcome::Completed(*exit),
            DesktopRunState::Cleaning { .. } | DesktopRunState::Executing => {
                DesktopRunOutcome::CleanupIncomplete
            }
            DesktopRunState::Fresh(_) | DesktopRunState::Failed => DesktopRunOutcome::Failed,
        };
        DesktopRunReport {
            outcome,
            diagnostics: self.diagnostics.clone(),
            startup_content_closure: self.startup_content_closure.clone(),
            presentation_verification: self.presentation_verification.clone(),
        }
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
struct DesktopRunInputs {
    project_root: DirectoryCapability,
    intent: DesktopRunIntent,
    overlay: Option<RuntimePluginOverlay>,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
enum DesktopRunState {
    Fresh(DesktopRunInputs),
    Executing,
    Cleaning {
        hosts: Vec<ProjectHost>,
        terminal: PendingDesktopTerminal,
    },
    Completed(AppExit),
    Failed,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl DesktopRunState {
    const fn label(&self) -> &'static str {
        match self {
            Self::Fresh(_) => "fresh",
            Self::Executing => "executing",
            Self::Cleaning { .. } => "cleaning",
            Self::Completed(_) => "completed",
            Self::Failed => "failed",
        }
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
#[derive(Debug, Clone, Copy)]
enum PendingDesktopTerminal {
    Completed(AppExit),
    Failed,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
fn desktop_terminal_after_runner(
    run_result: Result<AppExit, AppRunError>,
    runtime_state: RuntimeState,
    close_evidence: &RuntimeCloseEvidence,
    runtime_faulted: bool,
    diagnostics: &mut DiagnosticReport,
) -> PendingDesktopTerminal {
    if runtime_state == RuntimeState::CloseIncomplete {
        if close_evidence
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
        {
            diagnostics.extend(single_error(
                "project.desktop.shutdown-timeout",
                "Desktop runtime shutdown exceeded its bounded deadline",
            ));
        }
        match run_result {
            Err(error) => diagnostics.extend(desktop_runner_failure_report(&error)),
            Ok(_) => diagnostics.extend(single_error(
                "project.desktop.runtime-not-stopped",
                "Desktop runner returned before the managed runtime stopped",
            )),
        };
        return PendingDesktopTerminal::Failed;
    }

    match run_result {
        Ok(exit) if runtime_state == RuntimeState::Stopped => {
            if runtime_faulted {
                diagnostics.extend(single_error(
                    "project.desktop.runtime-faulted",
                    "Desktop runtime retained a runtime fault after stopping",
                ));
                PendingDesktopTerminal::Failed
            } else if close_evidence.has_terminal_failure() {
                diagnostics.extend(single_error(
                    "project.desktop.cleanup-failed",
                    "Desktop runtime cleanup retained a terminal failure",
                ));
                PendingDesktopTerminal::Failed
            } else {
                PendingDesktopTerminal::Completed(exit)
            }
        }
        Ok(_) => {
            diagnostics.extend(single_error(
                "project.desktop.runtime-not-stopped",
                "Desktop runner returned before the managed runtime stopped",
            ));
            PendingDesktopTerminal::Failed
        }
        Err(error) => {
            diagnostics.extend(desktop_runner_failure_report(&error));
            PendingDesktopTerminal::Failed
        }
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl PendingDesktopTerminal {
    fn finish(self) -> DesktopRunState {
        match self {
            Self::Completed(exit) => DesktopRunState::Completed(exit),
            Self::Failed => DesktopRunState::Failed,
        }
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
fn desktop_state_after_failure(host: ProjectHost) -> DesktopRunState {
    if host.has_cleanup_owner() {
        DesktopRunState::Cleaning {
            hosts: vec![host],
            terminal: PendingDesktopTerminal::Failed,
        }
    } else {
        DesktopRunState::Failed
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
fn desktop_runner_failure_report(error: &AppRunError) -> DiagnosticReport {
    let reason = match error {
        AppRunError::Plugin { .. } => "plugin",
        AppRunError::Runner { .. } => "runner",
        AppRunError::RunnerTeardown { .. } => "runner-teardown",
        AppRunError::Time { .. } => "time",
        AppRunError::ManagedRuntime { .. } => "managed-runtime",
    };
    let diagnostic = runtime_fault_diagnostic(
        error.runtime_fault_detail(),
        "project.desktop.runner-failed",
        "Desktop project runner failed",
    );
    let diagnostic = attach_identifier(diagnostic, "reason", reason);
    let diagnostic = match runtime_fault(error) {
        Some((kind, source)) => {
            let diagnostic =
                attach_identifier(diagnostic, "fault-kind", runtime_fault_kind_id(kind));
            attach_identifier(diagnostic, "fault-source", source)
        }
        None => diagnostic,
    };
    single_diagnostic(diagnostic)
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
fn runtime_fault(error: &AppRunError) -> Option<(RuntimeFaultKind, &'static str)> {
    match error {
        AppRunError::ManagedRuntime {
            kind, fault_source, ..
        } => Some((*kind, *fault_source)),
        AppRunError::RunnerTeardown { prior, teardown } => {
            runtime_fault(prior).or_else(|| runtime_fault(teardown))
        }
        AppRunError::Plugin { .. } | AppRunError::Runner { .. } | AppRunError::Time { .. } => None,
    }
}

#[cfg(all(test, feature = "desktop-winit", feature = "render-wgpu"))]
mod desktop_terminal_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use nara_app::{
        App, RuntimeAdmissionReservation, RuntimeCloseParticipant, RuntimeCloseParticipantError,
        RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
        RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger,
    };

    #[derive(Clone, Copy)]
    enum CloseFailureMode {
        Pending,
        RetryableParticipant,
        CompletedWithError,
    }

    struct ControlledCloseParticipant {
        released: Arc<AtomicBool>,
        mode: CloseFailureMode,
    }

    impl RuntimeCloseParticipant for ControlledCloseParticipant {
        fn begin_close(
            &mut self,
            _context: &mut nara_app::RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.close_once()
        }

        fn poll_close(
            &mut self,
            _context: &mut nara_app::RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.close_once()
        }
    }

    impl ControlledCloseParticipant {
        fn close_once(&self) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            if self.released.load(Ordering::Acquire) {
                return Ok(RuntimeCloseProgress::Complete);
            }
            match self.mode {
                CloseFailureMode::Pending => Ok(RuntimeCloseProgress::Pending),
                CloseFailureMode::RetryableParticipant => Err(
                    RuntimeCloseParticipantError::retryable("nara.test.desktop-close-participant"),
                ),
                CloseFailureMode::CompletedWithError => {
                    Ok(RuntimeCloseProgress::CompleteWithError {
                        code: "nara.test.desktop-close-terminal",
                    })
                }
            }
        }
    }

    fn has_code(report: &DiagnosticReport, code: &str) -> bool {
        report
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code)
    }

    fn diagnostic_field(report: &DiagnosticReport, code: &str, key: &str) -> Option<String> {
        report
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == code)
            .and_then(|diagnostic| {
                diagnostic
                    .fields()
                    .iter()
                    .find(|field| field.key().as_str() == key)
            })
            .map(|field| field.display_value().into_owned())
    }

    fn actual_close_evidence(mode: CloseFailureMode) -> RuntimeCloseEvidence {
        let released = Arc::new(AtomicBool::new(false));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.desktop-close-evidence"),
                ControlledCloseParticipant {
                    released: Arc::clone(&released),
                    mode,
                },
            )
            .unwrap();
        let policy = match mode {
            CloseFailureMode::Pending => RuntimeClosePolicy::new(Duration::ZERO),
            CloseFailureMode::RetryableParticipant | CloseFailureMode::CompletedWithError => {
                RuntimeClosePolicy::default()
            }
        };
        let mut runtime = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(App::new().seal().unwrap(), obligations, policy)
            .unwrap()
            .complete_startup()
            .unwrap()
            .promote();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        drive_until_state(&mut runtime, RuntimeState::CloseIncomplete);
        let evidence = runtime.close_evidence().clone();

        released.store(true, Ordering::Release);
        assert!(matches!(
            runtime.request_control(RuntimeControl::RetryClose),
            RuntimeControlRequestResult::Accepted(_)
        ));
        drive_until_state(&mut runtime, RuntimeState::Stopped);
        evidence
    }

    fn drive_until_state(runtime: &mut RuntimeInstance, expected: RuntimeState) {
        for _ in 0..8 {
            if runtime.state() == expected {
                return;
            }
            runtime.drive(Duration::ZERO).unwrap();
        }
        panic!(
            "runtime remained in {:?} instead of reaching {expected:?}",
            runtime.state()
        );
    }

    fn deadline_close_evidence() -> RuntimeCloseEvidence {
        let evidence = actual_close_evidence(CloseFailureMode::Pending);
        assert!(
            evidence
                .causes()
                .contains(&RuntimeCloseCause::DeadlineExceeded)
        );
        evidence
    }

    fn participant_close_evidence() -> RuntimeCloseEvidence {
        let evidence = actual_close_evidence(CloseFailureMode::RetryableParticipant);
        assert!(
            evidence
                .causes()
                .iter()
                .any(|cause| matches!(cause, RuntimeCloseCause::ParticipantError { .. }))
        );
        assert!(
            !evidence
                .causes()
                .contains(&RuntimeCloseCause::DeadlineExceeded)
        );
        evidence
    }

    #[test]
    fn deadline_close_incomplete_reports_shutdown_timeout() {
        let mut diagnostics = DiagnosticReport::default();

        let terminal = desktop_terminal_after_runner(
            Err(AppRunError::runner("managed runtime close is incomplete")),
            RuntimeState::CloseIncomplete,
            &deadline_close_evidence(),
            false,
            &mut diagnostics,
        );

        assert!(matches!(terminal, PendingDesktopTerminal::Failed));
        assert!(has_code(&diagnostics, "project.desktop.shutdown-timeout"));
        assert!(has_code(&diagnostics, "project.desktop.runner-failed"));

        let report = DesktopRun {
            state: DesktopRunState::Cleaning {
                hosts: vec![ProjectHost::new(RuntimeClosePolicy::default())],
                terminal,
            },
            diagnostics,
            startup_content_closure: None,
            presentation_verification: None,
        }
        .report();
        assert_eq!(report.outcome(), DesktopRunOutcome::CleanupIncomplete);
        assert!(has_code(
            report.diagnostics(),
            "project.desktop.shutdown-timeout"
        ));
        assert!(has_code(
            report.diagnostics(),
            "project.desktop.runner-failed"
        ));
    }

    #[test]
    fn participant_close_incomplete_is_not_misclassified_as_timeout() {
        let mut diagnostics = DiagnosticReport::default();

        let terminal = desktop_terminal_after_runner(
            Err(AppRunError::runner(
                "managed runtime close participant failed",
            )),
            RuntimeState::CloseIncomplete,
            &participant_close_evidence(),
            false,
            &mut diagnostics,
        );

        assert!(matches!(terminal, PendingDesktopTerminal::Failed));
        assert!(!has_code(&diagnostics, "project.desktop.shutdown-timeout"));
        assert!(has_code(&diagnostics, "project.desktop.runner-failed"));

        let report = DesktopRun {
            state: terminal.finish(),
            diagnostics,
            startup_content_closure: None,
            presentation_verification: None,
        }
        .report();
        assert_eq!(report.outcome(), DesktopRunOutcome::Failed);
        assert!(!has_code(
            report.diagnostics(),
            "project.desktop.shutdown-timeout"
        ));
        assert!(has_code(
            report.diagnostics(),
            "project.desktop.runner-failed"
        ));
    }

    #[test]
    fn only_a_stopped_runtime_can_complete_the_desktop_product_action() {
        let mut stopped_diagnostics = DiagnosticReport::default();
        assert!(matches!(
            desktop_terminal_after_runner(
                Ok(AppExit::Success),
                RuntimeState::Stopped,
                &RuntimeCloseEvidence::default(),
                false,
                &mut stopped_diagnostics,
            ),
            PendingDesktopTerminal::Completed(AppExit::Success)
        ));
        assert!(stopped_diagnostics.is_empty());

        let mut running_diagnostics = DiagnosticReport::default();
        assert!(matches!(
            desktop_terminal_after_runner(
                Ok(AppExit::Success),
                RuntimeState::Running,
                &RuntimeCloseEvidence::default(),
                false,
                &mut running_diagnostics,
            ),
            PendingDesktopTerminal::Failed
        ));
        assert!(has_code(
            &running_diagnostics,
            "project.desktop.runtime-not-stopped"
        ));
    }

    #[test]
    fn stopped_runtime_with_close_error_or_fault_is_not_reported_as_completed() {
        let mut close_error_diagnostics = DiagnosticReport::default();
        let terminal_evidence = terminal_close_evidence();
        assert!(matches!(
            desktop_terminal_after_runner(
                Ok(AppExit::Success),
                RuntimeState::Stopped,
                &terminal_evidence,
                false,
                &mut close_error_diagnostics,
            ),
            PendingDesktopTerminal::Failed
        ));
        assert!(has_code(
            &close_error_diagnostics,
            "project.desktop.cleanup-failed"
        ));

        let mut fault_diagnostics = DiagnosticReport::default();
        assert!(matches!(
            desktop_terminal_after_runner(
                Ok(AppExit::Success),
                RuntimeState::Stopped,
                &RuntimeCloseEvidence::default(),
                true,
                &mut fault_diagnostics,
            ),
            PendingDesktopTerminal::Failed
        ));
        assert!(has_code(
            &fault_diagnostics,
            "project.desktop.runtime-faulted"
        ));
    }

    fn terminal_close_evidence() -> RuntimeCloseEvidence {
        let released = Arc::new(AtomicBool::new(false));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.test.desktop-terminal-evidence"),
                ControlledCloseParticipant {
                    released,
                    mode: CloseFailureMode::CompletedWithError,
                },
            )
            .unwrap();
        let mut runtime = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                App::new().seal().unwrap(),
                obligations,
                RuntimeClosePolicy::default(),
            )
            .unwrap()
            .complete_startup()
            .unwrap()
            .promote();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        drive_until_state(&mut runtime, RuntimeState::Stopped);
        runtime.close_evidence().clone()
    }

    #[test]
    fn desktop_runner_failure_preserves_managed_runtime_fault_identity() {
        let managed =
            AppRunError::managed_runtime(RuntimeFaultKind::System, "nara.ecs.fallible-execution");
        let report = desktop_runner_failure_report(&managed);
        assert_eq!(
            diagnostic_field(&report, "project.desktop.runner-failed", "reason").as_deref(),
            Some("managed-runtime")
        );
        assert_eq!(
            diagnostic_field(&report, "project.desktop.runner-failed", "fault-kind").as_deref(),
            Some("system")
        );
        assert_eq!(
            diagnostic_field(&report, "project.desktop.runner-failed", "fault-source").as_deref(),
            Some("nara.ecs.fallible-execution")
        );

        let wrapped = AppRunError::runner_teardown(managed, AppRunError::runner("teardown"));
        let report = desktop_runner_failure_report(&wrapped);
        assert_eq!(
            diagnostic_field(&report, "project.desktop.runner-failed", "reason").as_deref(),
            Some("runner-teardown")
        );
        assert_eq!(
            diagnostic_field(&report, "project.desktop.runner-failed", "fault-kind").as_deref(),
            Some("system")
        );
    }
}

/// Stateful root-owned headless product action.
///
/// Calling [`Self::execute_bounded`] after incomplete cleanup only drives the retained owner. It
/// never reopens the manifest, rebuilds the project snapshot, resubmits commands, or starts a
/// second runtime.
pub struct HeadlessRun<O>
where
    O: Resource + Clone,
{
    state: HeadlessRunState<O>,
    diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
}

impl<O> fmt::Debug for HeadlessRun<O>
where
    O: Resource + Clone,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeadlessRun")
            .field("state", &self.state.label())
            .field("diagnostics", &self.diagnostics)
            .field(
                "startup_content_closure_present",
                &self.startup_content_closure.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl<O> HeadlessRun<O>
where
    O: Resource + Clone,
{
    /// Creates the ordinary bounded headless product action from one recipe.
    #[must_use]
    pub fn from_recipe(
        project_root: DirectoryCapability,
        recipe: ProductRecipe,
        fixed_ticks: NonZeroU32,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self::new(
            project_root,
            HeadlessRunIntent::new(fixed_ticks).with_recipe(recipe),
            commands,
        )
    }

    /// Creates a product run from an already-owned semantic-command buffer.
    ///
    /// Ownership does not bypass admission: every submission is still validated by the runtime
    /// command queue before the product action can complete.
    #[must_use]
    pub fn new(
        project_root: DirectoryCapability,
        intent: HeadlessRunIntent<O>,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self::new_inner(project_root, intent, None, commands)
    }

    fn new_inner(
        project_root: DirectoryCapability,
        intent: HeadlessRunIntent<O>,
        overlay: Option<RuntimePluginOverlay>,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self {
            state: HeadlessRunState::Fresh(HeadlessRunInputs {
                project_root,
                intent,
                overlay,
                commands,
            }),
            diagnostics: DiagnosticReport::default(),
            startup_content_closure: None,
        }
    }

    pub(crate) fn new_with_overlay(
        project_root: DirectoryCapability,
        intent: HeadlessRunIntent<O>,
        overlay: RuntimePluginOverlay,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self::new_inner(project_root, intent, Some(overlay), commands)
    }

    pub fn execute_bounded(&mut self) -> HeadlessRunReport<O> {
        let state = std::mem::replace(&mut self.state, HeadlessRunState::Executing);
        self.state = match state {
            HeadlessRunState::Fresh(inputs) => self.execute_fresh(inputs),
            HeadlessRunState::Cleaning { mut host, terminal } => match host.drive_cleanup_once() {
                CleanupDriveOutcome::Complete { failed: false, .. } => terminal.finish(),
                CleanupDriveOutcome::Complete {
                    failed: true,
                    diagnostics,
                } => {
                    self.diagnostics.extend(diagnostics);
                    HeadlessRunState::Failed
                }
                CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete => {
                    HeadlessRunState::Cleaning { host, terminal }
                }
            },
            HeadlessRunState::Completed(outcome) => HeadlessRunState::Completed(outcome),
            HeadlessRunState::Failed => HeadlessRunState::Failed,
            HeadlessRunState::Executing => {
                self.diagnostics.extend(single_error(
                    "project.run.reentered",
                    "Headless project run was reentered",
                ));
                HeadlessRunState::Failed
            }
        };
        self.report()
    }

    fn execute_fresh(&mut self, inputs: HeadlessRunInputs<O>) -> HeadlessRunState<O> {
        let PreparedStart {
            mut host,
            mut attempt,
            drive,
            terminal_predicate,
            plan_diagnostics,
            startup_content_closure,
        } = match prepare_start(inputs) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.startup_content_closure = error.startup_content_closure;
                self.diagnostics.extend(error.fault.diagnostics);
                return HeadlessRunState::Failed;
            }
        };
        self.startup_content_closure = Some(startup_content_closure);
        self.diagnostics.extend(plan_diagnostics);

        match host.complete_start(&mut attempt) {
            Ok(diagnostics) => {
                self.diagnostics.extend(diagnostics);
            }
            Err(fault) => {
                self.diagnostics.extend(fault.diagnostics);
                return state_after_failure(host);
            }
        }

        let mut terminal_outcome = None;
        for _ in 0..drive.count() {
            let driven = match drive {
                HeadlessDrive::ExactTicks(_) => host.drive_one_fixed_tick(),
                HeadlessDrive::Frames { real_delta, .. } => host.drive_one_frame(real_delta),
            };
            let app_exit = match driven {
                Ok(app_exit) => app_exit,
                Err(fault) => {
                    self.diagnostics.extend(fault.diagnostics);
                    host.retire_running();
                    return state_after_failure(host);
                }
            };
            if app_exit.is_some() {
                let outcome = match host.capture_outcome::<O>() {
                    Ok(outcome) => outcome,
                    Err(fault) => {
                        self.diagnostics.extend(fault.diagnostics);
                        host.retire_running();
                        return state_after_failure(host);
                    }
                };
                terminal_outcome = Some(outcome);
                break;
            }
            if let Some(predicate) = terminal_predicate.as_ref() {
                let outcome = match host.capture_outcome_if::<O>(predicate.as_ref()) {
                    Ok(Some(outcome)) => outcome,
                    Ok(None) => continue,
                    Err(fault) => {
                        self.diagnostics.extend(fault.diagnostics);
                        host.retire_running();
                        return state_after_failure(host);
                    }
                };
                terminal_outcome = Some(outcome);
                break;
            }
        }

        let outcome = if let Some(outcome) = terminal_outcome {
            outcome
        } else if terminal_predicate.is_some() {
            self.diagnostics.extend(drive.limit_report());
            if let Err(fault) = host.stop_running() {
                self.diagnostics.extend(fault.diagnostics);
            }
            return state_after_failure(host);
        } else {
            match host.capture_outcome::<O>() {
                Ok(outcome) => outcome,
                Err(fault) => {
                    self.diagnostics.extend(fault.diagnostics);
                    host.retire_running();
                    return state_after_failure(host);
                }
            }
        };

        match host.stop_running() {
            Ok(()) => HeadlessRunState::Completed(outcome),
            Err(fault) => {
                let failed = fault.diagnostics.has_errors();
                self.diagnostics.extend(fault.diagnostics);
                if host.has_cleanup_owner() {
                    HeadlessRunState::Cleaning {
                        host: Box::new(host),
                        terminal: if failed {
                            PendingTerminal::Failed
                        } else {
                            PendingTerminal::Completed(outcome)
                        },
                    }
                } else {
                    HeadlessRunState::Failed
                }
            }
        }
    }

    fn report(&self) -> HeadlessRunReport<O> {
        let outcome = match &self.state {
            HeadlessRunState::Completed(outcome) => HeadlessRunOutcome::Completed(outcome.clone()),
            HeadlessRunState::Cleaning { .. } | HeadlessRunState::Executing => {
                HeadlessRunOutcome::CleanupIncomplete
            }
            HeadlessRunState::Fresh(_) | HeadlessRunState::Failed => HeadlessRunOutcome::Failed,
        };
        HeadlessRunReport {
            outcome,
            diagnostics: self.diagnostics.clone(),
            startup_content_closure: self.startup_content_closure.clone(),
        }
    }

    pub(crate) fn incomplete_cleanup_diagnostics(&self) -> DiagnosticReport {
        let mut diagnostics = DiagnosticReport::default();
        if let HeadlessRunState::Cleaning { host, .. } = &self.state
            && let super::ProjectHostSlot::Cleaning { owner, .. } = &host.slot
        {
            let empty = RuntimeCloseEvidence::default();
            diagnostics.extend(super::cleanup_incomplete_report(
                owner.close_evidence().unwrap_or(&empty),
                owner.plugin_failure_report(),
            ));
        }
        diagnostics
    }
}

struct HeadlessRunInputs<O> {
    project_root: DirectoryCapability,
    intent: HeadlessRunIntent<O>,
    overlay: Option<RuntimePluginOverlay>,
    commands: Vec<GameplayCommandSubmission>,
}

struct PreparedStart<O> {
    host: ProjectHost,
    attempt: RuntimeStartAttempt,
    drive: HeadlessDrive,
    terminal_predicate: Option<TerminalPredicate<O>>,
    plan_diagnostics: DiagnosticReport,
    startup_content_closure: StartupContentClosureReceipt,
}

struct PreparedProjectStart {
    host: ProjectHost,
    attempt: RuntimeStartAttempt,
    plan_diagnostics: DiagnosticReport,
    startup_content_closure: StartupContentClosureReceipt,
}

struct PreparedProjectStartError {
    fault: HostFault,
    startup_content_closure: Option<StartupContentClosureReceipt>,
}

enum HeadlessRunState<O> {
    Fresh(HeadlessRunInputs<O>),
    Executing,
    Cleaning {
        host: Box<ProjectHost>,
        terminal: PendingTerminal<O>,
    },
    Completed(O),
    Failed,
}

impl<O> HeadlessRunState<O> {
    const fn label(&self) -> &'static str {
        match self {
            Self::Fresh(_) => "fresh",
            Self::Executing => "executing",
            Self::Cleaning { .. } => "cleaning",
            Self::Completed(_) => "completed",
            Self::Failed => "failed",
        }
    }
}

enum PendingTerminal<O> {
    Completed(O),
    Failed,
}

impl<O> PendingTerminal<O> {
    fn finish(self) -> HeadlessRunState<O> {
        match self {
            Self::Completed(outcome) => HeadlessRunState::Completed(outcome),
            Self::Failed => HeadlessRunState::Failed,
        }
    }
}

fn state_after_failure<O>(host: ProjectHost) -> HeadlessRunState<O> {
    if host.has_cleanup_owner() {
        HeadlessRunState::Cleaning {
            host: Box::new(host),
            terminal: PendingTerminal::Failed,
        }
    } else {
        HeadlessRunState::Failed
    }
}

fn prepare_start<O>(
    inputs: HeadlessRunInputs<O>,
) -> Result<PreparedStart<O>, PreparedProjectStartError> {
    let HeadlessRunInputs {
        project_root,
        intent,
        overlay,
        commands,
    } = inputs;
    let HeadlessRunIntent {
        profile,
        drive,
        cleanup_policy,
        recipe,
        terminal_predicate,
    } = intent;
    let prepared = prepare_project_start(
        project_root,
        profile,
        cleanup_policy,
        recipe,
        overlay,
        None,
        commands,
    )?;
    Ok(PreparedStart {
        host: prepared.host,
        attempt: prepared.attempt,
        drive,
        terminal_predicate,
        plan_diagnostics: prepared.plan_diagnostics,
        startup_content_closure: prepared.startup_content_closure,
    })
}

fn prepare_project_start(
    project_root: DirectoryCapability,
    profile: Option<String>,
    cleanup_policy: RuntimeClosePolicy,
    recipe: ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: Option<RuntimeHostPolicyContribution>,
    commands: Vec<GameplayCommandSubmission>,
) -> Result<PreparedProjectStart, PreparedProjectStartError> {
    let checked = match host_policy {
        Some(host_policy) => check_project_source_with_host_policy(
            project_root,
            profile.as_deref(),
            recipe,
            overlay,
            host_policy,
        ),
        None => {
            check_project_source_with_overlay(project_root, profile.as_deref(), recipe, overlay)
        }
    }
    .map_err(|error| PreparedProjectStartError {
        fault: project_source_check_fault(error),
        startup_content_closure: None,
    })?;
    let (candidate, plan, snapshot, loader) = checked.into_delivery_parts();
    let plan_diagnostics = runtime_plan_selected_report(&plan);
    let startup_content_closure = snapshot
        .startup_content_closure()
        .expect("the runtime startup check selects the manifest's default Scene");

    let mut host = ProjectHost::new(cleanup_policy);
    let attempt = host
        .begin_project_start(
            snapshot,
            plan,
            candidate,
            std::sync::Arc::new(loader),
            commands,
        )
        .map_err(|fault| PreparedProjectStartError {
            fault,
            startup_content_closure: Some(startup_content_closure.clone()),
        })?;
    Ok(PreparedProjectStart {
        host,
        attempt,
        plan_diagnostics,
        startup_content_closure,
    })
}

fn project_source_check_fault(error: ProjectSourceCheckError) -> HostFault {
    let diagnostics = match error {
        ProjectSourceCheckError::Manifest(error) => error.diagnostics().clone(),
        ProjectSourceCheckError::ProductComposition(error) => runtime_plan_failure_report(&error),
        ProjectSourceCheckError::StartupContent { error, .. } => error.diagnostics().clone(),
    };
    HostFault::new(diagnostics)
}
