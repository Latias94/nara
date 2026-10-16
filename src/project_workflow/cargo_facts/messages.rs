use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use thiserror::Error;

use super::{
    MAX_DIAGNOSTIC_BYTES, MAX_DIAGNOSTIC_CODE_BYTES, MAX_DIAGNOSTIC_COUNT,
    MAX_DIAGNOSTIC_MESSAGE_BYTES, MAX_FEATURE_BYTES, MAX_FEATURE_COUNT, MAX_PACKAGE_ID_BYTES,
    MAX_PACKAGE_NAME_BYTES, MAX_PATH_BYTES,
};

/// One normalized compiler-artifact message with no process or filesystem capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoCompilerArtifactFacts {
    pub(super) package_id: String,
    pub(super) manifest_path: PathBuf,
    pub(super) target: CargoTargetFacts,
    pub(super) features: Box<[String]>,
    pub(super) profile_test: bool,
    pub(super) executable: Option<PathBuf>,
    pub(super) fresh: bool,
}

impl CargoCompilerArtifactFacts {
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn target_name(&self) -> &str {
        &self.target.name
    }

    pub fn target_kind(&self) -> &[String] {
        &self.target.kind
    }

    pub fn target_crate_types(&self) -> &[String] {
        &self.target.crate_types
    }

    pub fn target_source_path(&self) -> &Path {
        &self.target.source_path
    }

    pub fn target_required_features(&self) -> Option<&[String]> {
        self.target.required_features.as_deref()
    }

    pub fn features(&self) -> &[String] {
        &self.features
    }

    pub const fn profile_test(&self) -> bool {
        self.profile_test
    }

    pub fn executable(&self) -> Option<&Path> {
        self.executable.as_deref()
    }

    pub const fn fresh(&self) -> bool {
        self.fresh
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CargoTargetFacts {
    pub(super) name: String,
    pub(super) kind: Box<[String]>,
    pub(super) crate_types: Box<[String]>,
    pub(super) source_path: PathBuf,
    pub(super) required_features: Option<Box<[String]>>,
}

/// Bounded compiler diagnostic retained separately from raw Cargo output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoCompilerDiagnostic {
    pub(super) level: CargoDiagnosticLevel,
    pub(super) code: Option<String>,
    pub(super) message: String,
    pub(super) truncated: bool,
}

impl CargoCompilerDiagnostic {
    pub const fn level(&self) -> CargoDiagnosticLevel {
        self.level
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    fn retained_bytes(&self) -> usize {
        self.message
            .len()
            .saturating_add(self.code.as_ref().map_or(0, String::len))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoDiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
    FailureNote,
    Other,
}

/// One normalized Cargo JSON record used by owner-specific selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoJsonMessage {
    CompilerArtifact(CargoCompilerArtifactFacts),
    CompilerDiagnostic(CargoCompilerDiagnostic),
    BuildScriptExecuted,
    BuildFinished { success: bool },
}

impl CargoJsonMessage {
    pub fn parse(value: Value) -> Result<Self, CargoJsonMessageError> {
        let object = value
            .as_object()
            .ok_or(CargoJsonMessageError::InvalidField { field: "message" })?;
        match message_string_field(object.get("reason"), "reason", 64)? {
            "compiler-artifact" => parse_compiler_artifact(object).map(Self::CompilerArtifact),
            "compiler-message" => parse_compiler_diagnostic(object).map(Self::CompilerDiagnostic),
            "build-script-executed" => Ok(Self::BuildScriptExecuted),
            "build-finished" => Ok(Self::BuildFinished {
                success: message_bool_field(object.get("success"), "success")?,
            }),
            _ => Err(CargoJsonMessageError::UnknownReason),
        }
    }

    pub(super) fn parse_slice(record: &[u8]) -> Result<Self, CargoJsonMessageError> {
        let value = serde_json::from_slice(record).map_err(CargoJsonMessageError::MalformedJson)?;
        Self::parse(value)
    }
}

#[derive(Debug, Error)]
pub enum CargoJsonMessageError {
    #[error("Cargo emitted malformed JSON")]
    MalformedJson(#[source] serde_json::Error),
    #[error("Cargo JSON field `{field}` was missing or invalid")]
    InvalidField { field: &'static str },
    #[error("Cargo JSON field `{field}` exceeded its bounded size")]
    FieldLimitExceeded { field: &'static str },
    #[error("Cargo emitted an unsupported JSON message reason")]
    UnknownReason,
}

/// Bounded structured diagnostics observed while decoding Cargo JSON messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoDiagnostics {
    pub(super) entries: Box<[CargoCompilerDiagnostic]>,
    pub(super) dropped_count: u64,
    pub(super) truncated_count: u64,
}

impl CargoDiagnostics {
    pub fn entries(&self) -> &[CargoCompilerDiagnostic] {
        &self.entries
    }

    pub const fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    pub const fn truncated_count(&self) -> u64 {
        self.truncated_count
    }
}

#[derive(Debug, Default)]
pub(super) struct CargoDiagnosticsBuilder {
    pub(super) entries: Vec<CargoCompilerDiagnostic>,
    pub(super) retained_bytes: usize,
    pub(super) dropped_count: u64,
    pub(super) truncated_count: u64,
}

impl CargoDiagnosticsBuilder {
    pub(super) fn push(&mut self, diagnostic: CargoCompilerDiagnostic) {
        if diagnostic.truncated {
            self.truncated_count = self.truncated_count.saturating_add(1);
        }
        let retained_bytes = diagnostic.retained_bytes();
        let Some(next_bytes) = self.retained_bytes.checked_add(retained_bytes) else {
            self.dropped_count = self.dropped_count.saturating_add(1);
            return;
        };
        if self.entries.len() >= MAX_DIAGNOSTIC_COUNT || next_bytes > MAX_DIAGNOSTIC_BYTES {
            self.dropped_count = self.dropped_count.saturating_add(1);
            return;
        }
        self.retained_bytes = next_bytes;
        self.entries.push(diagnostic);
    }

    pub(super) fn snapshot(&self) -> CargoDiagnostics {
        CargoDiagnostics {
            entries: self.entries.clone().into_boxed_slice(),
            dropped_count: self.dropped_count,
            truncated_count: self.truncated_count,
        }
    }
}

fn parse_compiler_artifact(
    object: &Map<String, Value>,
) -> Result<CargoCompilerArtifactFacts, CargoJsonMessageError> {
    let target = object
        .get("target")
        .and_then(Value::as_object)
        .ok_or(CargoJsonMessageError::InvalidField { field: "target" })?;
    let executable = match object.get("executable") {
        Some(Value::Null) => None,
        Some(Value::String(path)) => {
            validate_message_text(path, "executable", MAX_PATH_BYTES)?;
            Some(PathBuf::from(path))
        }
        _ => {
            return Err(CargoJsonMessageError::InvalidField {
                field: "executable",
            });
        }
    };
    Ok(CargoCompilerArtifactFacts {
        package_id: message_string_field(
            object.get("package_id"),
            "package_id",
            MAX_PACKAGE_ID_BYTES,
        )?
        .to_owned(),
        manifest_path: message_path_field(object.get("manifest_path"), "manifest_path")?,
        target: CargoTargetFacts {
            name: message_string_field(target.get("name"), "target.name", MAX_PACKAGE_NAME_BYTES)?
                .to_owned(),
            kind: normalized_message_array(target.get("kind"), "target.kind")?,
            crate_types: normalized_message_array(target.get("crate_types"), "target.crate_types")?,
            source_path: message_path_field(target.get("src_path"), "target.src_path")?,
            required_features: target
                .get("required-features")
                .map(|value| normalized_message_array(Some(value), "target.required-features"))
                .transpose()?,
        },
        features: normalized_message_array(object.get("features"), "features")?,
        profile_test: object
            .get("profile")
            .and_then(Value::as_object)
            .ok_or(CargoJsonMessageError::InvalidField { field: "profile" })
            .and_then(|profile| message_bool_field(profile.get("test"), "profile.test"))?,
        executable,
        fresh: message_bool_field(object.get("fresh"), "fresh")?,
    })
}

fn parse_compiler_diagnostic(
    object: &Map<String, Value>,
) -> Result<CargoCompilerDiagnostic, CargoJsonMessageError> {
    let message = object
        .get("message")
        .and_then(Value::as_object)
        .ok_or(CargoJsonMessageError::InvalidField { field: "message" })?;
    let level = match message_string_field(message.get("level"), "message.level", 64)? {
        "error" => CargoDiagnosticLevel::Error,
        "warning" => CargoDiagnosticLevel::Warning,
        "note" => CargoDiagnosticLevel::Note,
        "help" => CargoDiagnosticLevel::Help,
        "failure-note" => CargoDiagnosticLevel::FailureNote,
        _ => CargoDiagnosticLevel::Other,
    };
    let (message_text, message_truncated) = truncate_utf8(
        message_string_field_unbounded(message.get("message"), "message.message")?,
        MAX_DIAGNOSTIC_MESSAGE_BYTES,
    );
    let (code, code_truncated) = match message.get("code") {
        Some(Value::Null) | None => (None, false),
        Some(Value::Object(code)) => {
            let (code, truncated) = truncate_utf8(
                message_string_field_unbounded(code.get("code"), "message.code.code")?,
                MAX_DIAGNOSTIC_CODE_BYTES,
            );
            (Some(code), truncated)
        }
        Some(_) => {
            return Err(CargoJsonMessageError::InvalidField {
                field: "message.code",
            });
        }
    };
    Ok(CargoCompilerDiagnostic {
        level,
        code,
        message: message_text,
        truncated: message_truncated || code_truncated,
    })
}

fn message_path_field(
    value: Option<&Value>,
    field: &'static str,
) -> Result<PathBuf, CargoJsonMessageError> {
    Ok(PathBuf::from(message_string_field(
        value,
        field,
        MAX_PATH_BYTES,
    )?))
}

fn message_string_field<'a>(
    value: Option<&'a Value>,
    field: &'static str,
    max_bytes: usize,
) -> Result<&'a str, CargoJsonMessageError> {
    let value = message_string_field_unbounded(value, field)?;
    validate_message_text(value, field, max_bytes)?;
    Ok(value)
}

fn message_string_field_unbounded<'a>(
    value: Option<&'a Value>,
    field: &'static str,
) -> Result<&'a str, CargoJsonMessageError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(CargoJsonMessageError::InvalidField { field })
}

fn validate_message_text(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), CargoJsonMessageError> {
    if value.len() > max_bytes {
        Err(CargoJsonMessageError::FieldLimitExceeded { field })
    } else {
        Ok(())
    }
}

fn message_bool_field(
    value: Option<&Value>,
    field: &'static str,
) -> Result<bool, CargoJsonMessageError> {
    value
        .and_then(Value::as_bool)
        .ok_or(CargoJsonMessageError::InvalidField { field })
}

fn normalized_message_array(
    value: Option<&Value>,
    field: &'static str,
) -> Result<Box<[String]>, CargoJsonMessageError> {
    let values = value
        .and_then(Value::as_array)
        .ok_or(CargoJsonMessageError::InvalidField { field })?;
    if values.len() > MAX_FEATURE_COUNT {
        return Err(CargoJsonMessageError::FieldLimitExceeded { field });
    }
    let mut values = values
        .iter()
        .map(|value| message_string_field(Some(value), field, MAX_FEATURE_BYTES).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort_unstable();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CargoJsonMessageError::InvalidField { field });
    }
    Ok(values.into_boxed_slice())
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    (value[..end].to_owned(), true)
}
