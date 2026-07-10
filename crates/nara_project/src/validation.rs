use std::time::Duration;

use nara_diagnostic::{Diagnostic, DiagnosticReport};

use crate::path::ProjectPath;
use crate::{MAX_PROJECT_FIXED_DEBT_STEPS, MAX_PROJECT_FIXED_STEPS_PER_FRAME};

pub(crate) fn validate_profile_name(diagnostics: &mut DiagnosticReport, name: &str) {
    if name.is_empty() {
        diagnostics.push(Diagnostic::error(
            "project.profile.empty-name",
            "profile names cannot be empty",
        ));
        return;
    }

    if name.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    }) {
        diagnostics.push(
            Diagnostic::error(
                "project.profile.invalid-name",
                format!(
                    "profile name '{name}' can only contain ASCII letters, numbers, '-', '_', or '.'"
                ),
            )
            .with_field_path(format!("profiles.{name}")),
        );
    }
}

pub(crate) fn validate_path_field(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    path: &str,
) {
    if let Err(error) = ProjectPath::new(path.to_owned()) {
        diagnostics.push(
            Diagnostic::error(
                "project.path.invalid",
                format!("invalid project path '{path}': {error}"),
            )
            .with_field_path(field_path),
        );
    }
}

pub(crate) fn validate_optional_path_field(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    path: Option<&String>,
) {
    if let Some(path) = path {
        validate_path_field(diagnostics, field_path, path);
    }
}

pub(crate) fn duration_from_positive_seconds(value: f64) -> Option<Duration> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Duration::try_from_secs_f64(value)
        .ok()
        .filter(|duration| !duration.is_zero())
}

pub(crate) fn validate_duration_seconds(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    value: f64,
) {
    if !value.is_finite() || value <= 0.0 {
        diagnostics.push(
            Diagnostic::error(
                "project.runtime.invalid-duration",
                format!("{field_path} must be finite and greater than zero"),
            )
            .with_field_path(field_path),
        );
    } else if duration_from_positive_seconds(value).is_none() {
        diagnostics.push(
            Diagnostic::error(
                "project.runtime.unrepresentable-duration",
                format!(
                    "{field_path} must fit in Duration and remain non-zero at nanosecond precision"
                ),
            )
            .with_field_path(field_path),
        );
    }
}

pub(crate) fn validate_fixed_step_limits(
    diagnostics: &mut DiagnosticReport,
    prefix: &str,
    max_steps_per_frame: Option<u32>,
    max_debt_steps: Option<u32>,
) {
    for (field, value, maximum, zero_code, oversized_code) in [
        (
            "max_fixed_steps_per_frame",
            max_steps_per_frame,
            MAX_PROJECT_FIXED_STEPS_PER_FRAME,
            "project.runtime.invalid-max-fixed-steps",
            "project.runtime.max-fixed-steps-too-large",
        ),
        (
            "max_fixed_debt_steps",
            max_debt_steps,
            MAX_PROJECT_FIXED_DEBT_STEPS,
            "project.runtime.invalid-max-fixed-debt",
            "project.runtime.max-fixed-debt-too-large",
        ),
    ] {
        let Some(value) = value else {
            continue;
        };
        let field_path = format!("{prefix}.{field}");
        if value == 0 {
            diagnostics.push(
                Diagnostic::error(zero_code, format!("{field_path} must be greater than zero"))
                    .with_field_path(field_path),
            );
        } else if value > maximum {
            diagnostics.push(
                Diagnostic::error(oversized_code, format!("{field_path} must be <= {maximum}"))
                    .with_field_path(field_path),
            );
        }
    }
}
