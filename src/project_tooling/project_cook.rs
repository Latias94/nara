//! Compiled project cooking through the original source, artifact, package and Runtime owners.

mod content;
mod package;

use crate::{
    ProductRecipe,
    asset_build::{
        ArtifactBuildError, ArtifactBuildObservation, ArtifactBuildStore, ArtifactContentDigest,
    },
    fs::{DirectoryCapability, FsError, PathValidationError},
    project_host::{ProjectContentDeliveryReceipt, ProjectContentError, ProjectSettingsCandidate},
    project_workflow::cooked_content::{
        CookedContentError, CookedContentLimits, PreparedRuntimeContent,
    },
    runtime_content::RuntimeContentTarget,
};

pub use content::{
    ProjectCookContext, prepare_project_cooked_content, prepare_project_server_content,
};
pub use package::{
    PreparedProjectPlayerPackage, ProjectCookSourceSelection, project_rust_input_inventory,
};

/// Explicit project, executable recipe, target and existing build-store selection.
///
/// The callback passed to [`prepare_project_cooked_content`] selects typed build contributions.
/// Source-only import and a domain's separate build operation keep their distinct contracts.
pub struct ProjectCookRequest<'a> {
    pub root: &'a DirectoryCapability,
    pub candidate: &'a ProjectSettingsCandidate,
    pub recipe: ProductRecipe,
    pub target: RuntimeContentTarget,
    pub store: &'a ArtifactBuildStore,
    pub implementation: ArtifactContentDigest,
    pub limits: CookedContentLimits,
}

/// Selects the server Runtime from the same manifest and authored content as a Cook request.
///
/// Exclusions name admitted source component types. Every retained component must use a
/// compatible selected Schema and its normal dependencies. The server profile clears the
/// global action map and the recipe supplies semantic commands without the raw Input plugin.
pub struct ProjectServerCookTarget<'a> {
    pub candidate: &'a ProjectSettingsCandidate,
    pub recipe: ProductRecipe,
    pub omit_components: &'a [nara_reflect::ComponentTypeId],
}

/// Unpublished content together with the source selection used to prepare it.
///
/// No World, native resource, executable or installation is created by content preparation.
pub struct PreparedProjectCook {
    content: PreparedRuntimeContent,
    selection: ProjectCookSourceSelection,
}

impl PreparedProjectCook {
    #[must_use]
    pub fn content(&self) -> &PreparedRuntimeContent {
        &self.content
    }

    #[must_use]
    pub fn source(&self) -> &ProjectContentDeliveryReceipt {
        self.selection.delivery_receipt()
    }

    #[must_use]
    pub fn observations(&self) -> &[ArtifactBuildObservation] {
        self.selection.observations()
    }

    #[must_use]
    pub fn into_parts(self) -> (PreparedRuntimeContent, ProjectCookSourceSelection) {
        (self.content, self.selection)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectCookError {
    #[error("project source delivery failed: {0:?}")]
    Source(ProjectContentError),
    #[error("cooked content assembly failed: {0}")]
    Content(#[from] CookedContentError),
    #[error("typed asset build failed: {0}")]
    Artifact(#[from] ArtifactBuildError),
    #[error("Player package preparation failed: {0}")]
    Package(#[from] super::player_package::PlayerPackageError),
    #[error("Player Cargo request failed: {0}")]
    Cargo(#[from] super::cargo_build::CargoBuildFailure),
    #[error("project source access failed: {0}")]
    Filesystem(#[from] FsError),
    #[error("project input path failed: {0}")]
    Path(#[from] PathValidationError),
    #[error("{0}")]
    Invalid(String),
    #[error("the compiled project cook contribution panicked")]
    Panicked,
}
