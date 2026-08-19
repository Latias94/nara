//! First-party project creation, checking, export, and repair transactions.
//!
//! The initial public surface owns only the immutable `desktop-2d/v1` creation plan. Native paths,
//! Cargo process messages, CLI parsing, and desktop presentation remain Adapter concerns.

mod check;
mod creation;
mod dependency;
mod export;
mod publication;
mod repair;
mod starter;

pub use crate::project_host::{
    StartupContentClosureReceipt, StartupContentMemberKind, StartupContentMemberReceipt,
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
    LocalDependencyParent, NaraDependencyReceipt, NaraDependencySource, NaraDependencySourceError,
    NaraDependencySourceErrorKind, NaraDependencySourceKind,
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

/// Adapter-only construction surface for the first-party Cargo integration.
///
/// This Module is not a product extension seam. Public workflow consumers receive the resulting
/// opaque build receipt and never parse Cargo JSON or import executable native paths themselves.
#[doc(hidden)]
#[cfg(feature = "project-workflow-cli")]
pub mod __private {
    pub use super::export::{
        ProjectExportBuildPreparation, ProjectExportCargoArtifact, ProjectExportProductAgreement,
    };
}
