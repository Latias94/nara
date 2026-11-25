use std::{ffi::OsString, fs, io, path::Path};

use nara_scene::runtime_configuration::{ProductCapability, ProductCapabilitySet};
use thiserror::Error;

use super::cargo_build::{CargoBuildFailure, ProjectToolBootstrapError, run_project_tool};
use super::{ProjectToolFailure, ProjectToolRequest, ProjectToolSummary, host_fs, protocol};
use crate::project_host::read_manifest_profile;
use crate::project_workflow::cargo::CargoCompilerDiagnostic;

const DESKTOP_CAPABILITIES: ProductCapabilitySet = ProductCapabilitySet::EMPTY
    .with(ProductCapability::Presentation2d)
    .with(ProductCapability::DesktopWinit)
    .with(ProductCapability::RenderWgpu);

#[derive(Debug, Error)]
pub enum ProjectToolInvocationError {
    #[error("failed to resolve the explicit project directory")]
    Resolve(#[source] io::Error),
    #[error("{0}{details}", details = bootstrap_failure_details(.0))]
    ProcessOwner(#[from] CargoBuildFailure),
    #[error("{0}")]
    Bootstrap(String),
    #[error("{0}")]
    Protocol(&'static str),
    #[error("{0}")]
    Project(ProjectToolFailure),
}

impl From<ProjectToolBootstrapError> for ProjectToolInvocationError {
    fn from(error: ProjectToolBootstrapError) -> Self {
        match error {
            ProjectToolBootstrapError::Process(error) => Self::ProcessOwner(error),
            error => Self::Bootstrap(error.to_string()),
        }
    }
}

fn bootstrap_failure_details(failure: &CargoBuildFailure) -> String {
    let CargoBuildFailure::Compilation {
        status,
        diagnostics,
        protocol,
    } = failure
    else {
        return String::new();
    };
    let mut details = format!(" ({status})");
    if let Some(error) = protocol {
        details.push_str(&format!("\n{error}"));
    }
    for diagnostic in diagnostics.entries() {
        details.push('\n');
        append_compiler_diagnostic(&mut details, diagnostic);
    }
    if diagnostics.dropped_count() != 0 || diagnostics.truncated_count() != 0 {
        details.push_str(&format!(
            "\nCargo diagnostics: {} omitted, {} truncated",
            diagnostics.dropped_count(),
            diagnostics.truncated_count(),
        ));
    }
    details
}

fn append_compiler_diagnostic(output: &mut String, diagnostic: &CargoCompilerDiagnostic) {
    if let Some(location) = diagnostic.primary_location()
        && let Some(path) = location.path().to_str()
        && crate::fs::RelativePath::preflight(path).is_ok()
    {
        output.push_str(path);
        output.push_str(&format!(":{}:{}: ", location.line(), location.column()));
    }
    if let Some(code) = diagnostic.code() {
        output.push_str(code);
        output.push_str(": ");
    }
    output.push_str(diagnostic.message());
}

/// Builds the existing headless entry and delegates an operation without admitting project data.
pub fn invoke_project_tool(
    mut request: ProjectToolRequest,
) -> Result<ProjectToolSummary, ProjectToolInvocationError> {
    match &mut request {
        ProjectToolRequest::Check { project, .. }
        | ProjectToolRequest::Schema { project, .. }
        | ProjectToolRequest::RepairStartupScene { project, .. } => {
            *project = fs::canonicalize(&project).map_err(ProjectToolInvocationError::Resolve)?;
        }
    }
    let desktop_if_declared =
        profile_requests_desktop_tool(request.project(), request.profile()).unwrap_or(false);
    let mut arguments: Vec<OsString> = vec![
        protocol::TOOL_ARGUMENT.into(),
        protocol::PROTOCOL_VERSION.into(),
        request.operation().into(),
        "--project-root".into(),
        request.project().as_os_str().to_owned(),
    ];
    if let Some(profile) = request.profile() {
        arguments.extend(["--profile".into(), OsString::from(profile)]);
    }
    if let ProjectToolRequest::RepairStartupScene { replacement, .. } = &request {
        arguments.extend(["--replacement".into(), OsString::from(replacement)]);
    }
    let (status, bytes) = run_project_tool(
        request.project(),
        desktop_if_declared,
        arguments,
        protocol::MAX_RESPONSE_BYTES as usize,
    )?;
    finish_response(status.success(), &bytes, request.operation())
}

fn finish_response(
    success: bool,
    bytes: &[u8],
    operation: &str,
) -> Result<ProjectToolSummary, ProjectToolInvocationError> {
    match protocol::decode_response(bytes, operation)
        .map_err(ProjectToolInvocationError::Protocol)?
    {
        Ok(summary) if success => Ok(summary),
        Ok(_) => Err(ProjectToolInvocationError::Protocol(
            "project tool reported success but exited unsuccessfully",
        )),
        Err(_) if success => Err(ProjectToolInvocationError::Protocol(
            "project tool reported failure but exited successfully",
        )),
        Err(failure) => Err(ProjectToolInvocationError::Project(failure)),
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn exit_and_structured_outcome_must_agree_in_both_directions() {
        let success = protocol::encode_response(
            "check",
            Ok(ProjectToolSummary::Checked { content_members: 3 }),
        )
        .unwrap();
        let failure =
            protocol::encode_response("check", Err(super::super::ToolError::recipe(()))).unwrap();
        let success = serde_json::to_vec(&success).unwrap();
        let failure = serde_json::to_vec(&failure).unwrap();
        assert!(finish_response(true, &success, "check").is_ok());
        assert!(matches!(
            finish_response(false, &success, "check"),
            Err(ProjectToolInvocationError::Protocol(_))
        ));
        assert!(matches!(
            finish_response(true, &failure, "check"),
            Err(ProjectToolInvocationError::Protocol(_))
        ));
        let Err(ProjectToolInvocationError::Project(failure)) =
            finish_response(false, &failure, "check")
        else {
            panic!("ordinary project failure must stay structured")
        };
        assert_eq!(failure.operation(), "check");
        assert_eq!(
            failure.stage(),
            super::super::ProjectToolFailureStage::Recipe
        );
        assert_eq!(failure.code(), "project.tool.recipe-failed");
    }

    #[test]
    fn compiler_diagnostic_display_includes_only_admitted_locations() {
        use crate::project_workflow::cargo::CargoJsonMessage;

        for (path, expected) in [
            ("src/levels.rs", "src/levels.rs:7:11: E0308: invalid level"),
            ("../external.rs", "E0308: invalid level"),
        ] {
            let message = serde_json::json!({
                "reason": "compiler-message",
                "message": {
                    "level": "error", "code": {"code": "E0308"}, "message": "invalid level",
                    "rendered": "private-rendered-compiler-canary",
                    "spans": [{"is_primary": true, "file_name": path, "line_start": 7, "column_start": 11}]
                }
            });
            let CargoJsonMessage::CompilerDiagnostic(diagnostic) =
                CargoJsonMessage::parse(message).unwrap()
            else {
                panic!("expected a structured compiler diagnostic")
            };
            let mut output = String::new();
            append_compiler_diagnostic(&mut output, &diagnostic);
            assert_eq!(output, expected);
        }
    }
}

/// Observes a build hint. The compiled tool owns the operation and any project diagnostics.
fn profile_requests_desktop_tool(project: &Path, profile: Option<&str>) -> Option<bool> {
    let root = host_fs::readable_directory(project).ok()?;
    let path = crate::fs::RelativePath::new("nara.toml").expect("the manifest path is valid");
    let manifest = root.open_file(&path).ok()?;
    let requested = read_manifest_profile(&manifest, profile)
        .ok()?
        .requested_capabilities;
    Some(!requested.intersection(DESKTOP_CAPABILITIES).is_empty())
}
