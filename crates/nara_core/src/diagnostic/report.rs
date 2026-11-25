use std::collections::VecDeque;

use thiserror::Error;

use crate::{ByteLimit, ItemLimit};

use super::{
    DiagnosticBuildError, DiagnosticCode, DiagnosticField, DiagnosticReportSettings,
    DiagnosticSeverity, SafeSummary,
    field::{push_field, usize_to_u64},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticTruncation {
    dropped_fields: u64,
    truncated_fields: u64,
    truncated_text_bytes: u64,
}

impl DiagnosticTruncation {
    pub const fn dropped_fields(self) -> u64 {
        self.dropped_fields
    }

    pub const fn truncated_fields(self) -> u64 {
        self.truncated_fields
    }

    pub const fn truncated_text_bytes(self) -> u64 {
        self.truncated_text_bytes
    }
}

fn apply_field_text_limit(
    fields: &mut Vec<DiagnosticField>,
    field_text_byte_limit: ByteLimit,
    truncation: &mut DiagnosticTruncation,
) {
    let mut bounded = Vec::with_capacity(fields.len());
    for mut field in fields.drain(..) {
        if let Some(discarded_bytes) =
            field.discarded_project_text_bytes(field_text_byte_limit.get())
        {
            truncation.dropped_fields = truncation.dropped_fields.saturating_add(1);
            truncation.truncated_fields = truncation.truncated_fields.saturating_add(1);
            truncation.truncated_text_bytes = truncation
                .truncated_text_bytes
                .saturating_add(discarded_bytes);
            continue;
        }
        let (truncated_bytes, was_truncated) = field.truncate_text(field_text_byte_limit.get());
        truncation.truncated_text_bytes = truncation
            .truncated_text_bytes
            .saturating_add(truncated_bytes);
        if was_truncated {
            truncation.truncated_fields = truncation.truncated_fields.saturating_add(1);
        }
        bounded.push(field);
    }
    *fields = bounded;
}

/// A validated entry for bounded reports and runtime diagnostic publication.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename = "Diagnostic")
)]
pub struct ReportDiagnostic {
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    summary: SafeSummary,
    fields: Vec<DiagnosticField>,
}

impl ReportDiagnostic {
    #[must_use]
    pub fn new(code: DiagnosticCode, severity: DiagnosticSeverity, summary: SafeSummary) -> Self {
        Self {
            code,
            severity,
            summary,
            fields: Vec::new(),
        }
    }

    #[must_use]
    pub fn error(code: DiagnosticCode, summary: SafeSummary) -> Self {
        Self::new(code, DiagnosticSeverity::Error, summary)
    }

    #[must_use]
    pub fn warning(code: DiagnosticCode, summary: SafeSummary) -> Self {
        Self::new(code, DiagnosticSeverity::Warning, summary)
    }

    #[must_use]
    pub fn info(code: DiagnosticCode, summary: SafeSummary) -> Self {
        Self::new(code, DiagnosticSeverity::Info, summary)
    }

    pub fn try_with_field(mut self, field: DiagnosticField) -> Result<Self, DiagnosticBuildError> {
        push_field(&mut self.fields, field)?;
        Ok(self)
    }

    #[must_use]
    pub const fn code(&self) -> &DiagnosticCode {
        &self.code
    }

    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    #[must_use]
    pub const fn summary(&self) -> SafeSummary {
        self.summary
    }

    #[must_use]
    pub fn fields(&self) -> &[DiagnosticField] {
        &self.fields
    }

    /// Consumes the entry and returns its validated fields in insertion order.
    #[must_use]
    pub fn into_fields(self) -> Vec<DiagnosticField> {
        self.fields
    }

    /// Applies the receiving sink's limits to this entry and accounts for discarded content.
    pub fn apply_limits(
        &mut self,
        field_limit: ItemLimit,
        summary_byte_limit: ByteLimit,
        field_text_byte_limit: ByteLimit,
    ) -> DiagnosticTruncation {
        let original_fields = self.fields.len();
        self.fields.truncate(field_limit.get());
        let mut truncation = DiagnosticTruncation {
            dropped_fields: usize_to_u64(original_fields - self.fields.len()),
            ..DiagnosticTruncation::default()
        };
        let (summary, truncated_summary_bytes) = self.summary.truncate(summary_byte_limit.get());
        self.summary = summary;
        if truncated_summary_bytes > 0 {
            truncation.truncated_fields = truncation.truncated_fields.saturating_add(1);
            truncation.truncated_text_bytes = truncation
                .truncated_text_bytes
                .saturating_add(usize_to_u64(truncated_summary_bytes));
        }
        apply_field_text_limit(&mut self.fields, field_text_byte_limit, &mut truncation);
        truncation
    }

    /// Returns the entry's payload size for diagnostic retention budgets.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.code.as_str().len()
            + self.summary.as_str().len()
            + 1
            + self
                .fields
                .iter()
                .map(DiagnosticField::retained_bytes)
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DiagnosticReportRejection {
    #[error("diagnostic entry exceeds retained byte limit")]
    EntryTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticReportOutcome {
    Published {
        evicted_entries: u64,
        evicted_bytes: u64,
        truncation: DiagnosticTruncation,
    },
    Rejected(DiagnosticReportRejection),
}

impl DiagnosticReportOutcome {
    #[must_use]
    pub const fn is_published(self) -> bool {
        matches!(self, Self::Published { .. })
    }

    #[must_use]
    pub const fn rejection(self) -> Option<DiagnosticReportRejection> {
        match self {
            Self::Rejected(rejection) => Some(rejection),
            Self::Published { .. } => None,
        }
    }

    #[must_use]
    pub const fn evicted_entries(self) -> u64 {
        match self {
            Self::Published {
                evicted_entries, ..
            } => evicted_entries,
            Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn truncated_text_bytes(self) -> u64 {
        match self {
            Self::Published { truncation, .. } => truncation.truncated_text_bytes(),
            Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn dropped_fields(self) -> u64 {
        match self {
            Self::Published { truncation, .. } => truncation.dropped_fields(),
            Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn truncated_fields(self) -> u64 {
        match self {
            Self::Published { truncation, .. } => truncation.truncated_fields(),
            Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn evicted_bytes(self) -> u64 {
        match self {
            Self::Published { evicted_bytes, .. } => evicted_bytes,
            Self::Rejected(_) => 0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticReportMergeOutcome {
    attempted_entries: u64,
    published_entries: u64,
    rejected_entries: u64,
    evicted_entries: u64,
    evicted_bytes: u64,
    dropped_fields: u64,
    truncated_fields: u64,
    truncated_text_bytes: u64,
    propagated_unretained_errors: u64,
    propagated_unretained_warnings: u64,
    propagated_unretained_info: u64,
}

impl DiagnosticReportMergeOutcome {
    #[must_use]
    pub const fn attempted_entries(self) -> u64 {
        self.attempted_entries
    }

    #[must_use]
    pub const fn published_entries(self) -> u64 {
        self.published_entries
    }

    #[must_use]
    pub const fn rejected_entries(self) -> u64 {
        self.rejected_entries
    }

    #[must_use]
    pub const fn evicted_entries(self) -> u64 {
        self.evicted_entries
    }

    #[must_use]
    pub const fn evicted_bytes(self) -> u64 {
        self.evicted_bytes
    }

    #[must_use]
    pub const fn dropped_fields(self) -> u64 {
        self.dropped_fields
    }

    #[must_use]
    pub const fn truncated_fields(self) -> u64 {
        self.truncated_fields
    }

    #[must_use]
    pub const fn truncated_text_bytes(self) -> u64 {
        self.truncated_text_bytes
    }

    #[must_use]
    pub const fn propagated_unretained_errors(self) -> u64 {
        self.propagated_unretained_errors
    }

    #[must_use]
    pub const fn propagated_unretained_warnings(self) -> u64 {
        self.propagated_unretained_warnings
    }

    #[must_use]
    pub const fn propagated_unretained_info(self) -> u64 {
        self.propagated_unretained_info
    }

    fn record(&mut self, outcome: DiagnosticReportOutcome) {
        self.attempted_entries = self.attempted_entries.saturating_add(1);
        match outcome {
            DiagnosticReportOutcome::Published {
                evicted_entries,
                evicted_bytes,
                truncation,
            } => {
                self.published_entries = self.published_entries.saturating_add(1);
                self.evicted_entries = self.evicted_entries.saturating_add(evicted_entries);
                self.evicted_bytes = self.evicted_bytes.saturating_add(evicted_bytes);
                self.dropped_fields = self
                    .dropped_fields
                    .saturating_add(truncation.dropped_fields());
                self.truncated_fields = self
                    .truncated_fields
                    .saturating_add(truncation.truncated_fields());
                self.truncated_text_bytes = self
                    .truncated_text_bytes
                    .saturating_add(truncation.truncated_text_bytes());
            }
            DiagnosticReportOutcome::Rejected(_) => {
                self.rejected_entries = self.rejected_entries.saturating_add(1);
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DiagnosticReportStats {
    observed_errors: u64,
    observed_warnings: u64,
    observed_info: u64,
    published_entries: u64,
    rejected_entries: u64,
    evicted_entries: u64,
    evicted_bytes: u64,
    dropped_fields: u64,
    truncated_fields: u64,
    truncated_text_bytes: u64,
}

impl DiagnosticReportStats {
    #[must_use]
    pub const fn observed_entries(self) -> u64 {
        self.observed_errors
            .saturating_add(self.observed_warnings)
            .saturating_add(self.observed_info)
    }

    #[must_use]
    pub const fn observed_errors(self) -> u64 {
        self.observed_errors
    }

    #[must_use]
    pub const fn observed_warnings(self) -> u64 {
        self.observed_warnings
    }

    #[must_use]
    pub const fn observed_info(self) -> u64 {
        self.observed_info
    }

    #[must_use]
    pub const fn published_entries(self) -> u64 {
        self.published_entries
    }

    #[must_use]
    pub const fn rejected_entries(self) -> u64 {
        self.rejected_entries
    }

    #[must_use]
    pub const fn evicted_entries(self) -> u64 {
        self.evicted_entries
    }

    #[must_use]
    pub const fn evicted_bytes(self) -> u64 {
        self.evicted_bytes
    }

    #[must_use]
    pub const fn dropped_fields(self) -> u64 {
        self.dropped_fields
    }

    #[must_use]
    pub const fn truncated_fields(self) -> u64 {
        self.truncated_fields
    }

    #[must_use]
    pub const fn truncated_text_bytes(self) -> u64 {
        self.truncated_text_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DiagnosticReport {
    settings: DiagnosticReportSettings,
    diagnostics: VecDeque<ReportDiagnostic>,
    retained_bytes: usize,
    stats: DiagnosticReportStats,
}

impl Default for DiagnosticReport {
    fn default() -> Self {
        Self::new(DiagnosticReportSettings::default())
    }
}

impl DiagnosticReport {
    #[must_use]
    pub fn new(settings: DiagnosticReportSettings) -> Self {
        Self {
            diagnostics: VecDeque::with_capacity(settings.entry_limit().get()),
            settings,
            retained_bytes: 0,
            stats: DiagnosticReportStats::default(),
        }
    }

    pub fn push(&mut self, mut diagnostic: ReportDiagnostic) -> DiagnosticReportOutcome {
        self.record_observed_severity(diagnostic.severity);
        let truncation = diagnostic.apply_limits(
            self.settings.field_limit(),
            self.settings.summary_byte_limit(),
            self.settings.field_text_byte_limit(),
        );
        let entry_bytes = diagnostic.retained_bytes();
        if entry_bytes > self.settings.byte_limit().get() {
            self.stats.rejected_entries = self.stats.rejected_entries.saturating_add(1);
            return DiagnosticReportOutcome::Rejected(DiagnosticReportRejection::EntryTooLarge);
        }

        let mut evicted_entries = 0_u64;
        let mut evicted_bytes = 0_u64;
        while self.diagnostics.len() >= self.settings.entry_limit().get()
            || self.retained_bytes.saturating_add(entry_bytes) > self.settings.byte_limit().get()
        {
            let Some(evicted) = self.diagnostics.pop_front() else {
                break;
            };
            let bytes = evicted.retained_bytes();
            self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
            evicted_entries = evicted_entries.saturating_add(1);
            evicted_bytes = evicted_bytes.saturating_add(usize_to_u64(bytes));
        }
        self.retained_bytes = self.retained_bytes.saturating_add(entry_bytes);
        self.diagnostics.push_back(diagnostic);
        self.stats.published_entries = self.stats.published_entries.saturating_add(1);
        self.stats.evicted_entries = self.stats.evicted_entries.saturating_add(evicted_entries);
        self.stats.evicted_bytes = self.stats.evicted_bytes.saturating_add(evicted_bytes);
        self.stats.dropped_fields = self
            .stats
            .dropped_fields
            .saturating_add(truncation.dropped_fields());
        self.stats.truncated_fields = self
            .stats
            .truncated_fields
            .saturating_add(truncation.truncated_fields());
        self.stats.truncated_text_bytes = self
            .stats
            .truncated_text_bytes
            .saturating_add(truncation.truncated_text_bytes());

        DiagnosticReportOutcome::Published {
            evicted_entries,
            evicted_bytes,
            truncation,
        }
    }

    /// Merges a report through this report's limits while preserving sticky source severity.
    pub fn extend(&mut self, report: DiagnosticReport) -> DiagnosticReportMergeOutcome {
        let source_stats = report.stats;
        let target_stats = self.stats;
        let (retained_errors, retained_warnings, retained_info) =
            retained_severity_counts(&report.diagnostics);
        let missing_errors = source_stats.observed_errors.saturating_sub(retained_errors);
        let missing_warnings = source_stats
            .observed_warnings
            .saturating_sub(retained_warnings);
        let missing_info = source_stats.observed_info.saturating_sub(retained_info);
        let mut outcome = self.extend_diagnostics(report.into_retained_diagnostics());
        self.stats = DiagnosticReportStats {
            observed_errors: target_stats
                .observed_errors
                .saturating_add(source_stats.observed_errors),
            observed_warnings: target_stats
                .observed_warnings
                .saturating_add(source_stats.observed_warnings),
            observed_info: target_stats
                .observed_info
                .saturating_add(source_stats.observed_info),
            published_entries: target_stats
                .published_entries
                .saturating_add(source_stats.published_entries),
            rejected_entries: target_stats
                .rejected_entries
                .saturating_add(source_stats.rejected_entries)
                .saturating_add(outcome.rejected_entries),
            evicted_entries: target_stats
                .evicted_entries
                .saturating_add(source_stats.evicted_entries)
                .saturating_add(outcome.evicted_entries),
            evicted_bytes: target_stats
                .evicted_bytes
                .saturating_add(source_stats.evicted_bytes)
                .saturating_add(outcome.evicted_bytes),
            dropped_fields: target_stats
                .dropped_fields
                .saturating_add(source_stats.dropped_fields)
                .saturating_add(outcome.dropped_fields),
            truncated_fields: target_stats
                .truncated_fields
                .saturating_add(source_stats.truncated_fields)
                .saturating_add(outcome.truncated_fields),
            truncated_text_bytes: target_stats
                .truncated_text_bytes
                .saturating_add(source_stats.truncated_text_bytes)
                .saturating_add(outcome.truncated_text_bytes),
        };
        outcome.propagated_unretained_errors = missing_errors;
        outcome.propagated_unretained_warnings = missing_warnings;
        outcome.propagated_unretained_info = missing_info;
        outcome
    }

    /// Publishes explicit entries through this report's settings.
    ///
    /// A whole report is deliberately not an owned iterator. Use [`Self::extend`] to preserve its
    /// sticky severity and bounded-loss accounting.
    ///
    /// ```compile_fail
    /// use nara_core::diagnostic::DiagnosticReport;
    ///
    /// fn bypass_accounting(target: &mut DiagnosticReport, source: DiagnosticReport) {
    ///     target.extend_diagnostics(source);
    /// }
    /// ```
    pub fn extend_diagnostics(
        &mut self,
        diagnostics: impl IntoIterator<Item = ReportDiagnostic>,
    ) -> DiagnosticReportMergeOutcome {
        let mut outcome = DiagnosticReportMergeOutcome::default();
        for diagnostic in diagnostics {
            outcome.record(self.push(diagnostic));
        }
        outcome
    }

    #[must_use]
    pub const fn settings(&self) -> DiagnosticReportSettings {
        self.settings
    }

    #[must_use]
    pub const fn stats(&self) -> DiagnosticReportStats {
        self.stats
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        usize::try_from(self.stats.observed_entries()).unwrap_or(usize::MAX)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stats.observed_entries() == 0
    }

    #[must_use]
    pub fn retained_len(&self) -> usize {
        self.diagnostics.len()
    }

    #[must_use]
    pub fn is_retained_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ReportDiagnostic> {
        self.diagnostics.iter()
    }

    /// Consumes only the entries still retained by this report.
    ///
    /// This intentionally does not implement owned iteration for `DiagnosticReport`: callers
    /// merging whole reports must use [`Self::extend`] so sticky severity and bounded-loss
    /// accounting cannot be discarded accidentally.
    #[must_use]
    pub fn into_retained_diagnostics(
        self,
    ) -> std::collections::vec_deque::IntoIter<ReportDiagnostic> {
        self.diagnostics.into_iter()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.stats.observed_errors > 0
    }

    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.stats.observed_warnings > 0
    }

    #[must_use]
    pub fn has_info(&self) -> bool {
        self.stats.observed_info > 0
    }

    fn record_observed_severity(&mut self, severity: DiagnosticSeverity) {
        match severity {
            DiagnosticSeverity::Error => {
                self.stats.observed_errors = self.stats.observed_errors.saturating_add(1);
            }
            DiagnosticSeverity::Warning => {
                self.stats.observed_warnings = self.stats.observed_warnings.saturating_add(1);
            }
            DiagnosticSeverity::Info => {
                self.stats.observed_info = self.stats.observed_info.saturating_add(1);
            }
        }
    }
}

fn retained_severity_counts(diagnostics: &VecDeque<ReportDiagnostic>) -> (u64, u64, u64) {
    diagnostics.iter().fold(
        (0_u64, 0_u64, 0_u64),
        |(errors, warnings, info), diagnostic| match diagnostic.severity {
            DiagnosticSeverity::Error => (errors.saturating_add(1), warnings, info),
            DiagnosticSeverity::Warning => (errors, warnings.saturating_add(1), info),
            DiagnosticSeverity::Info => (errors, warnings, info.saturating_add(1)),
        },
    )
}

impl<'a> IntoIterator for &'a DiagnosticReport {
    type Item = &'a ReportDiagnostic;
    type IntoIter = std::collections::vec_deque::Iter<'a, ReportDiagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}
