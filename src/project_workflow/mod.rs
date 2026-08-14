//! First-party project creation, checking, export, and repair transactions.
//!
//! The initial public surface owns only the immutable `desktop-2d/v1` creation plan. Native paths,
//! Cargo process messages, CLI parsing, and desktop presentation remain Adapter concerns.

mod creation;
mod dependency;
mod publication;
mod starter;

pub use creation::{
    PreparedProjectCreation, ProjectCreationPlanError, ProjectCreationPlanErrorKind,
    ProjectCreationRequest, prepare_desktop_2d_project,
};
pub use dependency::{
    LocalDependencyParent, NaraDependencyReceipt, NaraDependencySource, NaraDependencySourceError,
    NaraDependencySourceErrorKind, NaraDependencySourceKind,
};
pub use publication::{
    ProjectCreationAdmissionError, ProjectCreationAdmissionErrorKind, ProjectCreationOrphan,
    ProjectCreationOutcome, ProjectCreationReceipt, ProjectCreationRejection,
    ProjectCreationRejectionKind, ProjectCreationUnresolved, ProjectTreeStageError,
    PublishedButUnadmitted,
};
pub use starter::{
    ProjectCreationPlan, ProjectCreationPlanReceipt, ProjectDirectory, ProjectMember,
    ProjectMemberPlan, ProjectMemberReceipt, ProjectSlug, ProjectSlugError, ProjectSlugErrorKind,
    ProjectStarter,
};
