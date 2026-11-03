//! Rewrites complete physical instance declarations in an admitted unpublished candidate.
//!
//! Structural targets retain their original selected-base environment. Payload references use
//! the caller's actual lexical environment, including in dormant additions and instance claims.

use std::{borrow::Cow, mem::size_of, slice::IterMut};

use nara_identity::EntityReference;
use nara_reflect::ComponentTypeId;

use crate::{
    AuthoringSchemaBasis, PrefabAddition, PrefabComponentIntent, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceBinding, PrefabInstanceClaim, PrefabOverlay,
    PrefabReferenceArguments, SceneComponentRecord, SceneEntityPlacement, ScenePlacementParent,
    ScenePlacementSlot, SourceDocumentKey,
};

use super::{
    ExtractionError, ExtractionWork, MAX_PHYSICAL_DEPTH,
    payload::{require_schema, rewrite_properties, rewrite_record, unavailable_schema_error},
    relocation::{AddressRole, BranchRelocation},
};

#[cfg(test)]
mod tests;

/// Each ancestor retains only its unvisited declarations and original structural scope.
/// Iterator items borrow the candidate independently of this frame, so descent never retains
/// a mutable borrow of the stack or queues all siblings. Payload scope stays with the caller.
struct DeclarationFrame<'a> {
    scope: &'a SourceDocumentKey,
    additions: IterMut<'a, PrefabAddition>,
    instances: IterMut<'a, PrefabInstanceClaim>,
    components: IterMut<'a, PrefabComponentIntent>,
    placements: IterMut<'a, SceneEntityPlacement>,
    suppressions: IterMut<'a, PrefabEntityTarget>,
}

/// The caller owns admission and the initial clone of the complete physical declaration.
/// A failure may leave this unpublished candidate partially rewritten; discard it together
/// with any moved-reference inputs collected by this preparation.
///
/// Reports actual binding and structural target changes, including nested declarations.
/// The caller's reference mapper separately tracks payload changes.
pub(super) fn rewrite_instance(
    instance: &mut PrefabInstance,
    relocation: &BranchRelocation<'_, '_>,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
    rewrite_reference: &mut impl FnMut(
        &EntityReference,
        &mut ExtractionWork,
    ) -> Result<EntityReference, ExtractionError>,
) -> Result<bool, ExtractionError> {
    let mut source_changed = false;
    let mut frames = Vec::new();
    let mut next = Some(instance);
    loop {
        if let Some(instance) = next.take() {
            // All retained ancestors have overlays. The root is depth one, and a child of
            // the last allowed ancestor is refused before visiting or mutating that child.
            if frames.len() == MAX_PHYSICAL_DEPTH {
                return Err(ExtractionError::InvalidSource);
            }
            work.visit(1, size_of::<PrefabInstance>(), 0)?;
            if !instance.source.is_primary() {
                return Err(ExtractionError::InvalidSource);
            }

            work.visit(1, size_of::<PrefabInstanceBinding>(), 0)?;
            if let Cow::Owned(binding) =
                relocation.retained_binding(&instance.binding, |bytes| work.admit(bytes))?
            {
                instance.binding = binding;
                source_changed = true;
            }
            if let Some(arguments) = &mut instance.arguments {
                work.visit(1, size_of::<PrefabReferenceArguments>(), 0)?;
                for (input, reference) in &mut arguments.values {
                    work.visit(1, input.as_str().len(), 0)?;
                    *reference = rewrite_reference(reference, work)?;
                }
            }

            if let Some(overlay) = &mut instance.overrides {
                work.visit(1, size_of::<PrefabOverlay>(), 0)?;
                if frames.len() == frames.capacity() {
                    let additional = frames.capacity().max(4);
                    // Charge the complete replacement allocation, not just its growth.
                    // Popped frames and previous allocations never refund cumulative work.
                    work.admit(
                        frames
                            .capacity()
                            .checked_add(additional)
                            .and_then(|capacity| {
                                capacity.checked_mul(size_of::<DeclarationFrame<'_>>())
                            }),
                    )?;
                    frames
                        .try_reserve_exact(additional)
                        .map_err(|_| ExtractionError::Capacity)?;
                }
                // Neither the source request nor any base predicate is a global source-key
                // mapping. Keep each overlay's original selected-base environment.
                frames.push(DeclarationFrame {
                    scope: &overlay.base,
                    additions: overlay.additions.iter_mut(),
                    instances: overlay.instances.iter_mut(),
                    components: overlay.components.iter_mut(),
                    placements: overlay.placements.iter_mut(),
                    suppressions: overlay.suppressions.iter_mut(),
                });
            }
        }

        let Some(frame) = frames.last_mut() else {
            return Ok(source_changed);
        };
        if let Some(addition) = frame.additions.next() {
            work.visit(1, size_of::<PrefabAddition>(), 0)?;
            work.visit(0, addition.id.as_str().len(), 0)?;
            for (component, record) in &mut addition.components {
                work.visit(1, size_of::<(ComponentTypeId, SceneComponentRecord)>(), 0)?;
                rewrite_record(
                    component,
                    record,
                    basis,
                    preserve_unavailable,
                    work,
                    rewrite_reference,
                )?;
            }
            next = addition.prefab.as_mut();
            continue;
        }
        if let Some(claim) = frame.instances.next() {
            work.visit(1, size_of::<PrefabInstanceClaim>(), 0)?;
            source_changed |= rewrite_target(
                &mut claim.target,
                frame.scope,
                AddressRole::InstanceSlot,
                relocation,
                work,
            )?;
            next = claim.value.as_mut();
            continue;
        }

        // Resume the parent's trailing declarations only after all additions and claims,
        // including dormant nested instances, have completed in stored depth-first order.
        for intent in frame.components.by_ref() {
            work.visit(1, size_of::<PrefabComponentIntent>(), 0)?;
            match intent {
                PrefabComponentIntent::Set {
                    target,
                    component,
                    record,
                    ..
                } => {
                    source_changed |=
                        rewrite_target(target, frame.scope, AddressRole::Entity, relocation, work)?;
                    rewrite_record(
                        component,
                        record,
                        basis,
                        preserve_unavailable,
                        work,
                        rewrite_reference,
                    )?;
                }
                PrefabComponentIntent::Suppress { target, component } => {
                    source_changed |=
                        rewrite_target(target, frame.scope, AddressRole::Entity, relocation, work)?;
                    match require_schema(component, None, basis, work) {
                        Ok(_) => {}
                        Err(error) if preserve_unavailable && unavailable_schema_error(&error) => {}
                        Err(error) => return Err(error),
                    }
                }
                PrefabComponentIntent::Properties {
                    target,
                    component,
                    version,
                    claims,
                } => {
                    source_changed |=
                        rewrite_target(target, frame.scope, AddressRole::Entity, relocation, work)?;
                    // Empty batches and Absent claims still require the selected Schema.
                    rewrite_properties(
                        component,
                        *version,
                        claims,
                        basis,
                        preserve_unavailable,
                        work,
                        rewrite_reference,
                    )?;
                }
            }
        }
        for declaration in frame.placements.by_ref() {
            work.visit(1, size_of::<SceneEntityPlacement>(), 0)?;
            source_changed |= rewrite_target(
                &mut declaration.target,
                frame.scope,
                AddressRole::Entity,
                relocation,
                work,
            )?;
            if let ScenePlacementParent::Entity(target) = &mut declaration.placement.parent {
                source_changed |=
                    rewrite_target(target, frame.scope, AddressRole::Entity, relocation, work)?;
            }
            if let ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) =
                &mut declaration.placement.slot
            {
                source_changed |=
                    rewrite_target(target, frame.scope, AddressRole::Entity, relocation, work)?;
            }
        }
        for target in frame.suppressions.by_ref() {
            source_changed |=
                rewrite_target(target, frame.scope, AddressRole::Entity, relocation, work)?;
        }
        frames.pop();
    }
}

fn rewrite_target(
    target: &mut PrefabEntityTarget,
    scope: &SourceDocumentKey,
    role: AddressRole,
    relocation: &BranchRelocation<'_, '_>,
    work: &mut ExtractionWork,
) -> Result<bool, ExtractionError> {
    work.visit(1, size_of::<PrefabEntityTarget>(), 0)?;
    if let Cow::Owned(rewritten) =
        relocation.retained_target(scope, target, role, |bytes| work.admit(bytes))?
    {
        *target = rewritten;
        return Ok(true);
    }
    Ok(false)
}
