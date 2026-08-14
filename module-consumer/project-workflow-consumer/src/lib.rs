//! Compile-only consumer for Nara's reusable project-creation transaction.
//!
//! Native path discovery, CLI parsing, Cargo processes, windows, and presentation remain outside
//! this package. Callers supply an already-authorized request and receive the public preparation or
//! terminal publication outcome unchanged.

use nara::project_workflow::{
    PreparedProjectCreation, ProjectCheckError, ProjectCheckRequest, ProjectCreationOutcome,
    ProjectCreationPlanError, ProjectCreationRequest, StartupContentClosureReceipt, check_project,
    prepare_desktop_2d_project,
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
pub fn check_startup_content(
    request: ProjectCheckRequest,
) -> Result<StartupContentClosureReceipt, ProjectCheckError> {
    check_project(request)
}
