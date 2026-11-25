//! Native adapters for tools hosted by a compiled project's existing headless executable.
//!
//! The native adapter builds that executable. Strict content admission, Schema refresh, and
//! startup Scene repair run in the project process with its typed product recipe.

use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use crate::{
    ProductRecipe,
    project_workflow::{ProjectCheckRequest, check_project},
};
use thiserror::Error;

mod bootstrap;
pub mod project_cook;
mod protocol;
mod schema;

pub use bootstrap::{ProjectToolInvocationError, invoke_project_tool};
pub use nara_project::{cargo::native as cargo_build, host_fs, player_package, process};
pub use protocol::{
    ProjectToolDiagnostic, ProjectToolDiagnosticLoss, ProjectToolFailure, ProjectToolFailureStage,
    ProjectToolRepairHint,
};

/// One operation delegated to the compiled project. Paths are native adapter inputs, not receipts.
pub enum ProjectToolRequest {
    Check {
        project: PathBuf,
        profile: Option<String>,
    },
    Schema {
        project: PathBuf,
        profile: Option<String>,
        write: bool,
    },
    RepairStartupScene {
        project: PathBuf,
        profile: Option<String>,
        replacement: String,
    },
}

impl ProjectToolRequest {
    fn project(&self) -> &std::path::Path {
        match self {
            Self::Check { project, .. }
            | Self::Schema { project, .. }
            | Self::RepairStartupScene { project, .. } => project,
        }
    }

    fn profile(&self) -> Option<&str> {
        match self {
            Self::Check { profile, .. }
            | Self::Schema { profile, .. }
            | Self::RepairStartupScene { profile, .. } => profile.as_deref(),
        }
    }

    fn operation(&self) -> &'static str {
        match self {
            Self::Check { .. } => "check",
            Self::Schema { write: false, .. } => "schema-check",
            Self::Schema { write: true, .. } => "schema-write",
            Self::RepairStartupScene { .. } => "repair-startup-scene",
        }
    }
}

/// Bounded observations for display. This value carries no filesystem or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectToolSummary {
    Checked { content_members: usize },
    Schema { written: bool },
    Repaired { replacement: String },
}

/// Runs explicit project tool mode before argument parsing or gameplay startup.
///
/// Pass arguments without the executable name. `None` means normal gameplay was requested. The
/// recipe factory is called once only after a valid tool request has been selected.
///
/// Direct [`crate::ProductRecipeError`] values retain bounded classifications and admitted plugin
/// identifiers. Other error types remain opaque and are never formatted or traversed.
pub fn run_project_tool_mode<E: 'static>(
    arguments: impl IntoIterator<Item = OsString>,
    recipe: impl FnOnce() -> Result<ProductRecipe, E>,
) -> Option<ExitCode> {
    let request = protocol::parse_request(arguments)?;
    let operation = request
        .as_ref()
        .map_or("invalid", ProjectToolRequest::operation);
    let result = request.map_err(ToolError::Protocol).and_then(|request| {
        let recipe = recipe().map_err(ToolError::recipe)?;
        execute(request, recipe)
    });
    let success = result.is_ok();
    let Ok(value) = protocol::encode_response(operation, result) else {
        return Some(ExitCode::FAILURE);
    };
    let mut stdout = io::stdout().lock();
    if serde_json::to_writer(&mut stdout, &value)
        .and_then(|()| stdout.write_all(b"\n").map_err(serde_json::Error::io))
        .is_err()
    {
        return Some(ExitCode::FAILURE);
    }
    Some(if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[derive(Debug, Error)]
enum ToolError {
    #[error("{0}")]
    Protocol(&'static str),
    #[error("project recipe construction failed")]
    Recipe(protocol::RecipeFailure),
    #[error("schema refresh failed")]
    Schema(crate::project_host::schema::SchemaRefreshError),
    #[error(transparent)]
    Directory(#[from] host_fs::HostDirectoryError),
    #[error(transparent)]
    Check(#[from] crate::project_workflow::ProjectCheckError),
    #[error("startup Scene repair failed")]
    Repair(crate::project_workflow::ProjectStartupSceneRepairError),
    #[error("the manifest was published but repair acceptance could not be established")]
    RepairPublished(crate::project_workflow::PublishedButNotRepaired),
    #[error("{summary}")]
    Operation {
        stage: ProjectToolFailureStage,
        code: &'static str,
        summary: &'static str,
    },
}

impl ToolError {
    fn recipe<E: 'static>(error: E) -> Self {
        Self::Recipe(protocol::RecipeFailure::from_error(error))
    }

    fn operation(
        stage: ProjectToolFailureStage,
        code: &'static str,
        summary: &'static str,
    ) -> Self {
        Self::Operation {
            stage,
            code,
            summary,
        }
    }

    fn failure(&self, operation: &str) -> ProjectToolFailure {
        use ProjectToolFailureStage as Stage;
        let (stage, code, summary) = match self {
            Self::Protocol(summary) => (Stage::Request, "project.tool.invalid-request", *summary),
            Self::Recipe(error) => return error.failure(operation),
            Self::Directory(_) => (
                Stage::ProjectAccess,
                "project.tool.directory-unavailable",
                "The selected project directory is unavailable",
            ),
            Self::Schema(error) => return ProjectToolFailure::from_schema(operation, *error),
            Self::Check(error) => return ProjectToolFailure::from_check(operation, error),
            Self::Repair(error) => return ProjectToolFailure::from_repair(operation, error),
            Self::RepairPublished(outcome) => {
                return ProjectToolFailure::from_repair_postcheck(operation, outcome.error());
            }
            Self::Operation {
                stage,
                code,
                summary,
            } => (*stage, *code, *summary),
        };
        ProjectToolFailure::classified(operation, stage, code, summary)
    }
}

#[cfg(test)]
pub(crate) fn test_schema_failure_protocol_roundtrip(
    error: crate::project_host::schema::SchemaRefreshError,
) -> ProjectToolFailure {
    test_failure_protocol_roundtrip("schema-write", ToolError::Schema(error))
}

#[cfg(test)]
fn test_failure_protocol_roundtrip(operation: &str, error: ToolError) -> ProjectToolFailure {
    let response = protocol::encode_response(operation, Err(error)).unwrap();
    let mut bytes = serde_json::to_vec(&response).unwrap();
    bytes.push(b'\n');
    protocol::decode_response(&bytes, operation)
        .unwrap()
        .unwrap_err()
}

fn execute(
    request: ProjectToolRequest,
    recipe: ProductRecipe,
) -> Result<ProjectToolSummary, ToolError> {
    match request {
        ProjectToolRequest::Check { project, profile } => {
            let root = host_fs::readable_directory(&project)?;
            let mut request = ProjectCheckRequest::new(root, recipe);
            if let Some(profile) = profile {
                request = request.with_profile(profile);
            }
            let closure = check_project(request)?;
            Ok(ProjectToolSummary::Checked {
                content_members: closure.members().len(),
            })
        }
        ProjectToolRequest::Schema {
            project,
            profile,
            write,
        } => schema::execute(project, profile, write, recipe),
        ProjectToolRequest::RepairStartupScene {
            project,
            profile,
            replacement,
        } => repair_startup_scene(project, profile, replacement, recipe),
    }
}

fn repair_startup_scene(
    project: PathBuf,
    profile: Option<String>,
    replacement: String,
    recipe: ProductRecipe,
) -> Result<ProjectToolSummary, ToolError> {
    use crate::project_workflow::{
        ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairRequest,
    };
    let root = host_fs::writable_directory(&project)?;
    let mut request = ProjectCheckRequest::new(root, recipe);
    if let Some(profile) = profile {
        request = request.with_profile(profile);
    }
    let error = match check_project(request) {
        Ok(_) => {
            return Err(ToolError::operation(
                ProjectToolFailureStage::Repair,
                "project.tool.repair-not-needed",
                "The project does not require startup Scene repair",
            ));
        }
        Err(error) => error,
    };
    if error.startup_scene_issue().is_none() {
        return Err(ToolError::Check(error));
    }
    let issue = error
        .into_startup_scene_issue()
        .expect("the repairable issue was observed");
    let replacement = crate::project::ProjectPath::new(replacement).map_err(|_| {
        ToolError::operation(
            ProjectToolFailureStage::Repair,
            "project.tool.repair-invalid-replacement",
            "The replacement is not a valid Scene path",
        )
    })?;
    match crate::project_workflow::repair_startup_scene(ProjectStartupSceneRepairRequest::new(
        issue,
        replacement,
    )) {
        Ok(ProjectStartupSceneRepairOutcome::Repaired(receipt)) => {
            Ok(ProjectToolSummary::Repaired {
                replacement: receipt.publication().replacement().as_str().to_owned(),
            })
        }
        Ok(ProjectStartupSceneRepairOutcome::PublishedButNotRepaired(outcome)) => {
            Err(ToolError::RepairPublished(outcome))
        }
        Err(error) => Err(finish_repair_failure(error)),
    }
}

fn finish_repair_failure(
    mut error: crate::project_workflow::ProjectStartupSceneRepairError,
) -> ToolError {
    if let crate::project_workflow::ProjectStartupSceneRepairError::TemporaryFile(failure) =
        &mut error
    {
        let _ = failure.retry_cleanup();
    }
    ToolError::Repair(error)
}

#[cfg(all(test, windows))]
#[test]
fn repair_failure_exit_cleans_retained_candidate() {
    use crate::fs::{ExpectedTarget, RelativeComponent};
    use crate::project_workflow::ProjectStartupSceneRepairError;
    let root = std::env::temp_dir().join(format!("nara-repair-exit-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let parent = host_fs::writable_directory(&root).unwrap();
    let candidate = parent
        .create_temp(&RelativeComponent::new("candidate.tmp").unwrap())
        .unwrap();
    std::fs::write(root.join("nara.toml"), b"external manifest").unwrap();
    let failure = parent
        .replace_temp(
            candidate,
            &RelativeComponent::new("nara.toml").unwrap(),
            ExpectedTarget::Missing,
        )
        .unwrap_err();
    assert!(root.join("candidate.tmp").exists());
    let _error = finish_repair_failure(ProjectStartupSceneRepairError::TemporaryFile(failure));
    assert!(!root.join("candidate.tmp").exists());
    assert_eq!(
        std::fs::read(root.join("nara.toml")).unwrap(),
        b"external manifest"
    );
    drop(parent);
    std::fs::remove_dir_all(root).unwrap();
}
