use nara_core::diagnostic::{DiagnosticReport, DiagnosticSeverity, ReportDiagnostic};

// Keep subscriber filters working across the crate move.
const REPORT_TRACING_TARGET: &str = "nara_diagnostic::report";

/// Emits the code and validated summary of one report entry at its severity level.
pub fn emit_diagnostic_to_tracing(diagnostic: &ReportDiagnostic) {
    let code = diagnostic.code().as_str();
    let summary = diagnostic.summary().as_str();
    match diagnostic.severity() {
        DiagnosticSeverity::Error => {
            tracing::error!(target: REPORT_TRACING_TARGET, code, summary, "diagnostic");
        }
        DiagnosticSeverity::Warning => {
            tracing::warn!(target: REPORT_TRACING_TARGET, code, summary, "diagnostic");
        }
        DiagnosticSeverity::Info => {
            tracing::info!(target: REPORT_TRACING_TARGET, code, summary, "diagnostic");
        }
    }
}

/// Emits a report's retained entries in order using their public tracing projection.
pub fn emit_report_to_tracing(report: &DiagnosticReport) {
    for diagnostic in report {
        emit_diagnostic_to_tracing(diagnostic);
    }
}
