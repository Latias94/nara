//! Owned tool diagnostics and bounded reports with validated public fields.
//!
//! Engine-owned identities are source-authored.
//!
//! ```compile_fail
//! use nara_core::diagnostic::DiagnosticCode;
//!
//! let runtime = String::from("runtime.value");
//! let _ = DiagnosticCode::new(&runtime);
//! ```

use std::{num::NonZeroU32, path::PathBuf};

mod field;
mod identity;
mod report;
mod settings;

pub use field::{DiagnosticBuildError, DiagnosticField, DiagnosticFieldClass, DiagnosticValueRef};
pub use identity::{
    DiagnosticCode, DiagnosticDomain, DiagnosticFieldKey, DiagnosticProducer, IdentityError,
    IdentityErrorReason, PressureMetricId, PressureSourceId, PublicDiagnosticIdentifier,
    SafeDisplayText, SafeSummary, SafeTextError, SafeTextErrorReason,
};
pub use report::{
    DiagnosticReport, DiagnosticReportMergeOutcome, DiagnosticReportOutcome,
    DiagnosticReportRejection, DiagnosticReportStats, DiagnosticTruncation, ReportDiagnostic,
};
pub use settings::{
    DiagnosticReportSettings, DiagnosticSettingsError, RuntimeDiagnosticRetention,
    RuntimeDiagnosticsSettings,
};

/// The largest entry limit accepted by a diagnostic report or runtime store.
pub const MAX_RUNTIME_DIAGNOSTIC_ENTRIES: usize = 4_096;
/// The largest payload budget accepted by a diagnostic report or runtime store.
pub const MAX_RUNTIME_DIAGNOSTIC_BYTES: usize = 4 * 1024 * 1024;
/// The hard pre-publication field cap for any diagnostic entry.
pub const MAX_DIAGNOSTIC_DRAFT_FIELDS: usize = 32;
/// The largest validated dynamic project-relative field accepted before publication.
pub const MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES: usize = 1_024;
/// The largest static summary or display string accepted before configured truncation.
pub const MAX_SAFE_STATIC_TEXT_BYTES: usize = 1_024;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// One message prepared for a diagnostics consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub message: String,
    pub location: Option<SourceLoc>,
}

/// A reported source location. The consumer decides how to display or resolve its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLoc {
    pub path: PathBuf,
    /// One-based line and column, when the producer can identify a position.
    pub line_column: Option<(NonZeroU32, NonZeroU32)>,
}
