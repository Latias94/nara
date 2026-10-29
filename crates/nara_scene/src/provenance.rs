use std::{collections::BTreeMap, mem::size_of};

use nara_asset::AssetRef;
use nara_identity::SceneEntityId;

use crate::SourceDocumentKey;

/// Internal refusal signal for borrowed cost walks. The outer owner retains its typed error.
pub(crate) type CostScratchAdmission<'a> = dyn FnMut(Option<usize>) -> bool + 'a;

/// Admits each complete Vec replacement request before asking the allocator for scratch.
/// Ordinary cost walks retain their existing Vec growth when no admission is requested.
pub(crate) fn reserve_cost_scratch<T>(
    values: &mut Vec<T>,
    additional: usize,
    admission: &mut Option<&mut CostScratchAdmission<'_>>,
) -> Option<()> {
    let Some(admit) = admission.as_deref_mut() else {
        return Some(());
    };
    if values.capacity().saturating_sub(values.len()) >= additional {
        return Some(());
    }
    let capacity = values
        .len()
        .checked_add(additional)
        .map(|needed| needed.max(values.capacity().saturating_mul(2)).max(4));
    if !admit(capacity.and_then(|capacity| capacity.checked_mul(size_of::<T>()))) {
        return None;
    }
    values.reserve_exact(capacity? - values.len());
    Some(())
}

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
        // Both identities are inline, already charged by the containing value's size_of.
        AssetRef::Product(_) => Some(0),
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

/// One instance-entry step in a concrete Prefab projection.
///
/// Source steps retain lookup evidence as well as canonical identity. An addition instead
/// belongs to the preceding anchor's overlay, not the overridden Prefab source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefabProjectionStep {
    /// An entity belonging to the admitted Prefab source, with separate authored lookup evidence.
    SourceEntity {
        source_document: AssetRef,
        canonical_source: SourceDocumentKey,
        entity: SceneEntityId,
    },
    /// An entity owned by the preceding anchor's overlay, not by its overridden base source.
    /// The preceding anchor is the root for the first step, or the entity in the previous step.
    InstanceAddition { entity: SceneEntityId },
}

impl PrefabProjectionStep {
    pub(crate) fn source_allocation_bytes(
        source_document: &AssetRef,
        canonical_source: &SourceDocumentKey,
        source_entity: &SceneEntityId,
    ) -> Option<usize> {
        size_of::<Self>()
            .checked_add(provenance_asset_ref_bytes(source_document)?)?
            .checked_add(canonical_source.checked_retained_bytes()?)?
            .checked_add(provenance_string_bytes(source_entity.as_str())?)
    }

    pub(crate) fn addition_allocation_bytes(entity: &SceneEntityId) -> Option<usize> {
        size_of::<Self>().checked_add(provenance_string_bytes(entity.as_str())?)
    }

    #[cfg(feature = "serde")]
    fn allocation_bytes(&self) -> Option<usize> {
        match self {
            Self::SourceEntity {
                source_document,
                canonical_source,
                entity,
            } => Self::source_allocation_bytes(source_document, canonical_source, entity),
            Self::InstanceAddition { entity } => Self::addition_allocation_bytes(entity),
        }
    }

    /// Returns the local entity ID in this step's source or instance-addition namespace.
    #[must_use]
    pub const fn entity_id(&self) -> &SceneEntityId {
        match self {
            Self::SourceEntity { entity, .. } | Self::InstanceAddition { entity } => entity,
        }
    }
}

/// Structured location of one prefab projection inside a containing scene.
///
/// The root anchor and typed instance ancestry remain separate from the private runtime locator.
/// Consumers use this structured path rather than derive source identity from a generated ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabProjectionPath {
    root_anchor: SceneEntityId,
    steps: Vec<PrefabProjectionStep>,
}

impl PrefabProjectionPath {
    pub(crate) const fn from_resolved_steps(
        root_anchor: SceneEntityId,
        steps: Vec<PrefabProjectionStep>,
    ) -> Self {
        Self { root_anchor, steps }
    }

    #[cfg(test)]
    pub(crate) const fn new(root_anchor: SceneEntityId) -> Self {
        Self {
            root_anchor,
            steps: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_step(
        mut self,
        source_document: AssetRef,
        canonical_source: SourceDocumentKey,
        source_entity: SceneEntityId,
    ) -> Self {
        self.steps.push(PrefabProjectionStep::SourceEntity {
            source_document,
            canonical_source,
            entity: source_entity,
        });
        self
    }

    #[cfg(feature = "serde")]
    pub(crate) fn extended_allocation_bytes(
        parent: Option<&Self>,
        root_anchor: &SceneEntityId,
        step_bytes: usize,
    ) -> Option<usize> {
        let root_anchor = parent.map_or(root_anchor, Self::root_anchor);
        let mut bytes = provenance_string_bytes(root_anchor.as_str())?;
        if let Some(parent) = parent {
            for step in &parent.steps {
                bytes = bytes.checked_add(step.allocation_bytes()?)?;
            }
        }
        bytes.checked_add(step_bytes)
    }

    /// Returns the authored local anchor in the containing source document.
    #[must_use]
    pub const fn root_anchor(&self) -> &SceneEntityId {
        &self.root_anchor
    }

    /// Returns the exact outer-to-inner source and instance-addition transitions.
    #[must_use]
    pub fn steps(&self) -> &[PrefabProjectionStep] {
        &self.steps
    }
}

/// Source identity carried beside one materialized scene entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEntityProvenance {
    local_entity_id: SceneEntityId,
    projection_path: Option<PrefabProjectionPath>,
}

impl SceneEntityProvenance {
    pub(crate) const fn local(local_entity_id: SceneEntityId) -> Self {
        Self {
            local_entity_id,
            projection_path: None,
        }
    }

    pub(crate) fn projected(
        local_entity_id: SceneEntityId,
        projection_path: PrefabProjectionPath,
    ) -> Self {
        Self {
            local_entity_id,
            projection_path: Some(projection_path),
        }
    }

    /// Returns the opaque terminal ID, relative to the source or overlay named by its provenance.
    #[must_use]
    pub const fn local_entity_id(&self) -> &SceneEntityId {
        &self.local_entity_id
    }

    /// Returns the structured instance location, or `None` for a containing-source local record.
    #[must_use]
    pub const fn projection_path(&self) -> Option<&PrefabProjectionPath> {
        self.projection_path.as_ref()
    }
}
