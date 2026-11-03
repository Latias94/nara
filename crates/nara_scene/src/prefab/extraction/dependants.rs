//! Rewrites retained declarations and verifies fixed stored before/after source pairs.
//!
//! Payloads keep their physical lexical source independently of structural target bases.
//! Only a separately proven preserved context permits retaining unavailable fragments;
//! understood declarations always pass through the shared rewrite owner.
//! These source-only operations prove neither occurrence correspondence nor publication.

use std::{borrow::Cow, mem::size_of};

use crate::{
    AuthoringSchemaBasis, PrefabEntityTarget, SceneEntityPlacement, SceneEntityRecord,
    ScenePlacementParent, ScenePlacementSlot, SourceDocumentKey, SourceDocumentKind,
};

use super::{
    ExtractionError, ExtractionWork, SourceDocumentInput,
    construction::SplitSourceDocument,
    declarations::rewrite_instance,
    ownership::RecordIndex,
    payload::rewrite_record,
    preservation::entity_storage_matches,
    relocation::{AddressRole, BranchRelocation},
};

#[cfg(test)]
mod tests;

pub(super) struct RewrittenSource {
    pub(super) document: SplitSourceDocument,
    /// Actual source intent changed by this pass, independently of allocation or context.
    pub(super) source_changed: bool,
}

pub(super) fn rewrite_dependant(
    source: SourceDocumentInput<'_>,
    source_key: &SourceDocumentKey,
    relocation: &BranchRelocation<'_, '_>,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
) -> Result<RewrittenSource, ExtractionError> {
    let kind = match source {
        SourceDocumentInput::Scene(_) => SourceDocumentKind::Scene,
        SourceDocumentInput::Prefab(_) => SourceDocumentKind::Prefab,
    };
    if source_key.kind() != kind {
        return Err(ExtractionError::InvalidSource);
    }
    work.source_copy(source)?;
    let mut candidate = match source {
        SourceDocumentInput::Scene(document) => SplitSourceDocument::Scene(document.clone()),
        SourceDocumentInput::Prefab(document) => SplitSourceDocument::Prefab(document.clone()),
    };
    let (entities, placements) = match &mut candidate {
        SplitSourceDocument::Scene(document) => (&mut document.entities, &mut document.placements),
        SplitSourceDocument::Prefab(document) => (&mut document.entities, &mut document.placements),
    };
    let mut source_changed = false;
    for entity in entities {
        source_changed |= rewrite_entity(
            entity,
            source_key,
            relocation,
            basis,
            preserve_unavailable,
            work,
        )?;
    }
    for declaration in placements {
        source_changed |= rewrite_placement(declaration, source_key, relocation, work)?;
    }
    Ok(RewrittenSource {
        document: candidate,
        source_changed,
    })
}

/// Verifies complete stored declarations without rebuilding another candidate document.
/// The caller proves unavailable-context independence and the complete corpus separately.
/// Both raw trees are admitted before any recursive clone or comparison; only one admitted
/// original entity or placement is cloned at a time, using the live construction's kernels.
pub(super) fn verify_dependant(
    before: SourceDocumentInput<'_>,
    source_key: &SourceDocumentKey,
    after: SourceDocumentInput<'_>,
    relocation: &BranchRelocation<'_, '_>,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    let (before_entities, after_entities, before_placements, after_placements, kind) =
        match (before, after) {
            (SourceDocumentInput::Scene(before), SourceDocumentInput::Scene(after)) => (
                &before.entities,
                &after.entities,
                &before.placements,
                &after.placements,
                SourceDocumentKind::Scene,
            ),
            (SourceDocumentInput::Prefab(before), SourceDocumentInput::Prefab(after)) => (
                &before.entities,
                &after.entities,
                &before.placements,
                &after.placements,
                SourceDocumentKind::Prefab,
            ),
            _ => return Err(ExtractionError::InvalidSource),
        };
    if source_key.kind() != kind {
        return Err(ExtractionError::InvalidSource);
    }
    // The measured source walker retains failed-prefix work on the same cumulative owner.
    // Its copy debit also admits the subsequent per-fragment clones.
    work.source_copy(before)?;
    work.source_copy(after)?;
    if before_entities.len() != after_entities.len()
        || before_placements.len() != after_placements.len()
    {
        return Err(ExtractionError::InvalidSource);
    }
    if let (SourceDocumentInput::Prefab(before), SourceDocumentInput::Prefab(after)) =
        (before, after)
        && before.inputs != after.inputs
    {
        return Err(ExtractionError::InvalidSource);
    }

    // Record order has no layout meaning and canonical encoding may sort it. The shared
    // declaration index admits lookups and rejects duplicate, missing and unclaimed IDs.
    let mut records = RecordIndex::new(after_entities, work)?;
    let mut source_changed = false;
    for entity in before_entities {
        let (_, stored) = records.claim(&entity.id, work)?;
        let mut expected = entity.clone();
        source_changed |= rewrite_entity(
            &mut expected,
            source_key,
            relocation,
            basis,
            preserve_unavailable,
            work,
        )?;
        // Includes masked defaults, dormant claims and exact unavailable payload storage.
        if !entity_storage_matches(&expected, stored, work)? {
            return Err(ExtractionError::InvalidSource);
        }
    }
    records.finish()?;
    // Placements retain their actual vector order, independently of record lookup order.
    for (before, after) in before_placements.iter().zip(after_placements) {
        let mut expected = before.clone();
        source_changed |= rewrite_placement(&mut expected, source_key, relocation, work)?;
        if &expected != after {
            return Err(ExtractionError::InvalidSource);
        }
    }
    Ok(source_changed)
}

fn rewrite_entity(
    entity: &mut SceneEntityRecord,
    source_key: &SourceDocumentKey,
    relocation: &BranchRelocation<'_, '_>,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(1, entity.id.as_str().len(), 0)?;
    let mut references_changed = false;
    let mut rewrite_reference = |reference: &nara_identity::EntityReference,
                                 work: &mut ExtractionWork| {
        let rewritten =
            relocation.retained_reference(source_key, reference, |bytes| work.admit(bytes))?;
        // This mapper owns only changed routes. Cloning a borrowed reference below is not
        // a source change, even though it allocates storage in the candidate.
        references_changed |= matches!(rewritten, Cow::Owned(_));
        work.own_reference(rewritten)
    };
    for (component, record) in &mut entity.components {
        rewrite_record(
            component,
            record,
            basis,
            preserve_unavailable,
            work,
            &mut rewrite_reference,
        )?;
    }
    let source_changed = if let Some(instance) = &mut entity.prefab {
        rewrite_instance(
            instance,
            relocation,
            basis,
            preserve_unavailable,
            work,
            &mut rewrite_reference,
        )?
    } else {
        false
    };
    Ok(source_changed || references_changed)
}

fn rewrite_placement(
    declaration: &mut SceneEntityPlacement,
    source_key: &SourceDocumentKey,
    relocation: &BranchRelocation<'_, '_>,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(1, size_of::<SceneEntityPlacement>(), 0)?;
    let mut source_changed = rewrite_target(&mut declaration.target, source_key, relocation, work)?;
    if let ScenePlacementParent::Entity(target) = &mut declaration.placement.parent {
        source_changed |= rewrite_target(target, source_key, relocation, work)?;
    }
    if let ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) =
        &mut declaration.placement.slot
    {
        source_changed |= rewrite_target(target, source_key, relocation, work)?;
    }
    Ok(source_changed)
}

fn rewrite_target(
    target: &mut PrefabEntityTarget,
    source_key: &SourceDocumentKey,
    relocation: &BranchRelocation<'_, '_>,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(1, size_of::<PrefabEntityTarget>(), 0)?;
    if let Cow::Owned(rewritten) =
        relocation.retained_target(source_key, target, AddressRole::Entity, |bytes| {
            work.admit(bytes)
        })?
    {
        *target = rewritten;
        return Ok(true);
    }
    Ok(false)
}
