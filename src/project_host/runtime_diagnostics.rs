//! Shared CPU lowering for managed Runtime and plugin diagnostics.

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_app::RuntimeAdmissionError;
use nara_app::{
    PluginFailureReport, PluginHook, PluginHookMutation, RuntimeCloseCause,
    RuntimeCloseErrorDisposition, RuntimeCloseEvidence, RuntimeCloseParticipantPhase, RuntimeFault,
    RuntimeFaultDetail, RuntimeFaultKind,
};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};

mod plugin;
pub(super) use plugin::append_plugin_shutdown_diagnostics;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) use plugin::runtime_construction_failure_report;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn admission_failure_report(error: &RuntimeAdmissionError) -> DiagnosticReport {
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

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn runtime_scope_failure_report(fault: &RuntimeFault) -> DiagnosticReport {
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

pub(super) fn runtime_fault_report(
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

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn runtime_startup_failure_report(fault: &RuntimeFault) -> DiagnosticReport {
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

pub(super) fn runtime_fault_diagnostic(
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

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn runtime_authority_invalid_report() -> DiagnosticReport {
    single_error(
        "project.run.runtime-authority-invalid",
        "Project runtime authority is invalid",
    )
}

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn cleanup_incomplete_report(
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

pub(super) fn cleanup_failed_report(
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

pub(super) fn append_close_cause_diagnostics(
    report: &mut DiagnosticReport,
    evidence: &RuntimeCloseEvidence,
) {
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

pub(super) fn failure_diagnostic(
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

pub(super) fn attach_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &str,
) -> Diagnostic {
    let field = PublicDiagnosticIdentifier::new(value).map_or_else(
        |_| DiagnosticField::sensitive(field_key(key)),
        |value| DiagnosticField::public_identifier(field_key(key), value),
    );
    diagnostic
        .try_with_field(field)
        .expect("project Host diagnostics use unique bounded fields")
}

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn attach_u64(diagnostic: Diagnostic, key: &'static str, value: usize) -> Diagnostic {
    diagnostic
        .try_with_field(DiagnosticField::public_u64(
            field_key(key),
            u64::try_from(value).unwrap_or(u64::MAX),
        ))
        .expect("project Host diagnostics use unique bounded fields")
}

pub(super) const fn plugin_hook_id(hook: PluginHook) -> &'static str {
    match hook {
        PluginHook::Preflight => "preflight",
        PluginHook::Build => "build",
        PluginHook::Finish => "finish",
        PluginHook::Shutdown => "shutdown",
    }
}

pub(super) const fn plugin_mutation_id(mutation: PluginHookMutation) -> &'static str {
    match mutation {
        PluginHookMutation::PluginMembership => "plugin-membership",
    }
}

pub(super) const fn close_phase_id(phase: RuntimeCloseParticipantPhase) -> &'static str {
    match phase {
        RuntimeCloseParticipantPhase::Begin => "begin",
        RuntimeCloseParticipantPhase::Poll => "poll",
    }
}

pub(super) const fn close_disposition_id(
    disposition: RuntimeCloseErrorDisposition,
) -> &'static str {
    match disposition {
        RuntimeCloseErrorDisposition::Retryable => "retryable",
        RuntimeCloseErrorDisposition::Terminal => "terminal",
    }
}

pub(super) fn single_error(code: &'static str, summary: &'static str) -> DiagnosticReport {
    single_diagnostic(Diagnostic::error(
        diagnostic_code(code),
        safe_summary(summary),
    ))
}

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn single_warning(code: &'static str, summary: &'static str) -> DiagnosticReport {
    single_diagnostic(Diagnostic::warning(
        diagnostic_code(code),
        safe_summary(summary),
    ))
}

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn single_warning_with_project_relative(
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

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(super) fn single_error_with_u64(
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

pub(super) fn single_diagnostic(diagnostic: Diagnostic) -> DiagnosticReport {
    let mut report = DiagnosticReport::default();
    report.push(diagnostic);
    report
}

pub(super) fn diagnostic_code(code: &'static str) -> DiagnosticCode {
    DiagnosticCode::new(code).expect("project Host diagnostic codes are engine-owned")
}

pub(super) fn safe_summary(summary: &'static str) -> SafeSummary {
    SafeSummary::new(summary).expect("project Host summaries are engine-owned")
}

pub(super) fn field_key(key: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(key).expect("project Host diagnostic field keys are engine-owned")
}
