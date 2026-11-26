use crate::{ComponentFieldPathError, ComponentValueError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComponentCodecError {
    #[error("missing component field '{field}'")]
    MissingField { field: String },
    #[error("invalid component field '{field}', expected {expected}")]
    InvalidField { field: String, expected: String },
    #[error(transparent)]
    Value(#[from] ComponentValueError),
    #[error(transparent)]
    FieldPath(#[from] ComponentFieldPathError),
    #[error("component field '{field}' is read-only")]
    ReadOnlyField { field: String },
    #[cfg(feature = "postcard")]
    #[error("this component does not have a typed Postcard codec")]
    PostcardUnavailable,
    #[error("{0}")]
    Message(String),
}

impl ComponentCodecError {
    #[must_use]
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    #[must_use]
    pub fn invalid_field(field: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            expected: expected.into(),
        }
    }
}
