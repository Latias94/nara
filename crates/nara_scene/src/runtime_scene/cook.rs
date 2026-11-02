use std::{collections::BTreeMap, mem::size_of};

use nara_asset::{AssetProductRef, AssetRef, AssetReferenceResolver, ImportedProductId};
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    __private::{
        AssetReferenceCollectionError, EntityReferencePlanError,
        collect_declared_effective_asset_references_with_admission,
        plan_declared_entity_references_with_admission,
    },
    ComponentFieldPathSegment, ComponentValueMaterializationLimits, DeclaredAssetReference,
    EntityReference, EntityReferenceTraversalLimits,
    rewrite_declared_asset_references_with_admission,
};

use super::{
    RuntimeSceneCandidate, RuntimeSceneError, RuntimeSceneLimits, admit_flat_claims,
    budget::{ProvenanceBudget, WorkBudget, provenance_cost},
    identity, require_scene_source, wire,
};
use crate::{
    CompiledSceneCandidate, SceneComponentBindings, SceneEntityId, SceneEntityProvenance,
    compiled::claims::{FlatSceneClaims, FlatSceneComponentClaim, FlatSceneEntityClaim},
};

pub(super) fn cook(
    compiled: &CompiledSceneCandidate,
    bindings: &SceneComponentBindings,
    asset_resolution: AssetReferenceResolver<'_>,
    limits: RuntimeSceneLimits,
) -> Result<RuntimeSceneCandidate, RuntimeSceneError> {
    require_scene_source(compiled.source_key())?;
    let mut work = WorkBudget::new(limits);
    // This exact independent payload, including actual owner strings, is admitted before
    // cloning. All subsequent edits belong exclusively to unpublished flat claims.
    let mut claims = clone_claims(compiled, &mut work)?;
    let identifiers =
        identity::canonical_ids(compiled.source_key(), compiled.document(), false, &mut work)?;
    let traversal = crate::validation::reference_traversal_limits(limits.value_limits());
    for root in &mut claims.roots {
        *root = copy_id(root, &identifiers, &mut work)?;
    }
    for entity in &mut claims.entities {
        entity.id = copy_id(&entity.id, &identifiers, &mut work)?;
        for child in &mut entity.children {
            *child = copy_id(child, &identifiers, &mut work)?;
        }
        for (component, record) in &mut entity.components {
            let schema = bindings.schema(component).ok_or_else(|| {
                RuntimeSceneError::refused(
                    "scene.runtime-component-definition",
                    "Runtime cook requires a selected component Schema",
                )
            })?;
            let references = collect_declared_effective_asset_references_with_admission(
                schema,
                record.version,
                &record.value,
                traversal,
                |nodes, bytes, allocated| work.admit(nodes, bytes, Some(allocated)),
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
                |nodes, bytes, allocated| work.admit(nodes, bytes, Some(allocated)),
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
                |nodes, bytes, allocated| work.admit(nodes, bytes, Some(allocated)),
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
                    EntityReference::Authored { .. } => Err(reference_error()),
                }
            })
            .map_err(|failure| match failure {
                nara_reflect::ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
                _ => reference_error(),
            })?;
        }
    }
    let mut provenance_budget = ProvenanceBudget::new(compiled.source_key(), limits)?;
    for (old, origin) in &claims.canonical_provenance {
        provenance_budget.entry(identifiers.get(old).ok_or_else(reference_error)?, origin)?;
    }
    work.reserve(claims.canonical_provenance.len().checked_mul(
        size_of::<(SceneEntityId, SceneEntityProvenance)>() + 6 * size_of::<usize>(),
    ))?;
    let mut provenance = BTreeMap::new();
    for (old, origin) in std::mem::take(&mut claims.canonical_provenance) {
        provenance.insert(copy_id(&old, &identifiers, &mut work)?, origin);
    }
    claims.canonical_provenance = provenance;
    work.claims(&claims)?;
    let document = admit_flat_claims(claims, bindings, &mut work)?;
    work.witness(bindings)?;
    let candidate = RuntimeSceneCandidate {
        compiled: document.bind_source(compiled.source_key().clone()),
        definitions: bindings.witness(),
        limits,
    };
    // Qualify the complete processed wire, including derived IDs and provenance. This is a
    // borrowed encoding and core shape preflight, not a source publication or second admission.
    wire::encode(&candidate, &mut work)?;
    Ok(candidate)
}

/// Copies only the independent flat storage. Neither source records nor effective defaults
/// participate; the caller reserves the containing key here for its eventual candidate binding.
pub(super) fn clone_claims(
    compiled: &CompiledSceneCandidate,
    work: &mut WorkBudget,
) -> Result<FlatSceneClaims, RuntimeSceneError> {
    let document = compiled.document();
    work.document_clone(document)?;
    let provenance = provenance_cost(
        compiled.source_key(),
        compiled.canonical_provenance(),
        work.limits(),
    )?;
    work.reserve(Some(provenance))?;
    Ok(FlatSceneClaims {
        roots: document.roots().to_vec(),
        entities: document
            .entities()
            .iter()
            .map(|entity| FlatSceneEntityClaim {
                id: entity.id().clone(),
                children: entity.children().to_vec(),
                components: entity
                    .components()
                    .iter()
                    .map(|(component, record)| {
                        (
                            component.clone(),
                            FlatSceneComponentClaim {
                                schema_owner: record.schema_owner().cloned(),
                                version: record.version(),
                                value: record.value().clone(),
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
        canonical_provenance: compiled.canonical_provenance().clone(),
    })
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
