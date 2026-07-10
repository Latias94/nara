use std::{error::Error, fmt};

use crate::MAX_SAFE_STATIC_TEXT_BYTES;

const MAX_CODE_BYTES: usize = 96;
const MAX_DOMAIN_BYTES: usize = 64;
const MAX_PRODUCER_BYTES: usize = 96;
const MAX_FIELD_KEY_BYTES: usize = 64;
const MAX_PRESSURE_SOURCE_BYTES: usize = 96;
const MAX_PRESSURE_METRIC_BYTES: usize = 96;
const MAX_PUBLIC_IDENTIFIER_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityErrorReason {
    Empty,
    TooLong,
    InvalidAscii,
    InvalidStart,
    InvalidCharacter,
    SensitiveShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityError {
    kind: &'static str,
    reason: IdentityErrorReason,
    maximum_bytes: usize,
    invalid_index: Option<usize>,
}

impl IdentityError {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    #[must_use]
    pub const fn reason(&self) -> IdentityErrorReason {
        self.reason
    }

    #[must_use]
    pub const fn maximum_bytes(&self) -> usize {
        self.maximum_bytes
    }

    #[must_use]
    pub const fn invalid_index(&self) -> Option<usize> {
        self.invalid_index
    }
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            IdentityErrorReason::Empty => write!(formatter, "{} must not be empty", self.kind),
            IdentityErrorReason::TooLong => write!(
                formatter,
                "{} exceeds its {} byte limit",
                self.kind, self.maximum_bytes
            ),
            IdentityErrorReason::InvalidAscii => {
                write!(formatter, "{} must contain ASCII only", self.kind)
            }
            IdentityErrorReason::InvalidStart => write!(
                formatter,
                "{} must start with a lowercase ASCII letter or digit",
                self.kind
            ),
            IdentityErrorReason::InvalidCharacter => write!(
                formatter,
                "{} contains an invalid character at byte {}",
                self.kind,
                self.invalid_index.unwrap_or_default()
            ),
            IdentityErrorReason::SensitiveShape => write!(
                formatter,
                "{} resembles sensitive data and cannot be public",
                self.kind
            ),
        }
    }
}

impl Error for IdentityError {}

fn validate_identity(
    kind: &'static str,
    value: &str,
    maximum_bytes: usize,
    reject_sensitive_shape: bool,
) -> Result<(), IdentityError> {
    if value.is_empty() {
        return Err(IdentityError {
            kind,
            reason: IdentityErrorReason::Empty,
            maximum_bytes,
            invalid_index: None,
        });
    }
    if value.len() > maximum_bytes {
        return Err(IdentityError {
            kind,
            reason: IdentityErrorReason::TooLong,
            maximum_bytes,
            invalid_index: None,
        });
    }
    if !value.is_ascii() {
        return Err(IdentityError {
            kind,
            reason: IdentityErrorReason::InvalidAscii,
            maximum_bytes,
            invalid_index: None,
        });
    }
    let first = value.as_bytes()[0];
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(IdentityError {
            kind,
            reason: IdentityErrorReason::InvalidStart,
            maximum_bytes,
            invalid_index: Some(0),
        });
    }
    if let Some((index, _)) = value.bytes().enumerate().find(|(_, byte)| {
        !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && !matches!(byte, b'.' | b'-' | b'_')
    }) {
        return Err(IdentityError {
            kind,
            reason: IdentityErrorReason::InvalidCharacter,
            maximum_bytes,
            invalid_index: Some(index),
        });
    }
    if reject_sensitive_shape && contains_sensitive_shape(value) {
        return Err(IdentityError {
            kind,
            reason: IdentityErrorReason::SensitiveShape,
            maximum_bytes,
            invalid_index: None,
        });
    }
    Ok(())
}

macro_rules! static_identity {
    ($name:ident, $kind:literal, $maximum:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name(&'static str);

        impl $name {
            /// Creates a source-authored engine identity. Runtime text cannot satisfy the
            /// `'static` input contract.
            pub fn new(value: &'static str) -> Result<Self, IdentityError> {
                validate_identity($kind, value, $maximum, false)?;
                Ok(Self(value))
            }

            #[must_use]
            pub const fn as_str(self) -> &'static str {
                self.0
            }
        }
    };
}

static_identity!(DiagnosticCode, "diagnostic code", MAX_CODE_BYTES);
static_identity!(DiagnosticDomain, "diagnostic domain", MAX_DOMAIN_BYTES);
static_identity!(
    DiagnosticProducer,
    "diagnostic producer",
    MAX_PRODUCER_BYTES
);
static_identity!(
    DiagnosticFieldKey,
    "diagnostic field key",
    MAX_FIELD_KEY_BYTES
);
static_identity!(
    PressureSourceId,
    "pressure source ID",
    MAX_PRESSURE_SOURCE_BYTES
);
static_identity!(
    PressureMetricId,
    "pressure metric ID",
    MAX_PRESSURE_METRIC_BYTES
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct PublicDiagnosticIdentifier(Box<str>);

impl PublicDiagnosticIdentifier {
    pub fn new(value: &str) -> Result<Self, IdentityError> {
        validate_public_identifier(value)?;
        Ok(Self(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_public_identifier(value: &str) -> Result<(), IdentityError> {
    const KIND: &str = "public diagnostic identifier";
    let error = |reason, invalid_index| IdentityError {
        kind: KIND,
        reason,
        maximum_bytes: MAX_PUBLIC_IDENTIFIER_BYTES,
        invalid_index,
    };
    if value.is_empty() {
        return Err(error(IdentityErrorReason::Empty, None));
    }
    if value.len() > MAX_PUBLIC_IDENTIFIER_BYTES {
        return Err(error(IdentityErrorReason::TooLong, None));
    }
    if !value.is_ascii() {
        return Err(error(IdentityErrorReason::InvalidAscii, None));
    }
    if resembles_absolute_path(value)
        || value.contains("\\")
        || value.contains(':')
        || value.contains('@')
        || value.contains('=')
        || value.starts_with('/')
        || value.ends_with('/')
        || value
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(error(IdentityErrorReason::InvalidCharacter, None));
    }
    if let Some((index, _)) = value.bytes().enumerate().find(|(_, byte)| {
        !byte.is_ascii_alphanumeric() && !matches!(byte, b'.' | b'-' | b'_' | b'/')
    }) {
        return Err(error(IdentityErrorReason::InvalidCharacter, Some(index)));
    }
    if contains_sensitive_shape(value) {
        return Err(error(IdentityErrorReason::SensitiveShape, None));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeTextErrorReason {
    Empty,
    TooLong,
    ControlCharacter,
    UnsafeFormatCharacter,
    SensitiveShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeTextError {
    reason: SafeTextErrorReason,
    maximum_bytes: usize,
    invalid_index: Option<usize>,
}

impl SafeTextError {
    #[must_use]
    pub const fn reason(&self) -> SafeTextErrorReason {
        self.reason
    }

    #[must_use]
    pub const fn maximum_bytes(&self) -> usize {
        self.maximum_bytes
    }

    #[must_use]
    pub const fn invalid_index(&self) -> Option<usize> {
        self.invalid_index
    }
}

impl fmt::Display for SafeTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            SafeTextErrorReason::Empty => formatter.write_str("safe text must not be empty"),
            SafeTextErrorReason::TooLong => write!(
                formatter,
                "safe text exceeds its {} byte hard limit",
                self.maximum_bytes
            ),
            SafeTextErrorReason::ControlCharacter => write!(
                formatter,
                "safe text contains a control character at byte {}",
                self.invalid_index.unwrap_or_default()
            ),
            SafeTextErrorReason::UnsafeFormatCharacter => write!(
                formatter,
                "safe text contains an unsafe format character at byte {}",
                self.invalid_index.unwrap_or_default()
            ),
            SafeTextErrorReason::SensitiveShape => {
                formatter.write_str("safe text resembles sensitive data")
            }
        }
    }
}

impl Error for SafeTextError {}

fn validate_safe_static_text(value: &'static str) -> Result<(), SafeTextError> {
    if value.is_empty() {
        return Err(SafeTextError {
            reason: SafeTextErrorReason::Empty,
            maximum_bytes: MAX_SAFE_STATIC_TEXT_BYTES,
            invalid_index: None,
        });
    }
    if value.len() > MAX_SAFE_STATIC_TEXT_BYTES {
        return Err(SafeTextError {
            reason: SafeTextErrorReason::TooLong,
            maximum_bytes: MAX_SAFE_STATIC_TEXT_BYTES,
            invalid_index: None,
        });
    }
    if let Some((index, _)) = value
        .char_indices()
        .find(|(_, character)| character.is_control())
    {
        return Err(SafeTextError {
            reason: SafeTextErrorReason::ControlCharacter,
            maximum_bytes: MAX_SAFE_STATIC_TEXT_BYTES,
            invalid_index: Some(index),
        });
    }
    if let Some((index, _)) = value
        .char_indices()
        .find(|(_, character)| is_unsafe_format_character(*character))
    {
        return Err(SafeTextError {
            reason: SafeTextErrorReason::UnsafeFormatCharacter,
            maximum_bytes: MAX_SAFE_STATIC_TEXT_BYTES,
            invalid_index: Some(index),
        });
    }
    if contains_sensitive_shape(value) || resembles_absolute_path(value) {
        return Err(SafeTextError {
            reason: SafeTextErrorReason::SensitiveShape,
            maximum_bytes: MAX_SAFE_STATIC_TEXT_BYTES,
            invalid_index: None,
        });
    }
    Ok(())
}

pub(crate) fn contains_sensitive_shape(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    lowercase.contains("bearer")
        || lowercase.contains("token")
        || lowercase.contains("password")
        || lowercase.contains("credential")
        || lowercase.contains("secret")
        || lowercase.contains("api_key")
        || lowercase.contains("api-key")
        || lowercase.contains("://")
}

pub(crate) fn is_unsafe_format_character(character: char) -> bool {
    matches!(
        character,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'
            | '\u{1160}'
            | '\u{17b4}'
            | '\u{17b5}'
            | '\u{180b}'..='\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e007f}'
    )
}

fn resembles_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.starts_with('/')
        || value.starts_with("\\\\")
        || value.starts_with("//")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

fn utf8_prefix(value: &'static str, maximum_bytes: usize) -> (&'static str, usize) {
    if value.len() <= maximum_bytes {
        return (value, 0);
    }
    let mut boundary = maximum_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    if boundary == 0 {
        return ("?", value.len());
    }
    (&value[..boundary], value.len() - boundary)
}

macro_rules! safe_static_text {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        pub struct $name(&'static str);

        impl $name {
            pub fn new(value: &'static str) -> Result<Self, SafeTextError> {
                validate_safe_static_text(value)?;
                Ok(Self(value))
            }

            #[must_use]
            pub const fn as_str(self) -> &'static str {
                self.0
            }

            pub(crate) fn truncate(self, maximum_bytes: usize) -> (Self, usize) {
                let (value, truncated_bytes) = utf8_prefix(self.0, maximum_bytes);
                (Self(value), truncated_bytes)
            }
        }
    };
}

safe_static_text!(SafeSummary);
safe_static_text!(SafeDisplayText);
