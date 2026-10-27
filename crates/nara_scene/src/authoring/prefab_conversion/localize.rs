//! Localizes an actual selected compilation before ordinary source preparation.

use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_reflect::{
    __private::{EntityReferencePlanError, plan_declared_entity_references_with_admission},
    ComponentEntityReferenceRewriteError, ComponentValueMaterializationLimits, EntityReference,
};

use super::{LocalizedPrefabEntity, PrefabConversionError, PrefabConversionFootprint};
use crate::{
    AuthoringSchemaBasis, PrefabExpansionOptions, PrefabProjectionPath, ResolvedPrefabSource,
    SceneAuthoringHistoryLimits, SceneEntityId, SceneEntityRecord, SceneFileLimits,
    ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    ScenePlacement, ScenePlacementOwner,
    authoring::{creation::allocate_entity_id, history_error_report},
    patch::bounded_scene_entities_source_cost,
    prefab::{
        AuthoringPrefabOmissionReason, PrefabExpansionWorkingSet, PrefabOverlayCostLimits,
        SelectedInstanceCompilation, SelectedInstanceSource, prefab_instance_cost,
        try_compile_selected_authoring_instance_with_loader,
    },
};

mod dependencies;
pub(super) use dependencies::CapturedPrefabSources;

pub(super) struct LocalizedConversion {
    pub(super) patch: ScenePatchDocument,
    pub(super) footprint: PrefabConversionFootprint,
    pub(super) entities: Vec<LocalizedPrefabEntity>,
    pub(super) dependencies: CapturedPrefabSources,
    pub(super) asset_references: Vec<nara_reflect::DeclaredAssetReference>,
}

pub(super) fn prepare<F, E>(
    source: SelectedInstanceSource<'_>,
    anchor: &SceneEntityId,
    basis: &AuthoringSchemaBasis,
    history: SceneAuthoringHistoryLimits,
    options: PrefabExpansionOptions,
    database: Option<&ProjectAssetDatabase>,
    mut loader: F,
) -> Result<LocalizedConversion, PrefabConversionError<E>>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    let value_nodes = nara_core::ItemLimit::new(
        history
            .max_entry_value_nodes()
            .min(options.limits.materialized_value_nodes().get()),
    )
    .ok_or_else(|| PrefabConversionError::Source(copy_limit()))?;
    let value_bytes = nara_core::ByteLimit::new(
        history
            .max_entry_logical_bytes()
            .min(options.limits.materialized_value_bytes().get()),
    )
    .ok_or_else(|| PrefabConversionError::Source(copy_limit()))?;
    let mut limits = options
        .limits
        .with_materialized_value_nodes(value_nodes)
        .with_materialized_value_bytes(value_bytes);
    let records = match &source {
        SelectedInstanceSource::Scene(document) => &document.entities,
        SelectedInstanceSource::Prefab(document) => &document.entities,
    };
    let mut dependencies = CapturedPrefabSources::new(limits.working_bytes().get());
    let compiler_limit = limits
        .working_bytes()
        .get()
        .checked_sub(dependencies.retained_bytes())
        .and_then(nara_core::ByteLimit::new)
        .ok_or_else(|| PrefabConversionError::Source(dependencies::source_budget()))?;
    limits = limits.with_working_bytes(compiler_limit);
    let mut compiled = try_compile_selected_authoring_instance_with_loader(
        source,
        anchor,
        basis,
        options.with_limits(limits),
        database,
        |request, working| {
            let mut loading = working
                .including_retained_working_bytes(dependencies.retained_bytes())
                .ok_or_else(|| PrefabConversionError::Source(dependencies::source_budget()))?;
            let loaded = loader(request, &mut loading).map_err(PrefabConversionError::Loader)?;
            let remaining = loading
                .working_byte_limit()
                .get()
                .checked_sub(dependencies.retained_bytes())
                .and_then(nara_core::ByteLimit::new)
                .ok_or_else(|| PrefabConversionError::Source(dependencies::source_budget()))?;
            working.restrict_working_bytes(remaining);
            if let Some(source) = &loaded {
                dependencies
                    .capture(request, source, basis, working)
                    .map_err(PrefabConversionError::Source)?;
            }
            Ok(loaded)
        },
    )?;
    let asset_references = std::mem::take(&mut compiled.asset_references);
    let (patch, footprint, entities) =
        lower(records, anchor, basis, history, compiled).map_err(PrefabConversionError::Source)?;
    Ok(LocalizedConversion {
        patch,
        footprint,
        entities,
        dependencies,
        asset_references,
    })
}

fn lower(
    source: &[SceneEntityRecord],
    anchor: &SceneEntityId,
    basis: &AuthoringSchemaBasis,
    history: SceneAuthoringHistoryLimits,
    compiled: SelectedInstanceCompilation,
) -> Result<
    (
        ScenePatchDocument,
        PrefabConversionFootprint,
        Vec<LocalizedPrefabEntity>,
    ),
    ScenePatchReport,
> {
    if compiled.budget_failed
        || compiled.work_bytes.is_none()
        || compiled.failure.is_some()
        || compiled.diagnostics.has_errors()
    {
        return Err(ScenePatchReport {
            applied: false,
            inverse: None,
            diagnostics: compiled.diagnostics,
        });
    }
    if compiled.omissions.iter().any(|omission| {
        matches!(
            omission.reason,
            AuthoringPrefabOmissionReason::RecordUnavailable(_)
        )
    }) {
        return Err(refused(
            "Conversion cannot localize an unavailable surviving projected record",
        ));
    }
    let fragment = compiled
        .fragment
        .ok_or_else(|| refused("Conversion requires complete selected semantic data"))?;
    if fragment.entities.len() >= crate::authoring_decode::MAX_AUTHORING_EDIT_OPERATIONS {
        return Err(refused(
            "Conversion exceeds the source transaction entity allowance",
        ));
    }
    let anchor_record = source
        .binary_search_by(|record| record.id.cmp(anchor))
        .ok()
        .map(|index| &source[index])
        .ok_or_else(|| refused("Conversion requires an actual direct source anchor"))?;
    let original = anchor_record
        .prefab
        .as_ref()
        .ok_or_else(|| refused("Conversion requires a source instance declaration"))?;
    let replay = ScenePatchApplyLimits::default();
    let maximum = history
        .max_entry_logical_bytes()
        .min(replay.validation_value_bytes().get());
    let nodes = history
        .max_entry_value_nodes()
        .min(replay.validation_value_nodes().get());
    let raw =
        bounded_scene_entities_source_cost(&fragment.entities, replay).ok_or_else(copy_limit)?;
    let old = prefab_instance_cost(
        original,
        PrefabOverlayCostLimits {
            nodes,
            depth: SceneFileLimits::default().shape().depth().get(),
            declarations: nodes,
            entities: nodes,
            placements: nodes,
            components: nodes,
            prefab_instances: nodes,
            value_nodes: nodes,
            value_bytes: maximum / 2,
            logical_bytes: maximum / 2,
        },
    )
    .map_err(|_| copy_limit())?;
    if raw
        .value_cost()
        .nodes()
        .saturating_add(old.value_cost.nodes().saturating_mul(2))
        > nodes
    {
        return Err(copy_limit());
    }
    let mut work = LocalizationWork { bytes: 0, maximum };
    work.reserve(raw.logical_bytes())?;
    work.reserve(old.logical_bytes().checked_mul(2).ok_or_else(copy_limit)?)?;
    work.reserve(
        anchor
            .as_str()
            .len()
            .checked_mul(2)
            .ok_or_else(copy_limit)?,
    )?;
    let root_placement_bytes = anchor
        .as_str()
        .len()
        .checked_add(
            anchor_record
                .children
                .first()
                .map_or(0, |child| child.as_str().len()),
        )
        .and_then(|bytes| bytes.checked_mul(fragment.roots.len().checked_add(1)?))
        .ok_or_else(copy_limit)?;
    work.reserve(root_placement_bytes)?;
    for record in &fragment.entities {
        // Two temporary ID indices, the replay footprint and the retained review mapping.
        work.reserve(512)?;
        work.reserve(record.id.as_str().len())?;
        let provenance = fragment.provenance.get(&record.id).ok_or_else(|| {
            refused("Every localized record requires its complete compiler provenance")
        })?;
        let path = provenance
            .projection_path()
            .filter(|path| path.root_anchor() == anchor)
            .ok_or_else(|| refused("Conversion cannot claim another source occurrence"))?;
        work.reserve(
            crate::provenance::provenance_entry_bytes(&record.id, provenance.local_entity_id())
                .and_then(|bytes| {
                    PrefabProjectionPath::extended_allocation_bytes(
                        Some(path),
                        path.root_anchor(),
                        0,
                    )
                    .and_then(|path_bytes| bytes.checked_add(path_bytes))
                })
                .ok_or_else(copy_limit)?,
        )?;
    }

    let mut identifiers = BTreeMap::new();
    let mut allocated = BTreeSet::new();
    for record in &fragment.entities {
        let entity = allocate_entity_id(|candidate| {
            source
                .binary_search_by(|record| record.id.cmp(candidate))
                .is_ok()
                || allocated.contains(candidate)
        })?;
        work.reserve(entity.as_str().len().saturating_mul(3))?;
        allocated.insert(entity.clone());
        identifiers.insert(record.id.clone(), entity);
    }
    let created = allocated.into_iter().collect::<Vec<_>>();
    let footprint = PrefabConversionFootprint::new(anchor, &created, maximum)?;
    let mut records = fragment
        .entities
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut provenance = fragment.provenance;
    let root_placement = match anchor_record.children.first() {
        Some(first) => ScenePlacement::before(
            ScenePlacementOwner::Parent {
                parent: anchor.clone(),
            },
            first.clone(),
        ),
        None => ScenePlacement::last(ScenePlacementOwner::Parent {
            parent: anchor.clone(),
        }),
    };
    let mut pending = fragment
        .roots
        .into_iter()
        .rev()
        .map(|root| (root, root_placement.clone()))
        .collect::<Vec<_>>();
    let mut operations = vec![ScenePatchOperation::SetPrefabInstance {
        entity: anchor.clone(),
        expected: Some(original.clone()),
        value: None,
    }];
    let mut entities = Vec::new();
    let reference_limits =
        crate::validation::reference_traversal_limits(ComponentValueMaterializationLimits::new(
            nara_core::ItemLimit::new(nodes).ok_or_else(copy_limit)?,
            nara_core::ByteLimit::new(maximum).ok_or_else(copy_limit)?,
            ComponentValueMaterializationLimits::default().depth(),
        ));
    while let Some((old_id, placement)) = pending.pop() {
        let mut record = records
            .remove(&old_id)
            .ok_or_else(|| refused("Conversion requires one complete selected entity forest"))?;
        if record.prefab.is_some() {
            return Err(refused(
                "Conversion requires fully composed selected records",
            ));
        }
        let entity = identifiers.get(&old_id).ok_or_else(copy_limit)?.clone();
        for (component, payload) in &mut record.components {
            let schema = basis
                .schema(component)
                .filter(|schema| {
                    basis.has_authoring_adapter(component)
                        && schema.version() == payload.version
                        && schema.has_capability(nara_reflect::ComponentCapability::Scene)
                })
                .ok_or_else(|| refused("Conversion requires the selected current data Schema"))?;
            let owner = basis
                .component_schema_owner(component)
                .ok_or_else(copy_limit)?;
            if payload.owner().is_some_and(|claim| claim != owner) {
                return Err(refused(
                    "Conversion cannot replace a component's declared Schema owner",
                ));
            }
            if payload.owner().is_none() {
                work.reserve(owner.as_str().len())?;
                payload.owner = Some(owner.clone());
            }
            let plan = plan_declared_entity_references_with_admission::<ScenePatchReport, _>(
                schema,
                &payload.value,
                reference_limits,
                |_, _, allocation_bytes| work.reserve(allocation_bytes),
            )
            .map_err(|error| match error {
                EntityReferencePlanError::Reference(error) => reference_error(error),
                EntityReferencePlanError::Admission(report) => report,
            })?;
            let mut reference_refusal = None;
            plan.visit(&payload.value, |_, reference| {
                if reference_refusal.is_some() {
                    return Ok::<_, ScenePatchReport>(());
                }
                let admitted = match reference {
                    EntityReference::SceneLocal { entity } => {
                        let target = identifiers.get(entity).unwrap_or(entity);
                        if !identifiers.contains_key(entity)
                            && source
                                .binary_search_by(|record| record.id.cmp(entity))
                                .is_err()
                        {
                            Err(refused(
                                "Conversion cannot retain another occurrence's projected locator",
                            ))
                        } else {
                            work.reserve(target.as_str().len())
                        }
                    }
                    EntityReference::Persistent { entity } => {
                        work.reserve(entity.namespace.as_str().len())
                    }
                };
                // Keep budget refusal outside visit's path-bearing error wrapper.
                reference_refusal = admitted.err();
                Ok(())
            })
            .map_err(reference_error)?;
            if let Some(report) = reference_refusal {
                return Err(report);
            }
            plan.rewrite_in_place(&mut payload.value, |_, reference| {
                Ok::<_, ScenePatchReport>(match reference {
                    EntityReference::SceneLocal { entity } => EntityReference::SceneLocal {
                        entity: identifiers.get(entity).unwrap_or(entity).clone(),
                    },
                    EntityReference::Persistent { .. } => reference.clone(),
                })
            })
            .map_err(reference_error)?;
        }
        let children = std::mem::take(&mut record.children);
        record.id = entity.clone();
        for child in children.into_iter().rev() {
            pending.push((
                child,
                ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: entity.clone(),
                }),
            ));
        }
        entities.push(LocalizedPrefabEntity {
            provenance: provenance.remove(&old_id).ok_or_else(copy_limit)?,
            entity,
        });
        operations.push(ScenePatchOperation::AddEntity {
            entity: record,
            placement,
        });
    }
    if !records.is_empty() || !provenance.is_empty() {
        return Err(refused(
            "Conversion requires a complete reachable selected entity forest",
        ));
    }
    Ok((ScenePatchDocument::new(operations), footprint, entities))
}

struct LocalizationWork {
    bytes: usize,
    maximum: usize,
}

impl LocalizationWork {
    fn reserve(&mut self, bytes: usize) -> Result<(), ScenePatchReport> {
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(copy_limit)?;
        if self.bytes > self.maximum {
            return Err(copy_limit());
        }
        Ok(())
    }
}

pub(super) fn copy_limit() -> ScenePatchReport {
    history_error_report(
        "scene.authoring-prefab-conversion-copy-limit",
        "Conversion data, remap and declaration snapshots exceed their source preparation allowance",
    )
}

fn reference_error(
    error: ComponentEntityReferenceRewriteError<ScenePatchReport>,
) -> ScenePatchReport {
    match error {
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. } => copy_limit(),
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        _ => refused("Conversion requires complete declared entity-reference slots"),
    }
}

pub(super) fn refused(summary: &'static str) -> ScenePatchReport {
    history_error_report("scene.authoring-prefab-conversion-rejected", summary)
}

#[cfg(test)]
mod tests;
