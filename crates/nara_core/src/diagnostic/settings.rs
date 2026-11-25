use std::num::NonZeroU64;

use thiserror::Error;

use crate::{ByteLimit, ItemLimit};

use super::{
    MAX_DIAGNOSTIC_DRAFT_FIELDS, MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES, MAX_RUNTIME_DIAGNOSTIC_BYTES,
    MAX_RUNTIME_DIAGNOSTIC_ENTRIES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DiagnosticSettingsError {
    #[error("diagnostic entry limit {requested} exceeds hard limit {maximum}")]
    EntryLimitTooLarge { requested: usize, maximum: usize },
    #[error("diagnostic byte limit {requested} exceeds hard limit {maximum}")]
    ByteLimitTooLarge { requested: usize, maximum: usize },
    #[error("diagnostic field limit {requested} exceeds hard limit {maximum}")]
    FieldLimitTooLarge { requested: usize, maximum: usize },
    #[error("diagnostic text limit {requested} exceeds hard limit {maximum}")]
    TextLimitTooLarge { requested: usize, maximum: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DiagnosticReportSettings {
    entry_limit: ItemLimit,
    byte_limit: ByteLimit,
    field_limit: ItemLimit,
    summary_byte_limit: ByteLimit,
    field_text_byte_limit: ByteLimit,
}

impl Default for DiagnosticReportSettings {
    fn default() -> Self {
        Self {
            entry_limit: ItemLimit::new(256).expect("default diagnostic entry limit is non-zero"),
            byte_limit: ByteLimit::new(256 * 1024)
                .expect("default diagnostic byte limit is non-zero"),
            field_limit: ItemLimit::new(16).expect("default diagnostic field limit is non-zero"),
            summary_byte_limit: ByteLimit::new(256)
                .expect("default diagnostic summary limit is non-zero"),
            field_text_byte_limit: ByteLimit::new(512)
                .expect("default diagnostic field text limit is non-zero"),
        }
    }
}

impl DiagnosticReportSettings {
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
}

fn validate_entry_and_byte_limits(
    entry_limit: ItemLimit,
    byte_limit: ByteLimit,
) -> Result<(), DiagnosticSettingsError> {
    if entry_limit.get() > MAX_RUNTIME_DIAGNOSTIC_ENTRIES {
        return Err(DiagnosticSettingsError::EntryLimitTooLarge {
            requested: entry_limit.get(),
            maximum: MAX_RUNTIME_DIAGNOSTIC_ENTRIES,
        });
    }
    if byte_limit.get() > MAX_RUNTIME_DIAGNOSTIC_BYTES {
        return Err(DiagnosticSettingsError::ByteLimitTooLarge {
            requested: byte_limit.get(),
            maximum: MAX_RUNTIME_DIAGNOSTIC_BYTES,
        });
    }
    Ok(())
}

fn validate_field_limit(field_limit: ItemLimit) -> Result<(), DiagnosticSettingsError> {
    if field_limit.get() > MAX_DIAGNOSTIC_DRAFT_FIELDS {
        return Err(DiagnosticSettingsError::FieldLimitTooLarge {
            requested: field_limit.get(),
            maximum: MAX_DIAGNOSTIC_DRAFT_FIELDS,
        });
    }
    Ok(())
}

fn validate_text_limit(text_limit: ByteLimit) -> Result<(), DiagnosticSettingsError> {
    if text_limit.get() > MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES {
        return Err(DiagnosticSettingsError::TextLimitTooLarge {
            requested: text_limit.get(),
            maximum: MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES,
        });
    }
    Ok(())
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
