use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};

use nara_core::ItemLimit;
use nara_ecs::Resource;
use thiserror::Error;

use crate::{PressureMetricId, PressureSourceId};

const MAX_PRESSURE_SOURCES: usize = 1_024;
const MAX_PRESSURE_MEASUREMENTS_PER_SOURCE: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PressureMetricKind {
    Gauge,
    Counter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PressureUnit {
    Count,
    Items,
    Bytes,
    Depth,
    Nanoseconds,
    Frames,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PressureMeasurement {
    metric: PressureMetricId,
    kind: PressureMetricKind,
    unit: PressureUnit,
    value: u64,
}

impl PressureMeasurement {
    #[must_use]
    pub const fn gauge(metric: PressureMetricId, unit: PressureUnit, value: u64) -> Self {
        Self {
            metric,
            kind: PressureMetricKind::Gauge,
            unit,
            value,
        }
    }

    #[must_use]
    pub const fn counter(metric: PressureMetricId, unit: PressureUnit, value: u64) -> Self {
        Self {
            metric,
            kind: PressureMetricKind::Counter,
            unit,
            value,
        }
    }

    #[must_use]
    pub const fn metric(&self) -> &PressureMetricId {
        &self.metric
    }

    #[must_use]
    pub const fn kind(&self) -> PressureMetricKind {
        self.kind
    }

    #[must_use]
    pub const fn unit(&self) -> PressureUnit {
        self.unit
    }

    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PressureDraftError {
    #[error("pressure draft measurement hard limit reached")]
    MeasurementLimitReached,
    #[error("pressure draft contains a duplicate metric ID")]
    DuplicateMetric,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePressureSnapshotDraft {
    source: PressureSourceId,
    measurements: Vec<PressureMeasurement>,
}

impl RuntimePressureSnapshotDraft {
    #[must_use]
    pub fn new(source: PressureSourceId) -> Self {
        Self {
            source,
            measurements: Vec::new(),
        }
    }

    pub fn try_with_measurement(
        mut self,
        measurement: PressureMeasurement,
    ) -> Result<Self, PressureDraftError> {
        if self.measurements.len() >= MAX_PRESSURE_MEASUREMENTS_PER_SOURCE {
            return Err(PressureDraftError::MeasurementLimitReached);
        }
        if self
            .measurements
            .iter()
            .any(|existing| existing.metric == measurement.metric)
        {
            return Err(PressureDraftError::DuplicateMetric);
        }
        self.measurements.push(measurement);
        Ok(self)
    }

    #[must_use]
    pub const fn source(&self) -> &PressureSourceId {
        &self.source
    }

    #[must_use]
    pub fn measurements(&self) -> &[PressureMeasurement] {
        &self.measurements
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum RuntimePressureRetention {
    #[default]
    Manual,
    FrameWindow(NonZeroU64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PressureSettingsError {
    #[error("pressure source limit {requested} exceeds hard limit {maximum}")]
    SourceLimitTooLarge { requested: usize, maximum: usize },
    #[error("pressure measurement limit {requested} exceeds hard limit {maximum}")]
    MeasurementLimitTooLarge { requested: usize, maximum: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimePressureSettings {
    source_limit: ItemLimit,
    measurements_per_source: ItemLimit,
    retention: RuntimePressureRetention,
}

impl Default for RuntimePressureSettings {
    fn default() -> Self {
        Self {
            source_limit: ItemLimit::new(128).expect("default pressure source limit is non-zero"),
            measurements_per_source: ItemLimit::new(32)
                .expect("default pressure measurement limit is non-zero"),
            retention: RuntimePressureRetention::Manual,
        }
    }
}

impl RuntimePressureSettings {
    pub fn new(
        source_limit: ItemLimit,
        measurements_per_source: ItemLimit,
    ) -> Result<Self, PressureSettingsError> {
        if source_limit.get() > MAX_PRESSURE_SOURCES {
            return Err(PressureSettingsError::SourceLimitTooLarge {
                requested: source_limit.get(),
                maximum: MAX_PRESSURE_SOURCES,
            });
        }
        if measurements_per_source.get() > MAX_PRESSURE_MEASUREMENTS_PER_SOURCE {
            return Err(PressureSettingsError::MeasurementLimitTooLarge {
                requested: measurements_per_source.get(),
                maximum: MAX_PRESSURE_MEASUREMENTS_PER_SOURCE,
            });
        }
        Ok(Self {
            source_limit,
            measurements_per_source,
            ..Self::default()
        })
    }

    #[must_use]
    pub const fn with_retention(mut self, retention: RuntimePressureRetention) -> Self {
        self.retention = retention;
        self
    }

    #[must_use]
    pub const fn with_retention_frame_window(mut self, frames: NonZeroU64) -> Self {
        self.retention = RuntimePressureRetention::FrameWindow(frames);
        self
    }

    #[must_use]
    pub const fn source_limit(self) -> ItemLimit {
        self.source_limit
    }

    #[must_use]
    pub const fn measurements_per_source(self) -> ItemLimit {
        self.measurements_per_source
    }

    #[must_use]
    pub const fn retention(self) -> RuntimePressureRetention {
        self.retention
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimePressureSnapshot {
    source: PressureSourceId,
    frame: u64,
    measurements: Vec<PressureMeasurement>,
}

impl RuntimePressureSnapshot {
    #[must_use]
    pub const fn source(&self) -> &PressureSourceId {
        &self.source
    }

    #[must_use]
    pub const fn frame(&self) -> u64 {
        self.frame
    }

    #[must_use]
    pub fn measurements(&self) -> &[PressureMeasurement] {
        &self.measurements
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PressurePublishRejection {
    #[error("pressure source limit reached")]
    SourceLimitReached { maximum: usize },
    #[error("pressure snapshot measurement limit exceeded")]
    MeasurementLimitExceeded { maximum: usize },
    #[error("pressure snapshot frame precedes the retained source frame")]
    StaleFrame { retained: u64, incoming: u64 },
    #[error("pressure snapshot frame precedes the active retention window")]
    ExpiredFrame {
        oldest_retained_frame: u64,
        incoming: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressurePublishOutcome {
    Inserted,
    Replaced,
    Rejected(PressurePublishRejection),
}

impl PressurePublishOutcome {
    #[must_use]
    pub const fn is_inserted(self) -> bool {
        matches!(self, Self::Inserted)
    }

    #[must_use]
    pub const fn is_replaced(self) -> bool {
        matches!(self, Self::Replaced)
    }

    #[must_use]
    pub const fn is_rejected(self) -> bool {
        matches!(self, Self::Rejected(_))
    }

    #[must_use]
    pub const fn rejection(self) -> Option<PressurePublishRejection> {
        match self {
            Self::Rejected(rejection) => Some(rejection),
            Self::Inserted | Self::Replaced => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PressureStats {
    inserted_snapshots: u64,
    replaced_snapshots: u64,
    rejected_snapshots: u64,
    expired_snapshots: u64,
}

impl PressureStats {
    #[must_use]
    pub const fn inserted_snapshots(self) -> u64 {
        self.inserted_snapshots
    }

    #[must_use]
    pub const fn replaced_snapshots(self) -> u64 {
        self.replaced_snapshots
    }

    #[must_use]
    pub const fn rejected_snapshots(self) -> u64 {
        self.rejected_snapshots
    }

    #[must_use]
    pub const fn expired_snapshots(self) -> u64 {
        self.expired_snapshots
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RuntimePressureSnapshotsSnapshot {
    snapshots: Vec<RuntimePressureSnapshot>,
    stats: PressureStats,
}

impl RuntimePressureSnapshotsSnapshot {
    #[must_use]
    pub fn snapshots(&self) -> &[RuntimePressureSnapshot] {
        &self.snapshots
    }

    #[must_use]
    pub const fn stats(&self) -> PressureStats {
        self.stats
    }
}

#[derive(Debug, Clone, Resource)]
pub struct RuntimePressureSnapshots {
    settings: RuntimePressureSettings,
    snapshots: BTreeMap<PressureSourceId, RuntimePressureSnapshot>,
    expiry_index: BTreeSet<(u64, PressureSourceId)>,
    retention_watermark: u64,
    stats: PressureStats,
}

impl Default for RuntimePressureSnapshots {
    fn default() -> Self {
        Self::new(RuntimePressureSettings::default())
    }
}

impl RuntimePressureSnapshots {
    #[must_use]
    pub fn new(settings: RuntimePressureSettings) -> Self {
        Self {
            settings,
            snapshots: BTreeMap::new(),
            expiry_index: BTreeSet::new(),
            retention_watermark: 0,
            stats: PressureStats::default(),
        }
    }

    pub fn publish(
        &mut self,
        draft: RuntimePressureSnapshotDraft,
        frame: u64,
    ) -> PressurePublishOutcome {
        if draft.measurements.len() > self.settings.measurements_per_source.get() {
            self.stats.rejected_snapshots = self.stats.rejected_snapshots.saturating_add(1);
            return PressurePublishOutcome::Rejected(
                PressurePublishRejection::MeasurementLimitExceeded {
                    maximum: self.settings.measurements_per_source.get(),
                },
            );
        }
        if let Some(oldest_retained_frame) = self.oldest_retained_frame()
            && frame < oldest_retained_frame
        {
            self.stats.rejected_snapshots = self.stats.rejected_snapshots.saturating_add(1);
            return PressurePublishOutcome::Rejected(PressurePublishRejection::ExpiredFrame {
                oldest_retained_frame,
                incoming: frame,
            });
        }
        let existing_frame = self
            .snapshots
            .get(&draft.source)
            .map(RuntimePressureSnapshot::frame);
        if let Some(existing_frame) = existing_frame
            && frame < existing_frame
        {
            self.stats.rejected_snapshots = self.stats.rejected_snapshots.saturating_add(1);
            return PressurePublishOutcome::Rejected(PressurePublishRejection::StaleFrame {
                retained: existing_frame,
                incoming: frame,
            });
        }
        if existing_frame.is_none() && self.snapshots.len() >= self.settings.source_limit.get() {
            self.stats.rejected_snapshots = self.stats.rejected_snapshots.saturating_add(1);
            return PressurePublishOutcome::Rejected(
                PressurePublishRejection::SourceLimitReached {
                    maximum: self.settings.source_limit.get(),
                },
            );
        }

        let was_replacement = existing_frame.is_some();
        let old_expiry = existing_frame.and_then(|existing_frame| {
            pressure_expiration_for(self.settings.retention, existing_frame)
        });
        let new_expiry = pressure_expiration_for(self.settings.retention, frame);
        let snapshot = RuntimePressureSnapshot {
            source: draft.source,
            frame,
            measurements: draft.measurements,
        };
        if let Some(expires_at) = old_expiry {
            self.expiry_index.remove(&(expires_at, draft.source));
        }
        self.snapshots.insert(draft.source, snapshot);
        if let Some(expires_at) = new_expiry {
            self.expiry_index.insert((expires_at, draft.source));
        }
        if was_replacement {
            self.stats.replaced_snapshots = self.stats.replaced_snapshots.saturating_add(1);
            PressurePublishOutcome::Replaced
        } else {
            self.stats.inserted_snapshots = self.stats.inserted_snapshots.saturating_add(1);
            PressurePublishOutcome::Inserted
        }
    }

    pub(crate) fn maintain(&mut self, current_frame: u64) {
        let RuntimePressureRetention::FrameWindow(_) = self.settings.retention else {
            return;
        };
        self.retention_watermark = self.retention_watermark.max(current_frame);
        while let Some(&(expires_at, source)) = self.expiry_index.first() {
            if expires_at > self.retention_watermark {
                break;
            }
            self.expiry_index.pop_first();
            let is_current = self.snapshots.get(&source).is_some_and(|snapshot| {
                pressure_expiration_for(self.settings.retention, snapshot.frame) == Some(expires_at)
            });
            if !is_current {
                continue;
            }
            self.snapshots.remove(&source);
            self.stats.expired_snapshots = self.stats.expired_snapshots.saturating_add(1);
        }
    }

    fn oldest_retained_frame(&self) -> Option<u64> {
        let RuntimePressureRetention::FrameWindow(window) = self.settings.retention else {
            return None;
        };
        Some(self.retention_watermark.saturating_sub(window.get()))
    }

    #[must_use]
    pub const fn settings(&self) -> RuntimePressureSettings {
        self.settings
    }

    #[must_use]
    pub const fn stats(&self) -> PressureStats {
        self.stats
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    #[must_use]
    pub fn get(&self, source: &PressureSourceId) -> Option<&RuntimePressureSnapshot> {
        self.snapshots.get(source)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &RuntimePressureSnapshot> {
        self.snapshots.values()
    }

    #[must_use]
    pub fn snapshot(&self) -> RuntimePressureSnapshotsSnapshot {
        RuntimePressureSnapshotsSnapshot {
            snapshots: self.snapshots.values().cloned().collect(),
            stats: self.stats,
        }
    }

    #[cfg(test)]
    pub(crate) fn maintain_for_test(&mut self, current_frame: u64) {
        self.maintain(current_frame);
    }

    #[cfg(test)]
    pub(crate) fn set_stats_for_test(&mut self, value: u64) {
        self.stats = PressureStats {
            inserted_snapshots: value,
            replaced_snapshots: value,
            rejected_snapshots: value,
            expired_snapshots: value,
        };
    }

    #[cfg(test)]
    pub(crate) fn expiry_index_len_for_test(&self) -> usize {
        self.expiry_index.len()
    }
}

fn pressure_expiration_for(retention: RuntimePressureRetention, last_frame: u64) -> Option<u64> {
    let RuntimePressureRetention::FrameWindow(window) = retention else {
        return None;
    };
    last_frame
        .checked_add(window.get())
        .and_then(|value| value.checked_add(1))
}
