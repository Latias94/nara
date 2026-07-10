use std::borrow::Cow;

use thiserror::Error;

use crate::{
    DiagnosticFieldKey, MAX_DIAGNOSTIC_DRAFT_FIELDS, MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES,
    PublicDiagnosticIdentifier, SafeDisplayText,
    identity::{contains_sensitive_shape, is_unsafe_format_character},
};

const REDACTED_MARKER: &str = "[REDACTED]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DiagnosticFieldClass {
    Public,
    ProjectRelative,
    Sensitive,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DiagnosticBuildError {
    #[error("diagnostic draft field hard limit reached")]
    FieldLimitReached,
    #[error("diagnostic field key is duplicated")]
    DuplicateField,
    #[error("project-relative field is empty")]
    EmptyProjectRelativePath,
    #[error("project-relative field exceeds its hard byte limit")]
    ProjectRelativePathTooLong,
    #[error("project-relative field is not a normalized relative path")]
    InvalidProjectRelativePath,
    #[error("project-relative field resembles sensitive data")]
    SensitiveProjectRelativePath,
    #[error("project-relative field contains an unsafe control or format character")]
    UnsafeProjectRelativeCharacter,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
enum DiagnosticFieldValue {
    Identifier(PublicDiagnosticIdentifier),
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    StaticDisplay(SafeDisplayText),
    ProjectRelative(Box<str>),
    Redacted(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticValueRef<'a> {
    Identifier(&'a str),
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Display(&'a str),
    ProjectRelative(&'a str),
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DiagnosticField {
    key: DiagnosticFieldKey,
    class: DiagnosticFieldClass,
    value: DiagnosticFieldValue,
}

impl DiagnosticField {
    #[must_use]
    pub fn public_identifier(key: DiagnosticFieldKey, value: PublicDiagnosticIdentifier) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Public,
            value: DiagnosticFieldValue::Identifier(value),
        }
    }

    #[must_use]
    pub fn public_u64(key: DiagnosticFieldKey, value: u64) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Public,
            value: DiagnosticFieldValue::Unsigned(value),
        }
    }

    #[must_use]
    pub fn public_i64(key: DiagnosticFieldKey, value: i64) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Public,
            value: DiagnosticFieldValue::Signed(value),
        }
    }

    #[must_use]
    pub fn public_bool(key: DiagnosticFieldKey, value: bool) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Public,
            value: DiagnosticFieldValue::Bool(value),
        }
    }

    #[must_use]
    pub fn public_display(key: DiagnosticFieldKey, value: SafeDisplayText) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Public,
            value: DiagnosticFieldValue::StaticDisplay(value),
        }
    }

    pub fn project_relative(
        key: DiagnosticFieldKey,
        value: &str,
    ) -> Result<Self, DiagnosticBuildError> {
        validate_project_relative(value)?;
        Ok(Self {
            key,
            class: DiagnosticFieldClass::ProjectRelative,
            value: DiagnosticFieldValue::ProjectRelative(value.into()),
        })
    }

    #[must_use]
    pub fn sensitive(key: DiagnosticFieldKey) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Sensitive,
            value: DiagnosticFieldValue::Redacted(REDACTED_MARKER),
        }
    }

    #[must_use]
    pub fn secret(key: DiagnosticFieldKey) -> Self {
        Self {
            key,
            class: DiagnosticFieldClass::Secret,
            value: DiagnosticFieldValue::Redacted(REDACTED_MARKER),
        }
    }

    #[must_use]
    pub const fn key(&self) -> &DiagnosticFieldKey {
        &self.key
    }

    #[must_use]
    pub const fn class(&self) -> DiagnosticFieldClass {
        self.class
    }

    #[must_use]
    pub fn value(&self) -> DiagnosticValueRef<'_> {
        match &self.value {
            DiagnosticFieldValue::Identifier(value) => {
                DiagnosticValueRef::Identifier(value.as_str())
            }
            DiagnosticFieldValue::Unsigned(value) => DiagnosticValueRef::Unsigned(*value),
            DiagnosticFieldValue::Signed(value) => DiagnosticValueRef::Signed(*value),
            DiagnosticFieldValue::Bool(value) => DiagnosticValueRef::Bool(*value),
            DiagnosticFieldValue::StaticDisplay(value) => {
                DiagnosticValueRef::Display(value.as_str())
            }
            DiagnosticFieldValue::ProjectRelative(value) => {
                DiagnosticValueRef::ProjectRelative(value)
            }
            DiagnosticFieldValue::Redacted(_) => DiagnosticValueRef::Redacted,
        }
    }

    #[must_use]
    pub fn display_value(&self) -> Cow<'_, str> {
        match self.value() {
            DiagnosticValueRef::Identifier(value)
            | DiagnosticValueRef::Display(value)
            | DiagnosticValueRef::ProjectRelative(value) => Cow::Borrowed(value),
            DiagnosticValueRef::Unsigned(value) => Cow::Owned(value.to_string()),
            DiagnosticValueRef::Signed(value) => Cow::Owned(value.to_string()),
            DiagnosticValueRef::Bool(value) => Cow::Borrowed(if value { "true" } else { "false" }),
            DiagnosticValueRef::Redacted => Cow::Borrowed(REDACTED_MARKER),
        }
    }

    pub(crate) fn truncate_text(&mut self, maximum_bytes: usize) -> (u64, bool) {
        match &mut self.value {
            DiagnosticFieldValue::StaticDisplay(value) => {
                let (bounded, truncated) = value.truncate(maximum_bytes);
                *value = bounded;
                (usize_to_u64(truncated), truncated > 0)
            }
            DiagnosticFieldValue::ProjectRelative(_) => (0, false),
            DiagnosticFieldValue::Identifier(_)
            | DiagnosticFieldValue::Unsigned(_)
            | DiagnosticFieldValue::Signed(_)
            | DiagnosticFieldValue::Bool(_)
            | DiagnosticFieldValue::Redacted(_) => (0, false),
        }
    }

    pub(crate) fn discarded_project_text_bytes(&self, maximum_bytes: usize) -> Option<u64> {
        match &self.value {
            DiagnosticFieldValue::ProjectRelative(value) if value.len() > maximum_bytes => {
                Some(usize_to_u64(value.len()))
            }
            DiagnosticFieldValue::Identifier(_)
            | DiagnosticFieldValue::Unsigned(_)
            | DiagnosticFieldValue::Signed(_)
            | DiagnosticFieldValue::Bool(_)
            | DiagnosticFieldValue::StaticDisplay(_)
            | DiagnosticFieldValue::ProjectRelative(_)
            | DiagnosticFieldValue::Redacted(_) => None,
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.key.as_str().len()
            + 1
            + match &self.value {
                DiagnosticFieldValue::Identifier(value) => value.as_str().len(),
                DiagnosticFieldValue::Unsigned(_) | DiagnosticFieldValue::Signed(_) => 8,
                DiagnosticFieldValue::Bool(_) => 1,
                DiagnosticFieldValue::StaticDisplay(value) => value.as_str().len(),
                DiagnosticFieldValue::ProjectRelative(value) => value.len(),
                DiagnosticFieldValue::Redacted(marker) => marker.len(),
            }
    }

    pub(crate) fn dedupe_component(&self) -> Option<DedupeField> {
        let value = match &self.value {
            DiagnosticFieldValue::Identifier(value) => {
                DedupeFieldValue::Identifier(value.as_str().into())
            }
            DiagnosticFieldValue::Unsigned(value) => DedupeFieldValue::Unsigned(*value),
            DiagnosticFieldValue::Signed(value) => DedupeFieldValue::Signed(*value),
            DiagnosticFieldValue::Bool(value) => DedupeFieldValue::Bool(*value),
            DiagnosticFieldValue::StaticDisplay(value) => {
                DedupeFieldValue::StaticDisplay(value.as_str().into())
            }
            DiagnosticFieldValue::ProjectRelative(value) => {
                DedupeFieldValue::ProjectRelative(value.clone())
            }
            DiagnosticFieldValue::Redacted(_) => return None,
        };
        Some(DedupeField {
            key: self.key,
            class: self.class,
            value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DedupeField {
    key: DiagnosticFieldKey,
    class: DiagnosticFieldClass,
    value: DedupeFieldValue,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum DedupeFieldValue {
    Identifier(Box<str>),
    StaticDisplay(Box<str>),
    ProjectRelative(Box<str>),
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
}

pub(crate) fn push_field(
    fields: &mut Vec<DiagnosticField>,
    field: DiagnosticField,
) -> Result<(), DiagnosticBuildError> {
    if fields.len() >= MAX_DIAGNOSTIC_DRAFT_FIELDS {
        return Err(DiagnosticBuildError::FieldLimitReached);
    }
    if fields.iter().any(|existing| existing.key == field.key) {
        return Err(DiagnosticBuildError::DuplicateField);
    }
    fields.push(field);
    Ok(())
}

fn validate_project_relative(value: &str) -> Result<(), DiagnosticBuildError> {
    if value.is_empty() {
        return Err(DiagnosticBuildError::EmptyProjectRelativePath);
    }
    if value.len() > MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES {
        return Err(DiagnosticBuildError::ProjectRelativePathTooLong);
    }
    if contains_sensitive_shape(value) {
        return Err(DiagnosticBuildError::SensitiveProjectRelativePath);
    }
    if value
        .chars()
        .any(|character| character.is_control() || is_unsafe_format_character(character))
    {
        return Err(DiagnosticBuildError::UnsafeProjectRelativeCharacter);
    }
    if value.starts_with('/')
        || value.starts_with("\\\\")
        || value.contains('\\')
        || value.contains(':')
        || value.contains("//")
        || value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(DiagnosticBuildError::InvalidProjectRelativePath);
    }
    Ok(())
}

pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
