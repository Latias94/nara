use std::{error::Error, fmt};

use nara_fs::DirectoryCapability;

use crate::{
    ProductRecipe,
    project_host::{
        ProjectCandidateError, ProjectContentError, RuntimePlanError, StartupContentClosureReceipt,
        check::{ProjectSourceCheckError, check_project_source},
    },
};

/// Inputs for one authoritative startup-content check.
pub struct ProjectCheckRequest {
    project_root: DirectoryCapability,
    profile: Option<String>,
    recipe: ProductRecipe,
}

impl ProjectCheckRequest {
    #[must_use]
    pub fn new(project_root: DirectoryCapability, recipe: ProductRecipe) -> Self {
        Self {
            project_root,
            profile: None,
            recipe,
        }
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }
}

impl fmt::Debug for ProjectCheckRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCheckRequest")
            .field("project_root_identity", &self.project_root.identity())
            .field("profile_present", &self.profile.is_some())
            .field("recipe", &self.recipe)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCheckErrorKind {
    Manifest,
    ProductComposition,
    StartupContent,
}

pub struct ProjectCheckError {
    inner: ProjectCheckErrorInner,
}

enum ProjectCheckErrorInner {
    Manifest(ProjectCandidateError),
    ProductComposition(RuntimePlanError),
    StartupContent(ProjectContentError),
}

impl ProjectCheckError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCheckErrorKind {
        match &self.inner {
            ProjectCheckErrorInner::Manifest(_) => ProjectCheckErrorKind::Manifest,
            ProjectCheckErrorInner::ProductComposition(_) => {
                ProjectCheckErrorKind::ProductComposition
            }
            ProjectCheckErrorInner::StartupContent(_) => ProjectCheckErrorKind::StartupContent,
        }
    }

    #[must_use]
    pub const fn manifest_error(&self) -> Option<&ProjectCandidateError> {
        match &self.inner {
            ProjectCheckErrorInner::Manifest(error) => Some(error),
            ProjectCheckErrorInner::ProductComposition(_)
            | ProjectCheckErrorInner::StartupContent(_) => None,
        }
    }

    #[must_use]
    pub const fn product_composition_error(&self) -> Option<&RuntimePlanError> {
        match &self.inner {
            ProjectCheckErrorInner::ProductComposition(error) => Some(error),
            ProjectCheckErrorInner::Manifest(_) | ProjectCheckErrorInner::StartupContent(_) => None,
        }
    }

    #[must_use]
    pub const fn startup_content_error(&self) -> Option<&ProjectContentError> {
        match &self.inner {
            ProjectCheckErrorInner::StartupContent(error) => Some(error),
            ProjectCheckErrorInner::Manifest(_) | ProjectCheckErrorInner::ProductComposition(_) => {
                None
            }
        }
    }
}

impl From<ProjectSourceCheckError> for ProjectCheckError {
    fn from(error: ProjectSourceCheckError) -> Self {
        let inner = match error {
            ProjectSourceCheckError::Manifest(error) => ProjectCheckErrorInner::Manifest(error),
            ProjectSourceCheckError::ProductComposition(error) => {
                ProjectCheckErrorInner::ProductComposition(error)
            }
            ProjectSourceCheckError::StartupContent(error) => {
                ProjectCheckErrorInner::StartupContent(error)
            }
        };
        Self { inner }
    }
}

impl fmt::Debug for ProjectCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCheckError")
            .field("kind", &self.kind())
            .field("source", &self.source())
            .finish()
    }
}

impl fmt::Display for ProjectCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "project check failed: {:?}", self.kind())
    }
}

impl Error for ProjectCheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.inner {
            ProjectCheckErrorInner::Manifest(error) => Some(error),
            ProjectCheckErrorInner::ProductComposition(error) => Some(error),
            ProjectCheckErrorInner::StartupContent(error) => Some(error),
        }
    }
}

/// Validates one authorized project and returns its exact admitted startup source closure.
pub fn check_project(
    request: ProjectCheckRequest,
) -> Result<StartupContentClosureReceipt, ProjectCheckError> {
    let ProjectCheckRequest {
        project_root,
        profile,
        recipe,
    } = request;
    let checked = check_project_source(project_root, profile.as_deref(), recipe)?;
    let (_, snapshot) = checked.into_parts();
    Ok(snapshot.startup_content_closure())
}
