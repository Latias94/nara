//! Concrete occurrence verification between two actual compiler observations of a source split.
//!
//! Relocation proposes addresses only. The actual after observation supplies every matched entity,
//! hierarchy edge and effective value; no source-shaped substitute can establish correspondence.

use std::{collections::BTreeMap, mem::size_of};

use nara_asset::AssetRef;
use nara_identity::EntityReference;
use nara_reflect::{
    __private::{
        ComponentValueAdmissionError, EntityReferencePlanError,
        admit_declared_entity_reference_plan_rewrite,
        plan_declared_entity_references_with_admission, validate_component_value_with_admission,
    },
    ComponentEntityReferenceRewriteError, ComponentSchema,
};

use super::{
    ExtractionError, ExtractionWork, SourceCut, occurrences, payload::require_schema,
    relocation::BranchRelocation,
};
use crate::{
    AuthoringSchemaBasis, ExpandedSceneDocument, ExpandedSceneEntity, PrefabProjectionStep,
    SceneComponentRecord, SceneEntityId, SceneEntityProvenance, SourceDocumentKey,
    provenance::{CanonicalSceneEntityAddress, provenance_string_bytes},
};

#[cfg(test)]
mod tests;

/// One verified entity pair in a containing document, ordered by the original entity ID.
#[derive(Debug)]
pub(super) struct OccurrenceCorrespondence {
    old: SceneEntityId,
    new: SceneEntityId,
}

impl OccurrenceCorrespondence {
    pub(super) const fn old(&self) -> &SceneEntityId {
        &self.old
    }

    pub(super) const fn new(&self) -> &SceneEntityId {
        &self.new
    }
}

/// Verifies exactly this extraction's complete containing observation. Declaration ownership and
/// dormant payload preservation belong to source construction, independently of these occurrences.
pub(super) fn verify_occurrences(
    cut: &SourceCut<'_>,
    original: &SourceDocumentKey,
    destination: &SourceDocumentKey,
    destination_reference: &AssetRef,
    containing: &SourceDocumentKey,
    before: &ExpandedSceneDocument,
    after: &ExpandedSceneDocument,
    basis: &AuthoringSchemaBasis,
    work: &mut ExtractionWork,
) -> Result<Vec<OccurrenceCorrespondence>, ExtractionError> {
    work.visit(
        1,
        original
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(destination.checked_retained_bytes()?))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    BranchRelocation::new(cut, original, destination)
        .map_err(|_| ExtractionError::InvalidSource)?;
    if !destination_reference.is_primary() {
        return Err(ExtractionError::InvalidSource);
    }
    if before.entities().len() != after.entities().len() {
        return Err(ExtractionError::InvalidSource);
    }
    // Validate both address namespaces, including untouched occurrences. An alias is lookup
    // evidence, never a way to make two otherwise equal logical addresses distinct.
    let old_index = index_occurrences(before, work)?;
    let max_old_id_bytes = old_index.max_id_bytes;
    drop(old_index);
    let after_index = index_occurrences(after, work)?;
    let count = before.entities().len();
    work.admit(
        count
            .checked_mul(size_of::<MatchedOccurrence<'_, '_>>())
            .and_then(|bytes| bytes.checked_add(count.checked_mul(size_of::<bool>())?)),
    )?;
    work.visit(count, 0, 0)?;
    let mut matched = Vec::with_capacity(count);
    let mut seen = vec![false; count];
    for (old, (_, provenance)) in before.entities().iter().zip(before.provenance()) {
        let proposed = occurrences::relocate_provenance(
            cut,
            original,
            destination,
            destination_reference,
            containing,
            provenance,
            work,
        )?;
        let cost = address_cost(&proposed, work)?;
        after_index.admit_lookup(cost, work)?;
        let index = *after_index
            .entries
            .get(&CanonicalSceneEntityAddress::new(&proposed))
            .ok_or(ExtractionError::InvalidSource)?;
        work.visit(2, 0, 0)?;
        let used = seen.get_mut(index).ok_or(ExtractionError::InvalidSource)?;
        if *used {
            return Err(ExtractionError::InvalidSource);
        }
        *used = true;
        let new = after
            .entities()
            .get(index)
            .ok_or(ExtractionError::InvalidSource)?;
        matched.push(MatchedOccurrence { old, new });
    }
    work.visit(seen.len(), 0, 0)?;
    if seen.iter().any(|used| !used) {
        return Err(ExtractionError::InvalidSource);
    }

    verify_order(
        before.roots(),
        after.roots(),
        &matched,
        max_old_id_bytes,
        work,
    )?;
    work.visit(matched.len(), 0, 0)?;
    for pair in &matched {
        verify_order(
            pair.old.children(),
            pair.new.children(),
            &matched,
            max_old_id_bytes,
            work,
        )?;
        verify_components(pair, &matched, max_old_id_bytes, basis, work)?;
    }

    work.admit(count.checked_mul(size_of::<OccurrenceCorrespondence>()))?;
    work.visit(count, 0, 0)?;
    let mut correspondence = Vec::with_capacity(count);
    for pair in matched {
        work.admit(
            provenance_string_bytes(pair.old.id().as_str()).and_then(|bytes| {
                bytes.checked_add(provenance_string_bytes(pair.new.id().as_str())?)
            }),
        )?;
        correspondence.push(OccurrenceCorrespondence {
            old: pair.old.id().clone(),
            new: pair.new.id().clone(),
        });
    }
    Ok(correspondence)
}

struct MatchedOccurrence<'old, 'new> {
    old: &'old ExpandedSceneEntity,
    new: &'new ExpandedSceneEntity,
}

#[derive(Clone, Copy, Default)]
struct AddressCost {
    nodes: usize,
    bytes: usize,
}

struct OccurrenceIndex<'a> {
    entries: BTreeMap<CanonicalSceneEntityAddress<'a>, usize>,
    cost: AddressCost,
    max_id_bytes: usize,
}

impl OccurrenceIndex<'_> {
    fn admit_lookup(
        &self,
        query: AddressCost,
        work: &mut ExtractionWork,
    ) -> Result<(), ExtractionError> {
        // Bound complete key comparisons along the balanced tree's search path. Sixteen
        // comparisons per binary-height level conservatively covers std's BTree nodes; billing
        // every retained key would turn actual logarithmic lookup into quadratic rejection.
        let comparisons = ((usize::BITS - self.entries.len().leading_zeros()) as usize + 1) * 16;
        work.visit(
            query
                .nodes
                .checked_add(self.cost.nodes)
                .and_then(|nodes| nodes.checked_mul(comparisons))
                .ok_or(ExtractionError::Capacity)?,
            query
                .bytes
                .checked_add(self.cost.bytes)
                .and_then(|bytes| bytes.checked_mul(comparisons))
                .ok_or(ExtractionError::Capacity)?,
            0,
        )
    }
}

fn index_occurrences<'a>(
    document: &'a ExpandedSceneDocument,
    work: &mut ExtractionWork,
) -> Result<OccurrenceIndex<'a>, ExtractionError> {
    work.visit(1, 0, 0)?;
    let count = document.entities().len();
    if count != document.provenance().len() {
        return Err(ExtractionError::InvalidSource);
    }
    work.admit(count.checked_mul(
        size_of::<CanonicalSceneEntityAddress<'_>>() + size_of::<usize>() + 6 * size_of::<usize>(),
    ))?;
    work.visit(count, 0, 0)?;
    let mut index = OccurrenceIndex {
        entries: BTreeMap::new(),
        cost: AddressCost::default(),
        max_id_bytes: 0,
    };
    for (ordinal, (entity, (id, provenance))) in document
        .entities()
        .iter()
        .zip(document.provenance())
        .enumerate()
    {
        // The compiler's records and provenance are both in entity-ID order. Checking the paired
        // keys also establishes the order required by the later borrowed binary-search ID map.
        if !same_id(entity.id(), id, work)? {
            return Err(ExtractionError::InvalidSource);
        }
        index.max_id_bytes = index.max_id_bytes.max(id.as_str().len());
        let cost = address_cost(provenance, work)?;
        index.admit_lookup(cost, work)?;
        if index
            .entries
            .insert(CanonicalSceneEntityAddress::new(provenance), ordinal)
            .is_some()
        {
            return Err(ExtractionError::InvalidSource);
        }
        index.cost.nodes = index.cost.nodes.max(cost.nodes);
        index.cost.bytes = index.cost.bytes.max(cost.bytes);
    }
    Ok(index)
}

fn address_cost(
    provenance: &SceneEntityProvenance,
    work: &mut ExtractionWork,
) -> Result<AddressCost, ExtractionError> {
    let mut cost = AddressCost {
        nodes: 1,
        bytes: provenance.local_entity_id().as_str().len(),
    };
    work.visit(cost.nodes, cost.bytes, 0)?;
    if let Some(path) = provenance.projection_path() {
        work.visit(path.steps().len(), path.root_anchor().as_str().len(), 0)?;
        cost.nodes = cost.nodes.checked_add(1).ok_or(ExtractionError::Capacity)?;
        cost.bytes = cost
            .bytes
            .checked_add(path.root_anchor().as_str().len())
            .ok_or(ExtractionError::Capacity)?;
        for step in path.steps() {
            let bytes = match step {
                PrefabProjectionStep::SourceEntity {
                    canonical_source,
                    entity,
                    ..
                } => canonical_source
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(entity.as_str().len())),
                PrefabProjectionStep::InstanceAddition { entity } => Some(entity.as_str().len()),
            }
            .ok_or(ExtractionError::Capacity)?;
            work.visit(4, bytes, 0)?;
            cost.nodes = cost.nodes.checked_add(4).ok_or(ExtractionError::Capacity)?;
            cost.bytes = cost
                .bytes
                .checked_add(bytes)
                .ok_or(ExtractionError::Capacity)?;
        }
    }
    Ok(cost)
}

fn mapped_id<'a>(
    old: &SceneEntityId,
    matched: &'a [MatchedOccurrence<'_, '_>],
    max_old_id_bytes: usize,
    work: &mut ExtractionWork,
) -> Result<&'a SceneEntityId, ExtractionError> {
    let comparisons = (usize::BITS - matched.len().leading_zeros()) as usize + 1;
    work.visit(
        comparisons,
        old.as_str()
            .len()
            .checked_add(max_old_id_bytes)
            .and_then(|bytes| bytes.checked_mul(comparisons))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    let index = matched
        .binary_search_by(|pair| pair.old.id().cmp(old))
        .map_err(|_| ExtractionError::InvalidSource)?;
    work.visit(1, 0, 0)?;
    Ok(matched
        .get(index)
        .ok_or(ExtractionError::InvalidSource)?
        .new
        .id())
}

fn same_id(
    left: &SceneEntityId,
    right: &SceneEntityId,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(
        1,
        left.as_str()
            .len()
            .checked_add(right.as_str().len())
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    Ok(left == right)
}

fn verify_order(
    before: &[SceneEntityId],
    after: &[SceneEntityId],
    matched: &[MatchedOccurrence<'_, '_>],
    max_old_id_bytes: usize,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(1, 0, 0)?;
    if before.len() != after.len() {
        return Err(ExtractionError::InvalidSource);
    }
    work.visit(before.len(), 0, 0)?;
    for (old, actual) in before.iter().zip(after) {
        if !same_id(
            mapped_id(old, matched, max_old_id_bytes, work)?,
            actual,
            work,
        )? {
            return Err(ExtractionError::InvalidSource);
        }
    }
    Ok(())
}

fn verify_components(
    pair: &MatchedOccurrence<'_, '_>,
    matched: &[MatchedOccurrence<'_, '_>],
    max_old_id_bytes: usize,
    basis: &AuthoringSchemaBasis,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(1, 0, 0)?;
    if pair.old.components().len() != pair.new.components().len() {
        return Err(ExtractionError::InvalidSource);
    }
    work.visit(pair.old.components().len(), 0, 0)?;
    for ((old_type, old), (new_type, new)) in
        pair.old.components().iter().zip(pair.new.components())
    {
        work.visit(
            1,
            old_type
                .as_str()
                .len()
                .checked_add(new_type.as_str().len())
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        if old_type != new_type {
            return Err(ExtractionError::InvalidSource);
        }
        let schema = require_schema(old_type, Some(old.version), basis, work)?;
        if new.version != schema.version() {
            return Err(ExtractionError::VersionMismatch);
        }
        work.visit(1, old_type.as_str().len(), 0)?;
        let owner = basis.component_schema_owner(old_type);
        work.visit(
            2,
            old.owner()
                .map_or(0, |owner| owner.as_str().len())
                .checked_mul(2)
                .and_then(|bytes| {
                    bytes.checked_add(new.owner().map_or(0, |owner| owner.as_str().len()))
                })
                .and_then(|bytes| bytes.checked_add(owner.map_or(0, |owner| owner.as_str().len())))
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        if old.owner() != new.owner() || old.owner().is_some_and(|claim| Some(claim) != owner) {
            return Err(ExtractionError::OwnerMismatch);
        }
        verify_value(schema, old, new, matched, max_old_id_bytes, work)?;
    }
    Ok(())
}

fn verify_value(
    schema: &ComponentSchema,
    before: &SceneComponentRecord,
    after: &SceneComponentRecord,
    matched: &[MatchedOccurrence<'_, '_>],
    max_old_id_bytes: usize,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    // Compare the compiler's actual effective record values. Contextual defaults remain source
    // declarations, and must not be reapplied here in a different lexical source environment.
    for value in [&before.value, &after.value] {
        validate_component_value_with_admission(
            schema,
            value,
            work.materialization()?,
            |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
        )
        .map_err(|error| match error {
            ComponentValueAdmissionError::Admission(error) => error,
            ComponentValueAdmissionError::Value(error) => ExtractionError::Value(error),
        })?;
    }
    let cost = work.value(&before.value)?;
    let traversal = work.traversal()?;
    let plan = plan_declared_entity_references_with_admission::<ExtractionError, _>(
        schema,
        &before.value,
        traversal,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        EntityReferencePlanError::Admission(error) => error,
        EntityReferencePlanError::Reference(error) => reference_error(error),
    })?;
    // Use the ordinary bounded value-cost API for this exact copy, without inserting defaults
    // or introducing a separate value interpreter. The source compiler already bounds depth.
    work.admit(
        cost.nodes()
            .checked_mul(96)
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes())),
    )?;
    let mut expected = before.value.clone();
    admit_declared_entity_reference_plan_rewrite(&plan, cost, |nodes, bytes, allocated| {
        work.visit(nodes, bytes, allocated.ok_or(ExtractionError::Capacity)?)
    })?;
    plan.rewrite_in_place(&mut expected, |_, reference| match reference {
        EntityReference::SceneLocal { entity } => {
            let actual = mapped_id(entity, matched, max_old_id_bytes, work)?;
            work.admit(
                provenance_string_bytes(actual.as_str())
                    .and_then(|bytes| bytes.checked_add(size_of::<EntityReference>())),
            )?;
            Ok(EntityReference::SceneLocal {
                entity: actual.clone(),
            })
        }
        EntityReference::Persistent { .. } => {
            work.admit(Some(size_of::<EntityReference>()))?;
            Ok(reference.clone())
        }
        EntityReference::Authored { .. } => Err(ExtractionError::InvalidSource),
    })
    .map_err(reference_error)?;
    // Actual locator growth participates in the cumulative allowance, including the final
    // complete comparison. Persistent values participate unchanged in ordinary value equality.
    work.value(&expected)?;
    work.value(&after.value)?;
    if expected != after.value {
        return Err(ExtractionError::InvalidSource);
    }
    Ok(())
}

fn reference_error(
    error: ComponentEntityReferenceRewriteError<ExtractionError>,
) -> ExtractionError {
    match error {
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. }
        | ComponentEntityReferenceRewriteError::PathIndexOverflow => ExtractionError::Capacity,
        _ => ExtractionError::InvalidSource,
    }
}
