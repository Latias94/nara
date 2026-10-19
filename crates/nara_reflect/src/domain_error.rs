//! Safe domain facts carried by native component codecs and shared validation.

use std::{error::Error, fmt};

use nara_diagnostic::{DiagnosticCode, SafeSummary};

use crate::ComponentFieldId;

/// A source-authored component failure whose public facts do not contain payload text.
///
/// Use a namespaced code to distinguish the component owner's domain failures. The optional
/// field identifies a declared root field, not a nested value path. Diagnostic adapters must
/// check it against their admitted component schema before publishing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDomainError {
    code: DiagnosticCode,
    summary: SafeSummary,
    field: Option<ComponentFieldId>,
}

impl ComponentDomainError {
    #[must_use]
    pub const fn new(code: DiagnosticCode, summary: SafeSummary) -> Self {
        Self {
            code,
            summary,
            field: None,
        }
    }

    #[must_use]
    pub fn with_field(mut self, field: ComponentFieldId) -> Self {
        self.field = Some(field);
        self
    }

    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    #[must_use]
    pub const fn summary(&self) -> SafeSummary {
        self.summary
    }

    #[must_use]
    pub const fn field(&self) -> Option<&ComponentFieldId> {
        self.field.as_ref()
    }
}

impl fmt::Display for ComponentDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}",
            self.code.as_str(),
            self.summary.as_str()
        )
    }
}

impl Error for ComponentDomainError {}
