use std::{error::Error, fmt};

use nara_fs::{ContentDigest, DirectoryCapability, RelativePath};

use super::{
    ProjectCandidateError, ProjectContentError, ProjectContentLoader, ProjectContentSnapshot,
    RuntimePlan, RuntimePlanError,
    composition::{RuntimePluginOverlay, resolve_product_recipe_with_overlay},
    ingest_project_manifest, ingest_project_manifest_with_expected_digest,
};
use crate::ProductRecipe;

const PROJECT_MANIFEST: &str = "nara.toml";

pub(crate) struct CheckedProjectSource {
    plan: RuntimePlan,
    snapshot: ProjectContentSnapshot,
}

impl CheckedProjectSource {
    pub(crate) fn into_parts(self) -> (RuntimePlan, ProjectContentSnapshot) {
        (self.plan, self.snapshot)
    }
}

pub(crate) enum ProjectSourceCheckError {
    Manifest(ProjectCandidateError),
    ProductComposition(RuntimePlanError),
    StartupContent(ProjectContentError),
}

impl fmt::Debug for ProjectSourceCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => formatter.debug_tuple("Manifest").field(error).finish(),
            Self::ProductComposition(error) => formatter
                .debug_tuple("ProductComposition")
                .field(error)
                .finish(),
            Self::StartupContent(error) => formatter
                .debug_tuple("StartupContent")
                .field(error)
                .finish(),
        }
    }
}

impl fmt::Display for ProjectSourceCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Manifest(_) => "project manifest check failed",
            Self::ProductComposition(_) => "project product composition check failed",
            Self::StartupContent(_) => "project startup content check failed",
        })
    }
}

impl Error for ProjectSourceCheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Manifest(error) => Some(error),
            Self::ProductComposition(error) => Some(error),
            Self::StartupContent(error) => Some(error),
        }
    }
}

pub(crate) fn check_project_source(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    check_project_source_inner(project_root, profile, recipe, None, None, None)
}

pub(crate) fn check_project_source_with_overlay(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    check_project_source_inner(project_root, profile, recipe, overlay, None, None)
}

pub(crate) fn check_project_source_with_expected_digests(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    manifest_digest: ContentDigest,
    startup_scene_digest: ContentDigest,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    check_project_source_inner(
        project_root,
        profile,
        recipe,
        None,
        Some(manifest_digest),
        Some(startup_scene_digest),
    )
}

fn check_project_source_inner(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    manifest_digest: Option<ContentDigest>,
    startup_scene_digest: Option<ContentDigest>,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    let manifest_path = RelativePath::new(PROJECT_MANIFEST)
        .expect("the engine-owned manifest name is a valid relative path");
    let manifest = project_root.open_file(&manifest_path).map_err(|error| {
        ProjectSourceCheckError::Manifest(ProjectCandidateError::from_manifest_authority(error))
    })?;
    let candidate = match manifest_digest {
        Some(expected) => {
            ingest_project_manifest_with_expected_digest(&manifest, profile, expected)
        }
        None => ingest_project_manifest(&manifest, profile),
    }
    .map_err(ProjectSourceCheckError::Manifest)?;
    drop(manifest);

    let plan = resolve_product_recipe_with_overlay(&candidate, recipe, overlay)
        .map_err(ProjectSourceCheckError::ProductComposition)?;
    let loader =
        ProjectContentLoader::new(project_root).map_err(ProjectSourceCheckError::StartupContent)?;
    let snapshot = match startup_scene_digest {
        Some(expected) => {
            loader.load_with_expected_startup_scene_digest(&candidate, &plan, expected)
        }
        None => loader.load(&candidate, &plan),
    }
    .map_err(ProjectSourceCheckError::StartupContent)?;

    Ok(CheckedProjectSource { plan, snapshot })
}
