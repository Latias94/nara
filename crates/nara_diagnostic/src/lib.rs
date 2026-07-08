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
    pub context: DiagnosticContext,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticContext {
    pub operation_index: Option<usize>,
    pub entity_id: Option<String>,
    pub component_id: Option<String>,
    pub field_path: Option<String>,
    pub asset_ref: Option<String>,
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
            context: DiagnosticContext::default(),
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

    #[must_use]
    pub fn with_operation_index(mut self, operation_index: usize) -> Self {
        self.context.operation_index = Some(operation_index);
        self
    }

    #[must_use]
    pub fn with_entity_id(mut self, entity_id: impl Into<String>) -> Self {
        self.context.entity_id = Some(entity_id.into());
        self
    }

    #[must_use]
    pub fn with_component_id(mut self, component_id: impl Into<String>) -> Self {
        self.context.component_id = Some(component_id.into());
        self
    }

    #[must_use]
    pub fn with_field_path(mut self, field_path: impl Into<String>) -> Self {
        self.context.field_path = Some(field_path.into());
        self
    }

    #[must_use]
    pub fn with_asset_ref(mut self, asset_ref: impl Into<String>) -> Self {
        self.context.asset_ref = Some(asset_ref.into());
        self
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

    pub fn extend(&mut self, report: Self) {
        for diagnostic in report.diagnostics {
            self.push(diagnostic);
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
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
    pub use crate::{
        Diagnostic, DiagnosticCode, DiagnosticContext, DiagnosticReport, DiagnosticSeverity,
    };
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

    #[test]
    fn diagnostic_context_identifies_scene_problem_location() {
        let diagnostic = Diagnostic::error("scene.invalid-field", "invalid field")
            .with_operation_index(3)
            .with_entity_id("player")
            .with_component_id("nara.transform.Transform2d")
            .with_field_path("translation.x")
            .with_asset_ref("textures/player.png");

        assert_eq!(diagnostic.context.operation_index, Some(3));
        assert_eq!(diagnostic.context.entity_id.as_deref(), Some("player"));
        assert_eq!(
            diagnostic.context.component_id.as_deref(),
            Some("nara.transform.Transform2d")
        );
        assert_eq!(
            diagnostic.context.field_path.as_deref(),
            Some("translation.x")
        );
        assert_eq!(
            diagnostic.context.asset_ref.as_deref(),
            Some("textures/player.png")
        );
    }
}
