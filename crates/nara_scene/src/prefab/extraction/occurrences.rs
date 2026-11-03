//! Proposed canonical entity correspondence for one exact source-owned cut.
//!
//! Inputs are canonical provenance from the original compiler. These rewrites preserve lookup
//! evidence but do not prove source edits, admit a source occurrence or establish equivalence.

use std::mem::size_of;

use nara_asset::AssetRef;

use super::{ExtractionError, ExtractionWork, SourceCut, relocation::BranchRelocation};
use crate::{
    PrefabProjectionPath, PrefabProjectionStep, SceneEntityId, SceneEntityProvenance,
    SourceDocumentKey, provenance::provenance_string_bytes,
};

#[cfg(test)]
mod tests;

/// Proposes the owned after-address of one compiler-selected canonical entity in `containing`.
/// The caller must match this against actual after-compilation canonical addresses and prove
/// injectivity. Declaration facets and instance slots are not additional canonical entities.
pub(super) fn relocate_provenance(
    cut: &SourceCut<'_>,
    original: &SourceDocumentKey,
    destination: &SourceDocumentKey,
    destination_reference: &AssetRef,
    containing: &SourceDocumentKey,
    provenance: &SceneEntityProvenance,
    work: &mut ExtractionWork,
) -> Result<SceneEntityProvenance, ExtractionError> {
    // Setup compares canonical keys too; charge their complete borrowed representation first.
    work.visit(
        1,
        key_comparison_bytes(original, destination).ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    BranchRelocation::new(cut, original, destination)
        .map_err(|_| ExtractionError::InvalidSource)?;
    if !destination_reference.is_primary() {
        return Err(ExtractionError::InvalidSource);
    }
    let in_original = same_key(containing, original, work)?;
    let local = provenance.local_entity_id();
    let Some(path) = provenance.projection_path() else {
        let moves = in_original
            && !same_entity(local, &cut.root().id, work)?
            && cut_contains(cut, local, work)?;
        let mut bytes = size_of::<SceneEntityProvenance>()
            .checked_add(provenance_string_bytes(local.as_str()).ok_or(ExtractionError::Capacity)?);
        if moves {
            bytes = bytes.and_then(|bytes| {
                bytes
                    .checked_add(provenance_string_bytes(cut.root().id.as_str())?)?
                    .checked_add(PrefabProjectionStep::source_allocation_bytes(
                        destination_reference,
                        destination,
                        local,
                    )?)
            });
        }
        work.admit(bytes)?;
        return Ok(if moves {
            SceneEntityProvenance::projected(
                local.clone(),
                PrefabProjectionPath::from_resolved_steps(
                    cut.root().id.clone(),
                    vec![PrefabProjectionStep::SourceEntity {
                        source_document: destination_reference.clone(),
                        canonical_source: destination.clone(),
                        entity: local.clone(),
                    }],
                ),
            )
        } else {
            SceneEntityProvenance::local(local.clone())
        });
    };

    let steps = path.steps();
    // Admit the whole route from its O(1) length before examining any step.
    work.visit(steps.len(), 0, 0)?;
    let moves_anchor = in_original && cut_contains(cut, path.root_anchor(), work)?;
    let root_anchor = if moves_anchor {
        &cut.root().id
    } else {
        path.root_anchor()
    };
    let mut count = steps
        .len()
        .checked_add(usize::from(moves_anchor))
        .ok_or(ExtractionError::Capacity)?;
    for (index, step) in steps.iter().enumerate() {
        if step_relocation(cut, original, step, index + 1 < steps.len(), work)?
            != StepRelocation::Keep
        {
            count = count.checked_add(1).ok_or(ExtractionError::Capacity)?;
        }
    }
    // The provenance owns the route container. Step helpers below charge only the dynamic
    // contents, because this exact buffer admission already covers every step's inline value.
    work.admit(
        count
            .checked_mul(size_of::<PrefabProjectionStep>())
            .and_then(|bytes| bytes.checked_add(size_of::<SceneEntityProvenance>()))
            .and_then(|bytes| bytes.checked_add(provenance_string_bytes(local.as_str())?))
            .and_then(|bytes| bytes.checked_add(provenance_string_bytes(root_anchor.as_str())?)),
    )?;
    work.visit(steps.len(), 0, 0)?;
    let mut relocated = Vec::with_capacity(count);
    if moves_anchor {
        push_source(
            &mut relocated,
            destination_reference,
            destination,
            path.root_anchor(),
            work,
        )?;
    }
    for (index, step) in steps.iter().enumerate() {
        let change = step_relocation(cut, original, step, index + 1 < steps.len(), work)?;
        match (step, change) {
            (
                PrefabProjectionStep::SourceEntity {
                    source_document,
                    canonical_source,
                    entity,
                },
                StepRelocation::MovedEntity,
            ) => {
                // The new P:k entry uses the original P lookup alias, never Q's lookup.
                push_source(
                    &mut relocated,
                    source_document,
                    canonical_source,
                    &cut.root().id,
                    work,
                )?;
                push_source(
                    &mut relocated,
                    destination_reference,
                    destination,
                    entity,
                    work,
                )?;
            }
            (PrefabProjectionStep::SourceEntity { entity, .. }, StepRelocation::RootSlot) => {
                push_preserved(&mut relocated, step, work)?;
                push_source(
                    &mut relocated,
                    destination_reference,
                    destination,
                    entity,
                    work,
                )?;
            }
            _ => push_preserved(&mut relocated, step, work)?,
        }
    }
    Ok(SceneEntityProvenance::projected(
        local.clone(),
        PrefabProjectionPath::from_resolved_steps(root_anchor.clone(), relocated),
    ))
}

#[derive(PartialEq, Eq)]
enum StepRelocation {
    Keep,
    MovedEntity,
    RootSlot,
}

fn step_relocation(
    cut: &SourceCut<'_>,
    original: &SourceDocumentKey,
    step: &PrefabProjectionStep,
    has_next: bool,
    work: &mut ExtractionWork,
) -> Result<StepRelocation, ExtractionError> {
    let PrefabProjectionStep::SourceEntity {
        canonical_source,
        entity,
        ..
    } = step
    else {
        return Ok(StepRelocation::Keep);
    };
    if !same_key(canonical_source, original, work)? {
        return Ok(StepRelocation::Keep);
    }
    if same_entity(entity, &cut.root().id, work)? {
        return Ok(if has_next {
            StepRelocation::RootSlot
        } else {
            StepRelocation::Keep
        });
    }
    Ok(if cut_contains(cut, entity, work)? {
        StepRelocation::MovedEntity
    } else {
        StepRelocation::Keep
    })
}

fn cut_contains(
    cut: &SourceCut<'_>,
    entity: &SceneEntityId,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    cut.contains_entity(entity, |bytes| {
        work.visit(1, bytes.ok_or(ExtractionError::Capacity)?, 0)
    })
}

fn same_entity(
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

fn key_comparison_bytes(left: &SourceDocumentKey, right: &SourceDocumentKey) -> Option<usize> {
    left.checked_retained_bytes()?
        .checked_add(right.checked_retained_bytes()?)
}

fn same_key(
    left: &SourceDocumentKey,
    right: &SourceDocumentKey,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(
        1,
        key_comparison_bytes(left, right).ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    Ok(left == right)
}

fn push_source(
    output: &mut Vec<PrefabProjectionStep>,
    reference: &AssetRef,
    source: &SourceDocumentKey,
    entity: &SceneEntityId,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.admit(
        PrefabProjectionStep::source_allocation_bytes(reference, source, entity)
            .and_then(|bytes| bytes.checked_sub(size_of::<PrefabProjectionStep>())),
    )?;
    output.push(PrefabProjectionStep::SourceEntity {
        source_document: reference.clone(),
        canonical_source: source.clone(),
        entity: entity.clone(),
    });
    Ok(())
}

fn push_preserved(
    output: &mut Vec<PrefabProjectionStep>,
    step: &PrefabProjectionStep,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    match step {
        PrefabProjectionStep::SourceEntity {
            source_document,
            canonical_source,
            entity,
        } => push_source(output, source_document, canonical_source, entity, work),
        PrefabProjectionStep::InstanceAddition { entity } => {
            work.admit(
                PrefabProjectionStep::addition_allocation_bytes(entity)
                    .and_then(|bytes| bytes.checked_sub(size_of::<PrefabProjectionStep>())),
            )?;
            output.push(PrefabProjectionStep::InstanceAddition {
                entity: entity.clone(),
            });
            Ok(())
        }
    }
}
