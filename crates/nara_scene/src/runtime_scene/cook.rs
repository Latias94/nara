use std::{borrow::Cow, collections::BTreeMap, mem::size_of};

use nara_asset::{AssetProductRef, AssetRef, AssetReferenceResolver, ImportedProductId};
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    __private::{
        AssetReferenceCollectionError, EffectiveValueAdmissionError, EntityReferencePlanError,
        collect_declared_effective_asset_references_with_admission,
        effective_component_value_with_admission, plan_declared_entity_references_with_admission,
    },
    ComponentCodecError, ComponentFieldPathSegment, ComponentValueMaterializationLimits,
    DeclaredAssetReference, EntityReference, EntityReferenceTraversalLimits,
    rewrite_declared_asset_references_with_admission,
};

use super::{
    RuntimeSceneCandidate, RuntimeSceneError, RuntimeSceneLimits,
    budget::{ProvenanceBudget, WorkBudget, provenance_cost, scene_budget_error, value_cost},
    identity, require_scene_source, validate_values, wire,
};
use crate::{
    ExpandedSceneCandidate, SceneComponentBindings, SceneEntityId,
    diagnostics::{error, with_codec_error},
    schema_authority::SchemaAuthority,
};

pub(super) fn cook(
    expanded: &ExpandedSceneCandidate,
    bindings: &SceneComponentBindings,
    asset_resolution: AssetReferenceResolver<'_>,
    limits: RuntimeSceneLimits,
) -> Result<RuntimeSceneCandidate, RuntimeSceneError> {
    require_scene_source(expanded.source_key())?;
    let mut work = WorkBudget::new(limits);
    provenance_cost(expanded.source_key(), expanded.provenance(), limits)?;
    work.document_clone(expanded.document())?;
    let diagnostics = crate::validation::validate_scene_structure(expanded.document());
    if diagnostics.has_errors() {
        return Err(RuntimeSceneError { diagnostics });
    }
    let identifiers = identity::canonical_ids(
        expanded.source_key(),
        expanded.document(),
        expanded.provenance(),
        false,
        &mut work,
    )?;
    // Everything below owns its unpublished copies. Failure never edits the source candidate.
    let mut document = expanded.document().clone();
    let mut used_nodes = 0usize;
    let mut used_bytes = 0usize;
    let traversal = crate::validation::reference_traversal_limits(limits.value_limits());
    for root in &mut document.roots {
        *root = copy_id(root, &identifiers, &mut work)?;
    }
    for entity in &mut document.entities {
        entity.id = copy_id(&entity.id, &identifiers, &mut work)?;
        for child in &mut entity.children {
            *child = copy_id(child, &identifiers, &mut work)?;
        }
        for (component, record) in &mut entity.components {
            let schema = SchemaAuthority::Mixed(bindings).current_schema(component, record)
                .map_err(|_| RuntimeSceneError::refused(
                    "scene.runtime-component-definition",
                    "Runtime cook requires a current Scene-capable component and its selected owner",
                ))?;
            let residual = residual_limits(limits, used_nodes, used_bytes)?;
            let effective = effective_component_value_with_admission(
                schema,
                &record.value,
                residual,
                |nodes, bytes, allocated| work.admit(nodes, bytes, allocated),
            )
            .map_err(|failure| match failure {
                EffectiveValueAdmissionError::Admission(error) => error,
                EffectiveValueAdmissionError::Value(error) => component_error(&error, schema),
            })?;
            if let Cow::Owned(value) = effective {
                record.value = value;
            }
            let references = collect_declared_effective_asset_references_with_admission(
                schema,
                record.version,
                &record.value,
                traversal,
                |nodes, bytes, allocated| work.admit(nodes, bytes, allocated),
            )
            .map_err(|failure| match failure {
                AssetReferenceCollectionError::Admission(error) => error,
                AssetReferenceCollectionError::Reference(_) => reference_error(),
            })?;
            // Scoped Path lookup joins a canonical spelling. Admit each lookup's temporary
            // bytes before the rewrite callback allocates it; ROOT and Product lookups borrow.
            for reference in &references {
                work.reserve(asset_resolution.resolution_work_bytes(reference.asset_ref()))?;
            }
            rewrite_declared_asset_references_with_admission(
                &mut record.value,
                &references,
                asset_rewrite_limits(limits.value_limits(), &references),
                |reference| canonical_product(reference.asset_ref(), asset_resolution),
                |nodes, bytes, allocated| work.admit(nodes, bytes, allocated),
            )
            .map_err(|failure| match failure {
                nara_reflect::AssetReferenceRewriteError::Admission(error) => error,
                nara_reflect::AssetReferenceRewriteError::Rewrite { error, .. } => error,
                _ => reference_error(),
            })?;
            let plan = plan_declared_entity_references_with_admission::<(), RuntimeSceneError>(
                schema,
                &record.value,
                traversal,
                |nodes, bytes, allocated| work.admit(nodes, bytes, allocated),
            )
            .map_err(|failure| match failure {
                EntityReferencePlanError::Admission(error) => error,
                EntityReferencePlanError::Reference(_) => reference_error(),
            })?;
            plan.rewrite_in_place(&mut record.value, |_, reference| {
                match reference {
                    EntityReference::SceneLocal { entity } => Ok(EntityReference::SceneLocal {
                        entity: copy_id(entity, &identifiers, &mut work)?,
                    }),
                    EntityReference::Persistent { entity } => {
                        // Persistent references retain their independent namespace and axes.
                        // Their source clone was already admitted; rewriting creates one more.
                        work.reserve(Some(entity.namespace.as_str().len()))?;
                        Ok(reference.clone())
                    }
                }
            })
            .map_err(|failure| match failure {
                nara_reflect::ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
                _ => reference_error(),
            })?;
            let cost = value_cost(&record.value, used_nodes, used_bytes, &mut work)?;
            used_nodes = used_nodes.saturating_add(cost.nodes());
            used_bytes = used_bytes.saturating_add(cost.logical_bytes());
        }
    }
    document
        .entities
        .sort_unstable_by(|left, right| left.id.cmp(&right.id));
    crate::format::validate_scene_value_budget(&document, limits.base())
        .map_err(scene_budget_error)?;
    let mut provenance_budget = ProvenanceBudget::new(expanded.source_key(), limits)?;
    for (old, origin) in expanded.provenance() {
        provenance_budget.entry(identifiers.get(old).ok_or_else(reference_error)?, origin)?;
    }
    work.reserve(Some(provenance_budget.bytes()))?;
    let provenance = expanded
        .provenance()
        .iter()
        .map(|(old, origin)| {
            // The complete identifiers map was checked against every provenance entry above.
            identifiers
                .get(old)
                .map(|id| (id.clone(), origin.clone()))
                .ok_or_else(reference_error)
        })
        .collect::<Result<_, _>>()?;
    validate_values(&document, bindings, &mut work)?;
    work.witness(bindings)?;
    let candidate = RuntimeSceneCandidate {
        source_key: expanded.source_key().clone(),
        document,
        provenance,
        definitions: bindings.witness(),
        limits,
    };
    // Qualify the complete processed wire, including derived IDs and provenance. This is a
    // borrowed encoding and core shape preflight, not a source publication or second admission.
    wire::encode(&candidate, &mut work)?;
    Ok(candidate)
}

fn asset_rewrite_limits(
    values: ComponentValueMaterializationLimits,
    references: &[DeclaredAssetReference],
) -> EntityReferenceTraversalLimits {
    let (path_nodes, path_bytes) = references.iter().fold((0usize, 0usize), |cost, reference| {
        reference
            .path()
            .segments()
            .iter()
            .fold(cost, |(nodes, bytes), segment| {
                let length = match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
                };
                (nodes.saturating_add(1), bytes.saturating_add(length))
            })
    });
    // Rewrite validates one record per leaf, visits each path three times, then decodes and
    // encodes the reference values. Value limits still admit each complete input/output graph;
    // these are separate cumulative work units, including shared path prefixes on every visit.
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(
            values
                .nodes()
                .get()
                .saturating_mul(2)
                .saturating_add(references.len())
                .saturating_add(path_nodes.saturating_mul(3)),
        )
        .expect("nonzero value limits produce nonzero rewrite work limits"),
        ByteLimit::new(
            values
                .bytes()
                .get()
                .saturating_mul(2)
                .saturating_add(path_bytes.saturating_mul(3)),
        )
        .expect("nonzero value limits produce nonzero rewrite byte limits"),
        values.depth(),
    )
}

fn residual_limits(
    limits: RuntimeSceneLimits,
    used_nodes: usize,
    used_bytes: usize,
) -> Result<ComponentValueMaterializationLimits, RuntimeSceneError> {
    let maximum = limits.value_limits();
    let nodes =
        ItemLimit::new(maximum.nodes().get().saturating_sub(used_nodes)).ok_or_else(|| {
            RuntimeSceneError::budget(
                "component-value-nodes",
                used_nodes.saturating_add(1),
                maximum.nodes().get(),
            )
        })?;
    // A zero byte residual may still admit Null or empty containers. Reflect's borrowed default
    // planner accepts a nonzero typed ceiling; final cost admission enforces the exact residual.
    let bytes =
        ByteLimit::new(maximum.bytes().get().saturating_sub(used_bytes)).unwrap_or(ByteLimit::ONE);
    Ok(ComponentValueMaterializationLimits::new(
        nodes,
        bytes,
        maximum.depth(),
    ))
}

fn copy_id(
    source: &SceneEntityId,
    identifiers: &BTreeMap<&SceneEntityId, SceneEntityId>,
    work: &mut WorkBudget,
) -> Result<SceneEntityId, RuntimeSceneError> {
    let target = identifiers.get(source).ok_or_else(reference_error)?;
    work.reserve(Some(target.as_str().len()))?;
    Ok(target.clone())
}

fn canonical_product(
    reference: &AssetRef,
    asset_resolution: AssetReferenceResolver<'_>,
) -> Result<AssetRef, RuntimeSceneError> {
    // Borrowed lookups avoid copying a rejected path into an AssetDatabaseError.
    let source = match reference {
        AssetRef::Path(path) => asset_resolution.record_for_path(path),
        AssetRef::StableId(source) => asset_resolution.database().record_for_stable_id(*source),
        AssetRef::Product(product) => asset_resolution
            .database()
            .record_for_stable_id(product.source()),
    }
    .ok_or_else(|| {
        RuntimeSceneError::refused(
            "scene.runtime-asset-source-unavailable",
            "Runtime cook requires every declared asset source in the admitted source catalogue",
        )
    })?;
    match reference {
        AssetRef::Product(product) => Ok(AssetRef::Product(*product)),
        AssetRef::Path(_) | AssetRef::StableId(_) => {
            AssetProductRef::new(source.stable_id(), ImportedProductId::PRIMARY)
                .map(AssetRef::Product)
                .map_err(|_| {
                    RuntimeSceneError::refused(
                        "scene.runtime-asset-source-invalid",
                        "Runtime cook requires a nonnil stable source identity",
                    )
                })
        }
    }
}

fn reference_error() -> RuntimeSceneError {
    RuntimeSceneError::refused(
        "scene.runtime-reference",
        "Runtime Scene requires valid declared reference leaves and surviving local targets",
    )
}

fn component_error(
    error_value: &ComponentCodecError,
    schema: &nara_reflect::ComponentSchema,
) -> RuntimeSceneError {
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(with_codec_error(
        error(
            "scene.runtime-component-value",
            "Runtime component default materialization failed",
        ),
        error_value,
        Some(schema),
    ));
    RuntimeSceneError { diagnostics }
}
