//! Product lowering for typed plugin failures. App retains invocation and cleanup authority.

use nara_app::{
    PluginDomainError, PluginError, PluginFailure, PluginFailureReport, PluginInstantiationError,
    PluginPrepareError, RuntimeConstructionError,
};
use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, SafeSummary};

use super::{
    admission_failure_report, attach_identifier, failure_diagnostic, plugin_hook_id,
    plugin_mutation_id, single_diagnostic,
};

pub(super) fn runtime_construction_failure_report(
    error: &RuntimeConstructionError,
    failure: Option<&PluginFailure>,
) -> DiagnosticReport {
    match error {
        RuntimeConstructionError::Plugin(PluginInstantiationError::Prepare(error)) => {
            plugin_prepare_failure_report(error)
        }
        RuntimeConstructionError::Plugin(PluginInstantiationError::Plugin(error)) => {
            single_diagnostic(plugin_hook_failure_diagnostic(
                error,
                failure,
                "project.run.plugin-hook-failed",
                "Project plugin lifecycle hook failed",
            ))
        }
        RuntimeConstructionError::Admission(error) => admission_failure_report(error),
    }
}

fn domain_diagnostic(
    domain: Option<PluginDomainError>,
    context_code: &'static str,
    summary: &'static str,
    reason: &'static str,
) -> Diagnostic {
    // App validates these source-authored facts without depending on the diagnostic runtime.
    // Recheck at the presentation seam so invalid metadata can never weaken its privacy contract.
    let safe = domain.and_then(|domain| {
        DiagnosticCode::new(domain.code())
            .ok()
            .zip(SafeSummary::new(domain.summary()).ok())
    });
    match safe {
        Some((code, summary)) => {
            let diagnostic = Diagnostic::error(code, summary);
            let diagnostic = attach_identifier(diagnostic, "project-context-code", context_code);
            attach_identifier(diagnostic, "reason", reason)
        }
        None => failure_diagnostic(context_code, summary, reason),
    }
}

fn plugin_prepare_failure_report(error: &PluginPrepareError) -> DiagnosticReport {
    let (reason, plugin, domain) = match error {
        PluginPrepareError::Failed { plugin, source } => ("failed", *plugin, Some(*source)),
        PluginPrepareError::Panicked { plugin } => ("panicked", *plugin, None),
    };
    let mut diagnostic = domain_diagnostic(
        domain,
        "project.run.plugin-prepare-failed",
        "Project plugin preparation failed",
        reason,
    );
    diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
    diagnostic = attach_identifier(diagnostic, "phase", "prepare");
    if let Some(domain) = domain {
        diagnostic = attach_identifier(diagnostic, "failure-code", domain.code());
    }
    single_diagnostic(diagnostic)
}

fn plugin_hook_failure_diagnostic(
    error: &PluginError,
    failure: Option<&PluginFailure>,
    context_code: &'static str,
    summary: &'static str,
) -> Diagnostic {
    let mut diagnostic = domain_diagnostic(
        error.domain_error().copied(),
        context_code,
        summary,
        plugin_error_reason(error),
    );
    // The framework's invocation envelope takes precedence over a legacy error's claimed owner.
    if let Some(failure) = failure {
        diagnostic = attach_identifier(diagnostic, "plugin", failure.plugin().as_str());
        diagnostic = attach_identifier(diagnostic, "hook", plugin_hook_id(failure.hook()));
    }
    match error {
        PluginError::Domain(_) => {}
        PluginError::HookMutationForbidden {
            plugin,
            hook,
            mutation,
        } => {
            if failure.is_none() {
                diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
                diagnostic = attach_identifier(diagnostic, "hook", plugin_hook_id(*hook));
            }
            diagnostic = attach_identifier(diagnostic, "mutation", plugin_mutation_id(*mutation));
        }
        PluginError::SetupFailed { plugin, .. }
        | PluginError::ComponentRegistrationFailed { plugin, .. } => {
            if failure.is_none() {
                diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            }
            if let PluginError::ComponentRegistrationFailed { component, .. } = error {
                diagnostic = attach_identifier(diagnostic, "component", component);
            }
        }
        PluginError::HookPanicked { plugin, hook } => {
            if failure.is_none() {
                diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
                diagnostic = attach_identifier(diagnostic, "hook", plugin_hook_id(*hook));
            }
        }
        PluginError::CommittedPreflightRejected { plugin, source } => {
            if failure.is_none() {
                diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            }
            diagnostic = attach_identifier(diagnostic, "cause", plugin_error_reason(source));
        }
        PluginError::MissingShutdownObligation { plugin, obligation }
        | PluginError::UndeclaredShutdownObligation { plugin, obligation }
        | PluginError::DuplicateShutdownObligation { plugin, obligation } => {
            if failure.is_none() {
                diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            }
            diagnostic = attach_identifier(diagnostic, "obligation", obligation.as_str());
        }
        PluginError::DuplicateRuntimeCloseParticipant {
            plugin,
            obligation,
            participant_id,
        } => {
            if failure.is_none() {
                diagnostic = attach_identifier(diagnostic, "plugin", plugin.as_str());
            }
            diagnostic = attach_identifier(diagnostic, "obligation", obligation.as_str());
            diagnostic = attach_identifier(diagnostic, "participant", participant_id.as_str());
        }
        PluginError::AppSealed
        | PluginError::DuplicateDriverPort { .. }
        | PluginError::RawBuiltInScheduleMutationForbidden
        | PluginError::ScheduleCompatibility(_)
        | PluginError::ScheduleGraph(_)
        | PluginError::ShutdownObligationOutsideBuild
        | PluginError::LifecycleShutdown
        | PluginError::LifecyclePoisoned
        | PluginError::FinishReentered => {}
    }
    diagnostic
}

pub(super) fn append_plugin_shutdown_diagnostics(
    diagnostics: &mut DiagnosticReport,
    report: Option<&PluginFailureReport>,
) {
    if let Some(report) = report {
        for failure in report.shutdown_failures() {
            diagnostics.push(plugin_hook_failure_diagnostic(
                failure.error(),
                Some(failure),
                "project.run.cleanup-plugin-shutdown",
                "Project plugin shutdown failed",
            ));
        }
    }
}

fn plugin_error_reason(error: &PluginError) -> &'static str {
    match error {
        PluginError::Domain(_) => "domain",
        PluginError::HookMutationForbidden { .. } => "hook-mutation-forbidden",
        PluginError::AppSealed => "app-sealed",
        PluginError::DuplicateDriverPort { .. } => "duplicate-driver-port",
        PluginError::RawBuiltInScheduleMutationForbidden => "raw-schedule-mutation-forbidden",
        PluginError::ScheduleCompatibility(_) => "schedule-compatibility",
        PluginError::ScheduleGraph(_) => "schedule-graph",
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
