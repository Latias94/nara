use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    fmt,
    num::NonZeroU64,
};

use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Resource;
use thiserror::Error;

use crate::{
    DiagnosticBuildError, DiagnosticCode, DiagnosticDomain, DiagnosticField, DiagnosticProducer,
    DiagnosticSeverity, SafeSummary,
    field::{DedupeField, push_field, usize_to_u64},
    report::{
        DiagnosticSettingsError, DiagnosticTruncation, apply_field_text_limit,
        validate_entry_and_byte_limits, validate_field_limit, validate_text_limit,
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DiagnosticDedupePolicy {
    #[default]
    None,
    Code,
    CodeAndFields,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum RuntimeDiagnosticRetention {
    #[default]
    Manual,
    FrameWindow(NonZeroU64),
}

impl RuntimeDiagnosticRetention {
    #[must_use]
    pub const fn frame_window(frames: NonZeroU64) -> Self {
        Self::FrameWindow(frames)
    }

    #[must_use]
    pub const fn frames(self) -> Option<NonZeroU64> {
        match self {
            Self::Manual => None,
            Self::FrameWindow(frames) => Some(frames),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimeDiagnosticsSettings {
    entry_limit: ItemLimit,
    byte_limit: ByteLimit,
    field_limit: ItemLimit,
    summary_byte_limit: ByteLimit,
    field_text_byte_limit: ByteLimit,
    retention: RuntimeDiagnosticRetention,
}

impl Default for RuntimeDiagnosticsSettings {
    fn default() -> Self {
        Self {
            entry_limit: ItemLimit::new(256).expect("default runtime entry limit is non-zero"),
            byte_limit: ByteLimit::new(512 * 1024).expect("default runtime byte limit is non-zero"),
            field_limit: ItemLimit::new(16).expect("default runtime field limit is non-zero"),
            summary_byte_limit: ByteLimit::new(256)
                .expect("default runtime summary limit is non-zero"),
            field_text_byte_limit: ByteLimit::new(512)
                .expect("default runtime field text limit is non-zero"),
            retention: RuntimeDiagnosticRetention::Manual,
        }
    }
}

impl RuntimeDiagnosticsSettings {
    pub fn new(
        entry_limit: ItemLimit,
        byte_limit: ByteLimit,
    ) -> Result<Self, DiagnosticSettingsError> {
        validate_entry_and_byte_limits(entry_limit, byte_limit)?;
        Ok(Self {
            entry_limit,
            byte_limit,
            ..Self::default()
        })
    }

    pub fn with_field_limit(
        mut self,
        field_limit: ItemLimit,
    ) -> Result<Self, DiagnosticSettingsError> {
        validate_field_limit(field_limit)?;
        self.field_limit = field_limit;
        Ok(self)
    }

    pub fn with_summary_byte_limit(
        mut self,
        summary_byte_limit: ByteLimit,
    ) -> Result<Self, DiagnosticSettingsError> {
        validate_text_limit(summary_byte_limit)?;
        self.summary_byte_limit = summary_byte_limit;
        Ok(self)
    }

    pub fn with_field_text_byte_limit(
        mut self,
        field_text_byte_limit: ByteLimit,
    ) -> Result<Self, DiagnosticSettingsError> {
        validate_text_limit(field_text_byte_limit)?;
        self.field_text_byte_limit = field_text_byte_limit;
        Ok(self)
    }

    #[must_use]
    pub const fn with_retention(mut self, retention: RuntimeDiagnosticRetention) -> Self {
        self.retention = retention;
        self
    }

    #[must_use]
    pub const fn entry_limit(self) -> ItemLimit {
        self.entry_limit
    }

    #[must_use]
    pub const fn byte_limit(self) -> ByteLimit {
        self.byte_limit
    }

    #[must_use]
    pub const fn field_limit(self) -> ItemLimit {
        self.field_limit
    }

    #[must_use]
    pub const fn summary_byte_limit(self) -> ByteLimit {
        self.summary_byte_limit
    }

    #[must_use]
    pub const fn field_text_byte_limit(self) -> ByteLimit {
        self.field_text_byte_limit
    }

    #[must_use]
    pub const fn retention(self) -> RuntimeDiagnosticRetention {
        self.retention
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDiagnosticDraft {
    producer: DiagnosticProducer,
    domain: DiagnosticDomain,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    summary: SafeSummary,
    fields: Vec<DiagnosticField>,
    dedupe_policy: DiagnosticDedupePolicy,
}

impl RuntimeDiagnosticDraft {
    #[must_use]
    pub fn new(
        producer: DiagnosticProducer,
        domain: DiagnosticDomain,
        code: DiagnosticCode,
        severity: DiagnosticSeverity,
        summary: SafeSummary,
    ) -> Self {
        Self {
            producer,
            domain,
            code,
            severity,
            summary,
            fields: Vec::new(),
            dedupe_policy: DiagnosticDedupePolicy::None,
        }
    }

    pub fn try_with_field(mut self, field: DiagnosticField) -> Result<Self, DiagnosticBuildError> {
        push_field(&mut self.fields, field)?;
        Ok(self)
    }

    #[must_use]
    pub const fn with_dedupe_policy(mut self, policy: DiagnosticDedupePolicy) -> Self {
        self.dedupe_policy = policy;
        self
    }

    #[must_use]
    pub const fn dedupe_by_code(self) -> Self {
        self.with_dedupe_policy(DiagnosticDedupePolicy::Code)
    }

    #[must_use]
    pub const fn dedupe_by_code_and_fields(self) -> Self {
        self.with_dedupe_policy(DiagnosticDedupePolicy::CodeAndFields)
    }

    #[must_use]
    pub const fn producer(&self) -> &DiagnosticProducer {
        &self.producer
    }

    #[must_use]
    pub const fn domain(&self) -> &DiagnosticDomain {
        &self.domain
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

    #[must_use]
    pub const fn dedupe_policy(&self) -> DiagnosticDedupePolicy {
        self.dedupe_policy
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimeDiagnosticEntry {
    sequence: u64,
    producer: DiagnosticProducer,
    domain: DiagnosticDomain,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    summary: SafeSummary,
    fields: Vec<DiagnosticField>,
    first_frame: u64,
    last_frame: u64,
    repeat_count: u64,
    retained_bytes: usize,
}

impl RuntimeDiagnosticEntry {
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn producer(&self) -> &DiagnosticProducer {
        &self.producer
    }

    #[must_use]
    pub const fn domain(&self) -> &DiagnosticDomain {
        &self.domain
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

    #[must_use]
    pub const fn first_frame(&self) -> u64 {
        self.first_frame
    }

    #[must_use]
    pub const fn last_frame(&self) -> u64 {
        self.last_frame
    }

    #[must_use]
    pub const fn repeat_count(&self) -> u64 {
        self.repeat_count
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub fn emit_to_tracing(&self) {
        match self.severity {
            DiagnosticSeverity::Error => tracing::error!(
                sequence = self.sequence,
                producer = self.producer.as_str(),
                domain = self.domain.as_str(),
                code = self.code.as_str(),
                summary = self.summary.as_str(),
                repeat_count = self.repeat_count,
                "runtime diagnostic"
            ),
            DiagnosticSeverity::Warning => tracing::warn!(
                sequence = self.sequence,
                producer = self.producer.as_str(),
                domain = self.domain.as_str(),
                code = self.code.as_str(),
                summary = self.summary.as_str(),
                repeat_count = self.repeat_count,
                "runtime diagnostic"
            ),
            DiagnosticSeverity::Info => tracing::info!(
                sequence = self.sequence,
                producer = self.producer.as_str(),
                domain = self.domain.as_str(),
                code = self.code.as_str(),
                summary = self.summary.as_str(),
                repeat_count = self.repeat_count,
                "runtime diagnostic"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DedupeIdentity {
    producer: DiagnosticProducer,
    domain: DiagnosticDomain,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    fields: Vec<DedupeField>,
}

#[derive(Debug, Clone)]
struct StoredRuntimeDiagnostic {
    entry: RuntimeDiagnosticEntry,
    dedupe_identity: Option<DedupeIdentity>,
    expires_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuntimePublishRejection {
    #[error("diagnostic entry exceeds retained byte limit")]
    EntryTooLarge,
    #[error("runtime diagnostic sequence is exhausted")]
    SequenceExhausted,
    #[error("runtime diagnostic frame precedes the active retention window")]
    ExpiredFrame {
        oldest_retained_frame: u64,
        incoming: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePublishOutcome {
    Published {
        sequence: u64,
        evicted_entries: u64,
        evicted_bytes: u64,
        truncation: DiagnosticTruncation,
    },
    Deduplicated {
        sequence: u64,
        repeat_count: u64,
        truncation: DiagnosticTruncation,
    },
    Rejected(RuntimePublishRejection),
}

impl RuntimePublishOutcome {
    #[must_use]
    pub const fn is_published(self) -> bool {
        matches!(self, Self::Published { .. })
    }

    #[must_use]
    pub const fn is_rejected(self) -> bool {
        matches!(self, Self::Rejected(_))
    }

    #[must_use]
    pub const fn is_sequence_exhausted(self) -> bool {
        matches!(
            self,
            Self::Rejected(RuntimePublishRejection::SequenceExhausted)
        )
    }

    #[must_use]
    pub const fn sequence(self) -> Option<u64> {
        match self {
            Self::Published { sequence, .. } | Self::Deduplicated { sequence, .. } => {
                Some(sequence)
            }
            Self::Rejected(_) => None,
        }
    }

    #[must_use]
    pub const fn rejection(self) -> Option<RuntimePublishRejection> {
        match self {
            Self::Rejected(rejection) => Some(rejection),
            Self::Published { .. } | Self::Deduplicated { .. } => None,
        }
    }

    #[must_use]
    pub const fn evicted_entries(self) -> u64 {
        match self {
            Self::Published {
                evicted_entries, ..
            } => evicted_entries,
            Self::Deduplicated { .. } | Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn evicted_bytes(self) -> u64 {
        match self {
            Self::Published { evicted_bytes, .. } => evicted_bytes,
            Self::Deduplicated { .. } | Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn dropped_fields(self) -> u64 {
        match self {
            Self::Published { truncation, .. } | Self::Deduplicated { truncation, .. } => {
                truncation.dropped_fields()
            }
            Self::Rejected(_) => 0,
        }
    }

    #[must_use]
    pub const fn truncated_text_bytes(self) -> u64 {
        match self {
            Self::Published { truncation, .. } | Self::Deduplicated { truncation, .. } => {
                truncation.truncated_text_bytes()
            }
            Self::Rejected(_) => 0,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimeDiagnosticsStats {
    published_entries: u64,
    deduplicated_entries: u64,
    rejected_entries: u64,
    evicted_entries: u64,
    evicted_bytes: u64,
    expired_entries: u64,
    dropped_fields: u64,
    truncated_fields: u64,
    truncated_text_bytes: u64,
}

impl RuntimeDiagnosticsStats {
    #[must_use]
    pub const fn published_entries(self) -> u64 {
        self.published_entries
    }

    #[must_use]
    pub const fn deduplicated_entries(self) -> u64 {
        self.deduplicated_entries
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
    pub const fn expired_entries(self) -> u64 {
        self.expired_entries
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuntimeDiagnosticFilter {
    severity: Option<DiagnosticSeverity>,
    producer: Option<DiagnosticProducer>,
    domain: Option<DiagnosticDomain>,
    code: Option<DiagnosticCode>,
    first_frame: Option<u64>,
    last_frame: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RuntimeDiagnosticFilterError {
    #[error("runtime diagnostic frame range is reversed")]
    ReversedFrameRange,
}

impl RuntimeDiagnosticFilter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
    pub fn producer(mut self, producer: DiagnosticProducer) -> Self {
        self.producer = Some(producer);
        self
    }

    #[must_use]
    pub fn domain(mut self, domain: DiagnosticDomain) -> Self {
        self.domain = Some(domain);
        self
    }

    #[must_use]
    pub fn code(mut self, code: DiagnosticCode) -> Self {
        self.code = Some(code);
        self
    }

    pub fn frame_range(
        mut self,
        first_frame: u64,
        last_frame: u64,
    ) -> Result<Self, RuntimeDiagnosticFilterError> {
        if first_frame > last_frame {
            return Err(RuntimeDiagnosticFilterError::ReversedFrameRange);
        }
        self.first_frame = Some(first_frame);
        self.last_frame = Some(last_frame);
        Ok(self)
    }

    fn matches(&self, entry: &RuntimeDiagnosticEntry) -> bool {
        self.severity.is_none_or(|value| entry.severity == value)
            && self
                .producer
                .as_ref()
                .is_none_or(|value| entry.producer == *value)
            && self
                .domain
                .as_ref()
                .is_none_or(|value| entry.domain == *value)
            && self.code.as_ref().is_none_or(|value| entry.code == *value)
            && self
                .first_frame
                .is_none_or(|value| entry.last_frame >= value)
            && self
                .last_frame
                .is_none_or(|value| entry.first_frame <= value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimeDiagnosticsSnapshot {
    entries: Vec<RuntimeDiagnosticEntry>,
    retained_bytes: usize,
    stats: RuntimeDiagnosticsStats,
}

impl RuntimeDiagnosticsSnapshot {
    #[must_use]
    pub fn entries(&self) -> &[RuntimeDiagnosticEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub const fn stats(&self) -> RuntimeDiagnosticsStats {
        self.stats
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeTracingCursor {
    next_sequence: Option<u64>,
}

#[derive(Clone, Resource)]
pub struct RuntimeDiagnostics {
    settings: RuntimeDiagnosticsSettings,
    order: VecDeque<u64>,
    order_tombstones: usize,
    entries: HashMap<u64, StoredRuntimeDiagnostic>,
    dedupe_index: HashMap<DedupeIdentity, u64>,
    expiry_index: BTreeSet<(u64, u64)>,
    next_sequence: Option<u64>,
    retention_watermark: u64,
    retained_bytes: usize,
    stats: RuntimeDiagnosticsStats,
}

impl fmt::Debug for RuntimeDiagnostics {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeDiagnostics")
            .field("settings", &self.settings)
            .field("entries", &self.iter().collect::<Vec<_>>())
            .field("order_tombstones", &self.order_tombstones)
            .field("expiry_index_len", &self.expiry_index.len())
            .field("next_sequence", &self.next_sequence)
            .field("retention_watermark", &self.retention_watermark)
            .field("retained_bytes", &self.retained_bytes)
            .field("stats", &self.stats)
            .finish()
    }
}

impl Default for RuntimeDiagnostics {
    fn default() -> Self {
        Self::new(RuntimeDiagnosticsSettings::default())
    }
}

impl RuntimeDiagnostics {
    #[must_use]
    pub fn new(settings: RuntimeDiagnosticsSettings) -> Self {
        Self {
            order: VecDeque::with_capacity(settings.entry_limit.get()),
            order_tombstones: 0,
            entries: HashMap::with_capacity(settings.entry_limit.get()),
            dedupe_index: HashMap::with_capacity(settings.entry_limit.get()),
            expiry_index: BTreeSet::new(),
            settings,
            next_sequence: Some(0),
            retention_watermark: 0,
            retained_bytes: 0,
            stats: RuntimeDiagnosticsStats::default(),
        }
    }

    pub fn publish(&mut self, draft: RuntimeDiagnosticDraft, frame: u64) -> RuntimePublishOutcome {
        let (mut entry, truncation, dedupe_policy) = self.prepare_entry(draft, frame);
        if entry.retained_bytes > self.settings.byte_limit.get() {
            self.stats.rejected_entries = self.stats.rejected_entries.saturating_add(1);
            return RuntimePublishOutcome::Rejected(RuntimePublishRejection::EntryTooLarge);
        }
        if let Some(oldest_retained_frame) = self.oldest_retained_frame()
            && frame < oldest_retained_frame
        {
            self.stats.rejected_entries = self.stats.rejected_entries.saturating_add(1);
            return RuntimePublishOutcome::Rejected(RuntimePublishRejection::ExpiredFrame {
                oldest_retained_frame,
                incoming: frame,
            });
        }
        let dedupe_identity = make_dedupe_identity(&entry, dedupe_policy);
        if let Some(identity) = dedupe_identity.as_ref()
            && let Some(sequence) = self.dedupe_index.get(identity).copied()
        {
            if let Some(existing) = self.entries.get_mut(&sequence) {
                let first_frame = existing.entry.first_frame.min(frame);
                let last_frame = existing.entry.last_frame.max(frame);
                let old_expiry = existing.expires_at;
                let new_expiry = expiration_for(self.settings.retention, last_frame);
                if old_expiry != new_expiry {
                    if let Some(expires_at) = old_expiry {
                        self.expiry_index.remove(&(expires_at, sequence));
                    }
                    if let Some(expires_at) = new_expiry {
                        self.expiry_index.insert((expires_at, sequence));
                    }
                }
                existing.entry.repeat_count = existing.entry.repeat_count.saturating_add(1);
                existing.entry.first_frame = first_frame;
                existing.entry.last_frame = last_frame;
                existing.expires_at = new_expiry;
                let repeat_count = existing.entry.repeat_count;
                self.stats.deduplicated_entries = self.stats.deduplicated_entries.saturating_add(1);
                self.record_truncation(truncation);
                return RuntimePublishOutcome::Deduplicated {
                    sequence,
                    repeat_count,
                    truncation,
                };
            }
            self.dedupe_index.remove(identity);
        }

        let Some(sequence) = self.next_sequence else {
            self.stats.rejected_entries = self.stats.rejected_entries.saturating_add(1);
            return RuntimePublishOutcome::Rejected(RuntimePublishRejection::SequenceExhausted);
        };

        let mut evicted_entries = 0_u64;
        let mut evicted_bytes = 0_u64;
        while self.entries.len() >= self.settings.entry_limit.get()
            || self.retained_bytes.saturating_add(entry.retained_bytes)
                > self.settings.byte_limit.get()
        {
            let Some(bytes) = self.evict_oldest() else {
                break;
            };
            evicted_entries = evicted_entries.saturating_add(1);
            evicted_bytes = evicted_bytes.saturating_add(usize_to_u64(bytes));
        }
        self.compact_order_if_needed();

        entry.sequence = sequence;
        let expires_at = expiration_for(self.settings.retention, entry.last_frame);
        self.next_sequence = sequence.checked_add(1);
        self.retained_bytes = self.retained_bytes.saturating_add(entry.retained_bytes);
        if let Some(identity) = dedupe_identity.as_ref() {
            self.dedupe_index.insert(identity.clone(), sequence);
        }
        self.order.push_back(sequence);
        if let Some(expires_at) = expires_at {
            self.expiry_index.insert((expires_at, sequence));
        }
        self.entries.insert(
            sequence,
            StoredRuntimeDiagnostic {
                entry,
                dedupe_identity,
                expires_at,
            },
        );
        self.stats.published_entries = self.stats.published_entries.saturating_add(1);
        self.stats.evicted_entries = self.stats.evicted_entries.saturating_add(evicted_entries);
        self.stats.evicted_bytes = self.stats.evicted_bytes.saturating_add(evicted_bytes);
        self.record_truncation(truncation);

        RuntimePublishOutcome::Published {
            sequence,
            evicted_entries,
            evicted_bytes,
            truncation,
        }
    }

    fn prepare_entry(
        &self,
        draft: RuntimeDiagnosticDraft,
        frame: u64,
    ) -> (
        RuntimeDiagnosticEntry,
        DiagnosticTruncation,
        DiagnosticDedupePolicy,
    ) {
        let RuntimeDiagnosticDraft {
            producer,
            domain,
            code,
            severity,
            mut summary,
            mut fields,
            dedupe_policy,
        } = draft;
        let original_fields = fields.len();
        fields.truncate(self.settings.field_limit.get());
        let mut truncation = DiagnosticTruncation {
            dropped_fields: usize_to_u64(original_fields - fields.len()),
            ..DiagnosticTruncation::default()
        };
        let (bounded_summary, truncated_summary_bytes) =
            summary.truncate(self.settings.summary_byte_limit.get());
        summary = bounded_summary;
        if truncated_summary_bytes > 0 {
            truncation.truncated_fields = truncation.truncated_fields.saturating_add(1);
            truncation.truncated_text_bytes = truncation
                .truncated_text_bytes
                .saturating_add(usize_to_u64(truncated_summary_bytes));
        }
        apply_field_text_limit(
            &mut fields,
            self.settings.field_text_byte_limit,
            &mut truncation,
        );
        let retained_bytes = producer
            .as_str()
            .len()
            .saturating_add(domain.as_str().len())
            .saturating_add(code.as_str().len())
            .saturating_add(summary.as_str().len())
            .saturating_add(41)
            .saturating_add(
                fields
                    .iter()
                    .map(DiagnosticField::retained_bytes)
                    .sum::<usize>(),
            );
        (
            RuntimeDiagnosticEntry {
                sequence: 0,
                producer,
                domain,
                code,
                severity,
                summary,
                fields,
                first_frame: frame,
                last_frame: frame,
                repeat_count: 1,
                retained_bytes,
            },
            truncation,
            dedupe_policy,
        )
    }

    fn record_truncation(&mut self, truncation: DiagnosticTruncation) {
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
    }

    fn evict_oldest(&mut self) -> Option<usize> {
        loop {
            let sequence = self.order.pop_front()?;
            let Some(stored) = self.entries.remove(&sequence) else {
                self.order_tombstones = self.order_tombstones.saturating_sub(1);
                continue;
            };
            self.remove_indexes(&stored);
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(stored.entry.retained_bytes);
            return Some(stored.entry.retained_bytes);
        }
    }

    pub(crate) fn maintain(&mut self, current_frame: u64) {
        let RuntimeDiagnosticRetention::FrameWindow(_) = self.settings.retention else {
            return;
        };
        self.retention_watermark = self.retention_watermark.max(current_frame);
        while let Some(&(expires_at, sequence)) = self.expiry_index.first() {
            if expires_at > self.retention_watermark {
                break;
            }
            self.expiry_index.pop_first();
            let is_current = self
                .entries
                .get(&sequence)
                .is_some_and(|stored| stored.expires_at == Some(expires_at));
            if !is_current {
                continue;
            }
            let Some(stored) = self.entries.remove(&sequence) else {
                continue;
            };
            if let Some(identity) = stored.dedupe_identity.as_ref()
                && self.dedupe_index.get(identity) == Some(&stored.entry.sequence)
            {
                self.dedupe_index.remove(identity);
            }
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(stored.entry.retained_bytes);
            self.order_tombstones = self.order_tombstones.saturating_add(1);
            self.stats.expired_entries = self.stats.expired_entries.saturating_add(1);
        }
        self.compact_order_if_needed();
    }

    fn remove_indexes(&mut self, stored: &StoredRuntimeDiagnostic) {
        if let Some(identity) = stored.dedupe_identity.as_ref()
            && self.dedupe_index.get(identity) == Some(&stored.entry.sequence)
        {
            self.dedupe_index.remove(identity);
        }
        if let Some(expires_at) = stored.expires_at {
            self.expiry_index
                .remove(&(expires_at, stored.entry.sequence));
        }
    }

    fn compact_order_if_needed(&mut self) {
        if self.order_tombstones <= self.entries.len() {
            return;
        }
        self.order
            .retain(|sequence| self.entries.contains_key(sequence));
        self.order_tombstones = 0;
    }

    fn oldest_retained_frame(&self) -> Option<u64> {
        let RuntimeDiagnosticRetention::FrameWindow(window) = self.settings.retention else {
            return None;
        };
        Some(self.retention_watermark.saturating_sub(window.get()))
    }

    #[must_use]
    pub const fn settings(&self) -> RuntimeDiagnosticsSettings {
        self.settings
    }

    #[must_use]
    pub const fn stats(&self) -> RuntimeDiagnosticsStats {
        self.stats
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RuntimeDiagnosticEntry> {
        self.order
            .iter()
            .filter_map(|sequence| self.entries.get(sequence))
            .map(|stored| &stored.entry)
    }

    pub fn iter_filtered(
        &self,
        filter: RuntimeDiagnosticFilter,
    ) -> impl Iterator<Item = &RuntimeDiagnosticEntry> {
        self.iter().filter(move |entry| filter.matches(entry))
    }

    #[must_use]
    pub fn snapshot(&self) -> RuntimeDiagnosticsSnapshot {
        RuntimeDiagnosticsSnapshot {
            entries: self.iter().cloned().collect(),
            retained_bytes: self.retained_bytes,
            stats: self.stats,
        }
    }

    #[must_use]
    pub fn tracing_cursor(&self) -> RuntimeTracingCursor {
        RuntimeTracingCursor {
            next_sequence: self
                .order
                .iter()
                .copied()
                .find(|sequence| self.entries.contains_key(sequence))
                .or(self.next_sequence),
        }
    }

    pub fn emit_new_to_tracing(&self, cursor: &mut RuntimeTracingCursor) -> usize {
        let Some(next_sequence) = cursor.next_sequence else {
            return 0;
        };
        let Some(last_sequence) = self
            .order
            .iter()
            .rev()
            .copied()
            .find(|sequence| self.entries.contains_key(sequence))
        else {
            return 0;
        };
        if next_sequence > last_sequence {
            return 0;
        }

        let start = self.first_sequence_at_or_after(next_sequence);
        let mut emitted = 0_usize;
        for sequence in self.order.range(start..).copied() {
            let Some(stored) = self.entries.get(&sequence) else {
                continue;
            };
            stored.entry.emit_to_tracing();
            cursor.next_sequence = sequence.checked_add(1);
            emitted = emitted.saturating_add(1);
        }
        emitted
    }

    fn first_sequence_at_or_after(&self, target: u64) -> usize {
        let mut lower = 0;
        let mut upper = self.order.len();
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            let sequence = self.order[middle];
            if sequence < target {
                lower = middle + 1;
            } else {
                upper = middle;
            }
        }
        lower
    }

    #[cfg(test)]
    pub(crate) fn maintain_for_test(&mut self, current_frame: u64) {
        self.maintain(current_frame);
    }

    #[cfg(test)]
    pub(crate) fn set_next_sequence_for_test(&mut self, sequence: Option<u64>) {
        self.next_sequence = sequence;
    }

    #[cfg(test)]
    pub(crate) fn set_repeat_count_for_test(&mut self, sequence: u64, repeat_count: u64) {
        if let Some(stored) = self.entries.get_mut(&sequence) {
            stored.entry.repeat_count = repeat_count;
        }
    }

    #[cfg(test)]
    pub(crate) fn set_stats_for_test(&mut self, value: u64) {
        self.stats = RuntimeDiagnosticsStats {
            published_entries: value,
            deduplicated_entries: value,
            rejected_entries: value,
            evicted_entries: value,
            evicted_bytes: value,
            expired_entries: value,
            dropped_fields: value,
            truncated_fields: value,
            truncated_text_bytes: value,
        };
    }

    #[cfg(test)]
    pub(crate) fn expiry_index_len_for_test(&self) -> usize {
        self.expiry_index.len()
    }

    #[cfg(test)]
    pub(crate) fn order_storage_len_for_test(&self) -> usize {
        self.order.len()
    }

    #[cfg(test)]
    pub(crate) const fn order_tombstones_for_test(&self) -> usize {
        self.order_tombstones
    }
}

fn expiration_for(retention: RuntimeDiagnosticRetention, last_frame: u64) -> Option<u64> {
    let RuntimeDiagnosticRetention::FrameWindow(window) = retention else {
        return None;
    };
    last_frame
        .checked_add(window.get())
        .and_then(|value| value.checked_add(1))
}

fn make_dedupe_identity(
    entry: &RuntimeDiagnosticEntry,
    policy: DiagnosticDedupePolicy,
) -> Option<DedupeIdentity> {
    if policy == DiagnosticDedupePolicy::None {
        return None;
    }
    let mut fields = if policy == DiagnosticDedupePolicy::CodeAndFields {
        entry
            .fields
            .iter()
            .filter_map(DiagnosticField::dedupe_component)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    fields.sort_unstable();
    Some(DedupeIdentity {
        producer: entry.producer,
        domain: entry.domain,
        code: entry.code,
        severity: entry.severity,
        fields,
    })
}
