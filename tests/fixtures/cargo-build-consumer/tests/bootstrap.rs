//! Real bootstrap failure through the public caller, using the existing consumer workspace owner.

#![cfg(any(windows, target_os = "linux"))]
#![forbid(unsafe_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use engine::{
    project_tooling::{
        ProjectToolInvocationError, ProjectToolRequest, cargo_build::CargoBuildFailure,
        invoke_project_tool,
    },
    project_workflow::cargo::CargoBuildError,
};

#[path = "support/workspace.rs"]
#[allow(dead_code)]
mod workspace;

#[test]
#[ignore = "actual serial Cargo bootstrap failure; shared repository target required"]
fn bootstrap_compile_failure_reaches_the_caller_without_launching_the_tool() {
    let owner = workspace::Temporary::new();
    let project = &owner.root;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let target = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .expect("qualification must reuse the shared repository target"),
    );
    assert!(target.is_absolute());
    assert_eq!(
        target.canonicalize().unwrap(),
        repository.join("target").canonicalize().unwrap(),
    );
    fs::create_dir_all(project.join("src/bin")).unwrap();
    fs::create_dir(project.join(".cargo")).unwrap();
    fs::write(
        project.join(".cargo/config.toml"),
        format!(
            "[build]\njobs = 1\ntarget-dir = {}\n[net]\noffline = true\n",
            serde_json::to_string(target.to_str().unwrap()).unwrap(),
        ),
    )
    .unwrap();
    let manifest = project.join("Cargo.toml");
    let manifest_bytes = b"[package]\nname = 'nara-bootstrap-diagnostic-consumer'\n\
        version = '0.0.0'\nedition = '2024'\n\n\
        [package.metadata.nara]\nproject-tool-protocol = 1\n\n\
        [[bin]]\nname = 'headless'\npath = 'src/bin/headless.rs'\n\n\
        [workspace]\nresolver = '3'\n";
    fs::write(&manifest, manifest_bytes).unwrap();
    const DIAGNOSTIC: &str = "nara-bootstrap-F4-distinctive-compile-error";
    const MARKER: &str = "bootstrap-tool-was-launched";
    fs::write(
        project.join("src/bin/headless.rs"),
        format!(
            "compile_error!({DIAGNOSTIC:?});\n\
             fn main() {{ std::fs::write({MARKER:?}, b\"must not launch\").unwrap(); }}\n",
        ),
    )
    .unwrap();
    // Fixture preparation resolves the ordinary lockfile once. Bootstrap must then use --locked.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let initialized = Command::new(cargo)
        .current_dir(project)
        .args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(&manifest)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr),
    );
    let lockfile = fs::read(project.join("Cargo.lock")).unwrap();

    let error = invoke_project_tool(ProjectToolRequest::Check {
        project: project.clone(),
        profile: None,
    })
    .unwrap_err();
    let displayed = error.to_string();
    assert!(
        displayed.contains(DIAGNOSTIC),
        "the error displayed by the CLI must contain the compiler diagnostic: {displayed}",
    );
    let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
        status,
        diagnostics,
        protocol,
    }) = error
    else {
        panic!("the caller must retain the original structured build failure: {error:?}")
    };
    assert!(!status.success());
    assert!(matches!(protocol, Some(CargoBuildError::BuildFailed)));
    assert!(
        diagnostics
            .entries()
            .iter()
            .any(|diagnostic| diagnostic.message() == DIAGNOSTIC),
        "the public caller must retain the actionable compiler diagnostic",
    );
    assert!(
        !project.join(MARKER).exists(),
        "a failed bootstrap must never execute a project tool",
    );
    assert_eq!(fs::read(&manifest).unwrap(), manifest_bytes);
    assert_eq!(fs::read(project.join("Cargo.lock")).unwrap(), lockfile);
}
