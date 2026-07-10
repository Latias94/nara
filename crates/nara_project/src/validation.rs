use std::time::Duration;

use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};

use crate::path::{ProjectPath, ProjectPathError};
use crate::{MAX_PROJECT_FIXED_DEBT_STEPS, MAX_PROJECT_FIXED_STEPS_PER_FRAME};

pub(crate) fn error(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("project diagnostic codes are engine-owned literals"),
        SafeSummary::new(summary).expect("project diagnostic summaries are engine-owned literals"),
    )
}

pub(crate) fn with_field_path(diagnostic: Diagnostic, value: &str) -> Diagnostic {
    with_public_identifier(diagnostic, "field", value)
}

pub(crate) fn with_public_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &str,
) -> Diagnostic {
    let field_key = field_key(key);
    let field = PublicDiagnosticIdentifier::new(value).map_or_else(
        |_| DiagnosticField::sensitive(field_key),
        |value| DiagnosticField::public_identifier(field_key, value),
    );
    attach_field(diagnostic, field)
}

pub(crate) fn with_profile_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &str,
) -> Diagnostic {
    if is_valid_profile_name(value) {
        with_public_identifier(diagnostic, key, value)
    } else {
        with_sensitive(diagnostic, key)
    }
}

pub(crate) fn with_public_u64(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_u64(field_key(key), value),
    )
}

pub(crate) fn with_public_i64(diagnostic: Diagnostic, key: &'static str, value: i64) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_i64(field_key(key), value),
    )
}

pub(crate) fn with_public_bool(
    diagnostic: Diagnostic,
    key: &'static str,
    value: bool,
) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_bool(field_key(key), value),
    )
}

pub(crate) fn with_sensitive(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    attach_field(diagnostic, DiagnosticField::sensitive(field_key(key)))
}

pub(crate) fn with_secret(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    attach_field(diagnostic, DiagnosticField::secret(field_key(key)))
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("project diagnostic field keys are engine-owned literals")
}

fn attach_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("project diagnostics attach unique fields below the draft hard limit")
}

pub(crate) fn validate_profile_name(diagnostics: &mut DiagnosticReport, name: &str) -> bool {
    if name.is_empty() {
        diagnostics.push(error(
            "project.profile.empty-name",
            "Profile names cannot be empty",
        ));
        return false;
    }

    if !is_valid_profile_name(name) {
        let diagnostic = error(
            "project.profile.invalid-name",
            "A profile name contains unsupported characters",
        );
        diagnostics.push(with_sensitive(diagnostic, "profile"));
        return false;
    }

    true
}

fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

pub(crate) fn validate_path_field(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    path: &str,
) {
    if let Err(path_error) = ProjectPath::new(path.to_owned()) {
        let diagnostic = error("project.path.invalid", "A project-relative path is invalid");
        let diagnostic = with_field_path(diagnostic, field_path);
        let diagnostic = with_public_identifier(
            diagnostic,
            "reason",
            project_path_error_identifier(path_error),
        );
        diagnostics.push(with_sensitive(diagnostic, "path"));
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
        let diagnostic = error(
            "project.runtime.invalid-duration",
            "A runtime duration must be finite and greater than zero",
        );
        let diagnostic = with_field_path(diagnostic, field_path);
        let diagnostic = with_public_bool(diagnostic, "finite", value.is_finite());
        diagnostics.push(with_public_bool(diagnostic, "positive", value > 0.0));
    } else if duration_from_positive_seconds(value).is_none() {
        let diagnostic = error(
            "project.runtime.unrepresentable-duration",
            "A runtime duration cannot be represented at nanosecond precision",
        );
        diagnostics.push(with_field_path(diagnostic, field_path));
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
            let diagnostic = error(zero_code, "A fixed-step limit must be greater than zero");
            let diagnostic = with_field_path(diagnostic, &field_path);
            diagnostics.push(with_public_u64(diagnostic, "actual", u64::from(value)));
        } else if value > maximum {
            let diagnostic = error(oversized_code, "A fixed-step limit exceeds its hard limit");
            let diagnostic = with_field_path(diagnostic, &field_path);
            let diagnostic = with_public_u64(diagnostic, "actual", u64::from(value));
            diagnostics.push(with_public_u64(diagnostic, "limit", u64::from(maximum)));
        }
    }
}

fn project_path_error_identifier(error: ProjectPathError) -> &'static str {
    match error {
        ProjectPathError::Empty => "empty",
        ProjectPathError::Absolute => "absolute",
        ProjectPathError::ContainsBackslash => "backslash",
        ProjectPathError::ContainsDrivePrefix => "drive-prefix",
        ProjectPathError::ContainsNull => "null",
        ProjectPathError::ContainsEmptySegment => "empty-segment",
        ProjectPathError::ContainsCurrentDirectory => "current-directory",
        ProjectPathError::ContainsParentDirectory => "parent-directory",
    }
}
