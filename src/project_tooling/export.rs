use std::{fs, io};

use crate::{
    ProductRecipe,
    fs::{PathValidationError, RelativeComponent},
    project_workflow::{
        ProjectCheckError, ProjectExportOrphan, ProjectExportOutcome, ProjectExportPlanError,
        ProjectExportReceipt, ProjectExportRejection, ProjectExportRequest,
        ProjectExportUnresolved, prepare_project_export,
    },
};
use thiserror::Error;

use super::host_fs::{self, HostDirectoryError};

pub(super) struct ExportArguments {
    pub(super) project: std::path::PathBuf,
    pub(super) parent: std::path::PathBuf,
    pub(super) destination: std::ffi::OsString,
}

mod acceptance;
mod cargo;
#[cfg(test)]
mod trajectory;

pub(crate) enum ExportCommandOutcome {
    Accepted {
        receipt: ProjectExportReceipt,
        acceptance: acceptance::ExportAcceptanceReceipt,
    },
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
    recipe: ProductRecipe,
) -> Result<ExportCommandOutcome, ExportCommandError> {
    let project_path =
        fs::canonicalize(arguments.project).map_err(ExportCommandError::ResolveProject)?;
    let parent_path =
        fs::canonicalize(arguments.parent).map_err(ExportCommandError::ResolveParent)?;
    let project_root = host_fs::readable_directory(&project_path)?;
    let deliveries = ProjectExportRequest::check_deliveries(project_root, &recipe)?;
    let build = cargo::build(
        &project_path,
        deliveries
            .each_ref()
            .map(|delivery| delivery.startup_content()),
    )?
    .into_receipt();
    let publication_parent = host_fs::writable_directory(&parent_path)?;
    let destination_name = arguments.destination;
    let published_root = parent_path.join(&destination_name);
    let destination =
        RelativeComponent::new(&destination_name).map_err(ExportCommandError::Destination)?;
    let prepared = prepare_project_export(ProjectExportRequest::new(
        publication_parent,
        destination,
        deliveries,
        build,
        recipe,
    ))?;
    Ok(match prepared.publish() {
        ProjectExportOutcome::Published(receipt) => {
            match acceptance::accept(&published_root, receipt.plan()) {
                Ok(acceptance) => ExportCommandOutcome::Accepted {
                    receipt,
                    acceptance,
                },
                Err(error) => ExportCommandOutcome::PublishedButUnaccepted { receipt, error },
            }
        }
        ProjectExportOutcome::Rejected(error) => ExportCommandOutcome::Rejected(error),
        ProjectExportOutcome::Orphaned(error) => ExportCommandOutcome::Orphaned(error),
        ProjectExportOutcome::Unresolved(error) => ExportCommandOutcome::Unresolved(error),
    })
}
