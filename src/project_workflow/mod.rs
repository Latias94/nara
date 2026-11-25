//! Strict authored-content checks, cooked content assembly, and repair transactions.
//!
//! Native process execution, CLI parsing and desktop presentation remain Adapter concerns.
//! The `cargo` submodule supplies bounded pure facts for explicitly selected trusted builds;
//! build output does not grant product activation or Runtime readiness.

#[cfg(feature = "project-workflow-cargo-facts")]
#[path = "cargo_facts.rs"]
pub mod cargo;
mod check;
pub mod cooked_content;
mod repair;

pub use crate::project_host::{
    ProjectContentMemberKind, ProjectContentMemberReceipt, StartupContentClosureReceipt,
};
pub use check::{
    ProjectCheckError, ProjectCheckErrorKind, ProjectCheckRequest, ProjectStartupSceneIssue,
    check_project,
};
pub use repair::{
    ProjectStartupScenePostcheckError, ProjectStartupScenePublicationReceipt,
    ProjectStartupSceneRepairError, ProjectStartupSceneRepairOutcome,
    ProjectStartupSceneRepairReceipt, ProjectStartupSceneRepairRequest, PublishedButNotRepaired,
    repair_startup_scene,
};
