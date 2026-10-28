//! Complete instance conversion through the existing source transaction and history.

use std::collections::BTreeSet;

use super::history_error_report;
use crate::{
    SceneDocument, SceneEntityId, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    ScenePlacement, ScenePlacementOwner,
};

mod localize;
mod request;
pub(crate) mod retention;
pub(crate) mod work;
pub use request::{
    LocalizedPrefabEntity, PrefabConversionError, PrefabConversionReceipt, PrefabConversionRequest,
    PreparedPrefabConversion, PreparedScenePrefabConversion, ScenePrefabConversionRequest,
};

/// Closed source-owner evidence, never accepted from a serialized or public patch.
///
/// Payloads remain in the exact patch/inverse. The history content state proves their
/// predecessor; this footprint restricts which records and placement edges may change.
#[derive(Debug, Clone)]
pub(crate) struct PrefabConversionFootprint {
    anchor: SceneEntityId,
    created: BTreeSet<SceneEntityId>,
    retained_bytes: usize,
}

impl PrefabConversionFootprint {
    pub(super) fn new(
        anchor: &SceneEntityId,
        created: &[SceneEntityId],
        maximum_bytes: usize,
    ) -> Result<Self, ScenePatchReport> {
        if created.len() >= crate::authoring_decode::MAX_AUTHORING_EDIT_OPERATIONS {
            return Err(rejected());
        }
        let base_bytes = anchor
            .as_str()
            .len()
            .checked_add(128)
            .ok_or_else(rejected)?;
        let bytes = created
            .iter()
            .try_fold(base_bytes, |bytes, entity| {
                bytes.checked_add(128)?.checked_add(entity.as_str().len())
            })
            .filter(|bytes| *bytes <= maximum_bytes)
            .ok_or_else(rejected)?;
        let ids = created.iter().cloned().collect::<BTreeSet<_>>();
        if ids.len() != created.len() || ids.contains(anchor) {
            return Err(rejected());
        }
        Ok(Self {
            anchor: anchor.clone(),
            created: ids,
            retained_bytes: bytes,
        })
    }

    pub(crate) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub(crate) fn admit_patch(&self, patch: &ScenePatchDocument) -> Result<(), ScenePatchReport> {
        if patch.operations.len() != self.created.len().saturating_add(1) {
            return Err(rejected());
        }
        let detaching = matches!(
            patch.operations.first(),
            Some(ScenePatchOperation::SetPrefabInstance { entity, expected: Some(_), value: None })
                if entity == &self.anchor
        );
        let attaching = matches!(
            patch.operations.last(),
            Some(ScenePatchOperation::SetPrefabInstance { entity, expected: None, value: Some(_) })
                if entity == &self.anchor
        );
        if detaching == attaching {
            return Err(rejected());
        }
        let mut declarations = 0;
        let mut records = BTreeSet::new();
        for operation in &patch.operations {
            match operation {
                ScenePatchOperation::SetPrefabInstance {
                    entity,
                    expected,
                    value,
                } if entity == &self.anchor && expected.is_some() != value.is_some() => {
                    declarations += 1;
                }
                ScenePatchOperation::AddEntity { entity, .. }
                | ScenePatchOperation::RestoreEntity { entity, .. }
                    if detaching && self.created.contains(&entity.id) =>
                {
                    if !records.insert(&entity.id) {
                        return Err(rejected());
                    }
                }
                ScenePatchOperation::RemoveEntity { entity, .. }
                    if attaching && self.created.contains(entity) =>
                {
                    if !records.insert(entity) {
                        return Err(rejected());
                    }
                }
                _ => return Err(rejected()),
            }
        }
        if declarations != 1 || records.len() != self.created.len() {
            return Err(rejected());
        }
        Ok(())
    }

    pub(crate) fn admit_operation(
        &self,
        document: &SceneDocument,
        operation: &ScenePatchOperation,
    ) -> Result<(), ScenePatchReport> {
        match operation {
            ScenePatchOperation::AddEntity { entity, placement }
                if self.created.contains(&entity.id)
                    && entity.children.is_empty()
                    && entity.prefab.is_none()
                    && self.admits_placement(placement) =>
            {
                Ok(())
            }
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                placement,
            } if self.created.contains(&entity.id)
                && entity.children.is_empty()
                && entity.prefab.is_none()
                && descendants.is_empty()
                && self.admits_placement(placement) =>
            {
                Ok(())
            }
            ScenePatchOperation::RemoveEntity { entity, from }
                if self.created.contains(entity)
                    && self.admits_placement(from)
                    && document.entities.iter().any(|record| {
                        &record.id == entity
                            && record.prefab.is_none()
                            && record.children.is_empty()
                    }) =>
            {
                Ok(())
            }
            ScenePatchOperation::SetPrefabInstance {
                entity,
                expected,
                value,
            } if entity == &self.anchor && expected.is_some() != value.is_some() => Ok(()),
            _ => Err(rejected()),
        }
    }

    fn admits_placement(&self, placement: &ScenePlacement) -> bool {
        matches!(
            &placement.owner,
            ScenePlacementOwner::Parent { parent }
                if parent == &self.anchor || self.created.contains(parent)
        )
    }
}

fn rejected() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-prefab-conversion-footprint-mismatch",
        "Conversion replay requires exactly its source-owned declaration and local records",
    )
}

#[cfg(test)]
mod tests;
