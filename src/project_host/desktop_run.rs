//! One concrete native desktop action for code-first and optional project-backed Runtimes.

use std::{fmt, time::Duration};

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_app::RuntimeClosePolicy;
use nara_app::{
    AppExit, AppRunError, RuntimeCloseCause, RuntimeCloseEvidence, RuntimeFault, RuntimeFaultKind,
    RuntimeState,
};
use nara_diagnostic::DiagnosticReport;
use nara_winit::{WinitControlFlow, WinitRunner, WinitSession};

mod presentation;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) mod verification;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use super::runtime_diagnostics::cleanup_incomplete_report;
use super::runtime_diagnostics::{
    append_plugin_shutdown_diagnostics, attach_identifier, cleanup_failed_report,
    runtime_fault_diagnostic, runtime_fault_kind_id, runtime_fault_report, single_diagnostic,
    single_error,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use super::{
    StartupContentClosureReceipt,
    runtime::{DesktopRunInputs, DesktopRuntimeContext, ProjectHost},
};
pub use presentation::{
    DesktopPresentationControl, DesktopPresentationObservation, DesktopTelemetrySealOutcome,
    DesktopTelemetrySealReport,
};
use presentation::{
    DesktopPresentationDriver, DesktopPresentationOptions, DesktopPresentationReports,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use verification::DesktopPresentationVerificationReceipt;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use verification::{
    DesktopFirstPresentFailure, DesktopFirstPresentOutcome, DesktopFirstPresentVerification,
};

/// Desktop execution policy. Native retirement has its own finite drive budget.
#[derive(Debug)]
pub struct DesktopRunOptions {
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(super) cleanup_policy: RuntimeClosePolicy,
    native_retirement_budget: Duration,
    runner: WinitRunner,
    presentation: DesktopPresentationOptions,
}

impl Default for DesktopRunOptions {
    fn default() -> Self {
        Self {
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            cleanup_policy: RuntimeClosePolicy::default(),
            native_retirement_budget: Duration::from_secs(5),
            runner: WinitRunner::default(),
            presentation: DesktopPresentationOptions::default(),
        }
    }
}

impl DesktopRunOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    /// Selects logical Runtime close policy for source/processed construction.
    /// An already-started Runtime retains its original policy; see [`DesktopRun::from_runtime`].
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub fn with_cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.cleanup_policy = RuntimeClosePolicy::new(timeout);
        self
    }

    /// Bounds each native session retirement drive, independently of Runtime logical close.
    #[must_use]
    pub fn with_native_retirement_budget(mut self, budget: Duration) -> Self {
        self.native_retirement_budget = budget;
        self
    }

    /// Configures the local native owner before its first submission.
    #[must_use]
    pub fn with_telemetry(mut self, config: nara_render_wgpu::WgpuBackendTelemetryConfig) -> Self {
        self.presentation.telemetry = Some(config);
        self
    }

    /// Observes actual post-Host CPU facts and finite maintenance, without native GPU access.
    #[must_use]
    pub fn with_presentation_observer(
        mut self,
        observer: impl FnMut(DesktopPresentationObservation<'_>) -> DesktopPresentationControl + 'static,
    ) -> Self {
        self.presentation.observer = Some(Box::new(observer));
        self
    }

    #[must_use]
    pub fn with_control_flow(mut self, control_flow: WinitControlFlow) -> Self {
        self.runner = WinitRunner::new(control_flow);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRunOutcome {
    Completed(AppExit),
    Failed,
    CleanupIncomplete,
}

#[derive(Debug, Clone)]
pub struct DesktopRunReport {
    outcome: DesktopRunOutcome,
    diagnostics: DiagnosticReport,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    startup_content_closure: Option<StartupContentClosureReceipt>,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    presentation_verification: Option<DesktopPresentationVerificationReceipt>,
    telemetry_seal: Option<DesktopTelemetrySealReport>,
}

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
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub const fn startup_content_closure(&self) -> Option<&StartupContentClosureReceipt> {
        self.startup_content_closure.as_ref()
    }

    /// Returns the first-party desktop presentation proof when verification was requested and
    /// accepted before shutdown.
    #[must_use]
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub const fn presentation_verification(
        &self,
    ) -> Option<&DesktopPresentationVerificationReceipt> {
        self.presentation_verification.as_ref()
    }

    /// Final telemetry counters and remaining samples, captured before device-epoch cleanup.
    #[must_use]
    pub const fn telemetry_seal(&self) -> Option<&DesktopTelemetrySealReport> {
        self.telemetry_seal.as_ref()
    }
}

pub struct DesktopRun {
    pub(super) state: DesktopRunState,
    pub(super) diagnostics: DiagnosticReport,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(super) startup_content_closure: Option<StartupContentClosureReceipt>,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(super) presentation_verification: Option<DesktopPresentationVerificationReceipt>,
    pub(super) telemetry_seal: Option<DesktopTelemetrySealReport>,
}

impl fmt::Debug for DesktopRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("DesktopRun");
        debug
            .field("state", &self.state.label())
            .field("diagnostics", &self.diagnostics);
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        debug
            .field(
                "startup_content_closure_present",
                &self.startup_content_closure.is_some(),
            )
            .field(
                "presentation_verification_present",
                &self.presentation_verification.is_some(),
            );
        debug.finish_non_exhaustive()
    }
}

impl DesktopRun {
    /// Transfers an already-started Runtime into the ordinary owned native desktop action.
    ///
    /// Its logical close policy remains the one selected at construction. Only
    /// `with_native_retirement_budget` bounds a subsequent finite native retirement drive;
    /// `with_cleanup_timeout` is a source/processed startup option.
    #[must_use]
    pub fn from_runtime(runtime: nara_app::RuntimeInstance, execution: DesktopRunOptions) -> Self {
        Self {
            state: DesktopRunState::FreshRuntime { runtime, execution },
            diagnostics: DiagnosticReport::default(),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            startup_content_closure: None,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            presentation_verification: None,
            telemetry_seal: None,
        }
    }

    /// Runs the event loop once, or advances only retained cleanup after an incomplete close.
    pub fn execute(&mut self) -> DesktopRunReport {
        let state = std::mem::replace(&mut self.state, DesktopRunState::Executing);
        self.state = match state {
            DesktopRunState::FreshRuntime { runtime, execution } => self.start_native(
                runtime,
                #[cfg(all(feature = "runtime-2d", feature = "serde"))]
                None,
                execution,
                #[cfg(all(feature = "runtime-2d", feature = "serde"))]
                None,
            ),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            DesktopRunState::Fresh(inputs) => self.execute_fresh(inputs),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            DesktopRunState::Cleaning { hosts, terminal } => self.drive_cleanup(hosts, terminal),
            DesktopRunState::Native(mut native) => {
                native.session.execute();
                self.finish_native(*native)
            }
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

    pub(super) fn start_native(
        &mut self,
        runtime: nara_app::RuntimeInstance,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))] context: Option<
            DesktopRuntimeContext,
        >,
        execution: DesktopRunOptions,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))] verification: Option<
            DesktopFirstPresentVerification,
        >,
    ) -> DesktopRunState {
        let reports = DesktopPresentationReports::default();
        let driver = DesktopPresentationDriver::new(
            &runtime,
            execution.presentation,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            verification.clone(),
            reports.clone(),
        );
        // This consumes the one published Runtime. Its publication slot is never refilled.
        let session = execution.runner.__run_with_presentation(
            runtime,
            Box::new(driver),
            execution.native_retirement_budget,
        );
        self.finish_native(DesktopNativeRun {
            session,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            _context: context,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            verification,
            reports,
        })
    }

    fn finish_native(&mut self, native: DesktopNativeRun) -> DesktopRunState {
        self.telemetry_seal = native.reports.telemetry_seal();
        if !native.session.is_retired() {
            return DesktopRunState::Native(Box::new(native));
        }
        let DesktopNativeRun {
            session,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            _context,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            verification,
            reports,
        } = native;
        let retired = match session.into_retired() {
            Ok(retired) => retired,
            Err(session) => {
                return DesktopRunState::Native(Box::new(DesktopNativeRun {
                    session,
                    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
                    _context,
                    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
                    verification,
                    reports,
                }));
            }
        };
        let runtime = retired.runtime;
        let mut terminal = desktop_terminal_after_runner(
            retired.result.map(|exit| reports.reconcile_exit(exit)),
            runtime.state(),
            runtime.close_evidence(),
            runtime.fault(),
            &mut self.diagnostics,
        );
        if runtime.close_evidence().has_terminal_failure() {
            self.diagnostics.extend(cleanup_failed_report(
                runtime.close_evidence(),
                runtime.plugin_failure_report(),
            ));
        } else {
            append_plugin_shutdown_diagnostics(
                &mut self.diagnostics,
                runtime.plugin_failure_report(),
            );
        }
        if self
            .telemetry_seal
            .as_ref()
            .is_some_and(|report| report.outcome() != DesktopTelemetrySealOutcome::Complete)
        {
            self.diagnostics.extend(single_error(
                "project.desktop.telemetry-seal-failed",
                "Desktop telemetry did not seal with exact completed evidence before epoch retirement",
            ));
            terminal = PendingDesktopTerminal::Failed;
        }
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
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
        // Native extraction is legal only after the Runtime and original native aggregate retired.
        drop(runtime);
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        drop(_context);
        terminal.finish()
    }

    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    pub(crate) fn incomplete_cleanup_diagnostics(&self) -> DiagnosticReport {
        let mut diagnostics = DiagnosticReport::default();
        self.append_startup_cleanup_diagnostics(&mut diagnostics);
        if let DesktopRunState::Native(native) = &self.state {
            let runtime = native.session.runtime();
            diagnostics.extend(cleanup_incomplete_report(
                runtime.close_evidence(),
                runtime.plugin_failure_report(),
            ));
            diagnostics.extend(single_error(
                "project.desktop.native-retirement-incomplete",
                "Desktop native retirement remains incomplete and retains its original session",
            ));
        }
        diagnostics
    }

    fn report(&self) -> DesktopRunReport {
        let outcome = match &self.state {
            DesktopRunState::Completed(exit) => DesktopRunOutcome::Completed(*exit),
            DesktopRunState::Native(_) | DesktopRunState::Executing => {
                DesktopRunOutcome::CleanupIncomplete
            }
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            DesktopRunState::Cleaning { .. } => DesktopRunOutcome::CleanupIncomplete,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            DesktopRunState::Fresh(_) => DesktopRunOutcome::Failed,
            DesktopRunState::FreshRuntime { .. } | DesktopRunState::Failed => {
                DesktopRunOutcome::Failed
            }
        };
        DesktopRunReport {
            outcome,
            diagnostics: self.diagnostics.clone(),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            startup_content_closure: self.startup_content_closure.clone(),
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            presentation_verification: self.presentation_verification.clone(),
            telemetry_seal: self.telemetry_seal.clone(),
        }
    }
}

pub(super) enum DesktopRunState {
    FreshRuntime {
        runtime: nara_app::RuntimeInstance,
        execution: DesktopRunOptions,
    },
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    Fresh(DesktopRunInputs),
    Executing,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    Cleaning {
        hosts: Vec<ProjectHost>,
        terminal: PendingDesktopTerminal,
    },
    Native(Box<DesktopNativeRun>),
    Completed(AppExit),
    Failed,
}

pub(super) struct DesktopNativeRun {
    session: WinitSession,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    _context: Option<DesktopRuntimeContext>,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    verification: Option<DesktopFirstPresentVerification>,
    reports: DesktopPresentationReports,
}

impl DesktopRunState {
    const fn label(&self) -> &'static str {
        match self {
            Self::FreshRuntime { .. } => "fresh-runtime",
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            Self::Fresh(_) => "fresh",
            Self::Executing => "executing",
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            Self::Cleaning { .. } => "startup-cleaning",
            Self::Native(_) => "native-retirement",
            Self::Completed(_) => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PendingDesktopTerminal {
    Completed(AppExit),
    Failed,
}

fn desktop_terminal_after_runner(
    run_result: Result<AppExit, AppRunError>,
    runtime_state: RuntimeState,
    close_evidence: &RuntimeCloseEvidence,
    runtime_fault: Option<&RuntimeFault>,
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
            if let Some(fault) = runtime_fault {
                diagnostics.extend(runtime_fault_report(
                    fault,
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

impl PendingDesktopTerminal {
    pub(super) fn finish(self) -> DesktopRunState {
        match self {
            Self::Completed(exit) => DesktopRunState::Completed(exit),
            Self::Failed => DesktopRunState::Failed,
        }
    }
}

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

#[cfg(test)]
mod tests;
