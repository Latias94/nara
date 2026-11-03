//! Complete physical record/placement correspondence for this exact source construction.
//!
//! Nested declaration paths keep their stored suffix, independently of canonical entity aliases.
//! The private constructor owns semantic rewrites. This map verifies preservation of their full
//! declaration topology, not arbitrary source equivalence or authority to publish a project.

use std::{collections::BTreeMap, mem::size_of};

use nara_identity::ReferenceInputId;
use nara_reflect::{
    ComponentFieldId, ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyStep,
    ComponentTypeId,
};

use crate::{
    PrefabAddition, PrefabComponentIntent, PrefabInstance, PrefabInstanceBinding,
    PrefabInstanceClaim, PrefabInstancePath, PrefabOverlay, PrefabOverlaySlot,
    PrefabPropertyIntent, SceneComponentRecord, SceneEntityId, SceneEntityPlacement,
    SceneEntityRecord, ScenePlacementSlot, SourceDocumentKey,
};

use super::{
    ExtractionError, ExtractionWork, MAX_PHYSICAL_DEPTH, SourceDocumentInput,
    construction::RecordDestination, corpus::CapturedExtractionSource,
    placements::PlacementDestination,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputSource {
    Captured(usize),
    Extracted,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RecordDestinationFact {
    pub(super) source: OutputSource,
    pub(super) record: usize,
}

pub(super) struct PhysicalDeclarationMap {
    records: Vec<Vec<RecordDestinationFact>>,
    placements: Vec<Vec<(OutputSource, usize)>>,
    generated_placements: Vec<usize>,
}

/// Original and new complete records. Every verified physical instance/overlay suffix resolves
/// in this pair; a canonical root alias never changes where its components and old slot belong.
pub(super) struct MappedRecord<'a> {
    pub(super) source: &'a SourceDocumentKey,
    pub(super) before: &'a SceneEntityRecord,
    pub(super) after: &'a SceneEntityRecord,
}

pub(super) struct MappedInstanceSlot<'a> {
    pub(super) source: &'a SourceDocumentKey,
    pub(super) before: Option<&'a PrefabInstance>,
    pub(super) after: Option<&'a PrefabInstance>,
}

pub(super) struct MappedOverlaySlot<'a> {
    pub(super) source: &'a SourceDocumentKey,
    pub(super) before: &'a PrefabOverlay,
    pub(super) after: &'a PrefabOverlay,
    pub(super) slot: PrefabOverlaySlot,
}

impl<'a> MappedRecord<'a> {
    pub(super) fn instance(&self, path: &PrefabInstancePath) -> Option<MappedInstanceSlot<'a>> {
        if path.anchor != self.before.id {
            return None;
        }
        self.instance_steps(&path.steps)
    }

    pub(super) fn instance_steps(
        &self,
        steps: &[crate::PrefabInstancePathSegment],
    ) -> Option<MappedInstanceSlot<'a>> {
        // The last accepted instance can still own an explicitly absent child slot.
        // That terminal None has one more path step without another physical instance.
        if steps.len() > MAX_PHYSICAL_DEPTH {
            return None;
        }
        let before = crate::authoring::prefab_instance_slot_at(&self.before.prefab, steps).ok()?;
        let after = crate::authoring::prefab_instance_slot_at(&self.after.prefab, steps).ok()?;
        Some(MappedInstanceSlot {
            source: self.source,
            before: before.as_ref(),
            after: after.as_ref(),
        })
    }
}

impl<'a> MappedInstanceSlot<'a> {
    pub(super) fn intent(&self, slot: PrefabOverlaySlot) -> Option<MappedOverlaySlot<'a>> {
        let before = self.before?.overrides.as_ref()?;
        let after = self.after?.overrides.as_ref()?;
        if !contains_slot(before, slot) || !contains_slot(after, slot) {
            return None;
        }
        Some(MappedOverlaySlot {
            source: self.source,
            before,
            after,
            slot,
        })
    }
}

fn contains_slot(overlay: &PrefabOverlay, slot: PrefabOverlaySlot) -> bool {
    match slot {
        PrefabOverlaySlot::Addition { index } => index < overlay.additions.len(),
        PrefabOverlaySlot::InstanceClaim { index } => index < overlay.instances.len(),
        PrefabOverlaySlot::Component { index } => index < overlay.components.len(),
        PrefabOverlaySlot::Property {
            component_index,
            claim_index,
        } => {
            matches!(overlay.components.get(component_index),
                Some(PrefabComponentIntent::Properties { claims, .. }) if claim_index < claims.len())
        }
        PrefabOverlaySlot::Placement { index } => index < overlay.placements.len(),
        PrefabOverlaySlot::Suppression { index } => index < overlay.suppressions.len(),
    }
}

impl PhysicalDeclarationMap {
    pub(super) fn record(&self, source: usize, record: usize) -> Option<RecordDestinationFact> {
        self.records.get(source)?.get(record).copied()
    }

    pub(super) fn placement(&self, source: usize, index: usize) -> Option<(OutputSource, usize)> {
        self.placements.get(source)?.get(index).copied()
    }

    pub(super) fn generated_placements(&self) -> &[usize] {
        &self.generated_placements
    }
}

pub(super) fn map_declarations(
    originals: &[CapturedExtractionSource],
    outputs: &[CapturedExtractionSource],
    selected: usize,
    extracted: &crate::PrefabDocument,
    selected_records: &[RecordDestination],
    selected_placements: &[PlacementDestination],
    generated_placements: Vec<usize>,
    root: &SceneEntityId,
    work: &mut ExtractionWork,
) -> Result<PhysicalDeclarationMap, ExtractionError> {
    if originals.len() != outputs.len() || selected >= originals.len() {
        return Err(ExtractionError::InvalidSource);
    }
    work.admit(originals.len().checked_mul(
        size_of::<Vec<RecordDestinationFact>>() + size_of::<Vec<(OutputSource, usize)>>(),
    ))?;
    let mut records = Vec::with_capacity(originals.len());
    let mut placements = Vec::with_capacity(originals.len());
    let mut instance_frames = Vec::new();
    let mut extracted_index = RecordIndex::new(&extracted.entities, work)?;
    work.admit(extracted.placements.len().checked_mul(size_of::<bool>()))?;
    let mut extracted_placements = vec![false; extracted.placements.len()];
    for index in &generated_placements {
        work.visit(1, 0, 0)?;
        let seen = extracted_placements
            .get_mut(*index)
            .ok_or(ExtractionError::InvalidSource)?;
        if *seen {
            return Err(ExtractionError::InvalidSource);
        }
        *seen = true;
    }
    for (source, (before, after)) in originals.iter().zip(outputs).enumerate() {
        if !same_key(before.key(), after.key(), work)? {
            return Err(ExtractionError::InvalidSource);
        }
        let (old_records, old_placements, old_inputs) = parts(before.document());
        let (new_records, new_placements, new_inputs) = parts(after.document());
        // The source copy and declaration owner retain exact declared inputs. Charge their
        // complete ID comparisons even when none of those inputs is currently demanded.
        for input in old_inputs
            .into_iter()
            .flatten()
            .chain(new_inputs.into_iter().flatten())
        {
            work.visit(1, input.as_str().len(), 0)?;
        }
        if old_inputs != new_inputs {
            return Err(ExtractionError::InvalidSource);
        }
        work.admit(
            old_records
                .len()
                .checked_mul(size_of::<RecordDestinationFact>()),
        )?;
        let mut mapping = Vec::with_capacity(old_records.len());
        let mut index = RecordIndex::new(new_records, work)?;
        if source == selected && selected_records.len() != old_records.len() {
            return Err(ExtractionError::InvalidSource);
        }
        for (position, before) in old_records.iter().enumerate() {
            let destination = if source == selected
                && selected_records[position] == RecordDestination::Extracted
            {
                OutputSource::Extracted
            } else {
                OutputSource::Captured(source)
            };
            let (record, after) = match destination {
                OutputSource::Captured(_) => index.claim(&before.id, work)?,
                OutputSource::Extracted => extracted_index.claim(&before.id, work)?,
            };
            record_topology(before, after, &mut instance_frames, work)?;
            mapping.push(RecordDestinationFact {
                source: destination,
                record,
            });
        }
        if source == selected {
            let (_, anchor) = index.claim(root, work)?;
            if !anchor.components.is_empty() || anchor.prefab.is_none() {
                return Err(ExtractionError::InvalidSource);
            }
        }
        index.finish()?;
        records.push(mapping);
        if source == selected && selected_placements.len() != old_placements.len() {
            return Err(ExtractionError::InvalidSource);
        }
        work.admit(
            old_placements
                .len()
                .checked_mul(size_of::<(OutputSource, usize)>()),
        )?;
        let mut destinations = Vec::with_capacity(old_placements.len());
        work.admit(new_placements.len().checked_mul(size_of::<bool>()))?;
        let mut retained_placements = vec![false; new_placements.len()];
        for (position, old) in old_placements.iter().enumerate() {
            let (source_slot, output, new) = if source == selected {
                match selected_placements[position] {
                    PlacementDestination::Retained(output) => (
                        OutputSource::Captured(source),
                        output,
                        new_placements.get(output),
                    ),
                    PlacementDestination::Extracted(output) => (
                        OutputSource::Extracted,
                        output,
                        extracted.placements.get(output),
                    ),
                }
            } else {
                (
                    OutputSource::Captured(source),
                    position,
                    new_placements.get(position),
                )
            };
            let new = new.ok_or(ExtractionError::InvalidSource)?;
            let seen = match source_slot {
                OutputSource::Captured(_) => &mut retained_placements,
                OutputSource::Extracted => &mut extracted_placements,
            }
            .get_mut(output)
            .ok_or(ExtractionError::InvalidSource)?;
            if *seen {
                return Err(ExtractionError::InvalidSource);
            }
            *seen = true;
            placement_topology(old, new, work)?;
            destinations.push((source_slot, output));
        }
        if source != selected && old_placements.len() != new_placements.len() {
            return Err(ExtractionError::InvalidSource);
        }
        work.visit(retained_placements.len(), 0, 0)?;
        if retained_placements.iter().any(|seen| !seen) {
            return Err(ExtractionError::InvalidSource);
        }
        placements.push(destinations);
    }
    extracted_index.finish()?;
    work.visit(extracted_placements.len(), 0, 0)?;
    if extracted_placements.iter().any(|seen| !seen) {
        return Err(ExtractionError::InvalidSource);
    }
    Ok(PhysicalDeclarationMap {
        records,
        placements,
        generated_placements,
    })
}

fn parts(
    source: SourceDocumentInput<'_>,
) -> (
    &[SceneEntityRecord],
    &[SceneEntityPlacement],
    Option<&std::collections::BTreeSet<ReferenceInputId>>,
) {
    match source {
        SourceDocumentInput::Scene(document) => (&document.entities, &document.placements, None),
        SourceDocumentInput::Prefab(document) => (
            &document.entities,
            &document.placements,
            Some(&document.inputs),
        ),
    }
}

pub(super) struct RecordIndex<'a> {
    records: &'a [SceneEntityRecord],
    index: BTreeMap<&'a SceneEntityId, usize>,
    seen: Vec<bool>,
    maximum: usize,
    remaining: usize,
}

impl<'a> RecordIndex<'a> {
    pub(super) fn new(
        records: &'a [SceneEntityRecord],
        work: &mut ExtractionWork,
    ) -> Result<Self, ExtractionError> {
        work.admit(records.len().checked_mul(96 + size_of::<bool>()))?;
        let mut index = BTreeMap::new();
        let mut maximum = 0usize;
        for (position, record) in records.iter().enumerate() {
            admit_id_lookup(&record.id, maximum, index.len(), work)?;
            if index.insert(&record.id, position).is_some() {
                return Err(ExtractionError::InvalidSource);
            }
            maximum = maximum.max(record.id.as_str().len());
        }
        Ok(Self {
            records,
            index,
            seen: vec![false; records.len()],
            maximum,
            remaining: records.len(),
        })
    }

    pub(super) fn claim(
        &mut self,
        id: &SceneEntityId,
        work: &mut ExtractionWork,
    ) -> Result<(usize, &'a SceneEntityRecord), ExtractionError> {
        admit_id_lookup(id, self.maximum, self.index.len(), work)?;
        let position = *self.index.get(id).ok_or(ExtractionError::InvalidSource)?;
        let seen = self
            .seen
            .get_mut(position)
            .ok_or(ExtractionError::InvalidSource)?;
        if *seen {
            return Err(ExtractionError::InvalidSource);
        }
        *seen = true;
        self.remaining -= 1;
        Ok((position, &self.records[position]))
    }

    pub(super) fn finish(self) -> Result<(), ExtractionError> {
        if self.remaining == 0 {
            Ok(())
        } else {
            Err(ExtractionError::InvalidSource)
        }
    }
}

fn admit_id_lookup(
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

fn same_key(
    before: &SourceDocumentKey,
    after: &SourceDocumentKey,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(
        1,
        before
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(after.checked_retained_bytes()?))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    Ok(before == after)
}

fn record_topology<'a>(
    before: &'a SceneEntityRecord,
    after: &'a SceneEntityRecord,
    instance_frames: &mut Vec<InstanceTopologyFrame<'a>>,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(
        1,
        before
            .id
            .as_str()
            .len()
            .saturating_add(after.id.as_str().len()),
        0,
    )?;
    if before.id != after.id {
        return Err(ExtractionError::InvalidSource);
    }
    components_topology(&before.components, &after.components, work)?;
    instance_topology(
        before.prefab.as_ref(),
        after.prefab.as_ref(),
        instance_frames,
        work,
    )
}

fn components_topology(
    before: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
    after: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    if before.len() != after.len() {
        return Err(ExtractionError::InvalidSource);
    }
    for ((old_id, old), (new_id, new)) in before.iter().zip(after) {
        work.visit(
            1,
            old_id.as_str().len().saturating_add(new_id.as_str().len()),
            0,
        )?;
        if old_id != new_id {
            return Err(ExtractionError::InvalidSource);
        }
        component_topology(old, new, work)?;
    }
    Ok(())
}

fn component_topology(
    before: &SceneComponentRecord,
    after: &SceneComponentRecord,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(
        1,
        before
            .owner()
            .map_or(0, |owner| owner.as_str().len())
            .saturating_add(after.owner().map_or(0, |owner| owner.as_str().len())),
        0,
    )?;
    if before.version != after.version
        || before.owner() != after.owner()
        || before.contextual_defaults.len() != after.contextual_defaults.len()
    {
        return Err(ExtractionError::InvalidSource);
    }
    for (old, new) in before
        .contextual_defaults
        .keys()
        .zip(after.contextual_defaults.keys())
    {
        same_field(old, new, work)?;
    }
    Ok(())
}

fn same_field(
    before: &ComponentFieldId,
    after: &ComponentFieldId,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(
        1,
        before.as_str().len().saturating_add(after.as_str().len()),
        0,
    )?;
    if before == after {
        Ok(())
    } else {
        Err(ExtractionError::InvalidSource)
    }
}

struct InstanceTopologyFrame<'a> {
    additions:
        std::iter::Zip<std::slice::Iter<'a, PrefabAddition>, std::slice::Iter<'a, PrefabAddition>>,
    claims: std::iter::Zip<
        std::slice::Iter<'a, PrefabInstanceClaim>,
        std::slice::Iter<'a, PrefabInstanceClaim>,
    >,
    depth: usize,
}

fn instance_topology<'a>(
    before: Option<&'a PrefabInstance>,
    after: Option<&'a PrefabInstance>,
    frames: &mut Vec<InstanceTopologyFrame<'a>>,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    if let Some(frame) = inspect_instance_topology(before, after, work, 1)? {
        push_instance_frame(frames, frame, work)?;
    }
    while let Some(frame) = frames.last_mut() {
        let nested = if let Some((before, after)) = frame.additions.next() {
            work.visit(
                1,
                before
                    .id
                    .as_str()
                    .len()
                    .saturating_add(after.id.as_str().len()),
                0,
            )?;
            if before.id != after.id {
                return Err(ExtractionError::InvalidSource);
            }
            components_topology(&before.components, &after.components, work)?;
            (before.prefab.as_ref(), after.prefab.as_ref())
        } else if let Some((before, after)) = frame.claims.next() {
            (before.value.as_ref(), after.value.as_ref())
        } else {
            frames.pop();
            continue;
        };
        let depth = frame.depth + 1;
        if let Some(frame) = inspect_instance_topology(nested.0, nested.1, work, depth)? {
            push_instance_frame(frames, frame, work)?;
        }
    }
    Ok(())
}

fn push_instance_frame<'a>(
    frames: &mut Vec<InstanceTopologyFrame<'a>>,
    frame: InstanceTopologyFrame<'a>,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    if frames.len() == frames.capacity() {
        let additional = frames.capacity().max(4);
        work.admit(
            frames
                .capacity()
                .checked_add(additional)
                .and_then(|capacity| capacity.checked_mul(size_of::<InstanceTopologyFrame<'_>>())),
        )?;
        frames
            .try_reserve_exact(additional)
            .map_err(|_| ExtractionError::Capacity)?;
    }
    frames.push(frame);
    Ok(())
}

fn inspect_instance_topology<'a>(
    before: Option<&'a PrefabInstance>,
    after: Option<&'a PrefabInstance>,
    work: &mut ExtractionWork,
    depth: usize,
) -> Result<Option<InstanceTopologyFrame<'a>>, ExtractionError> {
    work.visit(1, 0, 0)?;
    let (before, after) = match (before, after) {
        (None, None) => return Ok(None),
        (Some(before), Some(after)) if depth <= MAX_PHYSICAL_DEPTH => (before, after),
        _ => return Err(ExtractionError::InvalidSource),
    };
    work.visit(
        1,
        crate::provenance::provenance_asset_ref_bytes(&before.source)
            .and_then(|bytes| {
                bytes.checked_add(crate::provenance::provenance_asset_ref_bytes(
                    &after.source,
                )?)
            })
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    if before.source != after.source {
        return Err(ExtractionError::InvalidSource);
    }
    match (&before.binding, &after.binding) {
        (PrefabInstanceBinding::Forest, PrefabInstanceBinding::Forest) => {}
        (
            PrefabInstanceBinding::Root { base: old, .. },
            PrefabInstanceBinding::Root { base: new, .. },
        ) if same_key(old, new, work)? => {}
        _ => return Err(ExtractionError::InvalidSource),
    }
    match (&before.arguments, &after.arguments) {
        (None, None) => {}
        (Some(old), Some(new)) => {
            if !same_key(&old.base, &new.base, work)? || old.values.len() != new.values.len() {
                return Err(ExtractionError::InvalidSource);
            }
            for (old, new) in old.values.keys().zip(new.values.keys()) {
                work.visit(1, old.as_str().len().saturating_add(new.as_str().len()), 0)?;
                if old != new {
                    return Err(ExtractionError::InvalidSource);
                }
            }
        }
        _ => return Err(ExtractionError::InvalidSource),
    }
    let (before, after) = match (&before.overrides, &after.overrides) {
        (None, None) => return Ok(None),
        (Some(before), Some(after)) => (before, after),
        _ => return Err(ExtractionError::InvalidSource),
    };
    if !same_key(&before.base, &after.base, work)?
        || before.additions.len() != after.additions.len()
        || before.instances.len() != after.instances.len()
        || before.components.len() != after.components.len()
        || before.placements.len() != after.placements.len()
        || before.suppressions.len() != after.suppressions.len()
    {
        return Err(ExtractionError::InvalidSource);
    }
    for (before, after) in before.components.iter().zip(&after.components) {
        work.visit(1, 0, 0)?;
        let (old_id, new_id) = match (before, after) {
            (
                PrefabComponentIntent::Set {
                    component: old,
                    record: old_record,
                    mode: old_mode,
                    ..
                },
                PrefabComponentIntent::Set {
                    component: new,
                    record: new_record,
                    mode: new_mode,
                    ..
                },
            ) if old_mode == new_mode => {
                component_topology(old_record, new_record, work)?;
                (old, new)
            }
            (
                PrefabComponentIntent::Suppress { component: old, .. },
                PrefabComponentIntent::Suppress { component: new, .. },
            ) => (old, new),
            (
                PrefabComponentIntent::Properties {
                    component: old,
                    version: old_version,
                    claims: old_claims,
                    ..
                },
                PrefabComponentIntent::Properties {
                    component: new,
                    version: new_version,
                    claims: new_claims,
                    ..
                },
            ) if old_version == new_version && old_claims.len() == new_claims.len() => {
                for (old, new) in old_claims.iter().zip(new_claims) {
                    property_path(&old.path, &new.path, work)?;
                    if !matches!(
                        (&old.intent, &new.intent),
                        (PrefabPropertyIntent::Set(_), PrefabPropertyIntent::Set(_))
                            | (PrefabPropertyIntent::Absent, PrefabPropertyIntent::Absent)
                    ) {
                        return Err(ExtractionError::InvalidSource);
                    }
                }
                (old, new)
            }
            _ => return Err(ExtractionError::InvalidSource),
        };
        work.visit(
            1,
            old_id.as_str().len().saturating_add(new_id.as_str().len()),
            0,
        )?;
        if old_id != new_id {
            return Err(ExtractionError::InvalidSource);
        }
    }
    for (old, new) in before.placements.iter().zip(&after.placements) {
        placement_topology(old, new, work)?;
    }
    work.visit(before.suppressions.len(), 0, 0)?;
    if before.additions.is_empty() && before.instances.is_empty() {
        return Ok(None);
    }
    Ok(Some(InstanceTopologyFrame {
        additions: before.additions.iter().zip(&after.additions),
        claims: before.instances.iter().zip(&after.instances),
        depth,
    }))
}

fn property_path(
    before: &ComponentPropertyPath,
    after: &ComponentPropertyPath,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(
        before
            .steps
            .len()
            .saturating_add(after.steps.len())
            .saturating_add(1),
        before
            .root
            .as_str()
            .len()
            .saturating_add(after.root.as_str().len()),
        0,
    )?;
    for step in before.steps.iter().chain(&after.steps) {
        let bytes = match step {
            ComponentPropertyStep::Member(id) => id.as_str().len(),
            ComponentPropertyStep::MapEntry(key) => key.len(),
            ComponentPropertyStep::ListItem { by, key } => {
                by.as_str().len().saturating_add(match key {
                    ComponentPropertyKey::String(key) => key.len(),
                    _ => 8,
                })
            }
        };
        work.visit(0, bytes, 0)?;
    }
    if before == after {
        Ok(())
    } else {
        Err(ExtractionError::InvalidSource)
    }
}

fn placement_topology(
    before: &SceneEntityPlacement,
    after: &SceneEntityPlacement,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    work.visit(1, 0, 0)?;
    let same = matches!(
        (&before.placement.slot, &after.placement.slot),
        (ScenePlacementSlot::First, ScenePlacementSlot::First)
            | (ScenePlacementSlot::Last, ScenePlacementSlot::Last)
            | (ScenePlacementSlot::Before(_), ScenePlacementSlot::Before(_))
            | (ScenePlacementSlot::After(_), ScenePlacementSlot::After(_))
    );
    if same {
        Ok(())
    } else {
        Err(ExtractionError::InvalidSource)
    }
}
