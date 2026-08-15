use std::{fs, io};

use nara::{
    ProductRecipe,
    project::{ProjectPath, ProjectPathError},
    project_workflow::{
        ProjectCheckError, ProjectCheckRequest, ProjectStartupSceneRepairError,
        ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairRequest, check_project,
        repair_startup_scene,
    },
};
use thiserror::Error;

use super::{
    args::RepairArguments,
    host_fs::{self, HostDirectoryError},
};

#[derive(Debug, Error)]
pub(crate) enum RepairCommandError {
    #[error("failed to resolve the project directory")]
    ResolveProject(#[source] io::Error),
    #[error(transparent)]
    HostDirectory(#[from] HostDirectoryError),
    #[error("the replacement Scene path is invalid")]
    ReplacementPath(#[from] ProjectPathError),
    #[error("the project does not require startup Scene repair")]
    RepairNotRequired,
    #[error("the project does not report a repairable missing startup Scene")]
    ProjectCheck(#[source] ProjectCheckError),
    #[error(transparent)]
    Repair(#[from] ProjectStartupSceneRepairError),
}

pub(crate) fn execute(
    arguments: RepairArguments,
) -> Result<ProjectStartupSceneRepairOutcome, RepairCommandError> {
    let project_path =
        fs::canonicalize(arguments.project).map_err(RepairCommandError::ResolveProject)?;
    let project_root = host_fs::writable_directory(&project_path)?;
    let request = ProjectCheckRequest::new(project_root, ProductRecipe::new());
    let request = match arguments.profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };
    let error = match check_project(request) {
        Ok(_) => return Err(RepairCommandError::RepairNotRequired),
        Err(error) => error,
    };
    if error.startup_scene_issue().is_none() {
        return Err(RepairCommandError::ProjectCheck(error));
    }
    let issue = error
        .into_startup_scene_issue()
        .expect("the repairable issue was observed before consuming the check error");
    let replacement = ProjectPath::new(arguments.replacement)?;
    repair_startup_scene(ProjectStartupSceneRepairRequest::new(issue, replacement))
        .map_err(RepairCommandError::from)
}
