use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use thiserror::Error;

use super::cargo_build::{
    CargoBuildFailure, CargoBuildTools, bounded_status_output as bounded_output, parse_host_target,
};
use super::{ProjectToolFailure, ProjectToolRequest, ProjectToolSummary, protocol};
use crate::project_workflow::cargo::{CargoBuildDecoder, CargoBuildSelection, CargoMetadataFacts};

#[derive(Debug, Error)]
pub enum ProjectToolInvocationError {
    #[error("failed to resolve the explicit project or export directory")]
    Resolve(#[source] io::Error),
    #[error(transparent)]
    ProcessOwner(#[from] CargoBuildFailure),
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
    let (status, bytes) = bounded_output(&mut command, protocol::MAX_RESPONSE_BYTES as usize)?;
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
    let tools = CargoBuildTools::discover_in(project)?;
    if !project.join("Cargo.lock").is_file() {
        // Starter initialization is an explicit authoring operation, separate from every locked
        // build below. Cargo remains the sole dependency/lock authority.
        let mut initialize = Command::new(tools.cargo_path());
        initialize
            .current_dir(project)
            .args(["generate-lockfile", "--manifest-path"])
            .arg(&manifest)
            .env("RUSTC", tools.rustc_path());
        let (status, _) = bounded_output(&mut initialize, 16 * 1024 * 1024)?;
        if !status.success() {
            return Err(ProjectToolInvocationError::Bootstrap(
                "Cargo could not initialize the project lockfile".to_owned(),
            ));
        }
    }
    let mut identity = Command::new(tools.rustc_path());
    identity.arg("-vV");
    let (status, bytes) = bounded_output(&mut identity, 16 * 1024)?;
    if !status.success() {
        return Err(ProjectToolInvocationError::Bootstrap(
            "Rust compiler identity was unavailable".to_owned(),
        ));
    }
    let host = parse_host_target(&bytes)?;
    let mut metadata = Command::new(tools.cargo_path());
    metadata
        .current_dir(project)
        .args([
            "metadata",
            "--locked",
            "--no-default-features",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--filter-platform")
        .arg(&host)
        .env("RUSTC", tools.rustc_path());
    if desktop {
        metadata.args(["--features", "desktop"]);
    }
    let (status, bytes) = bounded_output(&mut metadata, 16 * 1024 * 1024)?;
    if !status.success() {
        return Err(ProjectToolInvocationError::Bootstrap(
            "Cargo could not discover the project's headless tool target".to_owned(),
        ));
    }
    let package = CargoMetadataFacts::package_name_for_manifest(&bytes, &manifest)
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    // This is the desktop-2D starter's tool policy. The generic Cargo contract neither names
    // these binaries/features nor derives them from a product profile label.
    let features: Vec<&str> = if desktop { vec!["desktop"] } else { Vec::new() };
    let selection = CargoBuildSelection::new(package, "headless", "dev", &host, features, false)
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    let facts = CargoMetadataFacts::parse(&bytes, &manifest, &selection)
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    if facts.project_tool_protocol() != Some(1) {
        return Err(ProjectToolInvocationError::Bootstrap(
            "the project does not declare supported package.metadata.nara.project-tool-protocol = 1".to_owned(),
        ));
    }
    let invocation = facts
        .build_invocation(selection)
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    if !paths_match(
        invocation.source_path(),
        &project.join("src/bin/headless.rs"),
    ) {
        return Err(ProjectToolInvocationError::Bootstrap(
            "the headless target must use src/bin/headless.rs".to_owned(),
        ));
    }
    let mut build = Command::new(tools.cargo_path());
    build
        .current_dir(project)
        .args([
            "build",
            "--locked",
            "--no-default-features",
            "--bin",
            "headless",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--package")
        .arg(invocation.package_name())
        .arg("--profile")
        .arg(invocation.profile())
        .arg("--target")
        .arg(invocation.target_triple())
        .env("RUSTC", tools.rustc_path())
        .env("CARGO_TARGET_DIR", invocation.target_directory());
    if desktop {
        build.args(["--features", "desktop"]);
    }
    let (status, bytes) = bounded_output(&mut build, 64 * 1024 * 1024)?;
    if !status.success() {
        return Err(ProjectToolInvocationError::Bootstrap("the project headless bootstrap build failed; fix its Cargo diagnostics before running the project tool".to_owned()));
    }
    let mut decoder = CargoBuildDecoder::new(invocation);
    decoder
        .push(&bytes)
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    let selected = decoder
        .finish()
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    for diagnostic in selected.diagnostics().entries() {
        eprintln!("{}", diagnostic.message());
    }
    Ok(selected.artifact().executable().to_owned())
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
