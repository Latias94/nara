use std::{fs, io};

use nara::{
    ProductRecipe,
    project_workflow::{
        ProjectCheckError, ProjectCheckRequest, ProjectStartupSceneIssue,
        StartupContentClosureReceipt, check_project,
    },
};
use thiserror::Error;

use super::{
    args::CheckArguments,
    host_fs::{self, HostDirectoryError},
};

#[derive(Debug, Error)]
pub(crate) enum CheckCommandError {
    #[error("failed to resolve the project directory")]
    ResolveProject(#[source] io::Error),
    #[error(transparent)]
    HostDirectory(#[from] HostDirectoryError),
    #[error(transparent)]
    ProjectCheck(#[from] ProjectCheckError),
}

impl CheckCommandError {
    pub(crate) const fn startup_scene_issue(&self) -> Option<&ProjectStartupSceneIssue> {
        match self {
            Self::ProjectCheck(error) => error.startup_scene_issue(),
            Self::ResolveProject(_) | Self::HostDirectory(_) => None,
        }
    }
}

pub(crate) fn execute(
    arguments: CheckArguments,
) -> Result<StartupContentClosureReceipt, CheckCommandError> {
    let project_path =
        fs::canonicalize(arguments.project).map_err(CheckCommandError::ResolveProject)?;
    let project_root = host_fs::readable_directory(&project_path)?;
    let request = ProjectCheckRequest::new(project_root, ProductRecipe::new());
    let request = match arguments.profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };
    check_project(request).map_err(CheckCommandError::from)
}
