use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_identity::SceneEntityId;

/// Stable provenance for every entity in one expanded scene document.
pub type SceneEntityProvenanceMap = BTreeMap<SceneEntityId, SceneEntityProvenance>;

/// One source document/entity step in a concrete prefab projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabProjectionStep {
    source_document: AssetRef,
    source_entity: SceneEntityId,
}

impl PrefabProjectionStep {
    pub(crate) const fn new(source_document: AssetRef, source_entity: SceneEntityId) -> Self {
        Self {
            source_document,
            source_entity,
        }
    }

    /// Returns the prefab source that owns this projection step.
    #[must_use]
    pub const fn source_document(&self) -> &AssetRef {
        &self.source_document
    }

    /// Returns the entity identity inside the owning prefab source.
    #[must_use]
    pub const fn source_entity(&self) -> &SceneEntityId {
        &self.source_entity
    }
}

/// Structured location of one prefab projection inside a containing scene.
///
/// The root anchor and source chain remain separate from the expanded runtime locator. Consumers
/// must not parse the transitional `anchor/source_entity` encoding to reconstruct this value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabProjectionPath {
    root_anchor: SceneEntityId,
    source_chain: Vec<PrefabProjectionStep>,
}

impl PrefabProjectionPath {
    pub(crate) const fn new(root_anchor: SceneEntityId) -> Self {
        Self {
            root_anchor,
            source_chain: Vec::new(),
        }
    }

    pub(crate) fn with_step(
        mut self,
        source_document: AssetRef,
        source_entity: SceneEntityId,
    ) -> Self {
        self.source_chain
            .push(PrefabProjectionStep::new(source_document, source_entity));
        self
    }

    /// Returns the authored anchor in the containing scene document.
    #[must_use]
    pub const fn root_anchor(&self) -> &SceneEntityId {
        &self.root_anchor
    }

    /// Returns the exact outer-to-inner prefab source chain.
    #[must_use]
    pub fn source_chain(&self) -> &[PrefabProjectionStep] {
        &self.source_chain
    }
}

/// Source identity carried beside one materialized scene entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEntityProvenance {
    source_entity_id: SceneEntityId,
    projection_path: Option<PrefabProjectionPath>,
}

impl SceneEntityProvenance {
    pub(crate) const fn local(source_entity_id: SceneEntityId) -> Self {
        Self {
            source_entity_id,
            projection_path: None,
        }
    }

    pub(crate) fn projected(
        source_entity_id: SceneEntityId,
        mut projection_path: PrefabProjectionPath,
    ) -> Self {
        projection_path.source_chain.shrink_to_fit();
        Self {
            source_entity_id,
            projection_path: Some(projection_path),
        }
    }

    /// Returns the opaque entity identity in its owning source document.
    #[must_use]
    pub const fn source_entity_id(&self) -> &SceneEntityId {
        &self.source_entity_id
    }

    /// Returns the structured prefab location, or `None` for a scene-local entity.
    #[must_use]
    pub const fn projection_path(&self) -> Option<&PrefabProjectionPath> {
        self.projection_path.as_ref()
    }
}
