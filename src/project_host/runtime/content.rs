//! Closed source/processed input retained by the existing Runtime and Scene owners.

use std::sync::Arc;

use nara_scene::{SceneComponentBindings, SceneDocument};

use crate::{
    project_content::{ProjectContentSnapshot, ProjectDomainAssets},
    runtime_content::ProcessedSceneContent,
    startup_scene::SceneSource,
};

#[derive(Clone)]
pub(super) enum PreparedSceneContent {
    Source(ProjectContentSnapshot),
    Processed(Arc<ProcessedSceneContent>),
}

impl PreparedSceneContent {
    pub(super) fn scene_source(&self) -> SceneSource {
        match self {
            Self::Source(snapshot) => snapshot.scene_source(),
            Self::Processed(content) => {
                SceneSource::from_hosted_runtime_content(content.scene.clone())
            }
        }
    }

    pub(super) fn document(&self) -> &SceneDocument {
        match self {
            Self::Source(snapshot) => snapshot.expanded_scene(),
            Self::Processed(content) => content.scene.candidate().document(),
        }
    }

    pub(super) fn domain_assets(&self) -> &ProjectDomainAssets {
        match self {
            Self::Source(snapshot) => snapshot.domain_assets(),
            Self::Processed(content) => &content.domains,
        }
    }

    pub(super) fn shares_component_bindings(&self, bindings: &SceneComponentBindings) -> bool {
        match self {
            Self::Source(snapshot) => snapshot.shares_component_bindings(bindings),
            Self::Processed(content) => content.scene.candidate().definitions().matches(bindings),
        }
    }
}
