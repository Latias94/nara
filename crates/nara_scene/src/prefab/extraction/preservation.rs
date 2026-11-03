//! Unavailable observations remain admissible only with unchanged physical source custody.
//!
//! Canonical entity provenance cannot identify a root-bound component's original contributor.
//! Use the original compiler's selected declaration and the physical ownership map instead.

use std::{collections::BTreeMap, mem::size_of};

use nara_reflect::ComponentTypeId;

use crate::prefab::{
    AuthoringSceneCompilationOmissionReason, UnavailableComponentSlot, UnavailableSourceDeclaration,
};
use crate::{
    AuthoringRecordReadiness, PrefabComponentIntent, PrefabDeclarationSource, PrefabOverlaySlot,
    SceneComponentRecord, SceneEntityId,
};

use super::{
    ExtractionError, ExtractionWork, SourceDocumentInput,
    corpus::{CapturedExtractionSource, CompilationObservation},
    correspondence::OccurrenceCorrespondence,
    footprint::SourceContextFootprint,
    ownership::{MappedRecord, OutputSource, PhysicalDeclarationMap},
};

mod storage;
pub(super) use storage::entity_storage_matches;

struct UnavailableEntry<'a> {
    declaration: &'a UnavailableSourceDeclaration,
    reason: Option<AuthoringRecordReadiness>,
}

struct UnavailableIndex<'a> {
    entries: BTreeMap<(&'a SceneEntityId, &'a ComponentTypeId), UnavailableEntry<'a>>,
    maximum_key_bytes: usize,
}

impl<'a> UnavailableIndex<'a> {
    fn new(
        observation: &'a CompilationObservation,
        work: &mut ExtractionWork,
    ) -> Result<Self, ExtractionError> {
        work.admit(observation.unavailable.len().checked_mul(160))?;
        let mut index = Self {
            entries: BTreeMap::new(),
            maximum_key_bytes: 0,
        };
        for declaration in &observation.unavailable {
            index.admit_lookup(&declaration.entity, &declaration.component, work)?;
            let bytes = declaration
                .entity
                .as_str()
                .len()
                .saturating_add(declaration.component.as_str().len());
            if index
                .entries
                .insert(
                    (&declaration.entity, &declaration.component),
                    UnavailableEntry {
                        declaration,
                        reason: None,
                    },
                )
                .is_some()
            {
                return Err(ExtractionError::InvalidSource);
            }
            index.maximum_key_bytes = index.maximum_key_bytes.max(bytes);
        }
        for omission in &observation.omissions {
            work.visit(1, 0, 0)?;
            let AuthoringSceneCompilationOmissionReason::RecordUnavailable(reason) =
                omission.reason
            else {
                continue;
            };
            index.admit_lookup(&omission.entity, &omission.component, work)?;
            let entry = index
                .entries
                .get_mut(&(&omission.entity, &omission.component))
                .ok_or(ExtractionError::InvalidSource)?;
            if entry.reason.replace(reason).is_some() {
                return Err(ExtractionError::InvalidSource);
            }
        }
        work.visit(index.entries.len(), 0, 0)?;
        if index.entries.values().any(|entry| entry.reason.is_none()) {
            return Err(ExtractionError::InvalidSource);
        }
        Ok(index)
    }

    fn admit_lookup(
        &self,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        work: &mut ExtractionWork,
    ) -> Result<(), ExtractionError> {
        let comparisons = ((usize::BITS - self.entries.len().leading_zeros()) as usize + 1) * 16;
        work.visit(
            comparisons,
            entity
                .as_str()
                .len()
                .checked_add(component.as_str().len())
                .and_then(|bytes| {
                    bytes
                        .checked_add(self.maximum_key_bytes)?
                        .checked_mul(comparisons)
                })
                .ok_or(ExtractionError::Capacity)?,
            0,
        )
    }
}

pub(super) fn verify_unavailable_records(
    before: &CompilationObservation,
    after: &CompilationObservation,
    occurrences: &[OccurrenceCorrespondence],
    originals: &[CapturedExtractionSource],
    outputs: &[CapturedExtractionSource],
    footprint: &[SourceContextFootprint],
    declarations: &PhysicalDeclarationMap,
    work: &mut ExtractionWork,
    mut source_index: impl FnMut(
        &PrefabDeclarationSource,
        &mut ExtractionWork,
    ) -> Result<usize, ExtractionError>,
) -> Result<(), ExtractionError> {
    if before.unavailable.is_empty() && after.unavailable.is_empty() {
        work.visit(
            before.omissions.len().saturating_add(after.omissions.len()),
            0,
            0,
        )?;
        if before
            .omissions
            .iter()
            .chain(&after.omissions)
            .any(|omission| {
                matches!(
                    omission.reason,
                    AuthoringSceneCompilationOmissionReason::RecordUnavailable(_)
                )
            })
        {
            return Err(ExtractionError::InvalidSource);
        }
        return Ok(());
    }
    let before = UnavailableIndex::new(before, work)?;
    let after = UnavailableIndex::new(after, work)?;
    if before.entries.len() != after.entries.len()
        || originals.len() != outputs.len()
        || originals.len() != footprint.len()
    {
        return Err(ExtractionError::InvalidSource);
    }
    work.visit(occurrences.len(), 0, 0)?;
    let maximum_old_id = occurrences
        .iter()
        .map(|pair| pair.old().as_str().len())
        .max()
        .unwrap_or(0);
    work.admit(
        before
            .entries
            .len()
            .checked_mul(3 * size_of::<usize>() + size_of::<&UnavailableEntry<'_>>())
            .and_then(|bytes| bytes.checked_add(originals.len().checked_mul(size_of::<bool>())?)),
    )?;
    let mut needed = vec![false; originals.len()];
    let mut matched = Vec::with_capacity(before.entries.len());
    for entry in before.entries.values() {
        let old = entry.declaration;
        let comparisons = ((usize::BITS - occurrences.len().leading_zeros()) as usize + 1) * 16;
        work.visit(
            comparisons,
            old.entity
                .as_str()
                .len()
                .checked_add(maximum_old_id)
                .and_then(|bytes| bytes.checked_mul(comparisons))
                .ok_or(ExtractionError::Capacity)?,
            0,
        )?;
        let pair = occurrences
            .binary_search_by(|pair| pair.old().cmp(&old.entity))
            .ok()
            .and_then(|index| occurrences.get(index))
            .ok_or(ExtractionError::InvalidSource)?;
        after.admit_lookup(pair.new(), &old.component, work)?;
        let new = after
            .entries
            .get(&(pair.new(), &old.component))
            .ok_or(ExtractionError::InvalidSource)?;
        if entry.reason != new.reason {
            return Err(ExtractionError::InvalidSource);
        }
        let source = source_index(&old.source, work)?;
        let new_source = source_index(&new.declaration.source, work)?;
        if source != new_source || footprint.get(source) != Some(&SourceContextFootprint::Preserved)
        {
            return Err(ExtractionError::InvalidSource);
        }
        work.visit(
            1,
            old.record
                .as_str()
                .len()
                .saturating_add(new.declaration.record.as_str().len()),
            0,
        )?;
        if old.record != new.declaration.record {
            return Err(ExtractionError::InvalidSource);
        }
        let slot_steps = |slot: &UnavailableComponentSlot| match slot {
            UnavailableComponentSlot::Record => 0,
            UnavailableComponentSlot::Overlay { steps, .. } => steps.len(),
        };
        work.visit(
            slot_steps(&old.slot)
                .saturating_add(slot_steps(&new.declaration.slot))
                .saturating_add(1),
            0,
            0,
        )?;
        if old.slot != new.declaration.slot {
            return Err(ExtractionError::InvalidSource);
        }
        *needed
            .get_mut(source)
            .ok_or(ExtractionError::InvalidSource)? = true;
        matched.push((source, old));
    }

    // Build only the physical record indexes that actual unavailable observations need. The
    // keys borrow original immutable inputs; no raw value scan or per-component full-source
    // lookup can turn this into a quadratic Schema substitute.
    let mut records = BTreeMap::new();
    let mut maximum_record_bytes = 0usize;
    for (source, needed) in needed.iter().enumerate() {
        work.visit(1, 0, 0)?;
        if !needed {
            continue;
        }
        let before = &originals[source];
        let after = &outputs[source];
        let before = match (before.document(), after.document()) {
            (SourceDocumentInput::Scene(before), SourceDocumentInput::Scene(_)) => &before.entities,
            (SourceDocumentInput::Prefab(before), SourceDocumentInput::Prefab(_)) => {
                &before.entities
            }
            _ => return Err(ExtractionError::InvalidSource),
        };
        work.admit(before.len().checked_mul(128))?;
        for (index, record) in before.iter().enumerate() {
            record_lookup(&record.id, maximum_record_bytes, records.len(), work)?;
            if records.insert((source, &record.id), index).is_some() {
                return Err(ExtractionError::InvalidSource);
            }
            maximum_record_bytes = maximum_record_bytes.max(record.id.as_str().len());
        }
    }
    for (source, declaration) in matched {
        record_lookup(
            &declaration.record,
            maximum_record_bytes,
            records.len(),
            work,
        )?;
        let original = *records
            .get(&(source, &declaration.record))
            .ok_or(ExtractionError::InvalidSource)?;
        let fact = declarations
            .record(source, original)
            .ok_or(ExtractionError::InvalidSource)?;
        // The verified physical map binds the same source/entity across independently ordered
        // record arrays. A record's array position is snapshot-local, not its identity.
        if fact.source != OutputSource::Captured(source) {
            return Err(ExtractionError::InvalidSource);
        }
        let old = entity_records(&originals[source])
            .get(original)
            .ok_or(ExtractionError::InvalidSource)?;
        let new = entity_records(&outputs[source])
            .get(fact.record)
            .ok_or(ExtractionError::InvalidSource)?;
        let (old, new) = unavailable_record_pair(
            MappedRecord {
                source: originals[source].key(),
                before: old,
                after: new,
            },
            declaration,
            work,
        )?;
        if !std::ptr::eq(old, new) {
            work.visit(
                old.contextual_defaults
                    .len()
                    .saturating_add(new.contextual_defaults.len())
                    .saturating_add(1),
                old.owner()
                    .map_or(0, |owner| owner.as_str().len())
                    .saturating_add(new.owner().map_or(0, |owner| owner.as_str().len())),
                0,
            )?;
            work.value(&old.value)?;
            work.value(&new.value)?;
            for (field, value) in old
                .contextual_defaults
                .iter()
                .chain(&new.contextual_defaults)
            {
                work.visit(1, field.as_str().len(), 0)?;
                work.value(value)?;
            }
            if old != new {
                return Err(ExtractionError::InvalidSource);
            }
        }
    }
    Ok(())
}

/// Resolve only a compiler-observed physical slot through the already verified suffix map.
/// This selects raw records; it neither evaluates targets nor interprets unavailable values.
fn unavailable_record_pair<'a>(
    mapped: MappedRecord<'a>,
    declaration: &UnavailableSourceDeclaration,
    work: &mut ExtractionWork,
) -> Result<(&'a SceneComponentRecord, &'a SceneComponentRecord), ExtractionError> {
    let (old, new) = match &declaration.slot {
        UnavailableComponentSlot::Record => (&mapped.before.components, &mapped.after.components),
        UnavailableComponentSlot::Overlay { steps, slot } => {
            work.visit(steps.len().saturating_mul(2).saturating_add(1), 0, 0)?;
            let mapped = mapped
                .instance_steps(steps)
                .and_then(|instance| instance.intent(*slot))
                .ok_or(ExtractionError::InvalidSource)?;
            match mapped.slot {
                PrefabOverlaySlot::Addition { index } => (
                    &mapped.before.additions[index].components,
                    &mapped.after.additions[index].components,
                ),
                PrefabOverlaySlot::Component { index } => {
                    let PrefabComponentIntent::Set {
                        component: old,
                        record: before,
                        ..
                    } = &mapped.before.components[index]
                    else {
                        return Err(ExtractionError::InvalidSource);
                    };
                    let PrefabComponentIntent::Set {
                        component: new,
                        record: after,
                        ..
                    } = &mapped.after.components[index]
                    else {
                        return Err(ExtractionError::InvalidSource);
                    };
                    work.visit(
                        2,
                        old.as_str()
                            .len()
                            .saturating_add(new.as_str().len())
                            .saturating_add(declaration.component.as_str().len().saturating_mul(2)),
                        0,
                    )?;
                    if old != &declaration.component || new != &declaration.component {
                        return Err(ExtractionError::InvalidSource);
                    }
                    return Ok((before, after));
                }
                _ => return Err(ExtractionError::InvalidSource),
            }
        }
    };
    let comparisons = ((usize::BITS - old.len().max(new.len()).leading_zeros()) as usize + 1) * 32;
    work.visit(
        comparisons,
        declaration
            .component
            .as_str()
            .len()
            .checked_mul(comparisons)
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    Ok((
        old.get(&declaration.component)
            .ok_or(ExtractionError::InvalidSource)?,
        new.get(&declaration.component)
            .ok_or(ExtractionError::InvalidSource)?,
    ))
}

fn entity_records(source: &CapturedExtractionSource) -> &[crate::SceneEntityRecord] {
    match source.document() {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    }
}

fn record_lookup(
    id: &SceneEntityId,
    maximum: usize,
    count: usize,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    let comparisons = ((usize::BITS - count.leading_zeros()) as usize + 1) * 16;
    work.visit(
        comparisons,
        id.as_str()
            .len()
            .checked_add(maximum)
            .and_then(|bytes| bytes.checked_mul(comparisons))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )
}
