//! Complete instance conversion through the existing source transaction and history.

use std::collections::BTreeSet;

use super::history_error_report;
use crate::{
    SceneDocument, SceneEntityId, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
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
    components: BTreeSet<(SceneEntityId, nara_reflect::ComponentTypeId)>,
    instances: BTreeSet<SceneEntityId>,
    placements: bool,
    retained_bytes: usize,
}

impl PrefabConversionFootprint {
    pub(super) fn new(
        anchor: &SceneEntityId,
        created: &[SceneEntityId],
        patch: &ScenePatchDocument,
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
        let mut bytes = created
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
        let mut components = BTreeSet::new();
        let mut placements = false;
        let mut instances = BTreeSet::new();
        for operation in &patch.operations {
            match operation {
                ScenePatchOperation::AddComponent {
                    entity, component, ..
                }
                | ScenePatchOperation::ReplaceComponent {
                    entity, component, ..
                }
                | ScenePatchOperation::RemoveComponent { entity, component } => {
                    bytes = bytes
                        .checked_add(256)
                        .and_then(|bytes| bytes.checked_add(entity.as_str().len()))
                        .and_then(|bytes| bytes.checked_add(component.as_str().len()))
                        .filter(|bytes| *bytes <= maximum_bytes)
                        .ok_or_else(rejected)?;
                    if ids.contains(entity)
                        || !components.insert((entity.clone(), component.clone()))
                    {
                        return Err(rejected());
                    }
                }
                ScenePatchOperation::SetPlacements { .. } => placements = true,
                ScenePatchOperation::SetPrefabInstance {
                    entity,
                    expected: Some(_),
                    value: Some(_),
                } if entity != anchor => {
                    bytes = bytes
                        .checked_add(128)
                        .and_then(|bytes| bytes.checked_add(entity.as_str().len()))
                        .filter(|bytes| *bytes <= maximum_bytes)
                        .ok_or_else(rejected)?;
                    if !instances.insert(entity.clone()) {
                        return Err(rejected());
                    }
                }
                _ => {}
            }
        }
        let footprint = Self {
            anchor: anchor.clone(),
            created: ids,
            components,
            instances,
            placements,
            retained_bytes: bytes,
        };
        footprint.admit_patch(patch)?;
        Ok(footprint)
    }

    pub(crate) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub(crate) fn admit_patch(&self, patch: &ScenePatchDocument) -> Result<(), ScenePatchReport> {
        if patch.operations.len()
            != self
                .created
                .len()
                .saturating_add(self.components.len())
                .saturating_add(self.instances.len())
                .saturating_add(usize::from(self.placements))
                .saturating_add(1)
        {
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
        let mut components = BTreeSet::new();
        let mut placements = 0;
        let mut instances = BTreeSet::new();
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
                ScenePatchOperation::AddComponent {
                    entity, component, ..
                }
                | ScenePatchOperation::ReplaceComponent {
                    entity, component, ..
                }
                | ScenePatchOperation::RemoveComponent { entity, component }
                    if self
                        .components
                        .contains(&(entity.clone(), component.clone())) =>
                {
                    if !components.insert((entity, component)) {
                        return Err(rejected());
                    }
                }
                ScenePatchOperation::SetPlacements { .. } if self.placements => placements += 1,
                ScenePatchOperation::SetPrefabInstance {
                    entity,
                    expected: Some(_),
                    value: Some(_),
                } if self.instances.contains(entity) => {
                    if !instances.insert(entity) {
                        return Err(rejected());
                    }
                }
                _ => return Err(rejected()),
            }
        }
        if declarations != 1
            || records.len() != self.created.len()
            || components.len() != self.components.len()
            || instances.len() != self.instances.len()
            || placements != usize::from(self.placements)
        {
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
            ScenePatchOperation::AddEntity { entity }
                if self.created.contains(&entity.id) && entity.prefab.is_none() =>
            {
                Ok(())
            }
            ScenePatchOperation::RestoreEntity { entity }
                if self.created.contains(&entity.id) && entity.prefab.is_none() =>
            {
                Ok(())
            }
            ScenePatchOperation::RemoveEntity { entity }
                if self.created.contains(entity)
                    && document
                        .entities
                        .iter()
                        .any(|record| &record.id == entity && record.prefab.is_none()) =>
            {
                Ok(())
            }
            ScenePatchOperation::SetPrefabInstance {
                entity,
                expected,
                value,
            } if entity == &self.anchor && expected.is_some() != value.is_some() => Ok(()),
            ScenePatchOperation::AddComponent {
                entity, component, ..
            }
            | ScenePatchOperation::ReplaceComponent {
                entity, component, ..
            }
            | ScenePatchOperation::RemoveComponent { entity, component }
                if self
                    .components
                    .contains(&(entity.clone(), component.clone())) =>
            {
                Ok(())
            }
            ScenePatchOperation::SetPlacements { .. } if self.placements => Ok(()),
            ScenePatchOperation::SetPrefabInstance {
                entity,
                expected: Some(_),
                value: Some(_),
            } if self.instances.contains(entity) => Ok(()),
            _ => Err(rejected()),
        }
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
