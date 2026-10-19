use thiserror::Error;

use crate::failure_metadata;

/// Invalid source-authored plugin failure metadata.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PluginDomainErrorError {
    #[error("plugin domain error code is invalid")]
    InvalidCode,
    #[error("plugin domain error summary is invalid")]
    InvalidSummary,
}

/// Bounded, source-authored facts about an expected plugin failure.
///
/// The engine records the invoking plugin and lifecycle hook separately. Runtime text, paths, and
/// third-party error messages must not be copied into these static public facts.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("{summary} ({code})")]
pub struct PluginDomainError {
    code: &'static str,
    summary: &'static str,
}

impl PluginDomainError {
    pub const MAX_CODE_BYTES: usize = failure_metadata::MAX_CODE_BYTES;
    pub const MAX_SUMMARY_BYTES: usize = failure_metadata::MAX_SUMMARY_BYTES;

    /// Validates static public facts using the same policy as runtime fault metadata.
    pub fn new(code: &'static str, summary: &'static str) -> Result<Self, PluginDomainErrorError> {
        if !failure_metadata::is_valid_identifier(code, Self::MAX_CODE_BYTES, false) {
            return Err(PluginDomainErrorError::InvalidCode);
        }
        if !failure_metadata::is_valid_summary(summary) {
            return Err(PluginDomainErrorError::InvalidSummary);
        }
        Ok(Self { code, summary })
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        self.summary
    }
}
