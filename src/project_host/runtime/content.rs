//! Closed source/processed input retained by the existing Runtime and Scene owners.

use std::sync::Arc;

use nara_asset::{AssetProductRef, AssetRef, ImportedProductId};
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

    pub(super) fn scene_dependency(&self, product: AssetProductRef) -> Option<&SceneDocument> {
        if product.product() != ImportedProductId::PRIMARY {
            return None;
        }
        match self {
            Self::Source(snapshot) => {
                let record = snapshot
                    .asset_resolution()
                    .resolve_ref(&AssetRef::Product(product))
                    .ok()?;
                snapshot
                    .scene_dependencies()
                    .iter()
                    .find(|scene| scene.path() == record.path())
                    .map(|scene| scene.expanded_scene())
            }
            Self::Processed(content) => {
                let member = content.base().member(product)?;
                content
                    .dependencies
                    .iter()
                    .find(|scene| {
                        content
                            .base()
                            .member(scene.product())
                            .is_some_and(|candidate| std::ptr::eq(candidate, member))
                    })
                    .map(|scene| scene.candidate().document())
            }
        }
    }

    pub(super) fn shares_component_bindings(&self, bindings: &SceneComponentBindings) -> bool {
        match self {
            Self::Source(snapshot) => snapshot.shares_component_bindings(bindings),
            Self::Processed(content) => content.scene.candidate().definitions().matches(bindings),
        }
    }
}
