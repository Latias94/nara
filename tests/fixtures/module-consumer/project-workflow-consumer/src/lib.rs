//! Compile-only consumer for Nara's reusable project-workflow transactions.
//!
//! Native path discovery, CLI parsing, Cargo processes, windows, and presentation remain outside
//! this package. Callers supply an already-authorized request and receive the public preparation or
//! terminal publication outcome unchanged.

use nara::project_host::{
    ProjectContentDeliveryReceipt, ProjectContentError, ProjectContentLoader,
    ProjectContentSnapshot, ProjectSettingsCandidate, RuntimePlan, StartupContentClosureReceipt,
};
use nara::project_workflow::{
    NaraDependencySource, PreparedProjectCreation, PreparedProjectExport, ProjectCheckError,
    ProjectCheckRequest, ProjectCreationOutcome, ProjectCreationPlanError, ProjectCreationRequest,
    ProjectExportOutcome, ProjectExportPlanError, ProjectExportRequest, ProjectSlug,
    ProjectStarter, ProjectStartupSceneRepairError, ProjectStartupSceneRepairOutcome,
    ProjectStartupSceneRepairRequest, check_project, prepare_desktop_2d_project,
    prepare_project_export, repair_startup_scene,
};
use nara::{fs::DirectoryCapability, project::ProjectPath};

/// Prepares the explicitly selected Desktop 2D starter through the public root Module.
pub fn prepare_creation(
    starter: ProjectStarter,
    parent: DirectoryCapability,
    slug: ProjectSlug,
    dependency: NaraDependencySource,
) -> Result<PreparedProjectCreation, ProjectCreationPlanError> {
    prepare_desktop_2d_project(ProjectCreationRequest::new(
        starter, parent, slug, dependency,
    ))
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
pub fn check_delivery_content(
    request: ProjectCheckRequest,
) -> Result<ProjectContentDeliveryReceipt, ProjectCheckError> {
    check_project(request)
}

/// Selects a source Scene without rewriting startup settings or declaring delivery membership.
pub fn load_selected_scene(
    loader: &ProjectContentLoader,
    settings: &ProjectSettingsCandidate,
    plan: &RuntimePlan,
    scene: &ProjectPath,
) -> Result<ProjectContentSnapshot, ProjectContentError> {
    loader.load_scene(settings, plan, scene)
}

/// Retains the startup attestation explicitly within a complete delivery receipt.
pub fn startup_content(delivery: &ProjectContentDeliveryReceipt) -> &StartupContentClosureReceipt {
    delivery.startup_content()
}

/// Prepares one declared-content export without importing Cargo JSON, processes, or native paths.
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
