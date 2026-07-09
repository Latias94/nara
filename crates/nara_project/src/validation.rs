use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_tasks::MAX_TASK_POOL_THREADS_PER_KIND;

use crate::path::ProjectPath;

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

pub(crate) fn validate_positive_seconds(
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
    }
}

pub(crate) fn validate_thread_count(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    value: usize,
) {
    if value == 0 {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.invalid-thread-count",
                format!("{field_path} must be greater than zero in threaded mode"),
            )
            .with_field_path(field_path),
        );
    }
    validate_max_thread_count(diagnostics, field_path, value);
}

pub(crate) fn validate_max_thread_count(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    value: usize,
) {
    if value > MAX_TASK_POOL_THREADS_PER_KIND {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.thread-count-too-large",
                format!("{field_path} must be <= {MAX_TASK_POOL_THREADS_PER_KIND}"),
            )
            .with_field_path(field_path),
        );
    }
}

pub(crate) fn validate_optional_max_thread_count(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    value: Option<usize>,
) {
    if let Some(value) = value {
        validate_max_thread_count(diagnostics, field_path, value);
    }
}
