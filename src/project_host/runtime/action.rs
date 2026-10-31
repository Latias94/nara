use std::{fmt, num::NonZeroU32, time::Duration};

use nara_app::{RuntimeCloseEvidence, RuntimeClosePolicy};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::Resource;
use nara_fs::DirectoryCapability;
use nara_gameplay::GameplayCommandSubmission;
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_winit::WinitControlFlow;

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use super::super::desktop_run::{
    DesktopRun, DesktopRunOptions, DesktopRunState, PendingDesktopTerminal,
    verification::{DesktopFirstPresentRequest, DesktopFirstPresentVerification},
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use super::super::{ProductSupervisedDesktopSession, SupervisedDesktopProductIdentity};
use super::super::{
    StartupContentClosureReceipt,
    check::{
        ProjectSourceCheckError, check_project_source_with_host_policy,
        check_project_source_with_overlay,
    },
};
use super::{
    CleanupDriveOutcome, HostFault, ProjectHost, RuntimeHostPolicyContribution,
    RuntimePluginOverlay, RuntimeStartAttempt, runtime_plan_failure_report,
    runtime_plan_selected_report, single_error,
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

/// Execution policy shared by source-backed and processed headless runs.
pub struct HeadlessRunOptions<O> {
    drive: HeadlessDrive,
    cleanup_policy: RuntimeClosePolicy,
    terminal_predicate: Option<TerminalPredicate<O>>,
}

impl<O> fmt::Debug for HeadlessRunOptions<O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeadlessRunOptions")
            .field("drive", &self.drive)
            .field("cleanup_policy", &self.cleanup_policy)
            .field(
                "stops_on_terminal_outcome",
                &self.terminal_predicate.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl<O> HeadlessRunOptions<O> {
    #[must_use]
    pub fn new(fixed_ticks: NonZeroU32) -> Self {
        Self {
            drive: HeadlessDrive::ExactTicks(fixed_ticks),
            cleanup_policy: RuntimeClosePolicy::default(),
            terminal_predicate: None,
        }
    }

    #[must_use]
    pub fn frames(count: NonZeroU32, real_delta: Duration) -> Self {
        Self {
            drive: HeadlessDrive::Frames { count, real_delta },
            ..Self::new(count)
        }
    }

    #[must_use]
    pub fn with_cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.cleanup_policy = RuntimeClosePolicy::new(timeout);
        self
    }

    #[must_use]
    pub fn stop_when(mut self, predicate: impl Fn(&O) -> bool + Send + 'static) -> Self {
        self.terminal_predicate = Some(Box::new(predicate));
        self
    }
}

/// File-backed headless run intent for one concrete product action.
///
/// The recipe is the complete Rust product-composition input. Candidate, publication, and cleanup
/// choreography remain private to the root Host.
pub struct HeadlessRunIntent<O> {
    profile: Option<String>,
    recipe: ProductRecipe,
    execution: HeadlessRunOptions<O>,
}

impl<O> fmt::Debug for HeadlessRunIntent<O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeadlessRunIntent")
            .field("profile_present", &self.profile.is_some())
            .field("execution", &self.execution)
            .field("recipe", &self.recipe)
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
            recipe: ProductRecipe::new(),
            execution: HeadlessRunOptions::new(fixed_ticks),
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
            execution: HeadlessRunOptions::frames(count, real_delta),
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
        self.execution = self.execution.with_cleanup_timeout(timeout);
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
        self.execution = self.execution.stop_when(predicate);
        self
    }

    #[must_use]
    pub fn with_execution(mut self, execution: HeadlessRunOptions<O>) -> Self {
        self.execution = execution;
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
    /// manifest, product composition, or startup content check itself did not publish a closure,
    /// or that this run selected an already admitted processed Runtime base.
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
    recipe: ProductRecipe,
    execution: DesktopRunOptions,
    first_present: Option<DesktopFirstPresentRequest>,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl fmt::Debug for DesktopRunIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopRunIntent")
            .field("profile_present", &self.profile.is_some())
            .field("execution", &self.execution)
            .field("recipe", &self.recipe)
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
            recipe: ProductRecipe::new(),
            execution: DesktopRunOptions::new(),
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
        self.execution = self.execution.with_cleanup_timeout(timeout);
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
        self.execution = self.execution.with_control_flow(control_flow);
        self
    }

    #[must_use]
    pub fn with_execution(mut self, execution: DesktopRunOptions) -> Self {
        self.execution = execution;
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

    /// Runs an admitted processed base through the same desktop Runtime and close owner.
    ///
    /// Source-supervised activation and its source-closure attestation remain source operations.
    #[must_use]
    pub fn from_runtime_content(
        base: crate::runtime_content::RuntimeContentBase,
        mut execution: DesktopRunOptions,
    ) -> Self {
        let event_loop = execution.event_loop.take();
        Self {
            state: DesktopRunState::Fresh(DesktopRunInputs::Processed { base, execution }),
            diagnostics: DiagnosticReport::default(),
            startup_content_closure: None,
            presentation_verification: None,
            telemetry_seal: None,
            event_loop,
        }
    }

    fn new_inner(
        project_root: DirectoryCapability,
        mut intent: DesktopRunIntent,
        overlay: Option<RuntimePluginOverlay>,
    ) -> Self {
        let event_loop = intent.execution.event_loop.take();
        Self {
            state: DesktopRunState::Fresh(DesktopRunInputs::Source {
                project_root,
                intent,
                overlay,
            }),
            diagnostics: DiagnosticReport::default(),
            startup_content_closure: None,
            presentation_verification: None,
            telemetry_seal: None,
            event_loop,
        }
    }

    pub(crate) fn new_with_overlay(
        project_root: DirectoryCapability,
        intent: DesktopRunIntent,
        overlay: RuntimePluginOverlay,
    ) -> Self {
        Self::new_inner(project_root, intent, Some(overlay))
    }

    pub(in crate::project_host) fn drive_cleanup(
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
                    ..
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

    pub(in crate::project_host) fn execute_fresh(
        &mut self,
        inputs: DesktopRunInputs,
    ) -> DesktopRunState {
        let (prepared, execution, verification) = match inputs {
            DesktopRunInputs::Source {
                project_root,
                intent,
                overlay,
            } => {
                let DesktopRunIntent {
                    profile,
                    recipe,
                    execution,
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
                let prepared = prepare_project_start(
                    project_root,
                    profile,
                    execution.cleanup_policy,
                    recipe,
                    overlay,
                    None,
                    Vec::new(),
                );
                (prepared, execution, verification)
            }
            DesktopRunInputs::Processed { base, execution } => (
                prepare_runtime_content_start(base, execution.cleanup_policy, Vec::new()),
                execution,
                None,
            ),
        };
        let PreparedProjectStart {
            mut host,
            mut attempt,
            plan_diagnostics,
            startup_content_closure,
        } = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.startup_content_closure = error.startup_content_closure;
                self.diagnostics.extend(error.fault.diagnostics);
                return DesktopRunState::Failed;
            }
        };
        self.startup_content_closure = startup_content_closure;
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

        match host.take_desktop_runtime() {
            Ok((runtime, context)) => {
                self.start_native(runtime, Some(context), execution, verification)
            }
            Err(fault) => {
                self.diagnostics.extend(fault.diagnostics);
                desktop_state_after_failure(host)
            }
        }
    }

    pub(in crate::project_host) fn append_startup_cleanup_diagnostics(
        &self,
        diagnostics: &mut DiagnosticReport,
    ) {
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
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub(in crate::project_host) enum DesktopRunInputs {
    Source {
        project_root: DirectoryCapability,
        intent: DesktopRunIntent,
        overlay: Option<RuntimePluginOverlay>,
    },
    Processed {
        base: crate::runtime_content::RuntimeContentBase,
        execution: DesktopRunOptions,
    },
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

    /// Runs the startup and later Scenes of an already admitted immutable Runtime base.
    ///
    /// The base retains its original executable plan. Execution policy cannot choose another
    /// recipe, source profile or directory, and no source Check receipt is manufactured.
    #[must_use]
    pub fn from_runtime_content(
        base: crate::runtime_content::RuntimeContentBase,
        execution: HeadlessRunOptions<O>,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self {
            state: HeadlessRunState::Fresh(HeadlessRunInputs::Processed {
                base,
                execution,
                commands,
            }),
            diagnostics: DiagnosticReport::default(),
            startup_content_closure: None,
        }
    }

    fn new_inner(
        project_root: DirectoryCapability,
        intent: HeadlessRunIntent<O>,
        overlay: Option<RuntimePluginOverlay>,
        commands: Vec<GameplayCommandSubmission>,
    ) -> Self {
        Self {
            state: HeadlessRunState::Fresh(HeadlessRunInputs::Source {
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
                CleanupDriveOutcome::Complete {
                    failed,
                    diagnostics,
                    ..
                } => {
                    self.diagnostics.extend(diagnostics);
                    if failed {
                        HeadlessRunState::Failed
                    } else {
                        terminal.finish()
                    }
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
        self.startup_content_closure = startup_content_closure;
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

enum HeadlessRunInputs<O> {
    Source {
        project_root: DirectoryCapability,
        intent: HeadlessRunIntent<O>,
        overlay: Option<RuntimePluginOverlay>,
        commands: Vec<GameplayCommandSubmission>,
    },
    Processed {
        base: crate::runtime_content::RuntimeContentBase,
        execution: HeadlessRunOptions<O>,
        commands: Vec<GameplayCommandSubmission>,
    },
}

struct PreparedStart<O> {
    host: ProjectHost,
    attempt: RuntimeStartAttempt,
    drive: HeadlessDrive,
    terminal_predicate: Option<TerminalPredicate<O>>,
    plan_diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
}

struct PreparedProjectStart {
    host: ProjectHost,
    attempt: RuntimeStartAttempt,
    plan_diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
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
    match inputs {
        HeadlessRunInputs::Source {
            project_root,
            intent,
            overlay,
            commands,
        } => {
            let HeadlessRunIntent {
                profile,
                recipe,
                execution,
            } = intent;
            let HeadlessRunOptions {
                drive,
                cleanup_policy,
                terminal_predicate,
            } = execution;
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
        HeadlessRunInputs::Processed {
            base,
            execution,
            commands,
        } => {
            let HeadlessRunOptions {
                drive,
                cleanup_policy,
                terminal_predicate,
            } = execution;
            let prepared = prepare_runtime_content_start(base, cleanup_policy, commands)?;
            Ok(PreparedStart {
                host: prepared.host,
                attempt: prepared.attempt,
                drive,
                terminal_predicate,
                plan_diagnostics: prepared.plan_diagnostics,
                startup_content_closure: None,
            })
        }
    }
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
    let plan_diagnostics = runtime_plan_selected_report(plan.configuration_plan());
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
        startup_content_closure: Some(startup_content_closure),
    })
}

fn prepare_runtime_content_start(
    base: crate::runtime_content::RuntimeContentBase,
    cleanup_policy: RuntimeClosePolicy,
    commands: Vec<GameplayCommandSubmission>,
) -> Result<PreparedProjectStart, PreparedProjectStartError> {
    let plan_diagnostics = runtime_plan_selected_report(base.plan());
    let mut host = ProjectHost::new(cleanup_policy);
    let attempt = host
        .begin_processed_start(base, commands)
        .map_err(|fault| PreparedProjectStartError {
            fault,
            startup_content_closure: None,
        })?;
    Ok(PreparedProjectStart {
        host,
        attempt,
        plan_diagnostics,
        startup_content_closure: None,
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
