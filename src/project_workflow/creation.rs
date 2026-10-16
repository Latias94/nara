use nara_fs::{DirectoryCapability, RelativeComponent};
use thiserror::Error;

use super::{
    dependency::{
        DependencyPreparationError, NaraDependencySource, PreparedDependency,
        PreparedDependencyRevalidation, prepare_dependency,
    },
    starter::{ProjectCreationPlan, ProjectSlug, build_desktop_2d_v1_plan},
};

pub struct ProjectCreationRequest {
    publication_parent: DirectoryCapability,
    slug: ProjectSlug,
    dependency: NaraDependencySource,
}

impl ProjectCreationRequest {
    #[must_use]
    pub fn new(
        publication_parent: DirectoryCapability,
        slug: ProjectSlug,
        dependency: NaraDependencySource,
    ) -> Self {
        Self {
            publication_parent,
            slug,
            dependency,
        }
    }
}

pub struct PreparedProjectCreation {
    pub(crate) publication_parent: DirectoryCapability,
    pub(crate) destination: RelativeComponent,
    pub(crate) plan: ProjectCreationPlan,
    pub(crate) dependency_revalidation: PreparedDependencyRevalidation,
}

impl PreparedProjectCreation {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlan {
        &self.plan
    }

    #[must_use = "the project-creation terminal outcome must be handled"]
    pub fn publish(self) -> super::publication::ProjectCreationOutcome {
        super::publication::publish_prepared(self)
    }
}

impl std::fmt::Debug for PreparedProjectCreation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedProjectCreation")
            .field("parent_identity", &self.publication_parent.identity())
            .field("destination", &self.destination)
            .field("plan", &self.plan)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectCreationPlanErrorKind {
    LocalCheckoutUnavailable,
    LocalParentUnavailable,
    LocalParentMismatch,
    NonUtf8LocalCoordinate,
    StarterContract,
}

#[derive(Debug, Error)]
pub enum ProjectCreationPlanError {
    #[error("local dependency checkout could not be resolved through the common ancestor")]
    LocalCheckoutUnavailable(#[source] nara_fs::FsError),
    #[error("local dependency destination parent could not be resolved")]
    LocalParentUnavailable(#[source] nara_fs::FsError),
    #[error("local dependency destination parent does not match the common-ancestor relation")]
    LocalParentMismatch,
    #[error("local dependency coordinate is not UTF-8 and cannot be represented in Cargo.toml")]
    NonUtf8LocalCoordinate,
    #[error("desktop-2d/v1 starter contract is invalid: {0}")]
    StarterContract(&'static str),
}

impl ProjectCreationPlanError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCreationPlanErrorKind {
        match self {
            Self::LocalCheckoutUnavailable(_) => {
                ProjectCreationPlanErrorKind::LocalCheckoutUnavailable
            }
            Self::LocalParentUnavailable(_) => ProjectCreationPlanErrorKind::LocalParentUnavailable,
            Self::LocalParentMismatch => ProjectCreationPlanErrorKind::LocalParentMismatch,
            Self::NonUtf8LocalCoordinate => ProjectCreationPlanErrorKind::NonUtf8LocalCoordinate,
            Self::StarterContract(_) => ProjectCreationPlanErrorKind::StarterContract,
        }
    }
}

pub fn prepare_desktop_2d_project(
    request: ProjectCreationRequest,
) -> Result<PreparedProjectCreation, ProjectCreationPlanError> {
    let ProjectCreationRequest {
        publication_parent,
        slug,
        dependency,
    } = request;
    let destination = slug.destination_component();
    let dependency =
        prepare_dependency(dependency, &publication_parent).map_err(|error| match error {
            DependencyPreparationError::LocalCheckoutUnavailable(error) => {
                ProjectCreationPlanError::LocalCheckoutUnavailable(error)
            }
            DependencyPreparationError::LocalParentUnavailable(error) => {
                ProjectCreationPlanError::LocalParentUnavailable(error)
            }
            DependencyPreparationError::LocalParentMismatch => {
                ProjectCreationPlanError::LocalParentMismatch
            }
            DependencyPreparationError::NonUtf8LocalCoordinate => {
                ProjectCreationPlanError::NonUtf8LocalCoordinate
            }
        })?;
    let PreparedDependency {
        manifest_fragment,
        creation_command_arguments,
        receipt,
        revalidation: dependency_revalidation,
    } = dependency;
    let plan = build_desktop_2d_v1_plan(
        slug,
        &manifest_fragment,
        &creation_command_arguments,
        receipt,
    )
    .map_err(ProjectCreationPlanError::StarterContract)?;
    Ok(PreparedProjectCreation {
        publication_parent,
        destination,
        plan,
        dependency_revalidation,
    })
}
