use std::{fmt, num::NonZeroU32, time::Duration};

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_app::{AppExit, AppRunError, RuntimeCloseCause, RuntimeCloseEvidence, RuntimeState};
use nara_app::{Plugin, PluginDefinition, RuntimeClosePolicy};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::Resource;
use nara_fs::{DirectoryCapability, RelativePath};
use nara_gameplay::GameplayCommandSubmission;
use nara_reflect::ComponentSchemaProviderDefinition;
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_winit::{WinitControlFlow, WinitRunner};

use super::super::{
    ProjectCandidateError, ProjectContentLoader, ProjectRuntimePlugins, built_in_schema_providers,
    ingest_project_manifest, project_runtime_plugins, resolve_runtime_plan,
};
use super::{
    CleanupDriveOutcome, HostFault, PROJECT_MANIFEST, ProjectHost, RuntimeStartAttempt,
    runtime_plan_failure_report, runtime_plan_selected_report, single_error,
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use super::{failure_diagnostic, single_diagnostic};

type RuntimePluginEdit =
    Box<dyn FnOnce(ProjectRuntimePlugins) -> ProjectRuntimePlugins + Send + 'static>;
type TerminalPredicate<O> = Box<dyn Fn(&O) -> bool + Send + 'static>;

/// File-backed headless run intent for one concrete product action.
///
/// Plugin edits and schema providers are Rust authoring configuration. Candidate, publication, and
/// cleanup choreography remain private to the root Host.
pub struct HeadlessRunIntent<O> {
    profile: Option<String>,
    fixed_ticks: NonZeroU32,
    cleanup_policy: RuntimeClosePolicy,
    plugin_edits: Vec<RuntimePluginEdit>,
    schema_providers: Vec<ComponentSchemaProviderDefinition>,
    terminal_predicate: Option<TerminalPredicate<O>>,
}

impl<O> fmt::Debug for HeadlessRunIntent<O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HeadlessRunIntent")
            .field("profile_present", &self.profile.is_some())
            .field("fixed_ticks", &self.fixed_ticks)
            .field("cleanup_policy", &self.cleanup_policy)
            .field("plugin_edit_count", &self.plugin_edits.len())
            .field("schema_provider_count", &self.schema_providers.len())
            .field(
                "stops_on_terminal_outcome",
                &self.terminal_predicate.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl<O> HeadlessRunIntent<O> {
    #[must_use]
    pub fn new(fixed_ticks: NonZeroU32) -> Self {
        Self {
            profile: None,
            fixed_ticks,
            cleanup_policy: RuntimeClosePolicy::default(),
            plugin_edits: Vec::new(),
            schema_providers: Vec::new(),
            terminal_predicate: None,
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

    #[must_use]
    pub fn configure(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits
            .push(Box::new(move |request| request.configure(definition)));
        self
    }

    #[must_use]
    pub fn disable<P: Plugin>(mut self) -> Self {
        self.plugin_edits
            .push(Box::new(|request| request.disable::<P>()));
        self
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits.push(Box::new(move |request| {
            request.insert_after::<P>(definition)
        }));
        self
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits.push(Box::new(move |request| {
            request.insert_before::<P>(definition)
        }));
        self
    }

    #[must_use]
    pub fn with_schema_provider(mut self, provider: ComponentSchemaProviderDefinition) -> Self {
        self.schema_providers.push(provider);
        self
    }

    /// Stops after the first complete fixed tick whose typed outcome matches `predicate` or that
    /// publishes an application exit request.
    ///
    /// `fixed_ticks` remains the hard maximum. Reaching it without a match fails the product action
    /// with `project.run.tick-limit` and drives the same bounded shutdown path as other failures.
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
#[derive(Debug, Clone, PartialEq)]
pub struct HeadlessRunReport<O> {
    outcome: HeadlessRunOutcome<O>,
    diagnostics: DiagnosticReport,
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
    plugin_edits: Vec<RuntimePluginEdit>,
    schema_providers: Vec<ComponentSchemaProviderDefinition>,
    runner: WinitRunner,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl fmt::Debug for DesktopRunIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopRunIntent")
            .field("profile_present", &self.profile.is_some())
            .field("cleanup_policy", &self.cleanup_policy)
            .field("plugin_edit_count", &self.plugin_edits.len())
            .field("schema_provider_count", &self.schema_providers.len())
            .field("runner", &self.runner)
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
            plugin_edits: Vec::new(),
            schema_providers: Vec::new(),
            runner: WinitRunner::default(),
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

    #[must_use]
    pub fn with_control_flow(mut self, control_flow: WinitControlFlow) -> Self {
        self.runner = WinitRunner::new(control_flow);
        self
    }

    #[must_use]
    pub fn configure(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits
            .push(Box::new(move |request| request.configure(definition)));
        self
    }

    #[must_use]
    pub fn disable<P: Plugin>(mut self) -> Self {
        self.plugin_edits
            .push(Box::new(|request| request.disable::<P>()));
        self
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits.push(Box::new(move |request| {
            request.insert_after::<P>(definition)
        }));
        self
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits.push(Box::new(move |request| {
            request.insert_before::<P>(definition)
        }));
        self
    }

    #[must_use]
    pub fn with_schema_provider(mut self, provider: ComponentSchemaProviderDefinition) -> Self {
        self.schema_providers.push(provider);
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
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopRunReport {
    outcome: DesktopRunOutcome,
    diagnostics: DiagnosticReport,
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
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub struct DesktopRun {
    state: DesktopRunState,
    diagnostics: DiagnosticReport,
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl fmt::Debug for DesktopRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopRun")
            .field("state", &self.state.label())
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
impl DesktopRun {
    #[must_use]
    pub fn new(project_root: DirectoryCapability, intent: DesktopRunIntent) -> Self {
        Self {
            state: DesktopRunState::Fresh(DesktopRunInputs {
                project_root,
                intent,
            }),
            diagnostics: DiagnosticReport::default(),
        }
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
        } = inputs;
        let DesktopRunIntent {
            profile,
            cleanup_policy,
            plugin_edits,
            schema_providers,
            runner,
        } = intent;
        let PreparedProjectStart {
            mut host,
            mut attempt,
            plan_diagnostics,
        } = match prepare_project_start(
            project_root,
            profile,
            cleanup_policy,
            plugin_edits,
            schema_providers,
            Vec::new(),
        ) {
            Ok(prepared) => prepared,
            Err(fault) => {
                self.diagnostics.extend(fault.diagnostics);
                return DesktopRunState::Failed;
            }
        };
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
        let terminal = desktop_terminal_after_runner(
            run_result,
            runtime_state,
            &close_evidence,
            &mut self.diagnostics,
        );
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
        }
    }
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
struct DesktopRunInputs {
    project_root: DirectoryCapability,
    intent: DesktopRunIntent,
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
            PendingDesktopTerminal::Completed(exit)
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
        AppRunError::Shutdown { .. } => "shutdown",
    };
    let diagnostic = failure_diagnostic(
        "project.desktop.runner-failed",
        "Desktop project runner failed",
        reason,
    );
    single_diagnostic(diagnostic)
}

#[cfg(all(test, feature = "desktop-winit", feature = "render-wgpu"))]
mod desktop_terminal_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use nara_app::{
        App, RuntimeCandidate, RuntimeCloseParticipant, RuntimeCloseParticipantError,
        RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
        RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger,
    };

    #[derive(Clone, Copy)]
    enum CloseFailureMode {
        Pending,
        RetryableParticipant,
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
            }
        }
    }

    fn has_code(report: &DiagnosticReport, code: &str) -> bool {
        report
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code)
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
            CloseFailureMode::RetryableParticipant => RuntimeClosePolicy::default(),
        };
        let mut runtime =
            RuntimeCandidate::admit_with(App::new().seal().unwrap(), obligations, policy)
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
            &mut diagnostics,
        );

        assert!(matches!(terminal, PendingDesktopTerminal::Failed));
        assert!(!has_code(&diagnostics, "project.desktop.shutdown-timeout"));
        assert!(has_code(&diagnostics, "project.desktop.runner-failed"));

        let report = DesktopRun {
            state: terminal.finish(),
            diagnostics,
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
                &mut running_diagnostics,
            ),
            PendingDesktopTerminal::Failed
        ));
        assert!(has_code(
            &running_diagnostics,
            "project.desktop.runtime-not-stopped"
        ));
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
            .finish_non_exhaustive()
    }
}

impl<O> HeadlessRun<O>
where
    O: Resource + Clone,
{
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
        Self {
            state: HeadlessRunState::Fresh(HeadlessRunInputs {
                project_root,
                intent,
                commands,
            }),
            diagnostics: DiagnosticReport::default(),
        }
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
            fixed_ticks,
            terminal_predicate,
            plan_diagnostics,
        } = match prepare_start(inputs) {
            Ok(prepared) => prepared,
            Err(fault) => {
                self.diagnostics.extend(fault.diagnostics);
                return HeadlessRunState::Failed;
            }
        };
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
        for _ in 0..fixed_ticks.get() {
            let app_exit = match host.drive_one_fixed_tick() {
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
            self.diagnostics.extend(single_error(
                "project.run.tick-limit",
                "Headless project run reached its fixed-tick limit",
            ));
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
        }
    }
}

struct HeadlessRunInputs<O> {
    project_root: DirectoryCapability,
    intent: HeadlessRunIntent<O>,
    commands: Vec<GameplayCommandSubmission>,
}

struct PreparedStart<O> {
    host: ProjectHost,
    attempt: RuntimeStartAttempt,
    fixed_ticks: NonZeroU32,
    terminal_predicate: Option<TerminalPredicate<O>>,
    plan_diagnostics: DiagnosticReport,
}

struct PreparedProjectStart {
    host: ProjectHost,
    attempt: RuntimeStartAttempt,
    plan_diagnostics: DiagnosticReport,
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

fn prepare_start<O>(inputs: HeadlessRunInputs<O>) -> Result<PreparedStart<O>, HostFault> {
    let HeadlessRunInputs {
        project_root,
        intent,
        commands,
    } = inputs;
    let HeadlessRunIntent {
        profile,
        fixed_ticks,
        cleanup_policy,
        plugin_edits,
        schema_providers,
        terminal_predicate,
    } = intent;
    let prepared = prepare_project_start(
        project_root,
        profile,
        cleanup_policy,
        plugin_edits,
        schema_providers,
        commands,
    )?;
    Ok(PreparedStart {
        host: prepared.host,
        attempt: prepared.attempt,
        fixed_ticks,
        terminal_predicate,
        plan_diagnostics: prepared.plan_diagnostics,
    })
}

fn prepare_project_start(
    project_root: DirectoryCapability,
    profile: Option<String>,
    cleanup_policy: RuntimeClosePolicy,
    plugin_edits: Vec<RuntimePluginEdit>,
    schema_providers: Vec<ComponentSchemaProviderDefinition>,
    commands: Vec<GameplayCommandSubmission>,
) -> Result<PreparedProjectStart, HostFault> {
    let manifest_path = RelativePath::new(PROJECT_MANIFEST)
        .expect("the engine-owned manifest name is a valid relative path");
    let manifest = project_root
        .open_file(&manifest_path)
        .map_err(|error| HostFault::new(nara_project_failure(error).diagnostics().clone()))?;
    let candidate = ingest_project_manifest(&manifest, profile.as_deref())
        .map_err(|error| HostFault::new(error.diagnostics().clone()))?;
    drop(manifest);

    let mut request = project_runtime_plugins(&candidate);
    for edit in plugin_edits {
        request = edit(request);
    }
    let mut providers = built_in_schema_providers();
    providers.extend(schema_providers);
    let plan = resolve_runtime_plan(&candidate, request, providers)
        .map_err(|error| HostFault::new(runtime_plan_failure_report(&error)))?;
    let plan_diagnostics = runtime_plan_selected_report(&plan);
    let loader = ProjectContentLoader::new(project_root)
        .map_err(|error| HostFault::new(error.diagnostics().clone()))?;
    let snapshot = loader
        .load(&candidate, &plan)
        .map_err(|error| HostFault::new(error.diagnostics().clone()))?;

    let mut host = ProjectHost::new(cleanup_policy);
    let attempt = host.begin_start(snapshot, plan, commands)?;
    Ok(PreparedProjectStart {
        host,
        attempt,
        plan_diagnostics,
    })
}

fn nara_project_failure(error: nara_fs::FsError) -> ProjectCandidateError {
    ProjectCandidateError::from_manifest_authority(error)
}
