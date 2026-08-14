//! First-party project creation, checking, export, and repair transactions.
//!
//! The initial public surface owns only the immutable `desktop-2d/v1` creation plan. Native paths,
//! Cargo process messages, CLI parsing, and desktop presentation remain Adapter concerns.

mod creation;
mod dependency;
mod starter;

pub use creation::{
    PreparedProjectCreation, ProjectCreationPlanError, ProjectCreationPlanErrorKind,
    ProjectCreationRequest, prepare_desktop_2d_project,
};
pub use dependency::{
    LocalDependencyParent, NaraDependencyReceipt, NaraDependencySource, NaraDependencySourceError,
    NaraDependencySourceErrorKind, NaraDependencySourceKind,
};
pub use starter::{
    ProjectCreationPlan, ProjectDirectory, ProjectMember, ProjectMemberPlan, ProjectSlug,
    ProjectSlugError, ProjectSlugErrorKind, ProjectStarter,
};
