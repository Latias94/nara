use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

use serde_json::Value;
use thiserror::Error;

use super::{ProjectToolFailure, ProjectToolRequest, ProjectToolSummary, protocol};
use crate::project_workflow::__private::{CargoHeadlessMetadataFacts, CargoJsonMessage};

#[derive(Debug, Error)]
pub enum ProjectToolInvocationError {
    #[error("failed to resolve the explicit project or export directory")]
    Resolve(#[source] io::Error),
    #[error("failed to execute the project tool or Cargo bootstrap")]
    Process(#[source] io::Error),
    #[error("{0}")]
    Bootstrap(String),
    #[error("{0}")]
    Protocol(&'static str),
    #[error("{0}")]
    Project(ProjectToolFailure),
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
        ProjectToolRequest::Export {
            project, parent, ..
        } => {
            *project = fs::canonicalize(&project).map_err(ProjectToolInvocationError::Resolve)?;
            *parent = fs::canonicalize(&parent).map_err(ProjectToolInvocationError::Resolve)?;
        }
    }
    let executable = build_headless(request.project(), request.profile() == Some("desktop"))?;
    let mut command = Command::new(executable);
    command
        .current_dir(request.project())
        .args([
            protocol::TOOL_ARGUMENT,
            protocol::PROTOCOL_VERSION,
            request.operation(),
        ])
        .arg("--project-root")
        .arg(request.project());
    match &request {
        ProjectToolRequest::Check { profile, .. } | ProjectToolRequest::Schema { profile, .. } => {
            if let Some(profile) = profile {
                command.arg("--profile").arg(profile);
            }
        }
        ProjectToolRequest::Export {
            parent,
            destination,
            ..
        } => {
            command
                .arg("--parent")
                .arg(parent)
                .arg("--destination")
                .arg(destination);
        }
        ProjectToolRequest::RepairStartupScene {
            profile,
            replacement,
            ..
        } => {
            if let Some(profile) = profile {
                command.arg("--profile").arg(profile);
            }
            command.arg("--replacement").arg(replacement);
        }
    }
    let (status, bytes) = bounded_output(&mut command, protocol::MAX_RESPONSE_BYTES)?;
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
            protocol::encode_response("check", Err(super::super::ToolError::Recipe)).unwrap();
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
}

fn build_headless(project: &Path, desktop: bool) -> Result<PathBuf, ProjectToolInvocationError> {
    let manifest = project.join("Cargo.toml");
    let locked = project.join("Cargo.lock").is_file();
    let mut metadata = Command::new("cargo");
    metadata
        .current_dir(project)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest);
    if locked {
        metadata.arg("--locked");
    }
    let (status, bytes) = bounded_output(&mut metadata, 16 * 1024 * 1024)?;
    if !status.success() {
        return Err(ProjectToolInvocationError::Bootstrap(
            "Cargo could not discover the project's headless tool target".to_owned(),
        ));
    }
    let facts = CargoHeadlessMetadataFacts::parse(&bytes, &manifest)
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    if !paths_match(facts.source_path(), &project.join("src/bin/headless.rs")) {
        return Err(ProjectToolInvocationError::Bootstrap(
            "the headless target must use src/bin/headless.rs".to_owned(),
        ));
    }
    let mut build = Command::new("cargo");
    build
        .current_dir(project)
        .args([
            "build",
            "--no-default-features",
            "--bin",
            "headless",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(&manifest);
    if locked {
        build.arg("--locked");
    }
    if desktop {
        build.args(["--features", "desktop"]);
    }
    let (status, bytes) = bounded_output(&mut build, 64 * 1024 * 1024)?;
    let selected = select_headless(
        &bytes,
        facts.package_id(),
        &manifest,
        facts.source_path(),
        desktop,
    );
    if !status.success() {
        return Err(ProjectToolInvocationError::Bootstrap("the project headless bootstrap build failed; fix its Cargo diagnostics before running the project tool".to_owned()));
    }
    selected
}

fn select_headless(
    bytes: &[u8],
    package: &str,
    manifest: &Path,
    source: &Path,
    desktop: bool,
) -> Result<PathBuf, ProjectToolInvocationError> {
    let mut executable = None;
    let mut finished = None;
    for value in serde_json::Deserializer::from_slice(bytes).into_iter::<Value>() {
        let message =
            value.map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
        match CargoJsonMessage::parse(message)
            .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?
        {
            CargoJsonMessage::CompilerArtifact(artifact)
                if artifact.package_id() == package
                    && paths_match(artifact.manifest_path(), manifest)
                    && artifact.target_name() == "headless" =>
            {
                let expected_features: &[&str] = if desktop { &["desktop"] } else { &[] };
                if artifact.target_kind() != ["bin"]
                    || artifact.target_crate_types() != ["bin"]
                    || !paths_match(artifact.target_source_path(), source)
                    || artifact
                        .features()
                        .iter()
                        .map(String::as_str)
                        .ne(expected_features.iter().copied())
                    || artifact.profile_test()
                {
                    return Err(ProjectToolInvocationError::Bootstrap(
                        "Cargo headless artifact does not match the selected bootstrap invocation"
                            .to_owned(),
                    ));
                }
                let selected = artifact
                    .executable()
                    .filter(|path| path.is_absolute())
                    .ok_or_else(|| {
                        ProjectToolInvocationError::Bootstrap(
                            "Cargo headless artifact has no absolute executable path".to_owned(),
                        )
                    })?;
                if executable.replace(selected.to_owned()).is_some() {
                    return Err(ProjectToolInvocationError::Bootstrap(
                        "Cargo repeated the headless artifact".to_owned(),
                    ));
                }
            }
            CargoJsonMessage::BuildFinished { success } => {
                if finished.replace(success).is_some() {
                    return Err(ProjectToolInvocationError::Bootstrap(
                        "Cargo repeated its build completion".to_owned(),
                    ));
                }
            }
            CargoJsonMessage::CompilerDiagnostic(diagnostic) => {
                eprintln!("{}", diagnostic.message());
            }
            _ => {}
        }
    }
    if finished != Some(true) {
        return Err(ProjectToolInvocationError::Bootstrap(
            "Cargo did not complete the headless bootstrap".to_owned(),
        ));
    }
    executable.ok_or_else(|| {
        ProjectToolInvocationError::Bootstrap(
            "Cargo did not report the selected headless executable".to_owned(),
        )
    })
}

fn bounded_output(
    command: &mut Command,
    limit: u64,
) -> Result<(ExitStatus, Vec<u8>), ProjectToolInvocationError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(ProjectToolInvocationError::Process)?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped child stdout is available")
        .take(limit + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > limit {
        let _ = child.kill();
    }
    let status = child.wait().map_err(ProjectToolInvocationError::Process)?;
    read.map_err(ProjectToolInvocationError::Process)?;
    if bytes.len() as u64 > limit {
        return Err(ProjectToolInvocationError::Protocol(
            "project tool or Cargo output exceeded its bounded size",
        ));
    }
    Ok((status, bytes))
}

fn paths_match(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        let normalized = |path: &Path| {
            path.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('/', "\\")
                .to_ascii_lowercase()
        };
        normalized(left) == normalized(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
