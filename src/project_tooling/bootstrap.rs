use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use nara_project::{ProductCapability, ProductCapabilitySet};
use thiserror::Error;

use super::cargo_build::{
    CargoBuildFailure, CargoBuildTools, bounded_status_output as bounded_output, parse_host_target,
};
use super::{ProjectToolFailure, ProjectToolRequest, ProjectToolSummary, host_fs, protocol};
use crate::project_host::read_manifest_profile;
use crate::project_workflow::cargo::{
    CargoBuildDecoder, CargoBuildInvocation, CargoBuildSelection, CargoCompilerDiagnostic,
    CargoMetadataFacts,
};

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
    let executable = build_headless(request.project(), desktop_if_declared)?;
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

fn build_headless(
    project: &Path,
    desktop_if_declared: bool,
) -> Result<PathBuf, ProjectToolInvocationError> {
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
            "--no-deps",
            "--no-default-features",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest)
        .env("RUSTC", tools.rustc_path());
    let (status, declarations) = bounded_output(&mut metadata, 16 * 1024 * 1024)?;
    if !status.success() {
        return Err(ProjectToolInvocationError::Bootstrap(
            "Cargo could not discover the project's headless tool target".to_owned(),
        ));
    }
    let package = CargoMetadataFacts::project_tool_package(&declarations, &manifest, "desktop")
        .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    if package.protocol != Some(1) {
        return Err(ProjectToolInvocationError::Bootstrap(
            "the project does not declare supported package.metadata.nara.project-tool-protocol = 1".to_owned(),
        ));
    }
    let desktop = desktop_if_declared && package.declares_feature;
    // This is the desktop-2D starter's tool policy. The generic Cargo contract neither names
    // these binaries/features nor derives them from a product profile label.
    let features: Vec<&str> = if desktop { vec!["desktop"] } else { Vec::new() };
    let selection =
        CargoBuildSelection::new(package.name, "headless", "dev", &host, features, false)
            .map_err(|error| ProjectToolInvocationError::Bootstrap(error.to_string()))?;
    let invocation =
        CargoBuildInvocation::from_metadata_declarations(&declarations, &manifest, selection)
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
    finish_bootstrap_build(status, &bytes, invocation)
}

fn finish_bootstrap_build(
    status: ExitStatus,
    bytes: &[u8],
    invocation: CargoBuildInvocation,
) -> Result<PathBuf, ProjectToolInvocationError> {
    let project = invocation
        .manifest_path()
        .parent()
        .unwrap_or(invocation.workspace_root())
        .to_owned();
    let workspace = invocation.workspace_root().to_owned();
    let mut decoder = CargoBuildDecoder::new(invocation);
    let decoded = decoder.push(bytes).and_then(|()| decoder.finish());
    match decoded {
        Ok(selected) if status.success() => {
            for diagnostic in selected.diagnostics().entries() {
                eprintln!("{}", diagnostic.message());
            }
            Ok(selected.artifact().executable().to_owned())
        }
        decoded => Err(CargoBuildFailure::Compilation {
            status,
            // Both push and finish can reject after observing actionable compiler messages.
            diagnostics: decoder
                .diagnostics()
                .with_project_relative_locations(&project, &workspace),
            protocol: decoded.err(),
        }
        .into()),
    }
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

#[cfg(all(test, any(windows, target_os = "linux")))]
mod build_tests {
    #[cfg(target_os = "linux")]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    use serde_json::json;

    use super::*;
    use crate::project_workflow::cargo::CargoBuildError;

    const DIAGNOSTIC: &str = "bootstrap compiler diagnostic must survive failure";
    const PACKAGE_ID: &str = "bootstrap-test-package-id";

    #[test]
    fn failed_compilation_keeps_its_diagnostics_status_and_protocol_error() {
        let mut output = diagnostic_output("error", DIAGNOSTIC);
        output.extend_from_slice(b"{\"reason\":\"build-finished\",\"success\":false}\n");
        let error = finish_bootstrap_build(exit_status(false), &output, invocation()).unwrap_err();
        let displayed = error.to_string();
        assert!(displayed.contains(DIAGNOSTIC), "{displayed}");
        assert!(
            displayed.contains(&exit_status(false).to_string()),
            "{displayed}",
        );
        let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol,
        }) = error
        else {
            panic!("bootstrap must retain the structured compilation failure")
        };
        assert_eq!(status, exit_status(false));
        assert!(matches!(protocol, Some(CargoBuildError::BuildFailed)));
        assert_eq!(diagnostics.entries()[0].message(), DIAGNOSTIC);
    }

    #[test]
    fn decoder_push_failure_keeps_prior_diagnostics_and_the_original_protocol_error() {
        let mut output = diagnostic_output("error", DIAGNOSTIC);
        output.extend_from_slice(b"invalid Cargo JSON\n");
        let error = finish_bootstrap_build(exit_status(true), &output, invocation()).unwrap_err();
        let displayed = error.to_string();
        assert!(displayed.contains(DIAGNOSTIC), "{displayed}");
        let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol,
        }) = error
        else {
            panic!("bootstrap must retain a rejected stream's diagnostics and protocol error")
        };
        assert!(status.success());
        assert_eq!(diagnostics.entries()[0].message(), DIAGNOSTIC);
        assert!(matches!(protocol, Some(CargoBuildError::Message(_))));
    }

    #[test]
    fn decoder_finish_failure_keeps_diagnostics_even_after_a_successful_exit() {
        let mut output = diagnostic_output("error", DIAGNOSTIC);
        // finish must admit the final unterminated diagnostic before rejecting the missing result.
        assert_eq!(output.pop(), Some(b'\n'));
        let error = finish_bootstrap_build(exit_status(true), &output, invocation()).unwrap_err();
        let displayed = error.to_string();
        assert!(displayed.contains(DIAGNOSTIC), "{displayed}");
        assert!(
            displayed.contains("Cargo did not emit build-finished"),
            "{displayed}",
        );
        assert!(matches!(
            error,
            ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
                status,
                protocol: Some(CargoBuildError::MissingBuildFinished),
                ..
            }) if status.success()
        ));
    }

    #[test]
    fn complete_artifact_selection_still_requires_a_successful_process_exit() {
        let invocation = invocation();
        let executable = invocation
            .target_directory()
            .join(invocation.target_triple())
            .join("debug/exact-cargo-reported-executable");
        let artifact = json!({
            "reason": "compiler-artifact", "package_id": PACKAGE_ID,
            "manifest_path": invocation.manifest_path(),
            "target": {
                "kind": ["bin"], "crate_types": ["bin"], "name": "headless",
                "src_path": invocation.source_path(), "required-features": []
            },
            "profile": {"test": false}, "features": [],
            "executable": executable, "fresh": false
        });
        let mut output = diagnostic_output("warning", DIAGNOSTIC);
        serde_json::to_writer(&mut output, &artifact).unwrap();
        output.extend_from_slice(b"\n{\"reason\":\"build-finished\",\"success\":true}\n");
        let error =
            finish_bootstrap_build(exit_status(false), &output, invocation.clone()).unwrap_err();
        assert!(error.to_string().contains(DIAGNOSTIC));
        assert!(matches!(
            error,
            ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
                status,
                protocol: None,
                ..
            }) if !status.success()
        ));
        assert_eq!(
            finish_bootstrap_build(exit_status(true), &output, invocation).unwrap(),
            executable,
        );
    }

    #[test]
    fn bootstrap_failure_display_keeps_decoder_diagnostic_bounds_and_loss_facts() {
        let long_message = "⚠".repeat(6_000);
        let mut output = diagnostic_output("error", &long_message);
        for _ in 0..256 {
            output.extend_from_slice(&diagnostic_output("error", DIAGNOSTIC));
        }
        output.extend_from_slice(b"{\"reason\":\"build-finished\",\"success\":false}\n");
        let error = finish_bootstrap_build(exit_status(false), &output, invocation()).unwrap_err();
        let displayed = error.to_string();
        assert!(displayed.contains("1 omitted, 1 truncated"), "{displayed}");
        assert!(!displayed.contains(&long_message));
        let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
            diagnostics,
            ..
        }) = error
        else {
            panic!("bootstrap must keep the decoder's bounded diagnostic summary")
        };
        assert_eq!(diagnostics.entries().len(), 256);
        assert_eq!(diagnostics.dropped_count(), 1);
        assert_eq!(diagnostics.truncated_count(), 1);
    }

    #[test]
    fn primary_compiler_location_reaches_bootstrap_as_a_project_relative_observation() {
        let invocation = invocation();
        let project = invocation.manifest_path().parent().unwrap();
        let source = project.join("src/levels.rs");
        let mut output = diagnostic_output_at(&source.to_string_lossy());
        output.extend_from_slice(b"{\"reason\":\"build-finished\",\"success\":false}\n");
        let error = finish_bootstrap_build(exit_status(false), &output, invocation).unwrap_err();
        let displayed = error.to_string();
        assert!(
            displayed.contains(&format!("src/levels.rs:7:11: E0308: {DIAGNOSTIC}")),
            "{displayed}",
        );
        assert!(!displayed.contains("private-rendered-compiler-canary"));
        assert!(!displayed.contains(&source.to_string_lossy().into_owned()));
        let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
            diagnostics,
            ..
        }) = error
        else {
            panic!("the primary location must remain available to the public caller")
        };
        let location = diagnostics.entries()[0].primary_location().unwrap();
        assert_eq!(location.path(), Path::new("src/levels.rs"));
        assert_eq!((location.line(), location.column()), (7, 11));
        assert_eq!(diagnostics.truncated_count(), 0);
    }

    #[test]
    fn external_or_unsafe_compiler_locations_do_not_erase_the_build_failure() {
        let invocation = invocation();
        let external = invocation
            .workspace_root()
            .parent()
            .unwrap()
            .join("private-external-location.rs");
        for path in [
            external.to_string_lossy().into_owned(),
            "../private-external-location.rs".to_owned(),
            "src/private-\u{1b}-location.rs".to_owned(),
        ] {
            let mut output = diagnostic_output_at(&path);
            output.extend_from_slice(b"{\"reason\":\"build-finished\",\"success\":false}\n");
            let error = finish_bootstrap_build(exit_status(false), &output, invocation.clone())
                .unwrap_err();
            let displayed = error.to_string();
            assert!(displayed.contains(DIAGNOSTIC), "{displayed}");
            assert!(displayed.contains("E0308"), "{displayed}");
            assert!(!displayed.contains("private-external-location"));
            assert!(!displayed.contains("private-\u{1b}-location"));
            let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
                diagnostics,
                ..
            }) = error
            else {
                panic!("withheld locations must retain the original build failure")
            };
            assert!(diagnostics.entries()[0].primary_location().is_none());
            assert_eq!(diagnostics.truncated_count(), 1);
        }
    }

    fn invocation() -> CargoBuildInvocation {
        let root = if cfg!(windows) {
            PathBuf::from(r"C:\bootstrap-test")
        } else {
            PathBuf::from("/bootstrap-test")
        };
        let manifest = root.join("Cargo.toml");
        let selection = CargoBuildSelection::new(
            "bootstrap-test",
            "headless",
            "dev",
            "x86_64-unknown-linux-gnu",
            std::iter::empty::<&str>(),
            false,
        )
        .unwrap();
        let metadata = json!({
            "version": 1,
            "packages": [{
                "name": "bootstrap-test", "version": "0.0.0", "id": PACKAGE_ID, "source": null,
                "manifest_path": manifest, "features": {},
                "targets": [{
                    "kind": ["bin"], "crate_types": ["bin"], "name": "headless",
                    "src_path": root.join("src/bin/headless.rs"), "required-features": []
                }]
            }],
            "workspace_members": [PACKAGE_ID], "workspace_root": root,
            "target_directory": root.join("target"),
            "resolve": {"nodes": [{"id": PACKAGE_ID, "features": []}]}
        });
        CargoMetadataFacts::parse(
            &serde_json::to_vec(&metadata).unwrap(),
            &manifest,
            &selection,
        )
        .unwrap()
        .build_invocation(selection)
        .unwrap()
    }

    fn diagnostic_output(level: &str, message: &str) -> Vec<u8> {
        let mut output = serde_json::to_vec(&json!({
            "reason": "compiler-message",
            "message": {"level": level, "code": null, "message": message}
        }))
        .unwrap();
        output.push(b'\n');
        output
    }

    fn diagnostic_output_at(path: &str) -> Vec<u8> {
        let mut output = serde_json::to_vec(&json!({
            "reason": "compiler-message",
            "message": {
                "level": "error",
                "code": {"code": "E0308"},
                "message": DIAGNOSTIC,
                "rendered": "private-rendered-compiler-canary",
                "spans": [
                    {"is_primary": false, "file_name": "secondary.rs", "line_start": 1, "column_start": 1},
                    {"is_primary": true, "file_name": path, "line_start": 7, "column_start": 11}
                ]
            }
        }))
        .unwrap();
        output.push(b'\n');
        output
    }

    fn exit_status(success: bool) -> ExitStatus {
        #[cfg(windows)]
        let code = if success { 0 } else { 101 };
        #[cfg(target_os = "linux")]
        let code = if success { 0 } else { 101 << 8 };
        ExitStatus::from_raw(code)
    }
}
