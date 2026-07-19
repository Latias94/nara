use std::{fmt, num::NonZeroU32, time::Duration};

use nara_app::{Plugin, PluginDefinition, RuntimeClosePolicy};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::Resource;
use nara_fs::{DirectoryCapability, RelativePath};
use nara_gameplay::GameplayCommandSubmission;
use nara_reflect::ComponentSchemaProviderDefinition;

use super::super::{
    ProjectCandidateError, ProjectContentLoader, ProjectRuntimePlugins, built_in_schema_providers,
    ingest_project_manifest, project_runtime_plugins, resolve_runtime_plan,
};
use super::{
    CleanupDriveOutcome, HostFault, PROJECT_MANIFEST, ProjectHost, RuntimeStartAttempt,
    runtime_plan_failure_report, runtime_plan_selected_report, single_error,
};

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

    /// Stops after the first complete fixed tick whose typed outcome matches `predicate`.
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
                CleanupDriveOutcome::Incomplete => HeadlessRunState::Cleaning { host, terminal },
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
            if let Err(fault) = host.drive_one_fixed_tick() {
                self.diagnostics.extend(fault.diagnostics);
                host.retire_running();
                return state_after_failure(host);
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

        let outcome = if terminal_predicate.is_some() {
            let Some(outcome) = terminal_outcome else {
                self.diagnostics.extend(single_error(
                    "project.run.tick-limit",
                    "Headless project run reached its fixed-tick limit",
                ));
                if let Err(fault) = host.stop_running() {
                    self.diagnostics.extend(fault.diagnostics);
                }
                return state_after_failure(host);
            };
            outcome
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
    Ok(PreparedStart {
        host,
        attempt,
        fixed_ticks,
        terminal_predicate,
        plan_diagnostics,
    })
}

fn nara_project_failure(error: nara_fs::FsError) -> ProjectCandidateError {
    ProjectCandidateError::from_manifest_authority(error)
}
