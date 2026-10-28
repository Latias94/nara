//! Classification reuse for the source owner's complete forward conversion.
//!
//! The closed transaction preserves every existing direct record and every other declaration.
//! Its new records are complete compiler output: localization only changes typed entity IDs.
//! Reuse those prior admissions instead of reinterpreting unchanged values and defaults.

use nara_reflect::ComponentValueKind;
use std::mem::size_of;

use nara_reflect::__private::{
    AssetReferenceCollectionError, EntityReferencePlanError,
    collect_declared_effective_asset_references_with_admission,
    plan_declared_entity_references_with_admission,
};

use super::*;
use crate::authoring::prefab_conversion::work::{self, ConversionWork};

pub(super) fn classify_conversion_document(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    work: &mut ConversionWork,
) -> Result<AuthoringDocumentState, ScenePatchReport> {
    let Some(ScenePatchOperation::SetPrefabInstance { entity: anchor, .. }) =
        patch.operations.first()
    else {
        return Err(structure::authoring_structure_rejection(
            "Conversion requires its exact source-owned forward transaction",
        ));
    };
    work.reserve("classification-snapshot", state.checked_retained_bytes())?;
    work::copying("classification-snapshot");
    let mut next = state.clone();
    next.record_facts.retain(|fact| {
        !matches!(
            &fact.location,
            AuthoringRecordLocation::OverlayComponent { anchor: owner, .. } if owner == anchor
        )
    });
    next.overlay_asset_references.remove(anchor);
    next.unavailable_manifest.overlays.remove(anchor);
    next.unavailable_manifest.entries.retain(|entry| {
        let location = match entry {
            UnavailableRecordManifestEntry::Record(record) => &record.location,
            UnavailableRecordManifestEntry::OverlayTarget(target) => &target.location,
        };
        !matches!(location, AuthoringRecordLocation::OverlayComponent { anchor: owner, .. } if owner == anchor)
    });

    // This is the existing placement owner, with its actual borrowed clone cost admitted first.
    work.reserve("placement-index", placement_index_bytes(document))?;
    let placements = crate::patch::authoring_entity_placements(&document.roots, &document.entities);
    for entry in &mut next.unavailable_manifest.entries {
        let (entity, placement) = match entry {
            UnavailableRecordManifestEntry::Record(record) => match &record.location {
                AuthoringRecordLocation::EntityComponent { entity } => {
                    (entity, &mut record.placement)
                }
                AuthoringRecordLocation::OverlayComponent { anchor, .. } => {
                    (anchor, &mut record.anchor_placement)
                }
            },
            UnavailableRecordManifestEntry::OverlayTarget(target) => {
                let AuthoringRecordLocation::OverlayComponent { anchor, .. } = &target.location
                else {
                    unreachable!("overlay preservation facts retain their declaring anchor");
                };
                (anchor, &mut target.anchor_placement)
            }
        };
        let current = placements.get(entity);
        work.reserve(
            "preservation-placement",
            retention::placement_dynamic_bytes(current),
        )?;
        *placement = current.cloned();
    }
    work.reserve(
        "conversion-namespace",
        document
            .entities
            .len()
            .checked_mul(size_of::<&SceneEntityId>() + 6 * size_of::<usize>()),
    )?;
    let mut context = ClassificationContext {
        source: &state.source,
        kind: state.kind,
        basis: &state.basis,
        record_facts: Vec::new(),
        overlay_asset_references: BTreeMap::new(),
        manifest: Vec::new(),
        placements: BTreeMap::new(),
        namespace: document.entities.iter().map(|record| &record.id).collect(),
        limits: state.limits,
        property_read_budget: nara_reflect::ComponentPropertyReadBudget::new(
            state.limits.component_value_nodes(),
            state.limits.component_value_bytes(),
        ),
        asset_reference_nodes: 0,
        asset_reference_bytes: 0,
        admission: None,
    };
    // Aggregate inventory includes unavailable records and defaults retained in unchanged facts.
    for references in next
        .record_facts
        .iter()
        .map(|fact| fact.schema_declared_asset_references.as_slice())
        .chain(next.overlay_asset_references.values().map(Vec::as_slice))
    {
        context
            .reserve_asset_references(references)
            .map_err(publication_report)?;
    }
    for operation in patch.operations.iter().skip(1) {
        let ScenePatchOperation::AddEntity { entity, .. } = operation else {
            return Err(structure::authoring_structure_rejection(
                "Conversion can classify only its complete added local records",
            ));
        };
        for (component, record) in &entity.components {
            classify_materialized_record(&entity.id, component, record, &mut context, work)?;
        }
    }
    if !context.record_facts.is_empty() {
        work.reserve(
            "classification-fact-merge",
            next.record_facts
                .len()
                .checked_add(context.record_facts.len())
                .and_then(|count| count.checked_mul(size_of::<AuthoringRecordFact>())),
        )?;
        next.record_facts.reserve_exact(context.record_facts.len());
        next.record_facts.extend(context.record_facts);
    }
    // Stable ordering preserves direct-component and nested declaration order within each owner.
    work.reserve(
        "classification-fact-order",
        next.record_facts
            .len()
            .checked_mul(size_of::<AuthoringRecordFact>()),
    )?;
    next.record_facts
        .sort_by(|left, right| fact_owner(left).cmp(fact_owner(right)));
    next.direct_record_facts.clear();
    for (index, fact) in next.record_facts.iter().enumerate() {
        if let AuthoringRecordLocation::EntityComponent { entity } = &fact.location {
            work.reserve(
                "classification-index",
                entity
                    .as_str()
                    .len()
                    .checked_add(fact.component.as_str().len())
                    .and_then(|bytes| bytes.checked_add(256)),
            )?;
            next.direct_record_facts
                .entry(entity.clone())
                .or_default()
                .insert(fact.component.clone(), index);
        }
    }
    next.unavailable_manifest.record_count = next
        .unavailable_manifest
        .entries
        .iter()
        .filter(|entry| matches!(entry, UnavailableRecordManifestEntry::Record(_)))
        .count();
    next.unavailable_manifest.overlay_target_count =
        next.unavailable_manifest.entries.len() - next.unavailable_manifest.record_count;
    // No new unavailable records are admitted. Remaining diagnostics have the same metadata.
    work.reserve(
        "classification-diagnostics",
        Some(state.diagnostics.retained_bytes()),
    )?;
    next.diagnostics = unavailable_record_diagnostics(&next.unavailable_manifest.entries);
    next.readiness = if next.unavailable_manifest.total_count() == 0 {
        AuthoringDocumentReadiness::Complete
    } else {
        AuthoringDocumentReadiness::Degraded
    };
    Ok(next)
}

fn fact_owner(fact: &AuthoringRecordFact) -> &SceneEntityId {
    match &fact.location {
        AuthoringRecordLocation::EntityComponent { entity } => entity,
        AuthoringRecordLocation::OverlayComponent { anchor, .. } => anchor,
    }
}

fn publication_report(error: AuthoringDocumentPublicationError) -> ScenePatchReport {
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: error.into_diagnostics(),
    }
}

fn classify_materialized_record(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    context: &mut ClassificationContext<'_, '_, '_>,
    work: &mut ConversionWork,
) -> Result<(), ScenePatchReport> {
    let basis = context.basis;
    let entry = basis.schemas.get(component).ok_or_else(|| {
        structure::authoring_structure_rejection(
            "Converted records require their admitted current Schema",
        )
    })?;
    work.reserve(
        "classification-record",
        entity
            .as_str()
            .len()
            .checked_add(component.as_str().len())
            .and_then(|bytes| {
                bytes.checked_add(record.owner().map_or(0, |owner| owner.as_str().len()))
            })
            .and_then(|bytes| bytes.checked_add(entry.owner.as_str().len().checked_mul(3)?))
            .and_then(|bytes| bytes.checked_add(size_of::<AuthoringRecordFact>() + 256)),
    )?;
    let classified = basis
        .classify_record(component, record)
        .map_err(publication_report)?;
    if !classified.readiness.is_available() {
        return Err(structure::authoring_structure_rejection(
            "Conversion requires complete current data-authoring records",
        ));
    }
    let schema = &entry.schema;
    let limits = reference_traversal_limits(context.limits.component_materialization_limits());
    let plan = plan_declared_entity_references_with_admission::<(), _>(
        schema,
        &record.value,
        limits,
        |_, _, bytes| work.reserve("classification-reference-plan", Some(bytes)),
    )
    .map_err(|error| match error {
        EntityReferencePlanError::Admission(report) => report,
        EntityReferencePlanError::Reference(_) => structure::authoring_structure_rejection(
            "Converted records require valid declared entity references",
        ),
    })?;
    let mut missing = false;
    plan.visit(&record.value, |_, reference| {
        missing |= matches!(reference, EntityReference::SceneLocal { entity } if !context.namespace.contains(entity));
        Ok::<_, ()>(())
    }).map_err(|_| structure::authoring_structure_rejection(
        "Converted records require valid declared entity references",
    ))?;
    if missing {
        return Err(structure::authoring_structure_rejection(
            "Converted references must close over the complete source namespace",
        ));
    }
    let references = if schema
        .fields()
        .iter()
        .any(|field| field.shape().contains_kind(ComponentValueKind::AssetRef))
    {
        let limits = context.asset_reference_limits();
        collect_declared_effective_asset_references_with_admission(
            schema,
            record.version,
            &record.value,
            nara_reflect::EntityReferenceTraversalLimits::new(
                limits.nodes(),
                limits.bytes(),
                limits.depth(),
            ),
            |_, _, bytes| work.reserve("classification-asset-references", Some(bytes)),
        )
        .map_err(|error| match error {
            AssetReferenceCollectionError::Admission(report) => report,
            AssetReferenceCollectionError::Reference(_) => {
                structure::authoring_structure_rejection(
                    "Converted records require valid declared asset references",
                )
            }
        })?
    } else {
        Vec::new()
    };
    context
        .reserve_asset_references(&references)
        .map_err(publication_report)?;
    if context.record_facts.len() == context.record_facts.capacity() {
        let capacity = context
            .record_facts
            .len()
            .checked_mul(2)
            .unwrap_or(usize::MAX)
            .max(4);
        work.reserve(
            "classification-record-vector",
            capacity.checked_mul(size_of::<AuthoringRecordFact>()),
        )?;
        context
            .record_facts
            .reserve_exact(capacity - context.record_facts.len());
    }
    context.record_facts.push(AuthoringRecordFact {
        location: AuthoringRecordLocation::EntityComponent {
            entity: entity.clone(),
        },
        owner: classified.owner,
        resolved_owner: classified.resolved_owner,
        component: component.clone(),
        version: record.version,
        readiness: classified.readiness,
        adapter: classified.adapter,
        schema_declared_asset_references: references,
    });
    Ok(())
}

fn placement_index_bytes(document: &SceneDocument) -> Option<usize> {
    let mut bytes = size_of::<BTreeMap<SceneEntityId, crate::ScenePlacement>>();
    for (parent, siblings) in std::iter::once((None, document.roots.as_slice())).chain(
        document
            .entities
            .iter()
            .map(|entity| (Some(&entity.id), entity.children.as_slice())),
    ) {
        if let Some(parent) = parent {
            bytes =
                bytes.checked_add(crate::provenance::provenance_string_bytes(parent.as_str())?)?;
        }
        for (index, entity) in siblings.iter().enumerate() {
            bytes = bytes
                .checked_add(
                    size_of::<SceneEntityId>()
                        + size_of::<crate::ScenePlacement>()
                        + 6 * size_of::<usize>(),
                )?
                .checked_add(crate::provenance::provenance_string_bytes(entity.as_str())?)?;
            if let Some(parent) = parent {
                bytes = bytes
                    .checked_add(crate::provenance::provenance_string_bytes(parent.as_str())?)?;
            }
            if index > 0 {
                bytes = bytes.checked_add(crate::provenance::provenance_string_bytes(
                    siblings[index - 1].as_str(),
                )?)?;
            }
        }
    }
    Some(bytes)
}
