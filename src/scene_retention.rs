use std::mem::{size_of, size_of_val};

use nara_asset::AssetRef;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_scene::PrefabDocument;
use nara_scene::{
    PrefabInstance, SceneDocument, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};

pub(crate) const STRING_ALLOCATION_OVERHEAD: usize = 2 * size_of::<usize>();
pub(crate) const BTREE_ENTRY_OVERHEAD: usize = 6 * size_of::<usize>();
pub(crate) const ARC_CONTROL_BLOCK_BYTES: usize = 4 * size_of::<usize>();
pub(crate) const VALUE_NODE_ALLOCATION_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SceneRetentionOverflow;

pub(crate) fn scene_retained_bytes(
    document: &SceneDocument,
) -> Result<usize, SceneRetentionOverflow> {
    document_retained_bytes(size_of::<SceneDocument>(), &document.entities)
}

pub(crate) fn direct_startup_scene_retained_bytes(
    document: &SceneDocument,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([scene_retained_bytes(document)?, ARC_CONTROL_BLOCK_BYTES])
}

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) fn prefab_retained_bytes(
    document: &PrefabDocument,
) -> Result<usize, SceneRetentionOverflow> {
    document_retained_bytes(size_of::<PrefabDocument>(), &document.entities)
}

fn document_retained_bytes(
    document_bytes: usize,
    entities: &[SceneEntityRecord],
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        document_bytes,
        checked_product(entities.len(), size_of::<SceneEntityRecord>())?,
    ])?;
    for entity in entities {
        checked_add_to(&mut total, entity_dynamic_bytes(entity)?)?;
    }
    Ok(total)
}

fn entity_dynamic_bytes(entity: &SceneEntityRecord) -> Result<usize, SceneRetentionOverflow> {
    let mut total = string_retained_bytes(entity.id.as_str())?;
    if let Some(parent) = &entity.parent {
        checked_add_to(&mut total, string_retained_bytes(parent.as_str())?)?;
    }
    for (id, record) in &entity.components {
        checked_add_to(
            &mut total,
            checked_sum([
                size_of_val(id),
                size_of_val(record),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(id.as_str())?,
                component_value_retained_bytes(&record.value)?,
            ])?,
        )?;
    }
    if let Some(instance) = &entity.prefab {
        checked_add_to(&mut total, prefab_instance_retained_bytes(instance)?)?;
    }
    Ok(total)
}

fn prefab_instance_retained_bytes(
    instance: &PrefabInstance,
) -> Result<usize, SceneRetentionOverflow> {
    let source = match &instance.source {
        AssetRef::Path(path) => string_retained_bytes(path.as_str())?,
        AssetRef::StableId(_) => 16,
    };
    checked_sum([source, patch_retained_bytes(&instance.overrides)?])
}

pub(crate) fn patch_retained_bytes(
    patch: &ScenePatchDocument,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_product(patch.operations.len(), size_of::<ScenePatchOperation>())?;
    for operation in &patch.operations {
        let dynamic = match operation {
            ScenePatchOperation::AddEntity { entity } => entity_dynamic_bytes(entity)?,
            ScenePatchOperation::AddComponent {
                entity,
                component,
                value,
            }
            | ScenePatchOperation::ReplaceComponent {
                entity,
                component,
                value,
            } => checked_sum([
                string_retained_bytes(entity.as_str())?,
                string_retained_bytes(component.as_str())?,
                component_value_retained_bytes(&value.value)?,
            ])?,
            ScenePatchOperation::SetField {
                entity,
                component,
                field,
                value,
                ..
            } => checked_sum([
                string_retained_bytes(entity.as_str())?,
                string_retained_bytes(component.as_str())?,
                string_retained_bytes(field.as_str())?,
                component_value_retained_bytes(value)?,
            ])?,
            ScenePatchOperation::SetAssetRefField {
                entity,
                component,
                field,
                asset_ref,
                ..
            } => checked_sum([
                string_retained_bytes(entity.as_str())?,
                string_retained_bytes(component.as_str())?,
                string_retained_bytes(field.as_str())?,
                match asset_ref {
                    AssetRef::Path(path) => string_retained_bytes(path.as_str())?,
                    AssetRef::StableId(_) => 16,
                },
            ])?,
            ScenePatchOperation::RemoveEntity { entity } => string_retained_bytes(entity.as_str())?,
            ScenePatchOperation::RemoveComponent { entity, component } => checked_sum([
                string_retained_bytes(entity.as_str())?,
                string_retained_bytes(component.as_str())?,
            ])?,
            ScenePatchOperation::RemoveField {
                entity,
                component,
                field,
                ..
            } => checked_sum([
                string_retained_bytes(entity.as_str())?,
                string_retained_bytes(component.as_str())?,
                string_retained_bytes(field.as_str())?,
            ])?,
            ScenePatchOperation::Reparent { entity, parent } => checked_sum([
                string_retained_bytes(entity.as_str())?,
                parent
                    .as_ref()
                    .map(|parent| string_retained_bytes(parent.as_str()))
                    .transpose()?
                    .unwrap_or(0),
            ])?,
        };
        checked_add_to(&mut total, dynamic)?;
    }
    Ok(total)
}

fn component_value_retained_bytes(
    value: &nara_reflect::ComponentValue,
) -> Result<usize, SceneRetentionOverflow> {
    let cost = value.cost();
    checked_sum([
        cost.logical_bytes(),
        checked_product(cost.nodes(), VALUE_NODE_ALLOCATION_BYTES)?,
    ])
}

pub(crate) fn string_retained_bytes(value: &str) -> Result<usize, SceneRetentionOverflow> {
    value
        .len()
        .checked_add(STRING_ALLOCATION_OVERHEAD)
        .ok_or(SceneRetentionOverflow)
}

fn checked_product(count: usize, bytes: usize) -> Result<usize, SceneRetentionOverflow> {
    count.checked_mul(bytes).ok_or(SceneRetentionOverflow)
}

fn checked_sum<const N: usize>(values: [usize; N]) -> Result<usize, SceneRetentionOverflow> {
    values.into_iter().try_fold(0_usize, |total, value| {
        total.checked_add(value).ok_or(SceneRetentionOverflow)
    })
}

fn checked_add_to(total: &mut usize, value: usize) -> Result<(), SceneRetentionOverflow> {
    *total = total.checked_add(value).ok_or(SceneRetentionOverflow)?;
    Ok(())
}
