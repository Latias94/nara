use std::{error::Error, fmt};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity};
use nara_project::ProjectPath;

use crate::{
    ProductRecipe,
    project_host::{
        ProjectCandidateError, ProjectContentDeliveryReceipt, ProjectContentError,
        ProjectSettingsLineage, RuntimePlanError,
        check::{MissingStartupSceneContext, ProjectSourceCheckError, check_project_source},
    },
};

/// Inputs for one authoritative check of startup and all declared Scene roots.
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
    Content,
}

pub struct ProjectCheckError {
    inner: ProjectCheckErrorInner,
    startup_scene_issue: Option<ProjectStartupSceneIssue>,
}

enum ProjectCheckErrorInner {
    Manifest(ProjectCandidateError),
    ProductComposition(RuntimePlanError),
    Content(ProjectContentError),
}

impl ProjectCheckError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCheckErrorKind {
        match &self.inner {
            ProjectCheckErrorInner::Manifest(_) => ProjectCheckErrorKind::Manifest,
            ProjectCheckErrorInner::ProductComposition(_) => {
                ProjectCheckErrorKind::ProductComposition
            }
            ProjectCheckErrorInner::Content(_) => ProjectCheckErrorKind::Content,
        }
    }

    #[must_use]
    pub const fn manifest_error(&self) -> Option<&ProjectCandidateError> {
        match &self.inner {
            ProjectCheckErrorInner::Manifest(error) => Some(error),
            ProjectCheckErrorInner::ProductComposition(_) | ProjectCheckErrorInner::Content(_) => {
                None
            }
        }
    }

    #[must_use]
    pub const fn product_composition_error(&self) -> Option<&RuntimePlanError> {
        match &self.inner {
            ProjectCheckErrorInner::ProductComposition(error) => Some(error),
            ProjectCheckErrorInner::Manifest(_) | ProjectCheckErrorInner::Content(_) => None,
        }
    }

    #[must_use]
    pub const fn content_error(&self) -> Option<&ProjectContentError> {
        match &self.inner {
            ProjectCheckErrorInner::Content(error) => Some(error),
            ProjectCheckErrorInner::Manifest(_) | ProjectCheckErrorInner::ProductComposition(_) => {
                None
            }
        }
    }

    /// Returns the actionable issue only when the selected startup Scene is absent.
    #[must_use]
    pub const fn startup_scene_issue(&self) -> Option<&ProjectStartupSceneIssue> {
        self.startup_scene_issue.as_ref()
    }

    /// Consumes the check failure and returns its session-scoped repair evidence, if any.
    #[must_use]
    pub fn into_startup_scene_issue(self) -> Option<ProjectStartupSceneIssue> {
        self.startup_scene_issue
    }

    pub(super) fn from_source(
        error: ProjectSourceCheckError,
        project_root: DirectoryCapability,
        profile: Option<String>,
        recipe: ProductRecipe,
    ) -> Self {
        let (inner, missing_startup_scene) = match error {
            ProjectSourceCheckError::Manifest(error) => {
                (ProjectCheckErrorInner::Manifest(error), None)
            }
            ProjectSourceCheckError::ProductComposition(error) => {
                (ProjectCheckErrorInner::ProductComposition(error), None)
            }
            ProjectSourceCheckError::StartupContent {
                error,
                missing_startup_scene,
            } => (
                ProjectCheckErrorInner::Content(error),
                missing_startup_scene,
            ),
        };
        let startup_scene_issue = missing_startup_scene
            .map(|context| ProjectStartupSceneIssue::new(project_root, profile, recipe, context));
        Self {
            inner,
            startup_scene_issue,
        }
    }
}

/// Session-scoped evidence that one exact manifest selects a missing startup Scene.
pub struct ProjectStartupSceneIssue {
    pub(super) project_root: DirectoryCapability,
    pub(super) profile: Option<String>,
    pub(super) recipe: ProductRecipe,
    pub(super) manifest_identity: FileIdentity,
    pub(super) manifest_digest: ContentDigest,
    pub(super) settings_lineage: ProjectSettingsLineage,
    pub(super) scenes_path: ProjectPath,
    pub(super) selected_scene: ProjectPath,
}

impl ProjectStartupSceneIssue {
    fn new(
        project_root: DirectoryCapability,
        profile: Option<String>,
        recipe: ProductRecipe,
        context: MissingStartupSceneContext,
    ) -> Self {
        Self {
            project_root,
            profile,
            recipe,
            manifest_identity: context.manifest_identity,
            manifest_digest: context.manifest_digest,
            settings_lineage: context.settings_lineage,
            scenes_path: context.scenes_path,
            selected_scene: context.selected_scene,
        }
    }

    #[must_use]
    pub fn project_root_identity(&self) -> FileIdentity {
        self.project_root.identity()
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub const fn scenes_path(&self) -> &ProjectPath {
        &self.scenes_path
    }

    #[must_use]
    pub const fn selected_scene(&self) -> &ProjectPath {
        &self.selected_scene
    }

    #[cfg(feature = "project-tooling")]
    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

impl fmt::Debug for ProjectStartupSceneIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectStartupSceneIssue")
            .field("project_root_identity", &self.project_root.identity())
            .field("profile_present", &self.profile.is_some())
            .field("manifest_identity", &self.manifest_identity)
            .field("manifest_digest", &self.manifest_digest)
            .field("scenes_path", &self.scenes_path)
            .field("selected_scene", &self.selected_scene)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for ProjectCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCheckError")
            .field("kind", &self.kind())
            .field("startup_scene_issue", &self.startup_scene_issue.is_some())
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
            ProjectCheckErrorInner::Content(error) => Some(error),
        }
    }
}

/// Validates startup and all declared Scene roots, returning their exact delivery source union.
pub fn check_project(
    request: ProjectCheckRequest,
) -> Result<ProjectContentDeliveryReceipt, ProjectCheckError> {
    let ProjectCheckRequest {
        project_root,
        profile,
        recipe,
    } = request;
    let issue_project_root = project_root.share();
    let issue_profile = profile.clone();
    let issue_recipe = recipe.clone();
    let checked =
        check_project_source(project_root, profile.as_deref(), recipe).map_err(|error| {
            ProjectCheckError::from_source(error, issue_project_root, issue_profile, issue_recipe)
        })?;
    checked.into_delivery().map_err(|error| ProjectCheckError {
        inner: ProjectCheckErrorInner::Content(error),
        startup_scene_issue: None,
    })
}
