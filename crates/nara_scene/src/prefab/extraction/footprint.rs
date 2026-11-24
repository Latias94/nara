//! Conservative physical and modified-context dependencies, without interpreting overlays.
//!
//! Preservation admits unavailable payloads only in contexts disjoint from the selected source.
//! Known payloads still require rewriting. This proves no whole-document equality, activation,
//! or meaningful expansion of a dependency cycle.

use std::{collections::BTreeMap, mem::size_of};

use nara_asset::AssetRef;

use super::{
    ExtractionError, ExtractionWork, MAX_PHYSICAL_DEPTH, SourceDocumentInput,
    corpus::CapturedExtractionSource,
};
use crate::{
    PrefabAddition, PrefabComponentIntent, PrefabEntityTarget, PrefabInstance,
    PrefabInstanceBinding, PrefabInstanceClaim, SceneCompilationFailureKind,
    SceneCompilationLimits, SceneEntityPlacement, ScenePlacementParent, ScenePlacementSlot,
    SourceDocumentKey,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceContextFootprint {
    Preserved,
    Affected,
    Unproven,
}

impl SourceContextFootprint {
    pub(super) fn include(&mut self, evidence: Self) {
        if evidence == Self::Affected || *self == Self::Preserved {
            *self = evidence;
        }
    }
}

struct ReverseEdge {
    parent: usize,
    next: Option<usize>,
}

/// The resolver borrows the actual corpus alias index and admits its own lookup work. No source
/// is loaded or cloned here, including for dormant declarations and incomplete contexts.
pub(super) fn classify_contexts(
    sources: &[CapturedExtractionSource],
    members: &[usize],
    selected: usize,
    work: &mut ExtractionWork,
    resolve: impl FnMut(&AssetRef, &mut ExtractionWork) -> Result<Option<usize>, ExtractionError>,
) -> Result<Vec<SourceContextFootprint>, ExtractionError> {
    work.visit(1, 0, 0)?;
    let selected_key = sources
        .get(selected)
        .ok_or(ExtractionError::InvalidSource)?
        .key();
    let allocated = sources
        .len()
        .checked_mul(
            size_of::<SourceContextFootprint>() + size_of::<Option<usize>>() + size_of::<usize>(),
        )
        .ok_or(ExtractionError::Capacity)?;
    work.visit(sources.len(), 0, allocated)?;
    // Keys borrow the captured corpus. Admit complete comparisons before indexing or lookup,
    // just as the corpus owner does; selectors and target spelling are not source identities.
    work.admit(sources.len().checked_mul(256))?;
    let comparison_steps = ((usize::BITS - sources.len().leading_zeros()) as usize + 1) * 16;
    work.visit(sources.len().saturating_mul(comparison_steps), 0, 0)?;
    let mut maximum_key_bytes = 0usize;
    for source in sources {
        let bytes = source
            .key()
            .checked_retained_bytes()
            .ok_or(ExtractionError::Capacity)?;
        maximum_key_bytes = maximum_key_bytes.max(bytes);
    }
    work.visit(
        0,
        maximum_key_bytes
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_mul(sources.len()))
            .and_then(|bytes| bytes.checked_mul(comparison_steps))
            .ok_or(ExtractionError::Capacity)?,
        0,
    )?;
    let mut keys = BTreeMap::new();
    for (index, source) in sources.iter().enumerate() {
        if keys.insert(source.key(), index).is_some() {
            return Err(ExtractionError::InvalidSource);
        }
    }
    let mut queue = Vec::with_capacity(sources.len());
    let mut scan = PhysicalScan {
        extraction: Some(ExtractionContexts {
            sources,
            selected_key,
            keys,
            comparison_steps,
            maximum_key_bytes,
        }),
        work,
        resolve,
        states: vec![SourceContextFootprint::Preserved; sources.len()],
        heads: vec![None; sources.len()],
        edges: Vec::new(),
    };
    scan.states[selected] = SourceContextFootprint::Affected;
    let mut frames = Vec::new();
    scan.work.visit(members.len(), 0, 0)?;
    for &parent in members {
        let source = sources.get(parent).ok_or(ExtractionError::InvalidSource)?;
        scan.document(source.document(), parent, &mut frames)?;
    }

    // A known dependency on P wins over incomplete evidence. Each phase enqueues a source
    // at most once, so even cycles traverse each reverse edge at most twice in total.
    for evidence in [
        SourceContextFootprint::Affected,
        SourceContextFootprint::Unproven,
    ] {
        scan.propagate(evidence, &mut queue)?;
    }
    Ok(scan.states)
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ComponentApplySourceFootprint {
    states: Vec<SourceContextFootprint>,
    pub work_bytes: usize,
    pub reference_work: (usize, usize),
}

impl ComponentApplySourceFootprint {
    pub fn affects(&self, index: usize) -> bool {
        self.states.get(index) == Some(&SourceContextFootprint::Affected)
    }

    pub fn retained_bytes(&self) -> usize {
        self.states.capacity() * size_of::<SourceContextFootprint>()
    }
}

/// Finds the dependants of two existing replacements in a fixed catalogue. A path to
/// either replacement stops at its first changed member, so the same Guard edges cover
/// the original and candidate. The resolver must use the complete configuration aliases.
#[doc(hidden)]
pub fn classify_component_apply_dependants<'a>(
    source_count: usize,
    replacements: [Option<usize>; 2],
    limits: SceneCompilationLimits,
    document: impl Fn(usize) -> Option<SourceDocumentInput<'a>>,
    mut resolve: impl FnMut(&AssetRef) -> Option<usize>,
) -> Result<ComponentApplySourceFootprint, SceneCompilationFailureKind> {
    let mut classify = || {
        if replacements
            .iter()
            .flatten()
            .any(|&index| index >= source_count)
        {
            return Err(ExtractionError::InvalidSource);
        }
        let mut work = ExtractionWork::new(limits, Some(0))?;
        let mut reference_bytes = 0;
        work.visit(
            source_count,
            0,
            source_count
                .checked_mul(
                    size_of::<SourceContextFootprint>()
                        + size_of::<Option<usize>>()
                        + size_of::<usize>(),
                )
                .ok_or(ExtractionError::Capacity)?,
        )?;
        let mut scan = PhysicalScan {
            extraction: None,
            work: &mut work,
            resolve: |reference: &AssetRef, work: &mut ExtractionWork| {
                let bytes = match reference {
                    AssetRef::Path(path) => path.as_str().len(),
                    _ => size_of::<AssetRef>(),
                };
                work.visit(1, bytes, 0)?;
                reference_bytes += bytes;
                Ok(resolve(reference))
            },
            states: vec![SourceContextFootprint::Preserved; source_count],
            heads: vec![None; source_count],
            edges: Vec::new(),
        };
        let mut frames = Vec::new();
        for member in 0..source_count {
            if replacements.contains(&Some(member)) {
                scan.states[member] = SourceContextFootprint::Affected;
            } else {
                let source = document(member).ok_or(ExtractionError::InvalidSource)?;
                scan.document(source, member, &mut frames)?;
            }
        }
        scan.propagate(
            SourceContextFootprint::Affected,
            &mut Vec::with_capacity(source_count),
        )?;
        let states = scan.states;
        let (nodes, work_bytes) = work.usage();
        Ok(ComponentApplySourceFootprint {
            states,
            work_bytes,
            reference_work: (nodes, reference_bytes),
        })
    };
    classify().map_err(|error| match error {
        ExtractionError::Capacity => SceneCompilationFailureKind::WorkLimit,
        _ => SceneCompilationFailureKind::OwnerAdmission,
    })
}

struct ExtractionContexts<'a> {
    sources: &'a [CapturedExtractionSource],
    selected_key: &'a SourceDocumentKey,
    keys: BTreeMap<&'a SourceDocumentKey, usize>,
    comparison_steps: usize,
    maximum_key_bytes: usize,
}

struct PhysicalScan<'a, 'w, R> {
    extraction: Option<ExtractionContexts<'a>>,
    work: &'w mut ExtractionWork,
    resolve: R,
    states: Vec<SourceContextFootprint>,
    heads: Vec<Option<usize>>,
    edges: Vec<ReverseEdge>,
}

/// Only ancestor cursors remain live while descending. Wide overlays do not put all their
/// descendants on the frontier, and source depth never becomes native call-stack depth.
struct PhysicalInstanceFrame<'a> {
    additions: std::slice::Iter<'a, PrefabAddition>,
    claims: std::slice::Iter<'a, PrefabInstanceClaim>,
    parent: usize,
    context: Option<usize>,
    base_matches: bool,
    depth: usize,
}

impl<'a, R> PhysicalScan<'a, '_, R>
where
    R: FnMut(&AssetRef, &mut ExtractionWork) -> Result<Option<usize>, ExtractionError>,
{
    fn document(
        &mut self,
        source: SourceDocumentInput<'a>,
        parent: usize,
        frames: &mut Vec<PhysicalInstanceFrame<'a>>,
    ) -> Result<(), ExtractionError> {
        let (entities, placements) = match source {
            SourceDocumentInput::Scene(document) => (&document.entities, &document.placements),
            SourceDocumentInput::Prefab(document) => {
                if self.extraction.is_some() && !document.inputs.is_empty() {
                    self.mark(parent, None, SourceContextFootprint::Unproven);
                }
                (&document.entities, &document.placements)
            }
        };
        self.work.visit(entities.len(), 0, 0)?;
        for record in entities {
            if let Some(instance) = &record.prefab {
                self.instance(instance, parent, frames)?;
            }
        }
        if self.extraction.is_some() {
            self.placements(placements, parent)?;
        }
        Ok(())
    }

    fn propagate(
        &mut self,
        evidence: SourceContextFootprint,
        queue: &mut Vec<usize>,
    ) -> Result<(), ExtractionError> {
        queue.clear();
        self.work.visit(self.states.len(), 0, 0)?;
        for (index, state) in self.states.iter().enumerate() {
            if *state == evidence {
                queue.push(index);
            }
        }
        let mut cursor = 0;
        while cursor < queue.len() {
            self.work.visit(1, 0, 0)?;
            let mut edge = self.heads[queue[cursor]];
            cursor += 1;
            while let Some(index) = edge {
                self.work.visit(1, 0, 0)?;
                let reverse = &self.edges[index];
                let state = &mut self.states[reverse.parent];
                if *state == SourceContextFootprint::Preserved
                    || (evidence == SourceContextFootprint::Affected
                        && *state == SourceContextFootprint::Unproven)
                {
                    *state = evidence;
                    queue.push(reverse.parent);
                }
                edge = reverse.next;
            }
        }
        Ok(())
    }

    fn mark(&mut self, parent: usize, context: Option<usize>, evidence: SourceContextFootprint) {
        self.states[parent].include(evidence);
        if let Some(context) = context {
            self.states[context].include(evidence);
        }
    }

    fn context(
        &mut self,
        key: &SourceDocumentKey,
        parent: usize,
    ) -> Result<Option<usize>, ExtractionError> {
        let extraction = self
            .extraction
            .as_ref()
            .ok_or(ExtractionError::InvalidSource)?;
        let bytes = key
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(extraction.maximum_key_bytes))
            .and_then(|bytes| bytes.checked_mul(extraction.comparison_steps))
            .ok_or(ExtractionError::Capacity)?;
        self.work.visit(extraction.comparison_steps, bytes, 0)?;
        let context = extraction.keys.get(key).copied();
        if context.is_some_and(|index| {
            matches!(
                &extraction.sources[index],
                CapturedExtractionSource::Prefab { .. }
            )
        }) {
            Ok(context)
        } else {
            self.mark(parent, context, SourceContextFootprint::Unproven);
            Ok(None)
        }
    }

    fn edge(&mut self, dependency: usize, parent: usize) -> Result<(), ExtractionError> {
        self.work.visit(1, 0, 0)?;
        if self.edges.len() == self.edges.capacity() {
            let additional = self.edges.capacity().max(4);
            // Admission is cumulative: previous allocation requests are never refunded.
            self.work.admit(
                self.edges
                    .capacity()
                    .checked_add(additional)
                    .and_then(|capacity| capacity.checked_mul(size_of::<ReverseEdge>())),
            )?;
            self.edges
                .try_reserve_exact(additional)
                .map_err(|_| ExtractionError::Capacity)?;
        }
        let next = self.heads[dependency];
        self.heads[dependency] = Some(self.edges.len());
        self.edges.push(ReverseEdge { parent, next });
        Ok(())
    }

    fn base(
        &mut self,
        base: &SourceDocumentKey,
        actual: Option<usize>,
        parent: usize,
        context: Option<usize>,
    ) -> Result<bool, ExtractionError> {
        let extraction = self
            .extraction
            .as_ref()
            .ok_or(ExtractionError::InvalidSource)?;
        let matches = match actual {
            Some(index) => same_key(base, extraction.sources[index].key(), self.work)?,
            None => false,
        };
        if !matches {
            self.mark(parent, context, SourceContextFootprint::Unproven);
        }
        Ok(matches)
    }

    fn instance(
        &mut self,
        instance: &'a PrefabInstance,
        parent: usize,
        frames: &mut Vec<PhysicalInstanceFrame<'a>>,
    ) -> Result<(), ExtractionError> {
        if let Some(frame) = self.inspect_instance(instance, parent, None, 1)? {
            self.push_frame(frames, frame)?;
        }
        while let Some(frame) = frames.last_mut() {
            let nested = if let Some(addition) = frame.additions.next() {
                addition
                    .prefab
                    .as_ref()
                    .map(|nested| (nested, frame.context))
            } else if let Some(claim) = frame.claims.next() {
                let context = if self.extraction.is_some() {
                    self.claim_context(claim, frame)?
                } else {
                    None
                };
                claim.value.as_ref().map(|nested| (nested, context))
            } else {
                frames.pop();
                continue;
            };
            let Some((nested, context)) = nested else {
                continue;
            };
            let (parent, depth) = (frame.parent, frame.depth);
            if depth == MAX_PHYSICAL_DEPTH {
                // The unvisited suffix could modify a different captured canonical context.
                // Returning a partial graph cannot establish any preserved closure.
                return Err(ExtractionError::InvalidSource);
            } else if let Some(frame) = self.inspect_instance(nested, parent, context, depth + 1)? {
                self.push_frame(frames, frame)?;
            }
        }
        Ok(())
    }

    fn claim_context(
        &mut self,
        claim: &PrefabInstanceClaim,
        frame: &PhysicalInstanceFrame<'_>,
    ) -> Result<Option<usize>, ExtractionError> {
        self.target(&claim.target, frame.parent, None)?;
        if claim.value.is_none() {
            return Ok(None);
        }
        // A routed claim edits the last hop's source. A direct claim edits the overlay base.
        let context = if let Some(hop) = claim.target.through.last() {
            let routed = self.context(&hop.expected_base, frame.parent)?;
            if routed.is_none() {
                self.mark(
                    frame.parent,
                    frame.context,
                    SourceContextFootprint::Unproven,
                );
                for hop in &claim.target.through {
                    let context = self.context(&hop.expected_base, frame.parent)?;
                    self.mark(frame.parent, context, SourceContextFootprint::Unproven);
                }
            }
            routed
        } else {
            frame.context
        };
        if !frame.base_matches {
            self.mark(frame.parent, context, SourceContextFootprint::Unproven);
        }
        Ok(context)
    }

    fn push_frame(
        &mut self,
        frames: &mut Vec<PhysicalInstanceFrame<'a>>,
        frame: PhysicalInstanceFrame<'a>,
    ) -> Result<(), ExtractionError> {
        if frames.len() == frames.capacity() {
            let additional = frames.capacity().max(4);
            self.work.admit(
                frames
                    .capacity()
                    .checked_add(additional)
                    .and_then(|capacity| {
                        capacity.checked_mul(size_of::<PhysicalInstanceFrame<'_>>())
                    }),
            )?;
            frames
                .try_reserve_exact(additional)
                .map_err(|_| ExtractionError::Capacity)?;
        }
        frames.push(frame);
        Ok(())
    }

    fn inspect_instance(
        &mut self,
        instance: &'a PrefabInstance,
        parent: usize,
        context: Option<usize>,
        depth: usize,
    ) -> Result<Option<PhysicalInstanceFrame<'a>>, ExtractionError> {
        self.work.visit(1, 0, 0)?;
        let resolved = (self.resolve)(&instance.source, self.work)?;
        let Some(extraction) = self.extraction.as_ref() else {
            if let Some(index) = resolved {
                if index >= self.heads.len() {
                    return Err(ExtractionError::InvalidSource);
                }
                self.edge(index, parent)?;
            }
            let Some(overlay) = &instance.overrides else {
                return Ok(None);
            };
            self.work.visit(overlay.additions.len(), 0, 0)?;
            self.work.visit(overlay.instances.len(), 0, 0)?;
            return Ok(Self::frame(overlay, parent, None, true, depth));
        };
        let sources = extraction.sources;
        let actual = match resolved {
            Some(index) => {
                self.work.visit(1, 0, 0)?;
                let source = sources.get(index).ok_or(ExtractionError::InvalidSource)?;
                if matches!(source, CapturedExtractionSource::Prefab { .. }) {
                    self.edge(index, parent)?;
                    if let Some(context) = context {
                        self.edge(index, context)?;
                    }
                    Some(index)
                } else {
                    self.mark(parent, context, SourceContextFootprint::Unproven);
                    None
                }
            }
            None => {
                self.mark(parent, context, SourceContextFootprint::Unproven);
                None
            }
        };
        if let PrefabInstanceBinding::Root { base, root } = &instance.binding {
            self.base(base, actual, parent, context)?;
            self.target(root, parent, context)?;
        }
        if let Some(arguments) = &instance.arguments {
            self.base(&arguments.base, actual, parent, context)?;
            // Argument values belong to occurrence-specific lexical environments. This pass
            // proves no parameter environment and deliberately does not walk their values.
            if !arguments.values.is_empty() {
                self.mark(parent, context, SourceContextFootprint::Unproven);
            }
        }
        let Some(overlay) = &instance.overrides else {
            return Ok(None);
        };
        let matches = self.base(&overlay.base, actual, parent, context)?;
        let modified = self.context(&overlay.base, parent)?;
        if !matches {
            // A stale or unresolved overlay cannot establish disjointness for either the
            // requested canonical context or the actual source selected by its alias.
            self.mark(parent, actual, SourceContextFootprint::Unproven);
            self.mark(parent, modified, SourceContextFootprint::Unproven);
        }
        self.work.visit(overlay.additions.len(), 0, 0)?;
        self.work.visit(overlay.instances.len(), 0, 0)?;
        self.work.visit(overlay.components.len(), 0, 0)?;
        for intent in &overlay.components {
            let target = match intent {
                PrefabComponentIntent::Set { target, .. }
                | PrefabComponentIntent::Suppress { target, .. }
                | PrefabComponentIntent::Properties { target, .. } => target,
            };
            self.target(target, parent, None)?;
        }
        self.placements(&overlay.placements, parent)?;
        self.work.visit(overlay.suppressions.len(), 0, 0)?;
        for target in &overlay.suppressions {
            self.target(target, parent, None)?;
        }
        Ok(Self::frame(overlay, parent, modified, matches, depth))
    }

    fn frame(
        overlay: &'a crate::PrefabOverlay,
        parent: usize,
        context: Option<usize>,
        base_matches: bool,
        depth: usize,
    ) -> Option<PhysicalInstanceFrame<'a>> {
        if overlay.additions.is_empty() && overlay.instances.is_empty() {
            return None;
        }
        Some(PhysicalInstanceFrame {
            additions: overlay.additions.iter(),
            claims: overlay.instances.iter(),
            parent,
            context,
            base_matches,
            depth,
        })
    }

    fn target(
        &mut self,
        target: &PrefabEntityTarget,
        parent: usize,
        context: Option<usize>,
    ) -> Result<(), ExtractionError> {
        self.work
            .visit(target.through.len().saturating_add(1), 0, 0)?;
        if !target.through.is_empty() {
            // Canonical predicates alone do not prove that the physical path binds. Checking
            // anchors and layout here would create a second overlay interpreter.
            self.mark(parent, context, SourceContextFootprint::Unproven);
        }
        let selected_key = self
            .extraction
            .as_ref()
            .ok_or(ExtractionError::InvalidSource)?
            .selected_key;
        for hop in &target.through {
            if same_key(&hop.expected_base, selected_key, self.work)? {
                self.mark(parent, context, SourceContextFootprint::Affected);
            }
        }
        Ok(())
    }

    fn placements(
        &mut self,
        placements: &[SceneEntityPlacement],
        parent: usize,
    ) -> Result<(), ExtractionError> {
        self.work.visit(placements.len(), 0, 0)?;
        for declaration in placements {
            self.target(&declaration.target, parent, None)?;
            if let ScenePlacementParent::Entity(target) = &declaration.placement.parent {
                self.target(target, parent, None)?;
            }
            if let ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) =
                &declaration.placement.slot
            {
                self.target(target, parent, None)?;
            }
        }
        Ok(())
    }
}

fn same_key(
    left: &SourceDocumentKey,
    right: &SourceDocumentKey,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    let bytes = left
        .checked_retained_bytes()
        .and_then(|bytes| bytes.checked_add(right.checked_retained_bytes()?))
        .ok_or(ExtractionError::Capacity)?;
    work.visit(1, bytes, 0)?;
    Ok(left == right)
}
