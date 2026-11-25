use std::path::Path;

use nara::project_tooling::{
    ProjectToolFailure, ProjectToolInvocationError, ProjectToolRequest, ProjectToolSummary,
    cargo_build::CargoBuildTools, invoke_project_tool, process::SupervisedProcessCommand,
};

#[allow(dead_code)]
#[path = "../project_cook/process.rs"]
mod process;
pub(super) mod puzzle;
mod workspace;

fn command(
    executable: &Path,
    cwd: &Path,
    tools: &CargoBuildTools,
    target: &Path,
) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(executable, cwd)
        .environment("CARGO", tools.cargo_path())
        .environment("RUSTC", tools.rustc_path())
        .environment("RUSTUP_TOOLCHAIN", "1.95.0")
        .environment("RUSTUP_AUTO_INSTALL", "0")
        .environment("CARGO_NET_OFFLINE", "true")
        .environment("CARGO_BUILD_JOBS", "1")
        .environment("CARGO_TARGET_DIR", target)
}

fn check(project: &Path, profile: Option<&str>) -> Result<(), ProjectToolInvocationError> {
    // The compiled tool must return one structured response before the gameplay entry runs.
    let result = invoke_project_tool(ProjectToolRequest::Check {
        project: project.to_path_buf(),
        profile: profile.map(str::to_owned),
    })?;
    assert!(
        matches!(
            result,
            ProjectToolSummary::Checked {
                content_members: 1..
            }
        ),
        "{result:?}"
    );
    Ok(())
}

fn schema(
    project: &Path,
    profile: Option<&str>,
    write: bool,
) -> Result<bool, ProjectToolInvocationError> {
    match invoke_project_tool(ProjectToolRequest::Schema {
        project: project.to_path_buf(),
        profile: profile.map(str::to_owned),
        write,
    })? {
        ProjectToolSummary::Schema { written } => Ok(written),
        summary => panic!("expected the schema outcome: {summary:?}"),
    }
}

fn project_failure(error: ProjectToolInvocationError) -> ProjectToolFailure {
    match error {
        ProjectToolInvocationError::Project(failure) => failure,
        error => panic!("expected a compiled project failure: {error:?}"),
    }
}
