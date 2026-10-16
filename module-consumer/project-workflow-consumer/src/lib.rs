//! Compile-only consumer for Nara's reusable project-workflow transactions.
//!
//! Native path discovery, CLI parsing, Cargo processes, windows, and presentation remain outside
//! this package. Callers supply an already-authorized request and receive the public preparation or
//! terminal publication outcome unchanged.

use nara::project_workflow::{
    PreparedProjectCreation, PreparedProjectExport, ProjectCheckError, ProjectCheckRequest,
    ProjectCreationOutcome, ProjectCreationPlanError, ProjectCreationRequest, ProjectExportOutcome,
    ProjectExportPlanError, ProjectExportRequest, ProjectStartupSceneRepairError,
    ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairRequest,
    StartupContentClosureReceipt, check_project, prepare_desktop_2d_project,
    prepare_project_export, repair_startup_scene,
};

/// Prepares the immutable `desktop-2d/v1` creation plan through the public root Module.
pub fn prepare_creation(
    request: ProjectCreationRequest,
) -> Result<PreparedProjectCreation, ProjectCreationPlanError> {
    prepare_desktop_2d_project(request)
}

/// Publishes one prepared project and preserves the complete terminal outcome family.
#[must_use = "the project-creation terminal outcome must be handled"]
pub fn publish_creation(prepared: PreparedProjectCreation) -> ProjectCreationOutcome {
    prepared.publish()
}

/// Checks one authorized project without a CLI, Cargo process, or desktop presentation stack.
#[expect(
    clippy::result_large_err,
    reason = "This compile-only consumer preserves the public transaction result unchanged"
)]
pub fn check_startup_content(
    request: ProjectCheckRequest,
) -> Result<StartupContentClosureReceipt, ProjectCheckError> {
    check_project(request)
}

/// Prepares one startup-closure export without importing Cargo JSON, processes, or native paths.
pub fn prepare_export(
    request: ProjectExportRequest,
) -> Result<PreparedProjectExport, ProjectExportPlanError> {
    prepare_project_export(request)
}

/// Publishes one prepared export and preserves the complete terminal outcome family.
#[must_use = "the project-export terminal outcome must be handled"]
pub fn publish_export(prepared: PreparedProjectExport) -> ProjectExportOutcome {
    prepared.publish()
}

/// Repairs one explicit startup Scene selection without importing CLI or process adapters.
#[expect(
    clippy::result_large_err,
    reason = "This compile-only consumer preserves the public transaction result unchanged"
)]
pub fn repair_startup_scene_selection(
    request: ProjectStartupSceneRepairRequest,
) -> Result<ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairError> {
    repair_startup_scene(request)
}
