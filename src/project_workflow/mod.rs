//! First-party project creation, checking, export, and repair transactions.
//!
//! Native process execution, CLI parsing and desktop presentation remain Adapter concerns.
//! The `cargo` submodule supplies bounded pure facts for explicitly selected trusted builds;
//! build output does not grant product activation or Runtime readiness.

#[cfg(feature = "project-workflow-cargo-facts")]
#[path = "cargo_facts.rs"]
pub mod cargo;
mod check;
pub mod cooked_content;
mod creation;
mod dependency;
mod export;
mod publication;
mod repair;
mod starter;

pub use crate::project_host::{
    ProjectContentMemberKind, ProjectContentMemberReceipt, StartupContentClosureReceipt,
};
pub use check::{
    ProjectCheckError, ProjectCheckErrorKind, ProjectCheckRequest, ProjectStartupSceneIssue,
    check_project,
};
pub use creation::{
    PreparedProjectCreation, ProjectCreationPlanError, ProjectCreationPlanErrorKind,
    ProjectCreationRequest, prepare_desktop_2d_project,
};
pub use dependency::{
    LocalDependencyRelation, NaraDependencyReceipt, NaraDependencySource,
    NaraDependencySourceError, NaraDependencySourceErrorKind, NaraDependencySourceKind,
};
pub use export::{
    PreparedProjectExport, ProjectExportBinaryTarget, ProjectExportBuildError,
    ProjectExportBuildReceipt, ProjectExportExecutableReceipt, ProjectExportMemberKind,
    ProjectExportMemberReceipt, ProjectExportOrphan, ProjectExportOutcome, ProjectExportPlanError,
    ProjectExportPlanReceipt, ProjectExportReceipt, ProjectExportRejection,
    ProjectExportRejectionKind, ProjectExportRequest, ProjectExportUnresolved,
    prepare_project_export,
};
pub use publication::{
    ProjectCreationAdmissionError, ProjectCreationAdmissionErrorKind, ProjectCreationOrphan,
    ProjectCreationOutcome, ProjectCreationReceipt, ProjectCreationRejection,
    ProjectCreationRejectionKind, ProjectCreationUnresolved, ProjectTreeStageError,
    PublishedButUnadmitted,
};
pub use repair::{
    ProjectStartupScenePostcheckError, ProjectStartupScenePublicationReceipt,
    ProjectStartupSceneRepairError, ProjectStartupSceneRepairOutcome,
    ProjectStartupSceneRepairReceipt, ProjectStartupSceneRepairRequest, PublishedButNotRepaired,
    repair_startup_scene,
};
pub use starter::{
    ProjectCreationPlan, ProjectCreationPlanReceipt, ProjectDirectory, ProjectMember,
    ProjectMemberPlan, ProjectMemberReceipt, ProjectSlug, ProjectSlugError, ProjectSlugErrorKind,
    ProjectStarter,
};

/// Adapter-only construction surface for source Export product agreements.
///
/// This module is not a product extension seam. Workflow consumers receive the resulting opaque
/// receipt; explicit Cargo selection and pure messages belong to the separate `cargo` module.
#[doc(hidden)]
#[cfg(feature = "project-workflow-cargo-facts")]
pub mod __private {
    #[cfg(feature = "project-tooling")]
    pub use super::export::{
        ProjectExportBuildPreparation, ProjectExportCargoArtifact, ProjectExportProductAgreement,
    };
}
