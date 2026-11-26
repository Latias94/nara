//! Messages and source locations shared by diagnostics producers and consumers.

use std::{num::NonZeroU32, path::PathBuf};

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
