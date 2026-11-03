//! Constructs unpublished P/Q sources from one actual cut and complete physical declarations.
//!
//! This is source construction, not a complete-project correspondence or publication proof.
//! The containing source keeps its kind, original input declarations and exact source snapshot.

use std::mem::size_of;

use crate::{
    AuthoringSchemaBasis, PrefabDocument, PrefabEntityTarget, PrefabInstance,
    PrefabReferenceArguments, SceneDocument, SceneEntityRecord, SourceDocumentKey,
};
use nara_asset::AssetRef;

use super::{
    ExtractionError, ExtractionWork, SourceCut, SourceDocumentInput,
    declarations::rewrite_instance,
    payload::rewrite_record,
    placements::{PlacementDestination, split_placements},
    relocation::{BranchRelocation, MovedReferenceInputs},
};

#[cfg(test)]
mod tests;

mod verification;
pub(super) use verification::verify_source_split;

#[derive(Debug)]
pub(super) enum SplitSourceDocument {
    Scene(SceneDocument),
    Prefab(PrefabDocument),
}

impl SplitSourceDocument {
    fn entities_mut(&mut self) -> &mut Vec<SceneEntityRecord> {
        match self {
            Self::Scene(document) => &mut document.entities,
            Self::Prefab(document) => &mut document.entities,
        }
    }

    fn set_placements(&mut self, placements: Vec<crate::SceneEntityPlacement>) {
        match self {
            Self::Scene(document) => document.placements = placements,
            Self::Prefab(document) => document.placements = placements,
        }
    }
}

/// A record and every physical nested declaration suffix keep their entity ID and array order.
/// Canonical entity continuity is a separate relation: the old root record moves, its entity
/// keeps the containing anchor, and the containing anchor receives a new instance declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordDestination {
    Retained,
    Extracted,
}

pub(super) struct UnpublishedSourceSplit<'source> {
    pub(super) original: SourceDocumentInput<'source>,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) containing: SplitSourceDocument,
    pub(super) extracted: PrefabDocument,
    /// Aligned with the original immutable source's entity records, before output canonicalization.
    pub(super) records: Vec<RecordDestination>,
    /// Every original source placement has one actual destination. Generated baselines are new.
    pub(super) placements: Vec<PlacementDestination>,
    pub(super) generated_placements: Vec<usize>,
}

pub(super) fn construct_source_split<'source>(
    cut: &SourceCut<'source>,
    original: &SourceDocumentKey,
    destination: &SourceDocumentKey,
    destination_reference: &AssetRef,
    basis: &AuthoringSchemaBasis,
    work: &mut ExtractionWork,
) -> Result<UnpublishedSourceSplit<'source>, ExtractionError> {
    if !destination_reference.is_primary() {
        return Err(ExtractionError::InvalidSource);
    }
    let relocation = BranchRelocation::new(cut, original, destination)
        .map_err(|_| ExtractionError::InvalidSource)?;
    work.source_copy(cut.source())?;
    let mut containing = match cut.source() {
        SourceDocumentInput::Scene(document) => SplitSourceDocument::Scene(document.clone()),
        SourceDocumentInput::Prefab(document) => SplitSourceDocument::Prefab(document.clone()),
    };
    let placement_split = split_placements(cut, &relocation, work)?;
    let old_entities = std::mem::take(containing.entities_mut());
    let count = old_entities.len();
    let moved_count = cut.entities().len();
    let retained_count = count
        .checked_sub(moved_count)
        .and_then(|count| count.checked_add(1))
        .ok_or(ExtractionError::InvalidSource)?;
    work.admit(
        count
            .checked_mul(size_of::<RecordDestination>())
            .and_then(|bytes| bytes.checked_add(size_of::<UnpublishedSourceSplit<'_>>()))
            .and_then(|bytes| {
                bytes.checked_add(
                    count
                        .checked_add(1)?
                        .checked_mul(size_of::<SceneEntityRecord>())?,
                )
            }),
    )?;
    let mut retained = Vec::with_capacity(retained_count);
    let mut moved = Vec::with_capacity(moved_count);
    let mut records = Vec::with_capacity(count);
    for (index, record) in old_entities.into_iter().enumerate() {
        if cut.membership.entities.binary_search(&index).is_ok() {
            records.push(RecordDestination::Extracted);
            moved.push(record);
        } else {
            records.push(RecordDestination::Retained);
            retained.push(record);
        }
    }

    let mut inputs = MovedReferenceInputs::new(&relocation);
    {
        let mut rewrite_moved = |reference: &nara_identity::EntityReference,
                                 work: &mut ExtractionWork| {
            let rewritten = inputs.rewrite(reference, |bytes| work.admit(bytes))?;
            work.own_reference(rewritten)
        };
        rewrite_entities(&mut moved, &relocation, basis, work, &mut rewrite_moved)?;
    }
    let mut rewrite_retained = |reference: &nara_identity::EntityReference,
                                work: &mut ExtractionWork| {
        let rewritten =
            relocation.retained_reference(original, reference, |bytes| work.admit(bytes))?;
        work.own_reference(rewritten)
    };
    rewrite_entities(
        &mut retained,
        &relocation,
        basis,
        work,
        &mut rewrite_retained,
    )?;

    let root = &cut.root().id;
    work.admit(
        destination
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_mul(2))
            .and_then(|bytes| {
                bytes.checked_add(crate::provenance::provenance_asset_ref_bytes(
                    destination_reference,
                )?)
            })
            .and_then(|bytes| bytes.checked_add(root.as_str().len().checked_mul(2)?))
            .and_then(|bytes| bytes.checked_add(size_of::<PrefabInstance>()))
            .and_then(|bytes| bytes.checked_add(size_of::<PrefabReferenceArguments>())),
    )?;
    let mut extracted = PrefabDocument {
        inputs: Default::default(),
        placements: placement_split.extracted,
        entities: moved,
    };
    for input in inputs.arguments().keys() {
        work.admit(input.as_str().len().checked_add(128))?;
        extracted.inputs.insert(input.clone());
    }
    let mut instance = PrefabInstance::root(
        destination_reference.clone(),
        destination.clone(),
        PrefabEntityTarget::source(root.clone()),
    );
    if !inputs.arguments().is_empty() {
        instance.arguments = Some(PrefabReferenceArguments {
            base: destination.clone(),
            values: inputs.into_arguments(),
        });
    }
    let mut anchor = SceneEntityRecord::new(root.clone());
    anchor.prefab = Some(instance);
    retained.push(anchor);
    *containing.entities_mut() = retained;
    containing.set_placements(placement_split.retained);
    // Stored placement/overlay array order remains exact. Record order has no layout meaning.
    match &mut containing {
        SplitSourceDocument::Scene(document) => document.canonicalize(),
        SplitSourceDocument::Prefab(document) => document.canonicalize(),
    }
    extracted.canonicalize();
    Ok(UnpublishedSourceSplit {
        original: cut.source(),
        basis: basis.clone(),
        containing,
        extracted,
        records,
        placements: placement_split.originals,
        generated_placements: placement_split.generated,
    })
}

fn rewrite_entities(
    entities: &mut [SceneEntityRecord],
    relocation: &BranchRelocation<'_, '_>,
    basis: &AuthoringSchemaBasis,
    work: &mut ExtractionWork,
    rewrite: &mut impl FnMut(
        &nara_identity::EntityReference,
        &mut ExtractionWork,
    ) -> Result<nara_identity::EntityReference, ExtractionError>,
) -> Result<(), ExtractionError> {
    for entity in entities {
        work.visit(1, entity.id.as_str().len(), 0)?;
        for (component, record) in &mut entity.components {
            rewrite_record(component, record, basis, false, work, rewrite)?;
        }
        if let Some(instance) = &mut entity.prefab {
            rewrite_instance(instance, relocation, basis, false, work, rewrite)?;
        }
    }
    Ok(())
}
