//! Native adapters for tools hosted by a compiled project's existing headless executable.
//!
//! The generic CLI discovers and builds that executable. Strict content admission, export build
//! agreement, and publication stay in the project process with its own typed product recipe.

use std::{
    error::Error,
    ffi::OsString,
    fmt,
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
mod export;
pub mod host_fs;
mod product_observation;
mod protocol;
mod schema;

pub use bootstrap::{ProjectToolInvocationError, invoke_project_tool};
#[doc(hidden)]
pub use product_observation::{ProductObservationTarget, record_product_observation};

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
    Export {
        project: PathBuf,
        parent: PathBuf,
        destination: OsString,
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
            | Self::Export { project, .. }
            | Self::RepairStartupScene { project, .. } => project,
        }
    }

    fn profile(&self) -> Option<&str> {
        match self {
            Self::Check { profile, .. }
            | Self::Schema { profile, .. }
            | Self::RepairStartupScene { profile, .. } => profile.as_deref(),
            Self::Export { .. } => None,
        }
    }

    fn operation(&self) -> &'static str {
        match self {
            Self::Check { .. } => "check",
            Self::Schema { write: false, .. } => "schema-check",
            Self::Schema { write: true, .. } => "schema-write",
            Self::Export { .. } => "export",
            Self::RepairStartupScene { .. } => "repair-startup-scene",
        }
    }
}

/// Bounded observations for display. This value carries no filesystem or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectToolSummary {
    Checked {
        startup_members: usize,
    },
    Schema {
        written: bool,
    },
    Exported {
        startup_members: usize,
        product_observations: usize,
    },
    Repaired {
        replacement: String,
    },
}

/// Runs explicit project tool mode before argument parsing or gameplay startup.
///
/// Pass arguments without the executable name. `None` means normal gameplay was requested. The
/// recipe factory is called once only after a valid tool request has been selected.
pub fn run_project_tool_mode<E: fmt::Display>(
    arguments: impl IntoIterator<Item = OsString>,
    recipe: impl FnOnce() -> Result<ProductRecipe, E>,
) -> Option<ExitCode> {
    let request = protocol::parse_request(arguments)?;
    let operation = request
        .as_ref()
        .map_or("invalid", ProjectToolRequest::operation);
    let result = request.map_err(ToolError::Protocol).and_then(|request| {
        let recipe = recipe().map_err(|error| ToolError::Recipe(error.to_string()))?;
        execute(request, recipe)
    });
    let success = result.is_ok();
    let value = protocol::encode_response(operation, result);
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
    #[error("project recipe failed: {0}")]
    Recipe(String),
    #[error(transparent)]
    Directory(#[from] host_fs::HostDirectoryError),
    #[error(transparent)]
    Check(#[from] crate::project_workflow::ProjectCheckError),
    #[error(transparent)]
    Export(#[from] export::ExportCommandError),
    #[error("{0}")]
    Operation(String),
}

impl ToolError {
    fn diagnostic(&self) -> String {
        if let Self::Check(error) = self
            && let Some(issue) = error.startup_scene_issue()
        {
            let profile_option = issue
                .profile()
                .map_or_else(String::new, |profile| format!(" --profile {profile}"));
            return format!(
                "startup Scene `{}` is missing under the project Scene root `{}`.\nrepair: nara repair startup-scene <project-directory>{profile_option} --replacement <scenes-root-relative-scene>",
                issue.selected_scene(),
                issue.scenes_path(),
            );
        }
        let mut diagnostic = self.to_string();
        let mut source = self.source();
        while let Some(error) = source {
            diagnostic.push_str(": ");
            diagnostic.push_str(&error.to_string());
            if diagnostic.len() > protocol::MAX_DIAGNOSTIC_BYTES {
                break;
            }
            source = error.source();
        }
        diagnostic
    }
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
                startup_members: closure.members().len(),
            })
        }
        ProjectToolRequest::Schema {
            project,
            profile,
            write,
        } => schema::execute(project, profile, write, recipe),
        ProjectToolRequest::Export {
            project,
            parent,
            destination,
        } => {
            match export::execute(
                export::ExportArguments {
                    project,
                    parent,
                    destination,
                },
                recipe,
            )? {
                export::ExportCommandOutcome::Accepted {
                    receipt,
                    acceptance,
                } => {
                    export::record_acceptance_observation(&receipt, &acceptance)
                        .map_err(|error| ToolError::Operation(error.to_string()))?;
                    Ok(ProjectToolSummary::Exported {
                        startup_members: receipt.plan().members().len(),
                        product_observations: acceptance.products().len(),
                    })
                }
                export::ExportCommandOutcome::Rejected(error) => {
                    Err(ToolError::Operation(error.to_string()))
                }
                export::ExportCommandOutcome::Orphaned(error) => {
                    Err(ToolError::Operation(error.to_string()))
                }
                export::ExportCommandOutcome::Unresolved(error) => {
                    Err(ToolError::Operation(error.to_string()))
                }
                export::ExportCommandOutcome::PublishedButUnaccepted { receipt, error } => {
                    Err(ToolError::Operation(format!(
                        "candidate with {} startup-closure members was published but acceptance failed: {error}",
                        receipt.plan().members().len(),
                    )))
                }
            }
        }
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
            return Err(ToolError::Operation(
                "the project does not require startup Scene repair".to_owned(),
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
    let replacement = crate::project::ProjectPath::new(replacement)
        .map_err(|error| ToolError::Operation(error.to_string()))?;
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
            Err(ToolError::Operation(format!(
                "the manifest was published but the project did not pass its fresh check: {}",
                outcome.error()
            )))
        }
        Err(error) => Err(finish_repair_failure(error)),
    }
}

fn finish_repair_failure(
    mut error: crate::project_workflow::ProjectStartupSceneRepairError,
) -> ToolError {
    let mut diagnostic = error.to_string();
    if let crate::project_workflow::ProjectStartupSceneRepairError::TemporaryFile(failure) =
        &mut error
        && !failure.retry_cleanup()
    {
        let cleanup = failure.cleanup().expect("failed cleanup retains its owner");
        diagnostic.push_str(&format!("; cleanup pending: temporary manifest {:?}, identity {:?}; remove it before retrying repair", cleanup.name().to_str(), cleanup.identity()));
    }
    ToolError::Operation(diagnostic)
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
