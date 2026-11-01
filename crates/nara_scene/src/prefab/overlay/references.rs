//! Exact, single-consumption syntax plans for schema-declared local reference payload slots.

use std::collections::BTreeMap;

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::EntityReference;
use nara_reflect::__private::{
    DeclaredEntityReferencePlan, plan_declared_entity_references,
    plan_declared_value_entity_references,
};
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentValue, ComponentValueMaterializationLimits, ComponentValueShape,
    EntityReferenceTraversalLimits,
};

use super::components::check_value_cost;
use super::{PrefabEntityTarget, PrefabExpansionFailureKind, PrefabReferenceBinding};

/// Validates binding grammar in a complete, default-materialized component value.
///
/// The caller admits complete planner scratch and path/target copies from bounded metadata.
/// This retains authored targets without checking their existence or survival. The compiler
/// resolves only the references selected after all component and property replacements.
pub(super) fn plan_bindings(
    schema: &ComponentSchema,
    value: &ComponentValue,
    bindings: &[PrefabReferenceBinding],
    limits: ComponentValueMaterializationLimits,
) -> Result<BTreeMap<ComponentFieldPath, PrefabEntityTarget>, PrefabExpansionFailureKind> {
    check_value_cost(value, limits)?;
    let traversal = traversal_limits(limits, schema.fields().iter())?;
    let plan =
        plan_declared_entity_references::<PrefabExpansionFailureKind>(schema, value, traversal)
            .map_err(reference_error)?;
    match_bindings(plan, value, bindings, limits)
}

/// Plans bindings relative to the selected replacement payload, including an empty scalar path.
///
/// Defaults and scratch admission have the same caller-owned contract as [`plan_bindings`].
pub(super) fn plan_property_bindings(
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    payload: &ComponentValue,
    bindings: &[PrefabReferenceBinding],
    limits: ComponentValueMaterializationLimits,
) -> Result<BTreeMap<ComponentFieldPath, PrefabEntityTarget>, PrefabExpansionFailureKind> {
    check_value_cost(payload, limits)?;
    let traversal = traversal_limits(limits, std::iter::once(field))?;
    // Reflect roots this plan at the selected payload, including an empty scalar slot.
    let plan = plan_declared_value_entity_references::<PrefabExpansionFailureKind>(
        field, shape, payload, traversal,
    )
    .map_err(reference_error)?;
    match_bindings(plan, payload, bindings, limits)
}

fn traversal_limits<'field>(
    limits: ComponentValueMaterializationLimits,
    fields: impl Iterator<Item = &'field ComponentFieldSchema>,
) -> Result<EntityReferenceTraversalLimits, PrefabExpansionFailureKind> {
    // Reflect charges plan entries and paths as well as the payload. The caller admits that
    // scratch separately; derive finite traversal ceilings rather than charging it against
    // the exact payload allowance again. An unrepresentable ceiling is a work refusal.
    let mut field_count = 0usize;
    let mut id_bytes = 0usize;
    for field in fields {
        field_count = field_count
            .checked_add(1)
            .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
        id_bytes = id_bytes.max(field.id().as_str().len());
    }
    let path_factor = limits
        .depth()
        .get()
        .checked_add(2)
        .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
    let nodes = limits
        .nodes()
        .get()
        .checked_mul(path_factor)
        .and_then(|nodes| nodes.checked_add(field_count))
        .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
    let identity_bytes = limits
        .nodes()
        .get()
        .checked_mul(id_bytes)
        .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
    let bytes = limits
        .bytes()
        .get()
        .checked_mul(path_factor)
        .and_then(|bytes| bytes.checked_add(identity_bytes))
        .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
    Ok(EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).ok_or(PrefabExpansionFailureKind::WorkLimit)?,
        ByteLimit::new(bytes).ok_or(PrefabExpansionFailureKind::WorkLimit)?,
        limits.depth(),
    ))
}

fn reference_error(
    error: ComponentEntityReferenceRewriteError<PrefabExpansionFailureKind>,
) -> PrefabExpansionFailureKind {
    match error {
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. }
        | ComponentEntityReferenceRewriteError::PathIndexOverflow => {
            PrefabExpansionFailureKind::WorkLimit
        }
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        _ => PrefabExpansionFailureKind::Reference,
    }
}

fn match_bindings(
    plan: DeclaredEntityReferencePlan,
    value: &ComponentValue,
    bindings: &[PrefabReferenceBinding],
    limits: ComponentValueMaterializationLimits,
) -> Result<BTreeMap<ComponentFieldPath, PrefabEntityTarget>, PrefabExpansionFailureKind> {
    if bindings.len() > limits.nodes().get() {
        return Err(PrefabExpansionFailureKind::WorkLimit);
    }
    let mut pending = BTreeMap::new();
    for binding in bindings {
        if binding.slot.segments().len() > limits.depth().get()
            || binding.target.through.len() > limits.depth().get()
        {
            return Err(PrefabExpansionFailureKind::WorkLimit);
        }
        // Complete path and target copies belong to the caller's admitted planner scratch,
        // independently of the payload's exact node and byte allowance.
        if pending.insert(&binding.slot, binding).is_some() {
            return Err(PrefabExpansionFailureKind::Reference);
        }
    }

    let mut planned = BTreeMap::new();
    plan.visit(value, |slot, reference| {
        if let EntityReference::SceneLocal { entity } = reference {
            let binding = pending
                .remove(slot)
                .ok_or(PrefabExpansionFailureKind::Reference)?;
            if entity != &binding.expected {
                return Err(PrefabExpansionFailureKind::Reference);
            }
            planned.insert(slot.clone(), binding.target.clone());
        }
        Ok::<_, PrefabExpansionFailureKind>(())
    })
    .map_err(reference_error)?;
    // Persistent, Null, asset and non-reference slots never consume local bindings.
    if !pending.is_empty() {
        return Err(PrefabExpansionFailureKind::Reference);
    }
    Ok(planned)
}

#[cfg(test)]
#[path = "references/tests.rs"]
mod tests;
