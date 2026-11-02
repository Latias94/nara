//! Localizes an actual selected compilation before ordinary source preparation.

use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetRef, AssetReferenceResolver};
use nara_reflect::{
    __private::{
        DeclaredEntityReferencePlan, EntityReferencePlanError,
        plan_declared_entity_references_with_admission,
        plan_declared_value_entity_references_with_admission,
    },
    ComponentEntityReferenceRewriteError, ComponentValue, ComponentValueMaterializationLimits,
    EntityReference,
};

use super::work::ConversionWork;
use super::{LocalizedPrefabEntity, PrefabConversionError, PrefabConversionFootprint};
use crate::{
    AuthoringSchemaBasis, PrefabEntityTarget, PrefabInstanceBinding, PrefabProjectionPath,
    ResolvedPrefabSource, SceneAuthoringHistoryLimits, SceneCompilationOptions, SceneEntityId,
    SceneEntityPlacement, SceneEntityRecord, SceneFileLimits, ScenePatchApplyLimits,
    ScenePatchDocument, ScenePatchOperation, ScenePatchReport, ScenePlacement,
    ScenePlacementParent,
    authoring::{creation::allocate_entity_id, history_error_report},
    patch::bounded_component_records_source_cost,
    prefab::{
        AuthoringSceneCompilationOmissionReason, PrefabOverlayCostLimits,
        SceneCompilationWorkingSet, SelectedInstanceCompilation, SelectedInstanceSource,
        prefab_instance_cost, try_compile_selected_authoring_instance_with_loader,
    },
};

mod dependencies;
mod source;
pub(super) use dependencies::CapturedPrefabSources;

pub(super) struct LocalizedConversion {
    pub(super) patch: ScenePatchDocument,
    pub(super) footprint: PrefabConversionFootprint,
    pub(super) entities: Vec<LocalizedPrefabEntity>,
    pub(super) dependencies: CapturedPrefabSources,
    pub(super) asset_references: Vec<nara_reflect::DeclaredAssetReference>,
    pub(super) work: ConversionWork,
}

pub(super) fn prepare<F, E>(
    source: SelectedInstanceSource<'_>,
    anchor: &SceneEntityId,
    basis: &AuthoringSchemaBasis,
    history: SceneAuthoringHistoryLimits,
    source_limits: SceneFileLimits,
    options: SceneCompilationOptions,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    mut loader: F,
) -> Result<LocalizedConversion, PrefabConversionError<E>>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
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
    let (placements, records) = match &source {
        SelectedInstanceSource::Scene(document) => (&document.placements, &document.entities),
        SelectedInstanceSource::Prefab(document) => (&document.placements, &document.entities),
    };
    // Reading retained source is independent of how much selected output is materialized.
    // Preserve the actual source admission and configured reference-work ceilings.
    let source_references = crate::validation::reference_traversal_limits(
        source_limits.component_materialization_limits(),
    );
    let source_references = nara_reflect::EntityReferenceTraversalLimits::new(
        nara_core::ItemLimit::new(
            source_references
                .nodes()
                .get()
                .min(options.limits.reference_work_nodes().get()),
        )
        .expect("source read limits are nonzero"),
        nara_core::ByteLimit::new(
            source_references
                .bytes()
                .get()
                .min(options.limits.reference_work_bytes().get()),
        )
        .expect("source read limits are nonzero"),
        source_references.depth(),
    );
    let reference_queries = source::collect_source_reference_queries(
        records,
        anchor,
        basis,
        source_references,
        limits.working_bytes(),
    )
    .map_err(PrefabConversionError::Source)?;
    let dependency_limit = limits
        .working_bytes()
        .get()
        .checked_sub(reference_queries.work_bytes)
        .ok_or_else(|| PrefabConversionError::Source(copy_limit()))?;
    let mut dependencies = CapturedPrefabSources::new(dependency_limit);
    let mut working_limit = limits.working_bytes();
    let compiler_limit = dependency_limit
        .checked_sub(dependencies.retained_bytes())
        .and_then(nara_core::ByteLimit::new)
        .ok_or_else(|| PrefabConversionError::Source(dependencies::source_budget()))?;
    limits = limits.with_working_bytes(compiler_limit);
    let mut compiled = try_compile_selected_authoring_instance_with_loader(
        source,
        anchor,
        &reference_queries.values,
        reference_queries.reference_work,
        basis,
        options.with_limits(limits),
        asset_resolution,
        |request, working| {
            let mut loading = working
                .including_retained_working_bytes(
                    dependencies
                        .retained_bytes()
                        .checked_add(reference_queries.work_bytes)
                        .ok_or_else(|| PrefabConversionError::Source(copy_limit()))?,
                )
                .ok_or_else(|| PrefabConversionError::Source(dependencies::source_budget()))?;
            let loaded = loader(request, &mut loading).map_err(PrefabConversionError::Loader)?;
            working_limit = nara_core::ByteLimit::new(
                working_limit.get().min(loading.working_byte_limit().get()),
            )
            .ok_or_else(|| PrefabConversionError::Source(dependencies::source_budget()))?;
            let remaining = loading
                .working_byte_limit()
                .get()
                .checked_sub(dependencies.retained_bytes())
                .and_then(|bytes| bytes.checked_sub(reference_queries.work_bytes))
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
    if compiled.budget_failed
        || compiled.work_bytes.is_none()
        || compiled.failure.is_some()
        || compiled.diagnostics.has_errors()
    {
        return Err(PrefabConversionError::Source(ScenePatchReport {
            applied: false,
            inverse: None,
            diagnostics: compiled.diagnostics,
        }));
    }
    let mut work = ConversionWork::after_compilation(
        working_limit,
        compiled
            .work_bytes
            .ok_or_else(|| PrefabConversionError::Source(copy_limit()))?,
        dependencies.retained_bytes(),
    )
    .map_err(PrefabConversionError::Source)?;
    work.reserve(
        "source-reference-inventory",
        Some(reference_queries.work_bytes),
    )
    .map_err(PrefabConversionError::Source)?;
    work.reserve(
        "conversion-result",
        Some(std::mem::size_of::<LocalizedConversion>()),
    )
    .map_err(PrefabConversionError::Source)?;
    let asset_references = std::mem::take(&mut compiled.asset_references);
    let (patch, footprint, entities) = lower(
        placements, records, anchor, basis, history, compiled, &mut work,
    )
    .map_err(PrefabConversionError::Source)?;
    Ok(LocalizedConversion {
        patch,
        footprint,
        entities,
        dependencies,
        asset_references,
        work,
    })
}

fn lower(
    placements: &[SceneEntityPlacement],
    source: &[SceneEntityRecord],
    anchor: &SceneEntityId,
    basis: &AuthoringSchemaBasis,
    history: SceneAuthoringHistoryLimits,
    compiled: SelectedInstanceCompilation,
    conversion_work: &mut ConversionWork,
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
    if compiled.omissions.iter().any(|omission| {
        matches!(
            omission.reason,
            AuthoringSceneCompilationOmissionReason::RecordUnavailable(_)
        ) && (&omission.entity != anchor
            || !anchor_record.components.contains_key(&omission.component))
    }) {
        return Err(refused(
            "Conversion cannot localize an unavailable surviving projected record",
        ));
    }
    let replay = ScenePatchApplyLimits::default();
    let maximum = history
        .max_entry_logical_bytes()
        .min(replay.validation_value_bytes().get());
    let nodes = history
        .max_entry_value_nodes()
        .min(replay.validation_value_nodes().get());
    let raw = bounded_component_records_source_cost(
        fragment
            .entities
            .iter()
            .flat_map(|entity| entity.components.values()),
        replay,
    )
    .ok_or_else(copy_limit)?;
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
    let mut work = LocalizationWork {
        bytes: 0,
        maximum,
        conversion_work,
    };
    work.reserve(raw.logical_bytes())?;
    work.reserve(old.logical_bytes().checked_mul(2).ok_or_else(copy_limit)?)?;
    work.conversion_work.reserve(
        "localization-declaration",
        old.nodes
            .checked_mul(512)
            .and_then(|bytes| bytes.checked_add(old.value_cost.nodes().checked_mul(96)?))
            .and_then(|bytes| bytes.checked_mul(2)),
    )?;
    work.reserve(
        anchor
            .as_str()
            .len()
            .checked_mul(2)
            .ok_or_else(copy_limit)?,
    )?;
    let root_bound = matches!(original.binding, PrefabInstanceBinding::Root { .. });
    if root_bound && fragment.roots.as_slice() != std::slice::from_ref(anchor) {
        return Err(refused(
            "Root-bound conversion requires its merged original anchor",
        ));
    }
    for record in &fragment.entities {
        // Two temporary ID indices, the replay footprint and the retained review mapping.
        work.reserve(512)?;
        work.reserve(record.id.as_str().len())?;
        let provenance = fragment.provenance.get(&record.id).ok_or_else(|| {
            refused("Every localized record requires its complete compiler provenance")
        })?;
        let path = match provenance.projection_path() {
            Some(path) if path.root_anchor() == anchor => Some(path),
            None if root_bound
                && &record.id == anchor
                && provenance.local_entity_id() == anchor =>
            {
                None
            }
            _ => return Err(refused("Conversion cannot claim another source occurrence")),
        };
        work.reserve(
            crate::provenance::provenance_entry_bytes(&record.id, provenance.local_entity_id())
                .and_then(|bytes| match path {
                    Some(path) => PrefabProjectionPath::extended_allocation_bytes(
                        Some(path),
                        path.root_anchor(),
                        0,
                    )
                    .and_then(|path_bytes| bytes.checked_add(path_bytes)),
                    None => Some(bytes),
                })
                .ok_or_else(copy_limit)?,
        )?;
    }

    let mut identifiers = BTreeMap::new();
    let mut allocated = BTreeSet::new();
    for record in &fragment.entities {
        if root_bound && &record.id == anchor {
            identifiers.insert(record.id.clone(), anchor.clone());
            continue;
        }
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
    let mut localized_placements = source::localize_placements(
        placements,
        &fragment,
        &identifiers,
        anchor,
        source,
        &mut work,
    )?;
    let targets = source::SourceTargets {
        entities: &fragment.targets,
        references: &fragment.reference_targets,
        identifiers: &identifiers,
        anchor,
        source,
    };
    let mut provenance = fragment.provenance;
    for mut expanded in fragment.entities {
        let entity = identifiers
            .get(&expanded.id)
            .ok_or_else(copy_limit)?
            .clone();
        let merged_anchor = root_bound && &expanded.id == anchor;
        if merged_anchor {
            // Containing complete records win over the inherited root. Preserve raw
            // omissions and hidden defaults; replace only actual reference changes.
            for (component, original) in &anchor_record.components {
                expanded.components.remove(component);
                if let std::borrow::Cow::Owned(value) = source::localize_record(
                    original,
                    component,
                    basis,
                    targets,
                    reference_limits,
                    &mut work,
                )? {
                    operations.push(ScenePatchOperation::ReplaceComponent {
                        entity: entity.clone(),
                        component: component.clone(),
                        value,
                    });
                }
            }
        }
        for (component, payload) in &mut expanded.components {
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
            localize_references(
                &mut payload.value,
                &plan,
                &identifiers,
                &fragment.targets,
                source,
                &mut work,
            )?;
            for (id, value) in &mut payload.contextual_defaults {
                let field = schema
                    .fields()
                    .iter()
                    .find(|field| field.id() == id)
                    .ok_or_else(|| {
                        refused("Conversion requires the declared contextual default field")
                    })?;
                let plan =
                    plan_declared_value_entity_references_with_admission::<ScenePatchReport, _>(
                        field,
                        field.shape(),
                        value,
                        reference_limits,
                        |_, _, allocation_bytes| work.reserve(allocation_bytes),
                    )
                    .map_err(|error| match error {
                        EntityReferencePlanError::Reference(error) => reference_error(error),
                        EntityReferencePlanError::Admission(report) => report,
                    })?;
                localize_references(
                    value,
                    &plan,
                    &identifiers,
                    &fragment.targets,
                    source,
                    &mut work,
                )?;
            }
        }
        entities.push(LocalizedPrefabEntity {
            provenance: provenance.remove(&expanded.id).ok_or_else(copy_limit)?,
            entity: entity.clone(),
        });
        if merged_anchor {
            for (component, value) in expanded.components {
                operations.push(ScenePatchOperation::AddComponent {
                    entity: entity.clone(),
                    component,
                    value,
                });
            }
        } else {
            operations.push(ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord {
                    id: entity,
                    components: expanded.components,
                    prefab: None,
                },
            });
        }
    }
    if !provenance.is_empty() {
        return Err(refused(
            "Conversion requires a complete reachable selected entity forest",
        ));
    }
    for record in source {
        if !root_bound || &record.id != anchor {
            for (component, original) in &record.components {
                let value = source::localize_record(
                    original,
                    component,
                    basis,
                    targets,
                    reference_limits,
                    &mut work,
                )?;
                if value.as_ref() != original {
                    operations.push(ScenePatchOperation::ReplaceComponent {
                        entity: record.id.clone(),
                        component: component.clone(),
                        value: value.into_owned(),
                    });
                }
            }
        }
        if &record.id != anchor
            && let Some(original) = &record.prefab
            && let std::borrow::Cow::Owned(value) =
                source::localize_instance(original, basis, targets, reference_limits, &mut work)?
        {
            operations.push(ScenePatchOperation::SetPrefabInstance {
                entity: record.id.clone(),
                expected: Some(original.clone()),
                value: Some(value),
            });
        }
    }
    if localized_placements != placements {
        operations.push(ScenePatchOperation::SetPlacements {
            expected: placements.to_vec(),
            value: std::mem::take(&mut localized_placements),
        });
    }
    let patch = ScenePatchDocument::new(operations);
    let footprint = PrefabConversionFootprint::new(anchor, &created, &patch, maximum)?;
    Ok((patch, footprint, entities))
}

fn localize_references(
    value: &mut ComponentValue,
    plan: &DeclaredEntityReferencePlan,
    identifiers: &BTreeMap<SceneEntityId, SceneEntityId>,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    source: &[SceneEntityRecord],
    work: &mut LocalizationWork<'_>,
) -> Result<(), ScenePatchReport> {
    let mut reference_refusal = None;
    plan.visit(value, |_, reference| {
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
                    external_target(entity, targets).and_then(|target| {
                        let bytes = target.through.iter().try_fold(
                            256usize
                                .checked_add(target.entity.entity().as_str().len())
                                .ok_or_else(copy_limit)?,
                            |bytes, hop| {
                                bytes
                                    .checked_add(std::mem::size_of::<crate::PrefabInstanceHop>())
                                    .and_then(|bytes| {
                                        bytes.checked_add(hop.anchor.entity().as_str().len())
                                    })
                                    .and_then(|bytes| {
                                        bytes.checked_add(
                                            hop.expected_base.checked_retained_bytes()?,
                                        )
                                    })
                                    .ok_or_else(copy_limit)
                            },
                        )?;
                        work.reserve(bytes)
                    })
                } else {
                    work.reserve(target.as_str().len())
                }
            }
            EntityReference::Persistent { entity } => work.reserve(entity.namespace.as_str().len()),
            EntityReference::Authored { .. } => Err(refused(
                "Conversion requires resolved compiled entity references",
            )),
        };
        // Keep budget refusal outside visit's path-bearing error wrapper.
        reference_refusal = admitted.err();
        Ok(())
    })
    .map_err(reference_error)?;
    if let Some(report) = reference_refusal {
        return Err(report);
    }
    plan.rewrite_in_place(value, |_, reference| {
        Ok::<_, ScenePatchReport>(match reference {
            EntityReference::SceneLocal { entity } => {
                if let Some(local) = identifiers.get(entity) {
                    EntityReference::SceneLocal {
                        entity: local.clone(),
                    }
                } else if source
                    .binary_search_by(|record| record.id.cmp(entity))
                    .is_ok()
                {
                    EntityReference::SceneLocal {
                        entity: entity.clone(),
                    }
                } else {
                    EntityReference::Authored {
                        reference: external_target(entity, targets)?.clone().into(),
                    }
                }
            }
            EntityReference::Persistent { .. } => reference.clone(),
            EntityReference::Authored { .. } => {
                return Err(refused(
                    "Conversion requires resolved compiled entity references",
                ));
            }
        })
    })
    .map_err(reference_error)
}

fn external_target<'a>(
    entity: &SceneEntityId,
    targets: &'a BTreeMap<PrefabEntityTarget, SceneEntityId>,
) -> Result<&'a PrefabEntityTarget, ScenePatchReport> {
    targets
        .iter()
        .filter(|(_, actual)| *actual == entity)
        .min_by_key(|(target, _)| target.through.len())
        .map(|(target, _)| target)
        .ok_or_else(|| refused("Conversion requires the compiler's actual external source target"))
}

struct LocalizationWork<'a> {
    bytes: usize,
    maximum: usize,
    conversion_work: &'a mut ConversionWork,
}

impl LocalizationWork<'_> {
    fn reserve(&mut self, bytes: usize) -> Result<(), ScenePatchReport> {
        self.bytes = self.bytes.checked_add(bytes).ok_or_else(copy_limit)?;
        if self.bytes > self.maximum {
            return Err(copy_limit());
        }
        self.conversion_work.reserve("localization", Some(bytes))?;
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
