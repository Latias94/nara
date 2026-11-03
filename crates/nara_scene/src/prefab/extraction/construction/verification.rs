//! Borrowed stored P/Q validation using the live construction's fragment and placement kernels.

use std::collections::BTreeMap;

use crate::{PrefabEntityOrigin, PrefabInstanceBinding};

use super::*;
use crate::prefab::extraction::{
    ownership::RecordIndex, preservation::entity_storage_matches,
    relocation::StoredMovedReferenceInputs,
};

pub(in crate::prefab::extraction) struct VerifiedSourceSplit {
    pub(in crate::prefab::extraction) records: Vec<RecordDestination>,
    pub(in crate::prefab::extraction) placements: Vec<PlacementDestination>,
    pub(in crate::prefab::extraction) generated_placements: Vec<usize>,
}

pub(in crate::prefab::extraction) fn verify_source_split(
    cut: &SourceCut<'_>,
    original: &SourceDocumentKey,
    destination: &SourceDocumentKey,
    destination_reference: &AssetRef,
    basis: &AuthoringSchemaBasis,
    containing: SourceDocumentInput<'_>,
    extracted: &PrefabDocument,
    work: &mut ExtractionWork,
) -> Result<VerifiedSourceSplit, ExtractionError> {
    if !destination_reference.is_primary() {
        return Err(ExtractionError::InvalidSource);
    }
    let relocation = BranchRelocation::new(cut, original, destination)
        .map_err(|_| ExtractionError::InvalidSource)?;
    // The actual raw trees, including opaque and dormant values, precede all fragment copies
    // and recursive equality. A failed prefix remains spent in this original Work.
    work.source_copy(cut.source())?;
    work.source_copy(containing)?;
    work.source_copy(SourceDocumentInput::Prefab(extracted))?;
    match (cut.source(), containing) {
        (SourceDocumentInput::Scene(_), SourceDocumentInput::Scene(_)) => {}
        (SourceDocumentInput::Prefab(before), SourceDocumentInput::Prefab(after)) => {
            work.visit(before.inputs.len().saturating_add(after.inputs.len()), 0, 0)?;
            for name in before.inputs.iter().chain(&after.inputs) {
                work.visit(1, name.as_str().len(), 0)?;
            }
            if before.inputs != after.inputs {
                return Err(ExtractionError::InvalidSource);
            }
        }
        _ => return Err(ExtractionError::InvalidSource),
    }
    let originals = records(cut.source());
    let retained = records(containing);
    let moved_count = cut.entities().len();
    let retained_count = originals
        .len()
        .checked_sub(moved_count)
        .and_then(|count| count.checked_add(1))
        .ok_or(ExtractionError::InvalidSource)?;
    if retained.len() != retained_count || extracted.entities.len() != moved_count {
        return Err(ExtractionError::InvalidSource);
    }
    let mut retained = RecordIndex::new(retained, work)?;
    let mut moved = RecordIndex::new(&extracted.entities, work)?;
    let (_, anchor) = retained.claim(&cut.root().id, work)?;
    let instance = anchor
        .prefab
        .as_ref()
        .ok_or(ExtractionError::InvalidSource)?;
    work.visit(
        1,
        crate::provenance::provenance_asset_ref_bytes(&instance.source)
            .and_then(|bytes| {
                bytes.checked_add(crate::provenance::provenance_asset_ref_bytes(
                    destination_reference,
                )?)
            })
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    if !anchor.components.is_empty()
        || instance.source != *destination_reference
        || instance.overrides.is_some()
    {
        return Err(ExtractionError::InvalidSource);
    }
    let PrefabInstanceBinding::Root { base, root } = &instance.binding else {
        return Err(ExtractionError::InvalidSource);
    };
    work.visit(
        1,
        base.checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(destination.checked_retained_bytes()?))
            .and_then(|bytes| bytes.checked_add(root.entity.entity().as_str().len()))
            .and_then(|bytes| bytes.checked_add(cut.root().id.as_str().len()))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    if base != destination
        || !root.through.is_empty()
        || !matches!(&root.entity, PrefabEntityOrigin::SourceEntity(id) if id == &cut.root().id)
    {
        return Err(ExtractionError::InvalidSource);
    }
    let empty = BTreeMap::new();
    let arguments = match &instance.arguments {
        Some(arguments) => {
            work.visit(
                1,
                arguments
                    .base
                    .checked_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(destination.checked_retained_bytes()?))
                    .ok_or(ExtractionError::Capacity)?,
                0,
            )?;
            if arguments.base != *destination || arguments.values.is_empty() {
                return Err(ExtractionError::InvalidSource);
            }
            &arguments.values
        }
        None => &empty,
    };
    let mut inputs = StoredMovedReferenceInputs::new(&relocation, arguments, work)?;
    work.admit(
        originals
            .len()
            .checked_mul(size_of::<RecordDestination>())
            .and_then(|bytes| bytes.checked_add(size_of::<VerifiedSourceSplit>())),
    )?;
    let mut destinations = Vec::with_capacity(originals.len());
    for (index, before) in originals.iter().enumerate() {
        // No complete containing/extracted candidate is constructed. The common live fragment
        // kernel checks one already admitted original entity against its fixed destination.
        let mut expected = before.clone();
        if cut.membership.entities.binary_search(&index).is_ok() {
            let (_, after) = moved.claim(&before.id, work)?;
            let mut rewrite = |reference: &nara_identity::EntityReference,
                               work: &mut ExtractionWork| {
                let rewritten = inputs.rewrite(reference, work)?;
                work.own_reference(rewritten)
            };
            rewrite_entities(
                std::slice::from_mut(&mut expected),
                &relocation,
                basis,
                work,
                &mut rewrite,
            )?;
            if !entity_storage_matches(&expected, after, work)? {
                return Err(ExtractionError::InvalidSource);
            }
            destinations.push(RecordDestination::Extracted);
        } else {
            let (_, after) = retained.claim(&before.id, work)?;
            let mut rewrite = |reference: &nara_identity::EntityReference,
                               work: &mut ExtractionWork| {
                let rewritten = relocation
                    .retained_reference(original, reference, |bytes| work.admit(bytes))?;
                work.own_reference(rewritten)
            };
            rewrite_entities(
                std::slice::from_mut(&mut expected),
                &relocation,
                basis,
                work,
                &mut rewrite,
            )?;
            if !entity_storage_matches(&expected, after, work)? {
                return Err(ExtractionError::InvalidSource);
            }
            destinations.push(RecordDestination::Retained);
        }
    }
    retained.finish()?;
    moved.finish()?;
    inputs.finish(&extracted.inputs, work)?;
    // Reuse the coupled placement construction, not the materialized visual hierarchy. These
    // bounded expected constraints also supply the declaration map's original/new indices.
    let placements = split_placements(cut, &relocation, work)?;
    let actual = match containing {
        SourceDocumentInput::Scene(document) => &document.placements,
        SourceDocumentInput::Prefab(document) => &document.placements,
    };
    if placements.retained != *actual || placements.extracted != extracted.placements {
        return Err(ExtractionError::InvalidSource);
    }
    Ok(VerifiedSourceSplit {
        records: destinations,
        placements: placements.originals,
        generated_placements: placements.generated,
    })
}

fn records<'source>(source: SourceDocumentInput<'source>) -> &'source [SceneEntityRecord] {
    match source {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    }
}
