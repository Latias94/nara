use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use thiserror::Error;

use super::{
    CargoBuildFailure, CargoBuildTools, bounded_status_output, parse_host_target, same_path,
};
use crate::cargo::{
    CargoBuildDecoder, CargoBuildInvocation, ProjectToolTargetError, project_tool_invocation,
};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProjectToolBootstrapError {
    #[error(transparent)]
    Process(#[from] CargoBuildFailure),
    #[error("Cargo could not initialize the project lockfile")]
    InitializeLockfile,
    #[error("Rust compiler identity was unavailable")]
    CompilerIdentity,
    #[error("Cargo could not discover the project's headless tool target")]
    DiscoverTarget,
    #[error(transparent)]
    Target(#[from] ProjectToolTargetError),
    #[error("the headless target must use src/bin/headless.rs")]
    SourcePath,
}

/// Builds and runs the declared headless project tool, returning its exit status and bounded stdout.
///
/// The caller supplies the operation arguments and interprets the response. This synchronous
/// adapter executes Cargo and the selected executable from an explicitly trusted project.
pub fn run_project_tool(
    project: &Path,
    desktop_if_declared: bool,
    arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    output_limit: usize,
) -> Result<(ExitStatus, Vec<u8>), ProjectToolBootstrapError> {
    let executable = build_headless(project, desktop_if_declared)?;
    let mut command = Command::new(executable);
    command.current_dir(project).args(arguments);
    Ok(bounded_status_output(&mut command, output_limit)?)
}

fn build_headless(
    project: &Path,
    desktop_if_declared: bool,
) -> Result<PathBuf, ProjectToolBootstrapError> {
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
        let (status, _) = bounded_status_output(&mut initialize, 16 * 1024 * 1024)?;
        if !status.success() {
            return Err(ProjectToolBootstrapError::InitializeLockfile);
        }
    }
    let mut identity = Command::new(tools.rustc_path());
    identity.arg("-vV");
    let (status, bytes) = bounded_status_output(&mut identity, 16 * 1024)?;
    if !status.success() {
        return Err(ProjectToolBootstrapError::CompilerIdentity);
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
    let (status, declarations) = bounded_status_output(&mut metadata, 16 * 1024 * 1024)?;
    if !status.success() {
        return Err(ProjectToolBootstrapError::DiscoverTarget);
    }
    let invocation = project_tool_invocation(&declarations, &manifest, &host, desktop_if_declared)?;
    if !same_path(
        invocation.source_path(),
        &project.join("src/bin/headless.rs"),
    ) {
        return Err(ProjectToolBootstrapError::SourcePath);
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
    if !invocation.features().is_empty() {
        build.arg("--features").arg(invocation.features().join(","));
    }
    let (status, bytes) = bounded_status_output(&mut build, 64 * 1024 * 1024)?;
    Ok(finish_bootstrap_build(status, &bytes, invocation)?)
}

fn finish_bootstrap_build(
    status: ExitStatus,
    bytes: &[u8],
    invocation: CargoBuildInvocation,
) -> Result<PathBuf, CargoBuildFailure> {
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
        }),
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
    use crate::cargo::{CargoBuildError, CargoBuildSelection, CargoMetadataFacts};

    const DIAGNOSTIC: &str = "bootstrap compiler diagnostic must survive failure";
    const PACKAGE_ID: &str = "bootstrap-test-package-id";

    #[test]
    fn failed_compilation_keeps_its_diagnostics_status_and_protocol_error() {
        let mut output = diagnostic_output("error", DIAGNOSTIC);
        output.extend_from_slice(b"{\"reason\":\"build-finished\",\"success\":false}\n");
        let error = finish_bootstrap_build(exit_status(false), &output, invocation()).unwrap_err();
        let CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol,
        } = error
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
        let CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol,
        } = error
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
        let CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol: Some(CargoBuildError::MissingBuildFinished),
        } = error
        else {
            panic!("bootstrap must retain diagnostics when the finished message is absent")
        };
        assert!(status.success());
        assert_eq!(diagnostics.entries()[0].message(), DIAGNOSTIC);
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
        let CargoBuildFailure::Compilation {
            status,
            diagnostics,
            protocol: None,
        } = error
        else {
            panic!("a failed build process must keep its diagnostics even after a valid stream")
        };
        assert!(!status.success());
        assert_eq!(diagnostics.entries()[0].message(), DIAGNOSTIC);
        assert_eq!(
            finish_bootstrap_build(exit_status(true), &output, invocation).unwrap(),
            executable,
        );
    }

    #[test]
    fn bootstrap_failure_keeps_decoder_diagnostic_bounds_and_loss_facts() {
        let long_message = "⚠".repeat(6_000);
        let mut output = diagnostic_output("error", &long_message);
        for _ in 0..256 {
            output.extend_from_slice(&diagnostic_output("error", DIAGNOSTIC));
        }
        output.extend_from_slice(b"{\"reason\":\"build-finished\",\"success\":false}\n");
        let error = finish_bootstrap_build(exit_status(false), &output, invocation()).unwrap_err();
        let CargoBuildFailure::Compilation { diagnostics, .. } = error else {
            panic!("bootstrap must keep the decoder's bounded diagnostic summary")
        };
        assert_eq!(diagnostics.entries().len(), 256);
        assert!(diagnostics.entries()[0].message().len() < long_message.len());
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
        let CargoBuildFailure::Compilation { diagnostics, .. } = error else {
            panic!("the primary location must remain available to the public caller")
        };
        let diagnostic = &diagnostics.entries()[0];
        assert_eq!(diagnostic.message(), DIAGNOSTIC);
        assert_eq!(diagnostic.code(), Some("E0308"));
        let location = diagnostic.primary_location().unwrap();
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
            let CargoBuildFailure::Compilation { diagnostics, .. } = error else {
                panic!("withheld locations must retain the original build failure")
            };
            let diagnostic = &diagnostics.entries()[0];
            assert_eq!(diagnostic.message(), DIAGNOSTIC);
            assert_eq!(diagnostic.code(), Some("E0308"));
            assert!(diagnostic.primary_location().is_none());
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
