use std::io::Write;

use nara::project_tooling::cargo_build::CargoBuildFailure;
use serde_json::{Value, json};
use two_level_game::{
    development::{
        ActiveGeneration, DevelopmentError, RetirementReport, RetirementRole, RetirementStatus,
        protocol::Observation,
    },
    resume::{MAX_RESUME_BYTES, ResumeState, encode_resume},
};

pub(super) const MAX_STATE_BYTES: usize = MAX_RESUME_BYTES;
pub(super) const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_ERROR_BYTES: usize = 2048;
const MAX_DIAGNOSTICS: usize = 12;
const MAX_DIAGNOSTIC_BYTES: usize = 1024;

pub(super) fn emit(stdout: &mut impl Write, value: &Value) -> Result<(), String> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|error| format!("encode response: {error}"))?;
    if bytes.len() >= MAX_RESPONSE_BYTES {
        return Err("encoded response exceeds its 512 KiB byte limit".into());
    }
    bytes.push(b'\n');
    stdout
        .write_all(&bytes)
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("write response: {error}"))
}

pub(super) fn state(state: &ResumeState) -> Result<Value, String> {
    encode_resume(state)
        .map_err(|error| format!("capture exceeded its game state contract: {error}"))?;
    serde_json::to_value(state).map_err(|error| format!("encode complete state: {error}"))
}

pub(super) fn observation(observation: &Observation) -> Result<Value, String> {
    let bytes =
        serde_json::to_vec(observation).map_err(|error| format!("encode observation: {error}"))?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err("observation exceeds its 128 KiB state limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("encode observation object: {error}"))
}

pub(super) fn generation(generation: Option<ActiveGeneration>) -> Value {
    generation.map_or(Value::Null, |generation| {
        json!({
            "sequence": generation.sequence,
            "pid": generation.pid,
            "executable": format!("{:?}", generation.executable),
            "provenance": format!("{:?}", generation.provenance),
        })
    })
}

pub(super) fn retirement(status: RetirementStatus) -> &'static str {
    match status {
        RetirementStatus::Complete => "complete",
        RetirementStatus::Pending => "pending",
    }
}

pub(super) fn retirements(reports: &[RetirementReport]) -> Value {
    Value::Array(
        reports
            .iter()
            .map(|report| {
                let role = match report.role {
                    RetirementRole::Probe => "probe",
                    RetirementRole::Candidate => "candidate",
                    RetirementRole::Predecessor => "predecessor",
                    RetirementRole::Active => "active",
                };
                json!({
                    "generation": generation(Some(report.generation)),
                    "role": role,
                    "game_closed": report.game_closed,
                    "terminated": report.terminated,
                    "native": {
                        "exit_code": report.native.exit().code(),
                        "exit_success": report.native.exit().success(),
                        "containment_empty": report.native.containment_empty(),
                        "readers_stopped": report.native.readers_stopped(),
                        "output_drained": report.native.output_drained(),
                        "force_killed": report.native.force_killed(),
                    },
                })
            })
            .collect(),
    )
}

pub(super) fn text(message: &str) -> Value {
    Value::String(prefix(message, MAX_ERROR_BYTES).into())
}

pub(super) fn error(stage: &str, message: &str) -> Value {
    json!({
        "stage": stage,
        "message": text(message),
        "truncated": message.len() > MAX_ERROR_BYTES,
    })
}

pub(super) fn development_error(
    error: &DevelopmentError,
    failure: Option<&CargoBuildFailure>,
) -> Value {
    let mut result = self::error(
        &format!("{:?}", error.stage).to_ascii_lowercase(),
        &error.message,
    );
    if let Some(failure) = failure {
        result["build_failure"] = build_failure(failure);
    }
    result
}

pub(super) fn build_failure(failure: &CargoBuildFailure) -> Value {
    let mut result = error("build", &failure.to_string());
    match failure {
        CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol,
        } => {
            let entries = diagnostics
                .entries()
                .iter()
                .take(MAX_DIAGNOSTICS)
                .map(|diagnostic| {
                    json!({
                        "level": format!("{:?}", diagnostic.level()).to_ascii_lowercase(),
                        "code": diagnostic.code().map(|code| prefix(code, 128)),
                        "message": prefix(diagnostic.message(), MAX_DIAGNOSTIC_BYTES),
                        "truncated": diagnostic.truncated()
                            || diagnostic.message().len() > MAX_DIAGNOSTIC_BYTES
                            || diagnostic.code().is_some_and(|code| code.len() > 128),
                    })
                })
                .collect::<Vec<_>>();
            result["kind"] = json!("compilation");
            result["exit_code"] = json!(status.code());
            result["exit_status"] = json!(status.to_string());
            result["diagnostics"] = json!({
                "entries": entries,
                "retained_count": diagnostics.entries().len(),
                "omitted_count": diagnostics.entries().len().saturating_sub(MAX_DIAGNOSTICS),
                "cargo_dropped_count": diagnostics.dropped_count(),
                "cargo_truncated_count": diagnostics.truncated_count(),
            });
            result["protocol_error"] = protocol
                .as_ref()
                .map_or(Value::Null, |error| text(&error.to_string()));
        }
        CargoBuildFailure::CleanupIncomplete(custody) => {
            result["kind"] = json!("cleanup_incomplete");
            result["process_id"] = json!(custody.process_id());
            result["retirement"] = json!(if custody.process_id().is_some() {
                "pending"
            } else {
                "complete"
            });
        }
        _ => result["kind"] = json!("cargo_build_failure"),
    }
    result
}

fn prefix(message: &str, maximum: usize) -> &str {
    let mut end = message.len().min(maximum);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    &message[..end]
}
