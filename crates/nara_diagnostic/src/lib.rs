//! Structured diagnostics for runtime, tooling, and asset pipelines.

use nara_ecs::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

impl Diagnostic {
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: DiagnosticCode::new(code),
            severity,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, DiagnosticSeverity::Error, message)
    }

    #[must_use]
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, DiagnosticSeverity::Warning, message)
    }

    #[must_use]
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, DiagnosticSeverity::Info, message)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticReport {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        match diagnostic.severity {
            DiagnosticSeverity::Error => {
                tracing::error!(code = diagnostic.code.as_str(), "{}", diagnostic.message)
            }
            DiagnosticSeverity::Warning => {
                tracing::warn!(code = diagnostic.code.as_str(), "{}", diagnostic.message);
            }
            DiagnosticSeverity::Info => {
                tracing::info!(code = diagnostic.code.as_str(), "{}", diagnostic.message)
            }
        }
        self.diagnostics.push(diagnostic);
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}

pub mod prelude {
    pub use crate::{Diagnostic, DiagnosticCode, DiagnosticReport, DiagnosticSeverity};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_tracks_error_severity() {
        let mut report = DiagnosticReport::default();
        report.push(Diagnostic::warning(
            "asset.missing-meta",
            "missing metadata",
        ));
        assert!(!report.has_errors());

        report.push(Diagnostic::error("scene.invalid", "invalid scene"));

        assert!(report.has_errors());
        assert_eq!(report.diagnostics().len(), 2);
    }
}
