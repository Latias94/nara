use std::{error::Error, fmt};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, RelativePath};
use nara_project::ProjectPath;

use super::{
    ProjectCandidateError, ProjectContentError, ProjectContentErrorKind, ProjectContentLoader,
    ProjectContentSnapshot, ProjectSettingsCandidate, ProjectSettingsLineage, RuntimePlan,
    RuntimePlanError,
    composition::{
        RuntimeHostPolicyContribution, RuntimePluginOverlay,
        resolve_product_recipe_with_host_policy, resolve_product_recipe_with_overlay,
    },
    ingest_project_manifest, ingest_project_manifest_bytes,
    ingest_project_manifest_with_expected_digest,
    runtime_agreement::prepare_runtime_schema_agreement,
};
use crate::ProductRecipe;

const PROJECT_MANIFEST: &str = "nara.toml";

#[derive(Default)]
struct ProjectSourceCheckOptions {
    overlay: Option<RuntimePluginOverlay>,
    host_policy: Option<RuntimeHostPolicyContribution>,
    manifest_digest: Option<ContentDigest>,
    startup_scene_digest: Option<ContentDigest>,
}

pub(crate) struct CheckedProjectSource {
    candidate: ProjectSettingsCandidate,
    loader: ProjectContentLoader,
    plan: RuntimePlan,
    snapshot: ProjectContentSnapshot,
}

impl CheckedProjectSource {
    pub(crate) fn into_parts(self) -> (RuntimePlan, ProjectContentSnapshot) {
        (self.plan, self.snapshot)
    }

    pub(crate) fn into_delivery_parts(
        self,
    ) -> (
        ProjectSettingsCandidate,
        RuntimePlan,
        ProjectContentSnapshot,
        ProjectContentLoader,
    ) {
        (self.candidate, self.plan, self.snapshot, self.loader)
    }
}

pub(crate) enum ProjectSourceCheckError {
    Manifest(ProjectCandidateError),
    ProductComposition(RuntimePlanError),
    StartupContent {
        error: ProjectContentError,
        missing_startup_scene: Option<MissingStartupSceneContext>,
    },
}

pub(crate) struct MissingStartupSceneContext {
    pub(crate) manifest_identity: FileIdentity,
    pub(crate) manifest_digest: ContentDigest,
    pub(crate) settings_lineage: ProjectSettingsLineage,
    pub(crate) scenes_path: ProjectPath,
    pub(crate) selected_scene: ProjectPath,
}

impl fmt::Debug for ProjectSourceCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => formatter.debug_tuple("Manifest").field(error).finish(),
            Self::ProductComposition(error) => formatter
                .debug_tuple("ProductComposition")
                .field(error)
                .finish(),
            Self::StartupContent { error, .. } => formatter
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
            Self::StartupContent { .. } => "project startup content check failed",
        })
    }
}

impl Error for ProjectSourceCheckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Manifest(error) => Some(error),
            Self::ProductComposition(error) => Some(error),
            Self::StartupContent { error, .. } => Some(error),
        }
    }
}

pub(crate) fn check_project_source(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    check_project_source_inner(
        project_root,
        profile,
        recipe,
        ProjectSourceCheckOptions::default(),
    )
}

pub(crate) fn check_project_source_with_overlay(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    check_project_source_inner(
        project_root,
        profile,
        recipe,
        ProjectSourceCheckOptions {
            overlay,
            ..ProjectSourceCheckOptions::default()
        },
    )
}

pub(crate) fn check_project_source_with_host_policy(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    overlay: Option<RuntimePluginOverlay>,
    host_policy: RuntimeHostPolicyContribution,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    check_project_source_inner(
        project_root,
        profile,
        recipe,
        ProjectSourceCheckOptions {
            overlay,
            host_policy: Some(host_policy),
            ..ProjectSourceCheckOptions::default()
        },
    )
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
        ProjectSourceCheckOptions {
            manifest_digest: Some(manifest_digest),
            startup_scene_digest: Some(startup_scene_digest),
            ..ProjectSourceCheckOptions::default()
        },
    )
}

pub(crate) fn check_project_source_from_manifest_bytes(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    manifest_bytes: &[u8],
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    let candidate = ingest_project_manifest_bytes(manifest_bytes, profile, project_root.identity())
        .map_err(ProjectSourceCheckError::Manifest)?;
    load_checked_candidate(
        project_root,
        recipe,
        ProjectSourceCheckOptions::default(),
        candidate,
        None,
    )
}

fn check_project_source_inner(
    project_root: DirectoryCapability,
    profile: Option<&str>,
    recipe: ProductRecipe,
    options: ProjectSourceCheckOptions,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    let manifest_path = RelativePath::new(PROJECT_MANIFEST)
        .expect("the engine-owned manifest name is a valid relative path");
    let manifest = project_root.open_file(&manifest_path).map_err(|error| {
        ProjectSourceCheckError::Manifest(ProjectCandidateError::from_manifest_authority(error))
    })?;
    let manifest_identity = manifest.identity();
    let candidate = match options.manifest_digest {
        Some(expected) => {
            ingest_project_manifest_with_expected_digest(&manifest, profile, expected)
        }
        None => ingest_project_manifest(&manifest, profile),
    }
    .map_err(ProjectSourceCheckError::Manifest)?;
    drop(manifest);

    load_checked_candidate(
        project_root,
        recipe,
        options,
        candidate,
        Some(manifest_identity),
    )
}

fn load_checked_candidate(
    project_root: DirectoryCapability,
    recipe: ProductRecipe,
    options: ProjectSourceCheckOptions,
    candidate: ProjectSettingsCandidate,
    manifest_identity: Option<FileIdentity>,
) -> Result<CheckedProjectSource, ProjectSourceCheckError> {
    let plan = match options.host_policy {
        Some(host_policy) => resolve_product_recipe_with_host_policy(
            &candidate,
            recipe,
            options.overlay,
            host_policy,
        ),
        None => resolve_product_recipe_with_overlay(&candidate, recipe, options.overlay),
    }
    .map_err(ProjectSourceCheckError::ProductComposition)?;
    let runtime_agreement = prepare_runtime_schema_agreement(&project_root, &candidate, &plan)
        .map_err(|error| startup_content_failure(error, &candidate, manifest_identity))?;
    let loader = ProjectContentLoader::new(project_root).map_err(|error| {
        ProjectSourceCheckError::StartupContent {
            error,
            missing_startup_scene: None,
        }
    })?;
    let snapshot = match options.startup_scene_digest {
        Some(expected) => loader.load_with_expected_startup_scene_digest_and_runtime_agreement(
            &candidate,
            &plan,
            expected,
            runtime_agreement,
        ),
        None => loader.load_with_runtime_agreement(&candidate, &plan, runtime_agreement),
    }
    .map_err(|error| startup_content_failure(error, &candidate, manifest_identity))?;

    Ok(CheckedProjectSource {
        candidate,
        loader,
        plan,
        snapshot,
    })
}

fn startup_content_failure(
    error: ProjectContentError,
    candidate: &ProjectSettingsCandidate,
    manifest_identity: Option<FileIdentity>,
) -> ProjectSourceCheckError {
    let missing_startup_scene = if error.kind() == ProjectContentErrorKind::MissingStartupScene {
        manifest_identity.and_then(|manifest_identity| {
            candidate
                .settings()
                .startup
                .default_scene
                .as_ref()
                .map(|selected_scene| MissingStartupSceneContext {
                    manifest_identity,
                    manifest_digest: candidate.manifest_digest(),
                    settings_lineage: candidate.lineage(),
                    scenes_path: candidate.settings().paths.scenes.clone(),
                    selected_scene: selected_scene.clone(),
                })
        })
    } else {
        None
    };
    ProjectSourceCheckError::StartupContent {
        error,
        missing_startup_scene,
    }
}
