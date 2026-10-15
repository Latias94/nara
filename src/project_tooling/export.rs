use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
};

use crate::{
    ProductRecipe,
    fs::{PathValidationError, RelativeComponent},
    project_workflow::{
        ProjectCheckError, ProjectCheckRequest, ProjectExportOrphan, ProjectExportOutcome,
        ProjectExportPlanError, ProjectExportReceipt, ProjectExportRejection, ProjectExportRequest,
        ProjectExportUnresolved, check_project, prepare_project_export,
    },
};
use serde_json::json;
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

const COMMAND_OBSERVATION_ATTEMPT_ENV: &str = "NARA_EXPORT_OBSERVATION_ATTEMPT";
const COMMAND_OBSERVATION_FILE_ENV: &str = "NARA_EXPORT_OBSERVATION_FILE";

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
pub(super) enum ExportCommandObservationError {
    #[error("export command observation requires both attempt and output")]
    IncompleteEnvironment,
    #[error("export command observation attempt is invalid")]
    InvalidAttempt,
    #[error("failed to create the export command observation")]
    Create(#[source] io::Error),
    #[error("failed to encode the export command observation")]
    Encode(#[source] serde_json::Error),
    #[error("failed to finish the export command observation")]
    Finish(#[source] io::Error),
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
    let startup_content = check_project(ProjectCheckRequest::new(project_root, recipe))?;
    let manifest_digest = startup_content.manifest_digest();
    let build = cargo::build(&project_path, &startup_content)?.into_receipt();
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
            match acceptance::accept(&published_root, receipt.plan(), manifest_digest) {
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

pub(super) fn record_acceptance_observation(
    receipt: &ProjectExportReceipt,
    acceptance: &acceptance::ExportAcceptanceReceipt,
) -> Result<(), ExportCommandObservationError> {
    let (attempt, output) = match (
        env::var_os(COMMAND_OBSERVATION_ATTEMPT_ENV),
        env::var_os(COMMAND_OBSERVATION_FILE_ENV),
    ) {
        (None, None) => return Ok(()),
        (Some(attempt), Some(output)) => (attempt, output),
        _ => return Err(ExportCommandObservationError::IncompleteEnvironment),
    };
    let attempt = attempt
        .into_string()
        .map_err(|_| ExportCommandObservationError::InvalidAttempt)?;
    if attempt.is_empty()
        || attempt.len() > 128
        || !attempt
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ExportCommandObservationError::InvalidAttempt);
    }
    let plan = receipt.plan();
    let products = acceptance
        .products()
        .iter()
        .map(|product| {
            json!({
                "attempt": product.attempt(),
                "target": product.target().cargo_target_name(),
                "profile": product.profile(),
                "executable": digest_json(product.executable_digest()),
                "manifest": digest_json(product.manifest_digest()),
                "content": digest_json(product.startup_content_digest()),
                "closure_plan": digest_json(product.startup_closure_plan_digest()),
                "authoring_schema_bundle": digest_json(product.authoring_schema_bundle_digest()),
                "schema_fingerprint": product.schema_fingerprint(),
                "observation": digest_json(product.observation_digest()),
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "version": 1,
        "attempt": attempt,
        "startup_content": digest_json(plan.startup_content_digest()),
        "build_input": digest_json(plan.build_input_digest()),
        "member_plan": digest_json(plan.member_plan_digest()),
        "products": products,
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(ExportCommandObservationError::Create)?;
    serde_json::to_writer(&mut file, &value).map_err(ExportCommandObservationError::Encode)?;
    file.write_all(b"\n")
        .map_err(ExportCommandObservationError::Finish)
}

fn digest_json(digest: crate::fs::ContentDigest) -> serde_json::Value {
    json!({
        "length": digest.length(),
        "blake3": acceptance::digest_hex(digest),
    })
}
