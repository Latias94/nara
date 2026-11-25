//! Shared validation of values that may be supplied individually by a source profile.

use std::time::Duration;

use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, ReportDiagnostic as Diagnostic, SafeSummary,
};

use super::{MAX_PROJECT_FIXED_DEBT_STEPS, MAX_PROJECT_FIXED_STEPS_PER_FRAME};

pub(super) fn error(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("project diagnostic codes are engine-owned literals"),
        SafeSummary::new(summary).expect("project diagnostic summaries are engine-owned literals"),
    )
}

pub(super) fn with_field_path(diagnostic: Diagnostic, value: &str) -> Diagnostic {
    with_public_identifier(diagnostic, "field", value)
}

pub(super) fn with_public_identifier(
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

pub(super) fn with_public_u64(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_u64(field_key(key), value),
    )
}

pub(super) fn with_public_bool(
    diagnostic: Diagnostic,
    key: &'static str,
    value: bool,
) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_bool(field_key(key), value),
    )
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("project diagnostic field keys are engine-owned literals")
}

fn attach_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("project diagnostics attach unique fields below the draft hard limit")
}

/// Appends a diagnostic if the time scale is negative or nonfinite.
pub fn validate_time_scale(diagnostics: &mut DiagnosticReport, field_path: &str, value: f32) {
    if !value.is_finite() || value < 0.0 {
        let diagnostic = error(
            "project.runtime.invalid-time-scale",
            "Runtime time scale must be finite and non-negative",
        );
        let diagnostic = with_field_path(diagnostic, field_path);
        let diagnostic = with_public_bool(diagnostic, "finite", value.is_finite());
        diagnostics.push(with_public_bool(diagnostic, "non_negative", value >= 0.0));
    }
}

pub(super) fn validate_nonzero_duration(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    value: Duration,
) {
    if value.is_zero() {
        diagnostics.push(with_field_path(
            error(
                "project.runtime.invalid-duration",
                "Runtime duration must be greater than zero",
            ),
            field_path,
        ));
    }
}

/// Appends diagnostics for supplied fixed-step limits outside the v0 range.
pub fn validate_fixed_step_limits(
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

/// Appends diagnostics for invalid window fields supplied by a record or override.
pub fn validate_window_fields(
    diagnostics: &mut DiagnosticReport,
    prefix: &str,
    title: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    scale_factor: Option<f64>,
) {
    if title.is_some_and(|title| title.trim().is_empty()) {
        diagnostics.push(with_field_path(
            error("project.window.empty-title", "Window title cannot be empty"),
            &format!("{prefix}.title"),
        ));
    }
    if matches!(width, Some(0)) {
        let diagnostic = error(
            "project.window.invalid-width",
            "Window width must be greater than zero",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.width"));
        diagnostics.push(with_public_u64(diagnostic, "actual", 0));
    }
    if matches!(height, Some(0)) {
        let diagnostic = error(
            "project.window.invalid-height",
            "Window height must be greater than zero",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.height"));
        diagnostics.push(with_public_u64(diagnostic, "actual", 0));
    }
    if let Some(scale_factor) = scale_factor
        && (!scale_factor.is_finite() || scale_factor <= 0.0)
    {
        let diagnostic = error(
            "project.window.invalid-scale-factor",
            "Window scale factor must be finite and greater than zero",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.scale_factor"));
        let diagnostic = with_public_bool(diagnostic, "finite", scale_factor.is_finite());
        diagnostics.push(with_public_bool(diagnostic, "positive", scale_factor > 0.0));
    }
}
