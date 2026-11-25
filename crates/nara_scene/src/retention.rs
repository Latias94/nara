//! Retained storage used by compiled scenes, prefabs, and scene sources.

use std::mem::{size_of, size_of_val};

#[cfg(feature = "serde")]
use crate::PrefabDocument;
use crate::{
    CompiledSceneCandidate, CompiledSceneDocument, CompiledSceneEntity, PrefabInstance,
    PrefabInstanceBinding, PrefabProjectionPath, PrefabProjectionStep, SceneDocument,
    SceneEntityId, SceneEntityPlacement, SceneEntityProvenanceMap, SceneEntityRecord,
    ScenePlacement, ScenePlacementParent, ScenePlacementSlot,
};
use nara_asset::AssetRef;

pub const STRING_ALLOCATION_OVERHEAD: usize = 2 * size_of::<usize>();
pub const BTREE_ENTRY_OVERHEAD: usize = 6 * size_of::<usize>();
pub const ARC_CONTROL_BLOCK_BYTES: usize = 4 * size_of::<usize>();
pub const VALUE_NODE_ALLOCATION_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("scene retention exceeds the supported byte extent")]
pub struct SceneRetentionOverflow;

pub fn scene_retained_bytes(document: &SceneDocument) -> Result<usize, SceneRetentionOverflow> {
    document_retained_bytes(
        size_of::<SceneDocument>(),
        &document.placements,
        document.placements.capacity(),
        &document.entities,
        document.entities.capacity(),
    )
}

pub fn startup_scene_retained_bytes(
    candidate: &CompiledSceneCandidate,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        size_of::<CompiledSceneCandidate>(),
        candidate.source_key().retained_bytes(),
        compiled_scene_retained_bytes(candidate.document())?,
    ])
}

/// Charges the immutable candidate and all logically retained metadata, including shared owners.
/// The native witness is weak and therefore does not retain executable codecs or a registry.
#[cfg(feature = "serde")]
pub fn runtime_startup_scene_retained_bytes(
    candidate: &crate::RuntimeSceneCandidate,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        size_of::<crate::RuntimeSceneCandidate>(),
        candidate.source_key().retained_bytes(),
        compiled_scene_retained_bytes(candidate.document())?,
        runtime_definition_retained_bytes(candidate.definitions())?,
    ])
}

/// Logical ownership of sealed flat storage. Source records and contextual declarations have a
/// separate model. Roots, entities and child IDs use exact-length Box arrays, so borrowed slice
/// lengths cover their complete buffers. Map entries, selected owner strings, values and canonical
/// provenance remain fully charged.
pub fn compiled_scene_retained_bytes(
    document: &CompiledSceneDocument,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        size_of::<CompiledSceneDocument>(),
        scene_entity_ids_retained_bytes(document.roots(), document.roots().len())?,
        checked_product(document.entities().len(), size_of::<CompiledSceneEntity>())?,
        scene_provenance_retained_bytes(document.canonical_provenance())?,
    ])?;
    for entity in document.entities() {
        checked_add_to(
            &mut total,
            checked_sum([
                string_retained_bytes(entity.id().as_str())?,
                scene_entity_ids_retained_bytes(entity.children(), entity.children().len())?,
            ])?,
        )?;
        for (component, record) in entity.components() {
            checked_add_to(
                &mut total,
                checked_sum([
                    size_of_val(component),
                    size_of_val(record),
                    BTREE_ENTRY_OVERHEAD,
                    string_retained_bytes(component.as_str())?,
                    record
                        .schema_owner()
                        .map_or(Ok(0), |owner| string_retained_bytes(owner.as_str()))?,
                    component_value_retained_bytes_from_cost(record.value_cost())?,
                    record.value_storage_retained_bytes(),
                ])?,
            )?;
        }
    }
    Ok(total)
}

#[cfg(feature = "serde")]
fn runtime_definition_retained_bytes(
    definitions: &crate::SceneComponentBindingsWitness,
) -> Result<usize, SceneRetentionOverflow> {
    let owners = definitions.data_owners();
    let mut total = checked_sum([
        size_of_val(definitions),
        ARC_CONTROL_BLOCK_BYTES,
        checked_product(
            owners.len(),
            size_of::<nara_reflect::ComponentSchemaOwnerSnapshot>(),
        )?,
    ])?;
    for owner in owners {
        checked_add_to(
            &mut total,
            checked_sum([
                ARC_CONTROL_BLOCK_BYTES,
                size_of_val(owner.owner_id()),
                string_retained_bytes(owner.owner_id().as_str())?,
                schema_catalog_retained_bytes(owner.current())?,
                size_of::<Option<nara_reflect::ComponentSchemaCatalog>>(),
                owner
                    .predecessor()
                    .map_or(Ok(0), schema_catalog_retained_bytes)?,
            ])?,
        )?;
    }
    Ok(total)
}

#[cfg(feature = "serde")]
fn schema_catalog_retained_bytes(
    catalog: &nara_reflect::ComponentSchemaCatalog,
) -> Result<usize, SceneRetentionOverflow> {
    let nara_reflect::ComponentSchemaCatalog {
        generation: _,
        predecessor: _,
        components,
        type_tombstones,
    } = catalog;
    let mut total = checked_sum([
        size_of_val(catalog),
        checked_product(
            components.capacity(),
            size_of::<nara_reflect::ComponentSchema>(),
        )?,
        checked_product(
            type_tombstones.capacity(),
            size_of::<nara_reflect::ComponentTypeId>(),
        )?,
    ])?;
    for tombstone in type_tombstones {
        checked_add_to(&mut total, string_retained_bytes(tombstone.as_str())?)?;
    }
    for schema in components {
        let nara_reflect::ComponentSchema {
            id,
            aliases,
            version: _,
            capabilities,
            fields,
            field_tombstones,
            authoring_initial_value,
        } = schema;
        checked_add_to(
            &mut total,
            checked_sum([
                string_retained_bytes(id.as_str())?,
                schema_aliases_retained_bytes(aliases)?,
                schema_capabilities_retained_bytes(capabilities)?,
                checked_product(
                    fields.capacity(),
                    size_of::<nara_reflect::ComponentFieldSchema>(),
                )?,
                checked_product(
                    field_tombstones.capacity(),
                    size_of::<nara_reflect::ComponentFieldId>(),
                )?,
                authoring_initial_value
                    .as_ref()
                    .map_or(Ok(0), component_value_retained_bytes)?,
            ])?,
        )?;
        for tombstone in field_tombstones {
            checked_add_to(&mut total, string_retained_bytes(tombstone.as_str())?)?;
        }
        for field in fields {
            checked_add_to(&mut total, schema_field_dynamic_bytes(field)?)?;
        }
    }
    Ok(total)
}

#[cfg(feature = "serde")]
fn schema_aliases_retained_bytes(aliases: &Vec<String>) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_product(aliases.capacity(), size_of::<String>())?;
    for alias in aliases {
        checked_add_to(&mut total, string_retained_bytes(alias)?)?;
    }
    Ok(total)
}

#[cfg(feature = "serde")]
fn schema_capabilities_retained_bytes(
    capabilities: &std::collections::BTreeSet<nara_reflect::ComponentCapability>,
) -> Result<usize, SceneRetentionOverflow> {
    checked_product(
        capabilities.len(),
        size_of::<nara_reflect::ComponentCapability>() + BTREE_ENTRY_OVERHEAD,
    )
}

#[cfg(feature = "serde")]
fn schema_field_dynamic_bytes(
    field: &nara_reflect::ComponentFieldSchema,
) -> Result<usize, SceneRetentionOverflow> {
    let nara_reflect::ComponentFieldSchema {
        id,
        aliases,
        path,
        shape,
        required: _,
        capabilities,
        default_value,
        member_tombstones,
    } = field;
    let mut total = checked_sum([
        string_retained_bytes(id.as_str())?,
        schema_aliases_retained_bytes(aliases)?,
        checked_product(
            path.segments().len(),
            size_of::<nara_reflect::ComponentFieldPathSegment>(),
        )?,
        schema_shape_dynamic_bytes(shape)?,
        schema_capabilities_retained_bytes(capabilities)?,
        default_value
            .as_ref()
            .map_or(Ok(0), component_value_retained_bytes)?,
        checked_product(
            member_tombstones.capacity(),
            size_of::<nara_reflect::ComponentMemberPath>(),
        )?,
    ])?;
    for segment in path.segments() {
        if let nara_reflect::ComponentFieldPathSegment::Field(name) = segment {
            checked_add_to(&mut total, string_retained_bytes(name)?)?;
        }
    }
    for tombstone in member_tombstones {
        checked_add_to(
            &mut total,
            checked_product(
                tombstone.segments().len(),
                size_of::<nara_reflect::ComponentMemberPathSegment>(),
            )?,
        )?;
        for segment in tombstone.segments() {
            if let nara_reflect::ComponentMemberPathSegment::Member(id) = segment {
                checked_add_to(&mut total, string_retained_bytes(id.as_str())?)?;
            }
        }
    }
    Ok(total)
}

#[cfg(feature = "serde")]
fn schema_shape_dynamic_bytes(
    shape: &nara_reflect::ComponentValueShape,
) -> Result<usize, SceneRetentionOverflow> {
    use nara_reflect::ComponentValueShape;
    match shape {
        ComponentValueShape::Record { members } => {
            let mut total = checked_product(
                members.capacity(),
                size_of::<nara_reflect::ComponentShapeMember>(),
            )?;
            for member in members {
                checked_add_to(
                    &mut total,
                    checked_sum([
                        string_retained_bytes(&member.name)?,
                        member
                            .id
                            .as_ref()
                            .map_or(Ok(0), |id| string_retained_bytes(id.as_str()))?,
                        schema_shape_dynamic_bytes(&member.shape)?,
                    ])?,
                )?;
            }
            Ok(total)
        }
        ComponentValueShape::KeyedList { item, key } => checked_sum([
            size_of::<ComponentValueShape>(),
            string_retained_bytes(key.as_str())?,
            schema_shape_dynamic_bytes(item)?,
        ]),
        ComponentValueShape::List { item: value }
        | ComponentValueShape::Map { value }
        | ComponentValueShape::Optional { value } => checked_sum([
            size_of::<ComponentValueShape>(),
            schema_shape_dynamic_bytes(value)?,
        ]),
        ComponentValueShape::Scalar { .. }
        | ComponentValueShape::EntityRef
        | ComponentValueShape::AssetRef { .. }
        | ComponentValueShape::Opaque { .. } => Ok(0),
    }
}

fn scene_provenance_retained_bytes(
    provenance: &SceneEntityProvenanceMap,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = size_of::<SceneEntityProvenanceMap>();
    for (expanded_entity, source) in provenance {
        checked_add_to(
            &mut total,
            checked_sum([
                size_of_val(expanded_entity),
                size_of_val(source),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(expanded_entity.as_str())?,
                scene_provenance_dynamic_bytes(source)?,
            ])?,
        )?;
    }
    Ok(total)
}

fn scene_provenance_dynamic_bytes(
    source: &crate::SceneEntityProvenance,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        string_retained_bytes(source.local_entity_id().as_str())?,
        source
            .projection_path()
            .map_or(Ok(0), projection_path_retained_bytes)?,
    ])
}

#[cfg(feature = "serde")]
pub fn prefab_retained_bytes(document: &PrefabDocument) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        document_retained_bytes(
            size_of::<PrefabDocument>(),
            &document.placements,
            document.placements.capacity(),
            &document.entities,
            document.entities.capacity(),
        )?,
        reference_inputs_retained_bytes(&document.inputs)?,
    ])
}

fn reference_inputs_retained_bytes(
    inputs: &std::collections::BTreeSet<nara_ecs::identity::ReferenceInputId>,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_product(
        inputs.len(),
        size_of::<nara_ecs::identity::ReferenceInputId>() + BTREE_ENTRY_OVERHEAD,
    )?;
    for input in inputs {
        checked_add_to(&mut total, string_retained_bytes(input.as_str())?)?;
    }
    Ok(total)
}

fn document_retained_bytes(
    document_bytes: usize,
    placements: &[SceneEntityPlacement],
    placement_capacity: usize,
    entities: &[SceneEntityRecord],
    entity_capacity: usize,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        document_bytes,
        checked_product(placement_capacity, size_of::<SceneEntityPlacement>())?,
        checked_product(entity_capacity, size_of::<SceneEntityRecord>())?,
    ])?;
    for declaration in placements {
        checked_add_to(
            &mut total,
            checked_sum([
                target_retained_bytes(&declaration.target)?,
                placement_dynamic_bytes(&declaration.placement)?,
            ])?,
        )?;
    }
    for entity in entities {
        checked_add_to(&mut total, entity_dynamic_bytes(entity)?)?;
    }
    Ok(total)
}

fn entity_dynamic_bytes(entity: &SceneEntityRecord) -> Result<usize, SceneRetentionOverflow> {
    let mut total = string_retained_bytes(entity.id.as_str())?;
    for (id, record) in &entity.components {
        checked_add_to(&mut total, component_record_retained_bytes(id, record)?)?;
    }
    if let Some(instance) = &entity.prefab {
        checked_add_to(&mut total, prefab_instance_retained_bytes(instance)?)?;
    }
    Ok(total)
}

fn scene_entity_ids_retained_bytes(
    entity_ids: &[SceneEntityId],
    capacity: usize,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_product(capacity, size_of::<SceneEntityId>())?;
    for entity_id in entity_ids {
        checked_add_to(&mut total, string_retained_bytes(entity_id.as_str())?)?;
    }
    Ok(total)
}

fn prefab_instance_retained_bytes(
    instance: &PrefabInstance,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        asset_ref_retained_bytes(&instance.source)?,
        instance_binding_retained_bytes(&instance.binding)?,
        instance
            .arguments
            .as_ref()
            .map_or(Ok(0), reference_arguments_retained_bytes)?,
        instance
            .overrides
            .as_ref()
            .map_or(Ok(0), overlay_retained_bytes)?,
    ])
}

fn instance_binding_retained_bytes(
    binding: &PrefabInstanceBinding,
) -> Result<usize, SceneRetentionOverflow> {
    match binding {
        PrefabInstanceBinding::Forest => Ok(0),
        PrefabInstanceBinding::Root { base, root } => {
            checked_sum([base.retained_bytes(), target_retained_bytes(root)?])
        }
    }
}

fn placement_dynamic_bytes(placement: &ScenePlacement) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        match &placement.parent {
            ScenePlacementParent::Root => 0,
            ScenePlacementParent::Entity(target) => target_retained_bytes(target)?,
        },
        match &placement.slot {
            ScenePlacementSlot::First | ScenePlacementSlot::Last => 0,
            ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) => {
                target_retained_bytes(target)?
            }
        },
    ])
}

fn reference_arguments_retained_bytes(
    arguments: &crate::PrefabReferenceArguments,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = arguments.base.retained_bytes();
    for (input, reference) in &arguments.values {
        let cost = reference.cost();
        checked_add_to(
            &mut total,
            checked_sum([
                size_of_val(input),
                size_of_val(reference),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(input.as_str())?,
                cost.logical_bytes(),
                checked_product(cost.nodes(), VALUE_NODE_ALLOCATION_BYTES)?,
            ])?,
        )?;
    }
    Ok(total)
}

fn overlay_retained_bytes(overlay: &crate::PrefabOverlay) -> Result<usize, SceneRetentionOverflow> {
    use crate::{PrefabComponentIntent, PrefabPropertyIntent};
    let mut total = 0;
    let mut pending = vec![overlay];
    while let Some(overlay) = pending.pop() {
        checked_add_to(
            &mut total,
            checked_sum([
                overlay.base.retained_bytes(),
                checked_product(
                    overlay.additions.capacity(),
                    size_of::<crate::PrefabAddition>(),
                )?,
                checked_product(
                    overlay.instances.capacity(),
                    size_of::<crate::PrefabInstanceClaim>(),
                )?,
                checked_product(
                    overlay.components.capacity(),
                    size_of::<PrefabComponentIntent>(),
                )?,
                checked_product(
                    overlay.placements.capacity(),
                    size_of::<crate::SceneEntityPlacement>(),
                )?,
                checked_product(
                    overlay.suppressions.capacity(),
                    size_of::<crate::PrefabEntityTarget>(),
                )?,
            ])?,
        )?;
        for addition in &overlay.additions {
            checked_add_to(&mut total, string_retained_bytes(addition.id.as_str())?)?;
            for (id, record) in &addition.components {
                checked_add_to(&mut total, component_record_retained_bytes(id, record)?)?;
            }
            if let Some(instance) = &addition.prefab {
                checked_add_to(&mut total, asset_ref_retained_bytes(&instance.source)?)?;
                checked_add_to(
                    &mut total,
                    instance_binding_retained_bytes(&instance.binding)?,
                )?;
                if let Some(arguments) = &instance.arguments {
                    checked_add_to(&mut total, reference_arguments_retained_bytes(arguments)?)?;
                }
                pending.extend(instance.overrides.iter());
            }
        }
        for claim in &overlay.instances {
            checked_add_to(&mut total, target_retained_bytes(&claim.target)?)?;
            if let Some(instance) = &claim.value {
                checked_add_to(&mut total, asset_ref_retained_bytes(&instance.source)?)?;
                checked_add_to(
                    &mut total,
                    instance_binding_retained_bytes(&instance.binding)?,
                )?;
                if let Some(arguments) = &instance.arguments {
                    checked_add_to(&mut total, reference_arguments_retained_bytes(arguments)?)?;
                }
                pending.extend(instance.overrides.iter());
            }
        }
        for intent in &overlay.components {
            let (target, component) = match intent {
                PrefabComponentIntent::Set {
                    target, component, ..
                }
                | PrefabComponentIntent::Suppress { target, component }
                | PrefabComponentIntent::Properties {
                    target, component, ..
                } => (target, component),
            };
            checked_add_to(
                &mut total,
                checked_sum([
                    target_retained_bytes(target)?,
                    string_retained_bytes(component.as_str())?,
                ])?,
            )?;
            match intent {
                PrefabComponentIntent::Set { record, .. } => {
                    checked_add_to(
                        &mut total,
                        checked_sum([
                            record
                                .owner()
                                .map_or(Ok(0), |owner| string_retained_bytes(owner.as_str()))?,
                            component_value_retained_bytes(record.value())?,
                            record.value_storage_retained_bytes(),
                            contextual_defaults_retained_bytes(&record.contextual_defaults)?,
                        ])?,
                    )?;
                }
                PrefabComponentIntent::Properties { claims, .. } => {
                    checked_add_to(
                        &mut total,
                        checked_product(
                            claims.capacity(),
                            size_of::<crate::PrefabPropertyClaim>(),
                        )?,
                    )?;
                    for claim in claims {
                        checked_add_to(&mut total, property_path_retained_bytes(&claim.path)?)?;
                        if let PrefabPropertyIntent::Set(value) = &claim.intent {
                            checked_add_to(&mut total, component_value_retained_bytes(value)?)?;
                        }
                    }
                }
                PrefabComponentIntent::Suppress { .. } => {}
            }
        }
        for placement in &overlay.placements {
            checked_add_to(&mut total, target_retained_bytes(&placement.target)?)?;
            if let crate::ScenePlacementParent::Entity(target) = &placement.placement.parent {
                checked_add_to(&mut total, target_retained_bytes(target)?)?;
            }
            if let crate::ScenePlacementSlot::Before(target)
            | crate::ScenePlacementSlot::After(target) = &placement.placement.slot
            {
                checked_add_to(&mut total, target_retained_bytes(target)?)?;
            }
        }
        for target in &overlay.suppressions {
            checked_add_to(&mut total, target_retained_bytes(target)?)?;
        }
    }
    Ok(total)
}

fn component_record_retained_bytes(
    component: &nara_reflect::ComponentTypeId,
    record: &crate::SceneComponentRecord,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        size_of_val(component),
        size_of_val(record),
        BTREE_ENTRY_OVERHEAD,
        string_retained_bytes(component.as_str())?,
        record
            .owner()
            .map_or(Ok(0), |owner| string_retained_bytes(owner.as_str()))?,
        component_value_retained_bytes(record.value())?,
        record.value_storage_retained_bytes(),
        contextual_defaults_retained_bytes(&record.contextual_defaults)?,
    ])
}

fn target_retained_bytes(
    target: &crate::PrefabEntityTarget,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        string_retained_bytes(target.entity.entity().as_str())?,
        checked_product(
            target.through.capacity(),
            size_of::<crate::PrefabInstanceHop>(),
        )?,
    ])?;
    for hop in &target.through {
        checked_add_to(
            &mut total,
            checked_sum([
                string_retained_bytes(hop.anchor.entity().as_str())?,
                hop.expected_base.retained_bytes(),
            ])?,
        )?;
    }
    Ok(total)
}

fn contextual_defaults_retained_bytes(
    defaults: &std::collections::BTreeMap<
        nara_reflect::ComponentFieldId,
        nara_reflect::ComponentValue,
    >,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = 0;
    for (field, value) in defaults {
        checked_add_to(
            &mut total,
            checked_sum([
                size_of_val(field),
                size_of_val(value),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(field.as_str())?,
                component_value_retained_bytes(value)?,
            ])?,
        )?;
    }
    Ok(total)
}

fn property_path_retained_bytes(
    path: &nara_reflect::ComponentPropertyPath,
) -> Result<usize, SceneRetentionOverflow> {
    use nara_reflect::{ComponentPropertyKey, ComponentPropertyStep};
    let mut total = checked_sum([
        string_retained_bytes(path.root.as_str())?,
        checked_product(path.steps.capacity(), size_of::<ComponentPropertyStep>())?,
    ])?;
    for step in &path.steps {
        let bytes = match step {
            ComponentPropertyStep::Member(member) => string_retained_bytes(member.as_str())?,
            ComponentPropertyStep::MapEntry(key) => string_retained_bytes(key)?,
            ComponentPropertyStep::ListItem { by, key } => checked_sum([
                string_retained_bytes(by.as_str())?,
                match key {
                    ComponentPropertyKey::String(key) => string_retained_bytes(key)?,
                    _ => 0,
                },
            ])?,
        };
        checked_add_to(&mut total, bytes)?;
    }
    Ok(total)
}

fn projection_path_retained_bytes(
    path: &PrefabProjectionPath,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        string_retained_bytes(path.root_anchor().as_str())?,
        checked_product(path.steps().len(), size_of::<PrefabProjectionStep>())?,
    ])?;
    for step in path.steps() {
        let dynamic = match step {
            PrefabProjectionStep::SourceEntity {
                source_document,
                canonical_source,
                entity,
            } => checked_sum([
                asset_ref_retained_bytes(source_document)?,
                canonical_source.retained_bytes(),
                string_retained_bytes(entity.as_str())?,
            ])?,
            PrefabProjectionStep::InstanceAddition { entity } => {
                string_retained_bytes(entity.as_str())?
            }
        };
        checked_add_to(&mut total, dynamic)?;
    }
    Ok(total)
}

fn asset_ref_retained_bytes(reference: &AssetRef) -> Result<usize, SceneRetentionOverflow> {
    match reference {
        AssetRef::Path(path) => string_retained_bytes(path.as_str()),
        AssetRef::StableId(_) => Ok(16),
        AssetRef::Product(_) => Ok(size_of::<nara_asset::AssetProductRef>()),
    }
}

fn component_value_retained_bytes<'a>(
    value: impl Into<nara_reflect::ComponentValueRef<'a>>,
) -> Result<usize, SceneRetentionOverflow> {
    component_value_retained_bytes_from_cost(value.into().cost())
}

fn component_value_retained_bytes_from_cost(
    cost: nara_reflect::ComponentValueCost,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        cost.logical_bytes(),
        checked_product(cost.nodes(), VALUE_NODE_ALLOCATION_BYTES)?,
    ])
}

pub fn string_retained_bytes(value: &str) -> Result<usize, SceneRetentionOverflow> {
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

#[cfg(test)]
mod tests;
