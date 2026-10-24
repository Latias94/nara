use std::{collections::BTreeMap, mem::size_of};

use nara_asset::AssetRef;
use nara_identity::SceneEntityId;

use crate::SourceDocumentKey;

/// Stable provenance for every entity in one expanded scene document.
pub type SceneEntityProvenanceMap = BTreeMap<SceneEntityId, SceneEntityProvenance>;

// Logical ownership costs, including conservative per-string and tree-entry overhead. These
// match Scene retention's model; they do not describe allocator capacity or resident memory.
pub(crate) fn provenance_string_bytes(value: &str) -> Option<usize> {
    value.len().checked_add(2 * size_of::<usize>())
}

pub(crate) fn provenance_asset_ref_bytes(source: &AssetRef) -> Option<usize> {
    match source {
        AssetRef::Path(path) => provenance_string_bytes(path.as_str()),
        AssetRef::StableId(_) => Some(16),
    }
}

pub(crate) fn provenance_entry_bytes(
    expanded_entity: &SceneEntityId,
    source_entity: &SceneEntityId,
) -> Option<usize> {
    size_of::<SceneEntityId>()
        .checked_add(size_of::<SceneEntityProvenance>())?
        .checked_add(6 * size_of::<usize>())?
        .checked_add(provenance_string_bytes(expanded_entity.as_str())?)?
        .checked_add(provenance_string_bytes(source_entity.as_str())?)
}

/// One source document/entity step in a concrete prefab projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabProjectionStep {
    source_document: AssetRef,
    canonical_source: Option<SourceDocumentKey>,
    source_entity: SceneEntityId,
}

impl PrefabProjectionStep {
    fn allocation_bytes(
        source_document: &AssetRef,
        canonical_source: Option<&SourceDocumentKey>,
        source_entity: &SceneEntityId,
    ) -> Option<usize> {
        let key_bytes = match canonical_source {
            Some(key) => key.checked_retained_bytes()?,
            None => 0,
        };
        size_of::<Self>()
            .checked_add(provenance_asset_ref_bytes(source_document)?)?
            .checked_add(key_bytes)?
            .checked_add(provenance_string_bytes(source_entity.as_str())?)
    }

    pub(crate) const fn new(source_document: AssetRef, source_entity: SceneEntityId) -> Self {
        Self {
            source_document,
            canonical_source: None,
            source_entity,
        }
    }

    pub(crate) fn with_canonical_source(
        source_document: AssetRef,
        canonical_source: SourceDocumentKey,
        source_entity: SceneEntityId,
    ) -> Self {
        Self {
            source_document,
            canonical_source: Some(canonical_source),
            source_entity,
        }
    }

    /// Returns the prefab source that owns this projection step.
    #[must_use]
    pub const fn source_document(&self) -> &AssetRef {
        &self.source_document
    }

    /// Returns the admitted canonical source identity when the resolver supplied one.
    #[must_use]
    pub const fn canonical_source(&self) -> Option<&SourceDocumentKey> {
        self.canonical_source.as_ref()
    }

    /// Returns the entity identity inside the owning prefab source.
    #[must_use]
    pub const fn source_entity(&self) -> &SceneEntityId {
        &self.source_entity
    }
}

/// Structured location of one prefab projection inside a containing scene.
///
/// The root anchor and source chain remain separate from the private expanded runtime locator.
/// Consumers use this structured path rather than derive source identity from a generated ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabProjectionPath {
    root_anchor: SceneEntityId,
    source_chain: Vec<PrefabProjectionStep>,
}

impl PrefabProjectionPath {
    #[cfg(test)]
    pub(crate) const fn new(root_anchor: SceneEntityId) -> Self {
        Self {
            root_anchor,
            source_chain: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_step(
        mut self,
        source_document: AssetRef,
        source_entity: SceneEntityId,
    ) -> Self {
        self.source_chain
            .push(PrefabProjectionStep::new(source_document, source_entity));
        self
    }

    pub(crate) fn extended_allocation_bytes(
        parent: Option<&Self>,
        root_anchor: &SceneEntityId,
        source_document: &AssetRef,
        canonical_source: Option<&SourceDocumentKey>,
        source_entity: &SceneEntityId,
    ) -> Option<usize> {
        let root_anchor = parent.map_or(root_anchor, Self::root_anchor);
        let mut bytes = provenance_string_bytes(root_anchor.as_str())?;
        if let Some(parent) = parent {
            for step in &parent.source_chain {
                bytes = bytes.checked_add(PrefabProjectionStep::allocation_bytes(
                    &step.source_document,
                    step.canonical_source.as_ref(),
                    &step.source_entity,
                )?)?;
            }
        }
        bytes.checked_add(PrefabProjectionStep::allocation_bytes(
            source_document,
            canonical_source,
            source_entity,
        )?)
    }

    // Call only after admitting extended_allocation_bytes and the containing map entry.
    pub(crate) fn from_resolved_step(
        parent: Option<&Self>,
        root_anchor: &SceneEntityId,
        source_document: &AssetRef,
        canonical_source: Option<&SourceDocumentKey>,
        source_entity: &SceneEntityId,
    ) -> Self {
        let mut source_chain =
            Vec::with_capacity(parent.map_or(0, |path| path.source_chain.len()) + 1);
        if let Some(parent) = parent {
            source_chain.extend_from_slice(&parent.source_chain);
        }
        let step = match canonical_source {
            Some(canonical_source) => PrefabProjectionStep::with_canonical_source(
                source_document.clone(),
                canonical_source.clone(),
                source_entity.clone(),
            ),
            None => PrefabProjectionStep::new(source_document.clone(), source_entity.clone()),
        };
        source_chain.push(step);
        Self {
            root_anchor: parent.map_or(root_anchor, Self::root_anchor).clone(),
            source_chain,
        }
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
        projection_path: PrefabProjectionPath,
    ) -> Self {
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
