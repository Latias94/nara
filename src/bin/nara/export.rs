use std::{fs, io};

use nara::{
    ProductRecipe,
    fs::{PathValidationError, RelativeComponent},
    project_workflow::{
        ProjectCheckError, ProjectCheckRequest, ProjectExportOrphan, ProjectExportOutcome,
        ProjectExportPlanError, ProjectExportReceipt, ProjectExportRejection, ProjectExportRequest,
        ProjectExportUnresolved, check_project, prepare_project_export,
    },
};
use thiserror::Error;

use super::{
    args::ExportArguments,
    host_fs::{self, HostDirectoryError},
};

mod acceptance;
mod cargo;

pub(crate) enum ExportCommandOutcome {
    Accepted(ProjectExportReceipt),
    Rejected(ProjectExportRejection),
    Orphaned(ProjectExportOrphan),
    Unresolved(ProjectExportUnresolved),
    PublishedButUnaccepted {
        receipt: ProjectExportReceipt,
        error: acceptance::ExportAcceptanceError,
    },
}

#[derive(Debug, Error)]
pub(crate) enum ExportCommandError {
    #[error("failed to resolve the project directory")]
    ResolveProject(#[source] io::Error),
    #[error("failed to resolve the export parent directory")]
    ResolveParent(#[source] io::Error),
    #[error(transparent)]
    HostDirectory(#[from] HostDirectoryError),
    #[error("the export destination is not one valid directory name")]
    Destination(#[source] PathValidationError),
    #[error(transparent)]
    ProjectCheck(#[from] ProjectCheckError),
    #[error(transparent)]
    CargoBuild(#[from] cargo::CargoBuildAdapterError),
    #[error(transparent)]
    ExportPlan(#[from] ProjectExportPlanError),
}

pub(crate) fn execute(
    arguments: ExportArguments,
) -> Result<ExportCommandOutcome, ExportCommandError> {
    let project_path =
        fs::canonicalize(arguments.project).map_err(ExportCommandError::ResolveProject)?;
    let parent_path =
        fs::canonicalize(arguments.parent).map_err(ExportCommandError::ResolveParent)?;
    let project_root = host_fs::readable_directory(&project_path)?;
    let startup_content =
        check_project(ProjectCheckRequest::new(project_root, ProductRecipe::new()))?;
    let build = cargo::build(&project_path, &startup_content)?;
    let publication_parent = host_fs::writable_directory(&parent_path)?;
    let destination_name = arguments.destination;
    let published_root = parent_path.join(&destination_name);
    let destination =
        RelativeComponent::new(&destination_name).map_err(ExportCommandError::Destination)?;
    let prepared = prepare_project_export(ProjectExportRequest::new(
        publication_parent,
        destination,
        startup_content,
        build,
    ))?;
    Ok(match prepared.publish() {
        ProjectExportOutcome::Published(receipt) => {
            match acceptance::accept(&published_root, receipt.plan()) {
                Ok(()) => ExportCommandOutcome::Accepted(receipt),
                Err(error) => ExportCommandOutcome::PublishedButUnaccepted { receipt, error },
            }
        }
        ProjectExportOutcome::Rejected(error) => ExportCommandOutcome::Rejected(error),
        ProjectExportOutcome::Orphaned(error) => ExportCommandOutcome::Orphaned(error),
        ProjectExportOutcome::Unresolved(error) => ExportCommandOutcome::Unresolved(error),
    })
}
