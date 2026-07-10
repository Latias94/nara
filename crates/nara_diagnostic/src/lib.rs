//! Bounded, privacy-preserving diagnostics and numeric pressure observations.
//!
//! Engine-owned identities are source-authored rather than runtime strings:
//!
//! ```compile_fail
//! use nara_diagnostic::DiagnosticCode;
//!
//! let runtime = String::from("runtime.value");
//! let _ = DiagnosticCode::new(&runtime);
//! ```

mod field;
mod identity;
mod plugin;
mod pressure;
mod report;
mod runtime;

pub use field::{DiagnosticBuildError, DiagnosticField, DiagnosticFieldClass, DiagnosticValueRef};
pub use identity::{
    DiagnosticCode, DiagnosticDomain, DiagnosticFieldKey, DiagnosticProducer, IdentityError,
    IdentityErrorReason, PressureMetricId, PressureSourceId, PublicDiagnosticIdentifier,
    SafeDisplayText, SafeSummary, SafeTextError, SafeTextErrorReason,
};
pub use plugin::{DiagnosticCleanupSet, DiagnosticsPlugin};
pub use pressure::{
    PressureDraftError, PressureMeasurement, PressureMetricKind, PressurePublishOutcome,
    PressurePublishRejection, PressureSettingsError, PressureStats, PressureUnit,
    RuntimePressureRetention, RuntimePressureSettings, RuntimePressureSnapshot,
    RuntimePressureSnapshotDraft, RuntimePressureSnapshots, RuntimePressureSnapshotsSnapshot,
};
pub use report::{
    Diagnostic, DiagnosticReport, DiagnosticReportMergeOutcome, DiagnosticReportOutcome,
    DiagnosticReportRejection, DiagnosticReportSettings, DiagnosticReportStats,
    DiagnosticSettingsError, DiagnosticTruncation,
};
pub use runtime::{
    DiagnosticDedupePolicy, RuntimeDiagnosticDraft, RuntimeDiagnosticEntry,
    RuntimeDiagnosticFilter, RuntimeDiagnosticFilterError, RuntimeDiagnosticRetention,
    RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimeDiagnosticsSnapshot,
    RuntimeDiagnosticsStats, RuntimePublishOutcome, RuntimePublishRejection, RuntimeTracingCursor,
};

/// The largest number of runtime entries accepted by one diagnostics resource.
pub const MAX_RUNTIME_DIAGNOSTIC_ENTRIES: usize = 4_096;
/// The largest retained diagnostic payload budget accepted by one resource.
pub const MAX_RUNTIME_DIAGNOSTIC_BYTES: usize = 4 * 1024 * 1024;
/// The hard pre-publication field cap for any diagnostic draft.
pub const MAX_DIAGNOSTIC_DRAFT_FIELDS: usize = 32;
/// The largest validated dynamic project-relative field accepted before publication.
pub const MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES: usize = 1_024;
/// The largest static summary or display string accepted before configured truncation.
pub const MAX_SAFE_STATIC_TEXT_BYTES: usize = 1_024;

pub mod prelude {
    pub use crate::{
        Diagnostic, DiagnosticCode, DiagnosticDedupePolicy, DiagnosticDomain, DiagnosticField,
        DiagnosticFieldClass, DiagnosticFieldKey, DiagnosticProducer, DiagnosticReport,
        DiagnosticReportSettings, DiagnosticSeverity, DiagnosticsPlugin, PressureMeasurement,
        PressureMetricId, PressureMetricKind, PressureSourceId, PressureUnit,
        PublicDiagnosticIdentifier, RuntimeDiagnosticDraft, RuntimeDiagnosticEntry,
        RuntimeDiagnosticFilter, RuntimeDiagnosticRetention, RuntimeDiagnostics,
        RuntimeDiagnosticsSettings, RuntimePressureSettings, RuntimePressureSnapshotDraft,
        RuntimePressureSnapshots, SafeDisplayText, SafeSummary,
    };
}

/// Severity is part of runtime dedupe identity and remains stable across sinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[cfg(test)]
mod contract_tests;
