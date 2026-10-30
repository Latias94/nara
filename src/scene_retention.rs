use std::mem::{size_of, size_of_val};

use nara_asset::AssetRef;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use nara_scene::PrefabDocument;
use nara_scene::{
    ExpandedSceneCandidate, PrefabInstance, PrefabProjectionPath, PrefabProjectionStep,
    SceneDocument, SceneEntityId, SceneEntityProvenanceMap, SceneEntityRecord,
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
    document_retained_bytes(
        size_of::<SceneDocument>(),
        &document.roots,
        document.roots.capacity(),
        &document.entities,
        document.entities.capacity(),
    )
}

pub(crate) fn direct_startup_scene_retained_bytes(
    candidate: &ExpandedSceneCandidate,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        startup_scene_retained_bytes(candidate)?,
        2 * ARC_CONTROL_BLOCK_BYTES,
        size_of::<std::sync::Arc<SceneDocument>>(),
    ])
}

pub(crate) fn startup_scene_retained_bytes(
    candidate: &ExpandedSceneCandidate,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        size_of::<ExpandedSceneCandidate>(),
        candidate.source_key().retained_bytes(),
        scene_retained_bytes(candidate.document())?,
        scene_provenance_retained_bytes(candidate.provenance())?,
    ])
}

#[cfg(feature = "serde")]
pub(crate) fn direct_runtime_scene_retained_bytes(
    candidate: &nara_scene::RuntimeSceneCandidate,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        runtime_startup_scene_retained_bytes(candidate)?,
        2 * ARC_CONTROL_BLOCK_BYTES,
        size_of::<std::sync::Arc<nara_scene::RuntimeSceneCandidate>>(),
    ])
}

/// Charges the immutable candidate and all logically retained metadata, including shared owners.
/// The native witness is weak and therefore does not retain executable codecs or a registry.
#[cfg(feature = "serde")]
pub(crate) fn runtime_startup_scene_retained_bytes(
    candidate: &nara_scene::RuntimeSceneCandidate,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        size_of::<nara_scene::RuntimeSceneCandidate>(),
        candidate.source_key().retained_bytes(),
        scene_retained_bytes(candidate.document())?,
        scene_provenance_retained_bytes(candidate.provenance())?,
        runtime_definition_retained_bytes(candidate.definitions())?,
    ])
}

#[cfg(feature = "serde")]
fn runtime_definition_retained_bytes(
    definitions: &nara_scene::SceneComponentBindingsWitness,
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
        | ComponentValueShape::AssetRef
        | ComponentValueShape::Opaque { .. } => Ok(0),
    }
}

pub(crate) fn scene_provenance_retained_bytes(
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

pub(crate) fn scene_provenance_dynamic_bytes(
    source: &nara_scene::SceneEntityProvenance,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        string_retained_bytes(source.local_entity_id().as_str())?,
        source
            .projection_path()
            .map_or(Ok(0), projection_path_retained_bytes)?,
    ])
}

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) fn prefab_retained_bytes(
    document: &PrefabDocument,
) -> Result<usize, SceneRetentionOverflow> {
    document_retained_bytes(
        size_of::<PrefabDocument>(),
        &document.roots,
        document.roots.capacity(),
        &document.entities,
        document.entities.capacity(),
    )
}

fn document_retained_bytes(
    document_bytes: usize,
    roots: &[SceneEntityId],
    root_capacity: usize,
    entities: &[SceneEntityRecord],
    entity_capacity: usize,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        document_bytes,
        scene_entity_ids_retained_bytes(roots, root_capacity)?,
        checked_product(entity_capacity, size_of::<SceneEntityRecord>())?,
    ])?;
    for entity in entities {
        checked_add_to(&mut total, entity_dynamic_bytes(entity)?)?;
    }
    Ok(total)
}

fn entity_dynamic_bytes(entity: &SceneEntityRecord) -> Result<usize, SceneRetentionOverflow> {
    let mut total = string_retained_bytes(entity.id.as_str())?;
    checked_add_to(
        &mut total,
        scene_entity_ids_retained_bytes(&entity.children, entity.children.capacity())?,
    )?;
    for (id, record) in &entity.components {
        checked_add_to(
            &mut total,
            checked_sum([
                size_of_val(id),
                size_of_val(record),
                BTREE_ENTRY_OVERHEAD,
                string_retained_bytes(id.as_str())?,
                record
                    .owner()
                    .map_or(Ok(0), |owner| string_retained_bytes(owner.as_str()))?,
                component_value_retained_bytes(&record.value)?,
            ])?,
        )?;
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
    let source = asset_ref_retained_bytes(&instance.source)?;
    checked_sum([
        source,
        instance
            .overrides
            .as_ref()
            .map_or(Ok(0), overlay_retained_bytes)?,
    ])
}

pub(crate) fn overlay_retained_bytes(
    overlay: &nara_scene::PrefabOverlay,
) -> Result<usize, SceneRetentionOverflow> {
    use nara_scene::{PrefabComponentIntent, PrefabPropertyIntent};
    let mut total = 0;
    let mut pending = vec![overlay];
    while let Some(overlay) = pending.pop() {
        checked_add_to(
            &mut total,
            checked_sum([
                overlay.base.retained_bytes(),
                checked_product(
                    overlay.additions.capacity(),
                    size_of::<nara_scene::PrefabAddition>(),
                )?,
                checked_product(
                    overlay.instances.capacity(),
                    size_of::<nara_scene::PrefabInstanceClaim>(),
                )?,
                checked_product(
                    overlay.components.capacity(),
                    size_of::<PrefabComponentIntent>(),
                )?,
                checked_product(
                    overlay.placements.capacity(),
                    size_of::<nara_scene::PrefabPlacement>(),
                )?,
                checked_product(
                    overlay.suppressions.capacity(),
                    size_of::<nara_scene::PrefabEntityTarget>(),
                )?,
            ])?,
        )?;
        for addition in &overlay.additions {
            checked_add_to(&mut total, string_retained_bytes(addition.id.as_str())?)?;
            for (id, record) in &addition.components {
                checked_add_to(&mut total, component_record_retained_bytes(id, record)?)?;
            }
            for (id, bindings) in &addition.bindings {
                checked_add_to(
                    &mut total,
                    checked_sum([
                        size_of_val(id),
                        size_of_val(bindings),
                        BTREE_ENTRY_OVERHEAD,
                        string_retained_bytes(id.as_str())?,
                        bindings_retained_bytes(bindings)?,
                    ])?,
                )?;
            }
            if let Some(instance) = &addition.prefab {
                checked_add_to(&mut total, asset_ref_retained_bytes(&instance.source)?)?;
                pending.extend(instance.overrides.iter());
            }
        }
        for claim in &overlay.instances {
            checked_add_to(&mut total, target_retained_bytes(&claim.target)?)?;
            if let Some(instance) = &claim.value {
                checked_add_to(&mut total, asset_ref_retained_bytes(&instance.source)?)?;
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
                PrefabComponentIntent::Set {
                    record, bindings, ..
                } => {
                    checked_add_to(
                        &mut total,
                        checked_sum([
                            record
                                .owner()
                                .map_or(Ok(0), |owner| string_retained_bytes(owner.as_str()))?,
                            component_value_retained_bytes(&record.value)?,
                            bindings_retained_bytes(bindings)?,
                        ])?,
                    )?;
                }
                PrefabComponentIntent::Properties { claims, .. } => {
                    checked_add_to(
                        &mut total,
                        checked_product(
                            claims.capacity(),
                            size_of::<nara_scene::PrefabPropertyClaim>(),
                        )?,
                    )?;
                    for claim in claims {
                        checked_add_to(&mut total, property_path_retained_bytes(&claim.path)?)?;
                        checked_add_to(&mut total, bindings_retained_bytes(&claim.bindings)?)?;
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
            if let nara_scene::PrefabPlacementParent::Entity(target) = &placement.parent {
                checked_add_to(&mut total, target_retained_bytes(target)?)?;
            }
            if let nara_scene::PrefabPlacementSlot::Before(target)
            | nara_scene::PrefabPlacementSlot::After(target) = &placement.slot
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
    record: &nara_scene::SceneComponentRecord,
) -> Result<usize, SceneRetentionOverflow> {
    checked_sum([
        size_of_val(component),
        size_of_val(record),
        BTREE_ENTRY_OVERHEAD,
        string_retained_bytes(component.as_str())?,
        record
            .owner()
            .map_or(Ok(0), |owner| string_retained_bytes(owner.as_str()))?,
        component_value_retained_bytes(&record.value)?,
    ])
}

fn target_retained_bytes(
    target: &nara_scene::PrefabEntityTarget,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_sum([
        string_retained_bytes(target.entity.entity().as_str())?,
        checked_product(
            target.through.capacity(),
            size_of::<nara_scene::PrefabInstanceHop>(),
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

fn bindings_retained_bytes(
    bindings: &Vec<nara_scene::PrefabReferenceBinding>,
) -> Result<usize, SceneRetentionOverflow> {
    let mut total = checked_product(
        bindings.capacity(),
        size_of::<nara_scene::PrefabReferenceBinding>(),
    )?;
    for binding in bindings {
        checked_add_to(
            &mut total,
            checked_sum([
                string_retained_bytes(binding.expected.as_str())?,
                target_retained_bytes(&binding.target)?,
                checked_product(
                    binding.slot.segments().len(),
                    size_of::<nara_reflect::ComponentFieldPathSegment>(),
                )?,
            ])?,
        )?;
        for segment in binding.slot.segments() {
            if let nara_reflect::ComponentFieldPathSegment::Field(field) = segment {
                checked_add_to(&mut total, string_retained_bytes(field)?)?;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use nara_reflect::ComponentRegistry;
    use nara_scene::{
        InMemoryPrefabSourceResolver, PrefabDocument, SceneEntityProvenance, SourceDocumentKey,
        SourceDocumentKind,
    };

    fn scene_id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).expect("test scene entity id must be valid")
    }

    fn prefab_anchor(
        name: &str,
        source: AssetRef,
        base: SourceDocumentKey,
        additions: Vec<SceneEntityRecord>,
    ) -> SceneEntityRecord {
        let mut anchor = SceneEntityRecord::new(scene_id(name));
        let mut overlay = nara_scene::PrefabOverlay::new(base);
        for entity in additions {
            assert!(entity.children.is_empty());
            let mut addition = nara_scene::PrefabAddition::new(entity.id.clone());
            addition.components = entity.components;
            addition.prefab = entity.prefab;
            overlay.placements.push(nara_scene::PrefabPlacement {
                target: nara_scene::PrefabEntityTarget::addition(entity.id),
                parent: nara_scene::PrefabPlacementParent::Root,
                slot: nara_scene::PrefabPlacementSlot::Last,
            });
            overlay.additions.push(addition);
        }
        anchor.prefab = Some(PrefabInstance {
            source,
            overrides: Some(overlay),
        });
        anchor
    }

    fn string_bytes(value: &str) -> usize {
        value.len() + 2 * size_of::<usize>()
    }

    fn assert_provenance_retention(
        provenance: &SceneEntityProvenanceMap,
        expected_paths: &[(&str, usize)],
    ) {
        assert_eq!(provenance.len(), expected_paths.len());
        let mut expected = size_of::<SceneEntityProvenanceMap>();
        for (local_id, path_bytes) in expected_paths {
            let (expanded, source) = provenance
                .iter()
                .find(|(_, source)| source.local_entity_id().as_str() == *local_id)
                .expect("the expanded candidate must retain each entity's origin");
            match source.projection_path() {
                Some(path) => {
                    assert_eq!(path.root_anchor(), &scene_id("anchor"));
                    assert_eq!(projection_path_retained_bytes(path).unwrap(), *path_bytes);
                }
                None => assert_eq!(*path_bytes, 0),
            }
            expected += size_of::<SceneEntityId>()
                + size_of::<SceneEntityProvenance>()
                + 6 * size_of::<usize>()
                + string_bytes(expanded.as_str())
                + string_bytes(local_id)
                + *path_bytes;
        }
        assert_eq!(
            scene_provenance_retained_bytes(provenance).unwrap(),
            expected
        );
    }

    #[test]
    fn provenance_retention_distinguishes_source_entities_from_instance_additions() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let key = SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "retention.direct-prefab-source",
            SourceDocumentKind::Prefab,
        )
        .unwrap();
        let path_request = AssetRef::path("prefabs/retained-source.prefab.ron").unwrap();
        let path_request_bytes = string_bytes("prefabs/retained-source.prefab.ron");
        let stable_request = AssetRef::stable_id("602e48c3-50ce-467b-8e0d-80ec1b4f1d2a").unwrap();
        for (request, request_bytes) in [(path_request, path_request_bytes), (stable_request, 16)] {
            let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
                request.clone(),
                key.clone(),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("inherited"))]),
            );
            let document = SceneDocument::from_ordered_roots([prefab_anchor(
                "anchor",
                request.clone(),
                key.clone(),
                vec![SceneEntityRecord::new(scene_id("added-by-instance"))],
            )]);
            let expansion = document.expand_prefabs(&registry, &resolver);
            assert!(
                !expansion.diagnostics.has_errors(),
                "{:?}",
                expansion.diagnostics
            );
            let candidate = expansion.candidate.unwrap();
            let provenance = candidate.provenance();
            for source in provenance.values() {
                let Some(path) = source.projection_path() else {
                    continue;
                };
                let expected = match source.local_entity_id().as_str() {
                    "inherited" => PrefabProjectionStep::SourceEntity {
                        source_document: request.clone(),
                        canonical_source: key.clone(),
                        entity: scene_id("inherited"),
                    },
                    "added-by-instance" => PrefabProjectionStep::InstanceAddition {
                        entity: scene_id("added-by-instance"),
                    },
                    _ => panic!("unexpected projected entity"),
                };
                assert_eq!(path.steps(), &[expected]);
            }

            let path_base = string_bytes("anchor") + size_of::<PrefabProjectionStep>();
            assert_provenance_retention(
                provenance,
                &[
                    ("anchor", 0),
                    (
                        "inherited",
                        path_base
                            + request_bytes
                            + key.retained_bytes()
                            + string_bytes("inherited"),
                    ),
                    (
                        "added-by-instance",
                        path_base + string_bytes("added-by-instance"),
                    ),
                ],
            );
        }
    }

    #[test]
    fn provenance_retention_counts_both_origins_through_nested_prefab_anchors() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let universe = "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap();
        let outer_request = AssetRef::stable_id("602e48c3-50ce-467b-8e0d-80ec1b4f1d2a").unwrap();
        let inner_request = AssetRef::path("prefabs/nested-retained.prefab.ron").unwrap();
        let outer_key = SourceDocumentKey::named(
            universe,
            "retention.outer-prefab-source",
            SourceDocumentKind::Prefab,
        )
        .unwrap();
        let inner_key = SourceDocumentKey::named(
            universe,
            "retention.inner-prefab-source",
            SourceDocumentKind::Prefab,
        )
        .unwrap();
        for added_anchor in [false, true] {
            let nested = prefab_anchor(
                "nested-anchor",
                inner_request.clone(),
                inner_key.clone(),
                vec![SceneEntityRecord::new(scene_id("nested-addition"))],
            );
            let mut outer_entities = vec![SceneEntityRecord::new(scene_id("outer-source"))];
            let mut outer_additions = Vec::new();
            if added_anchor {
                outer_additions.push(nested);
            } else {
                outer_entities.push(nested);
            }
            let resolver = InMemoryPrefabSourceResolver::new()
                .with_prefab(
                    outer_request.clone(),
                    outer_key.clone(),
                    PrefabDocument::from_ordered_roots(outer_entities),
                )
                .with_prefab(
                    inner_request.clone(),
                    inner_key.clone(),
                    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id(
                        "inner-source",
                    ))]),
                );
            let document = SceneDocument::from_ordered_roots([prefab_anchor(
                "anchor",
                outer_request.clone(),
                outer_key.clone(),
                outer_additions,
            )]);
            let expansion = document.expand_prefabs(&registry, &resolver);
            assert!(
                !expansion.diagnostics.has_errors(),
                "{:?}",
                expansion.diagnostics
            );
            let candidate = expansion.candidate.unwrap();
            let parent_step = if added_anchor {
                PrefabProjectionStep::InstanceAddition {
                    entity: scene_id("nested-anchor"),
                }
            } else {
                PrefabProjectionStep::SourceEntity {
                    source_document: outer_request.clone(),
                    canonical_source: outer_key.clone(),
                    entity: scene_id("nested-anchor"),
                }
            };
            let nested_origin = |local_id: &str| {
                candidate
                    .provenance()
                    .values()
                    .find(|source| source.local_entity_id().as_str() == local_id)
                    .unwrap()
                    .projection_path()
                    .unwrap()
            };
            assert_eq!(
                nested_origin("nested-anchor").steps(),
                &[parent_step.clone()],
            );
            assert_eq!(
                nested_origin("inner-source").steps(),
                &[
                    parent_step.clone(),
                    PrefabProjectionStep::SourceEntity {
                        source_document: inner_request.clone(),
                        canonical_source: inner_key.clone(),
                        entity: scene_id("inner-source"),
                    },
                ],
            );
            assert_eq!(
                nested_origin("nested-addition").steps(),
                &[
                    parent_step,
                    PrefabProjectionStep::InstanceAddition {
                        entity: scene_id("nested-addition"),
                    },
                ],
            );

            let path_base = string_bytes("anchor");
            let source_base = size_of::<PrefabProjectionStep>() + 16 + outer_key.retained_bytes();
            let parent_bytes = size_of::<PrefabProjectionStep>()
                + string_bytes("nested-anchor")
                + if added_anchor {
                    0
                } else {
                    16 + outer_key.retained_bytes()
                };
            assert_provenance_retention(
                candidate.provenance(),
                &[
                    ("anchor", 0),
                    (
                        "outer-source",
                        path_base + source_base + string_bytes("outer-source"),
                    ),
                    ("nested-anchor", path_base + parent_bytes),
                    (
                        "inner-source",
                        path_base
                            + parent_bytes
                            + size_of::<PrefabProjectionStep>()
                            + string_bytes("prefabs/nested-retained.prefab.ron")
                            + inner_key.retained_bytes()
                            + string_bytes("inner-source"),
                    ),
                    (
                        "nested-addition",
                        path_base
                            + parent_bytes
                            + size_of::<PrefabProjectionStep>()
                            + string_bytes("nested-addition"),
                    ),
                ],
            );
        }
    }

    #[test]
    fn scene_retention_counts_ordered_roots_and_children() {
        let root = scene_id("root");
        let child = scene_id("root/child");
        let mut roots = Vec::with_capacity(3);
        roots.push(root.clone());
        let mut children = Vec::with_capacity(4);
        children.push(child.clone());
        let mut entities = Vec::with_capacity(5);
        entities.push(SceneEntityRecord::new(root.clone()).with_children(children));
        entities.push(SceneEntityRecord::new(child.clone()));
        let document = SceneDocument { roots, entities };

        let expected = checked_sum([
            size_of::<SceneDocument>(),
            document.roots.capacity() * size_of::<SceneEntityId>(),
            string_retained_bytes(root.as_str()).unwrap(),
            document.entities.capacity() * size_of::<SceneEntityRecord>(),
            string_retained_bytes(root.as_str()).unwrap(),
            document.entities[0].children.capacity() * size_of::<SceneEntityId>(),
            string_retained_bytes(child.as_str()).unwrap(),
            string_retained_bytes(child.as_str()).unwrap(),
        ])
        .unwrap();

        assert_eq!(scene_retained_bytes(&document).unwrap(), expected);
    }

    #[test]
    fn bound_candidate_retention_charges_the_containing_source_and_envelope() {
        let universe = "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap();
        let candidate = |name| {
            ExpandedSceneCandidate::plain(
                nara_scene::SourceDocumentKey::named(
                    universe,
                    name,
                    nara_scene::SourceDocumentKind::Scene,
                )
                .unwrap(),
                std::sync::Arc::new(SceneDocument::default()),
            )
        };
        let short = candidate("scene");
        let long = candidate("scene-with-a-longer-explicit-source-name");
        let short_bytes = startup_scene_retained_bytes(&short).unwrap();
        let long_bytes = startup_scene_retained_bytes(&long).unwrap();
        assert!(
            short_bytes
                >= size_of::<ExpandedSceneCandidate>() + short.source_key().retained_bytes()
        );
        assert_eq!(
            long_bytes - short_bytes,
            long.source_key().retained_bytes() - short.source_key().retained_bytes(),
        );
        assert!(direct_startup_scene_retained_bytes(&short).unwrap() > short_bytes);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn runtime_retention_charges_unused_definitions_and_enforces_the_exact_direct_limit() {
        use nara_reflect::{
            ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
            ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
            ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
            PersistentComponentSchemaOwnerId,
        };
        use nara_scene::{RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentBindings};

        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let document =
            std::sync::Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                scene_id("local"),
            )]));
        let key = SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "retention.runtime-definitions",
            SourceDocumentKind::Scene,
        )
        .unwrap();
        let cook = |alias: &str, initial: &str| {
            let owner = ComponentSchemaOwnerSnapshot::admit(
                PersistentComponentSchemaOwnerId::try_new("test.retention.runtime-data").unwrap(),
                ComponentSchemaCatalog {
                    components: vec![
                        ComponentSchema::new(
                            ComponentTypeId::new("test.retention.Unused"),
                            alias,
                            ComponentSchemaVersion::ONE,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                        .with_fields([ComponentFieldSchema::required(
                            ComponentFieldId::new("value"),
                            "Value",
                            ComponentFieldPath::empty(),
                            ComponentValueKind::String,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                        .with_authoring_initial_value(ComponentValue::String(initial.to_owned())),
                    ],
                    ..Default::default()
                },
                None,
            )
            .unwrap();
            let bindings =
                SceneComponentBindings::new(registry.snapshot().unwrap(), [owner]).unwrap();
            RuntimeSceneCandidate::cook(
                &ExpandedSceneCandidate::plain(key.clone(), std::sync::Arc::clone(&document)),
                &bindings,
                nara_asset::ProjectAssetDatabase::default()
                    .resolver(&nara_asset::AssetPathScope::ROOT),
                RuntimeSceneLimits::default(),
            )
            .unwrap()
        };
        let short = cook("Short", "x");
        let long_alias = "A longer unused definition alias";
        let long = cook(long_alias, "x");
        let initial = "A retained creation template independent of the document";
        let template = cook("Short", initial);
        assert_eq!(short.document(), long.document());
        assert_eq!(short.provenance(), long.provenance());
        assert_eq!(short.provenance().len(), 1);
        assert_eq!(
            runtime_startup_scene_retained_bytes(&long).unwrap()
                - runtime_startup_scene_retained_bytes(&short).unwrap(),
            long_alias.len() - "Short".len()
        );
        assert_eq!(
            runtime_startup_scene_retained_bytes(&template).unwrap()
                - runtime_startup_scene_retained_bytes(&short).unwrap(),
            initial.len() - "x".len()
        );
        let required = direct_runtime_scene_retained_bytes(&template).unwrap();
        let source = crate::startup_scene::SceneSource::runtime(
            template,
            nara_core::ByteLimit::new(required).unwrap(),
        )
        .unwrap();
        assert_eq!(source.retained_bytes(), required);
        assert!(!source.is_project_backed());
        assert!(source.asset_resolution().is_none());
        assert_eq!(
            crate::startup_scene::SceneSource::runtime(
                cook("Short", initial),
                nara_core::ByteLimit::new(required - 1).unwrap(),
            )
            .unwrap_err(),
            crate::startup_scene::SceneSourceError::RetainedBytesExceeded {
                required,
                limit: required - 1,
            }
        );
    }
}
