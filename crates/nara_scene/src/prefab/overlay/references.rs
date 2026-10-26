//! Exact, single-consumption binding of schema-declared local reference payload slots.

use std::collections::BTreeMap;

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::EntityReference;
use nara_reflect::__private::{
    DeclaredEntityReferencePlan, plan_declared_entity_references,
    plan_declared_value_entity_references,
};
use nara_reflect::{
    ComponentEntityReferenceRewriteError, ComponentFieldSchema, ComponentSchema, ComponentValue,
    ComponentValueMaterializationLimits, ComponentValueShape, EntityReferenceTraversalLimits,
};

use crate::SceneEntityId;

use super::{PrefabEntityTarget, PrefabOverlayConflict, PrefabReferenceBinding};

/// Reserves growth before default materialization or property/base copying. Taking only
/// positive deltas is conservative when another slot shrinks; no payload is copied here.
pub(super) fn binding_growth(
    bindings: &[PrefabReferenceBinding],
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    limits: ComponentValueMaterializationLimits,
) -> Result<usize, PrefabOverlayConflict> {
    if bindings.len() > limits.nodes().get() {
        return Err(PrefabOverlayConflict::WorkLimit);
    }
    let mut growth = 0usize;
    for binding in bindings {
        let target = targets
            .get(&binding.target)
            .ok_or_else(|| PrefabOverlayConflict::MissingTarget(binding.target.clone()))?;
        growth = growth
            .checked_add(
                target
                    .as_str()
                    .len()
                    .saturating_sub(binding.expected.as_str().len()),
            )
            .filter(|growth| *growth <= limits.bytes().get())
            .ok_or(PrefabOverlayConflict::WorkLimit)?;
    }
    Ok(growth)
}

pub(super) fn bind_references(
    schema: &ComponentSchema,
    value: &ComponentValue,
    bindings: &[PrefabReferenceBinding],
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValue, PrefabOverlayConflict> {
    let traversal = traversal_limits(limits, schema.fields().iter())?;
    let plan = plan_declared_entity_references::<PrefabOverlayConflict>(schema, value, traversal)
        .map_err(reference_error)?;
    bind_plan(plan, value, bindings, targets, limits)
}

pub(super) fn bind_property_references(
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    payload: &ComponentValue,
    bindings: &[PrefabReferenceBinding],
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValue, PrefabOverlayConflict> {
    let traversal = traversal_limits(limits, std::iter::once(field))?;
    // Reflect roots this plan at the selected payload, including an empty scalar slot.
    let plan = plan_declared_value_entity_references::<PrefabOverlayConflict>(
        field, shape, payload, traversal,
    )
    .map_err(reference_error)?;
    bind_plan(plan, payload, bindings, targets, limits)
}

pub(super) fn validate_references(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), PrefabOverlayConflict> {
    plan_declared_entity_references::<PrefabOverlayConflict>(
        schema,
        value,
        traversal_limits(limits, schema.fields().iter())?,
    )
    .map(|_| ())
    .map_err(reference_error)
}

fn traversal_limits<'field>(
    limits: ComponentValueMaterializationLimits,
    fields: impl Iterator<Item = &'field ComponentFieldSchema>,
) -> Result<EntityReferenceTraversalLimits, PrefabOverlayConflict> {
    // Reflect charges plan entries and their paths in addition to the payload. A payload
    // allowance must not charge that metadata twice. Bound it by admitted field identities,
    // payload size and depth while retaining separate exact candidate-value checks below.
    let (field_count, id_bytes) = fields.fold((0usize, 0usize), |(count, bytes), field| {
        (
            count.saturating_add(1),
            bytes.max(field.id().as_str().len()),
        )
    });
    let path_factor = limits.depth().get().saturating_add(2);
    let nodes = limits
        .nodes()
        .get()
        .saturating_mul(path_factor)
        .saturating_add(field_count);
    let bytes = limits
        .bytes()
        .get()
        .saturating_mul(path_factor)
        .saturating_add(limits.nodes().get().saturating_mul(id_bytes));
    Ok(EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).ok_or(PrefabOverlayConflict::WorkLimit)?,
        ByteLimit::new(bytes).ok_or(PrefabOverlayConflict::WorkLimit)?,
        limits.depth(),
    ))
}

fn reference_error(
    error: ComponentEntityReferenceRewriteError<PrefabOverlayConflict>,
) -> PrefabOverlayConflict {
    match error {
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. }
        | ComponentEntityReferenceRewriteError::PathIndexOverflow => {
            PrefabOverlayConflict::WorkLimit
        }
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        _ => PrefabOverlayConflict::Reference,
    }
}

fn bind_plan(
    plan: DeclaredEntityReferencePlan,
    value: &ComponentValue,
    bindings: &[PrefabReferenceBinding],
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValue, PrefabOverlayConflict> {
    if bindings.len() > limits.nodes().get() {
        return Err(PrefabOverlayConflict::WorkLimit);
    }
    let mut pending = BTreeMap::new();
    for binding in bindings {
        if binding.slot.segments().len() > limits.depth().get()
            || binding.target.through.len() > limits.depth().get()
        {
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        // The compiler admits raw binding metadata before calling this helper. Its cost
        // belongs to that work budget, not to this component's candidate-value allowance.
        if pending.insert(&binding.slot, binding).is_some() {
            return Err(PrefabOverlayConflict::Reference);
        }
    }

    let mut resolved = BTreeMap::new();
    let mut original_reference_bytes = 0usize;
    let mut resolved_reference_bytes = 0usize;
    plan.visit(value, |slot, reference| {
        if let EntityReference::SceneLocal { entity } = reference {
            let binding = pending
                .remove(slot)
                .ok_or(PrefabOverlayConflict::Reference)?;
            if entity != &binding.expected {
                return Err(PrefabOverlayConflict::Reference);
            }
            let target = targets
                .get(&binding.target)
                .ok_or_else(|| PrefabOverlayConflict::MissingTarget(binding.target.clone()))?;
            original_reference_bytes =
                original_reference_bytes.saturating_add(entity.as_str().len());
            resolved_reference_bytes =
                resolved_reference_bytes.saturating_add(target.as_str().len());
            resolved.insert(slot.clone(), target);
        }
        Ok::<_, PrefabOverlayConflict>(())
    })
    .map_err(reference_error)?;
    // Persistent, Null, asset and non-reference slots never consume local bindings.
    if !pending.is_empty() {
        return Err(PrefabOverlayConflict::Reference);
    }
    let cost = value
        .cost_up_to(limits.nodes().get(), limits.bytes().get())
        .ok_or(PrefabOverlayConflict::WorkLimit)?;
    let candidate_bytes = cost
        .logical_bytes()
        .checked_sub(original_reference_bytes)
        .and_then(|bytes| bytes.checked_add(resolved_reference_bytes))
        .ok_or(PrefabOverlayConflict::WorkLimit)?;
    if candidate_bytes > limits.bytes().get() {
        return Err(PrefabOverlayConflict::WorkLimit);
    }

    let mut candidate = value.clone();
    plan.rewrite_in_place(&mut candidate, |slot, reference| {
        Ok::<_, PrefabOverlayConflict>(match reference {
            EntityReference::SceneLocal { .. } => EntityReference::SceneLocal {
                entity: (*resolved.get(slot).ok_or(PrefabOverlayConflict::Reference)?).clone(),
            },
            EntityReference::Persistent { .. } => reference.clone(),
        })
    })
    .map_err(reference_error)?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use nara_core::{ByteLimit, DepthLimit, ItemLimit};
    use nara_reflect::{ComponentCapability, ComponentFieldId, ComponentFieldPath};

    use super::*;

    fn id(name: &str) -> SceneEntityId {
        SceneEntityId::new(name).unwrap()
    }

    fn local(name: &str) -> ComponentValue {
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(name) })
    }

    fn field() -> ComponentFieldSchema {
        ComponentFieldSchema::required(
            ComponentFieldId::new("link"),
            "Link",
            ComponentFieldPath::from_fields(["wire_link"]),
            ComponentValueShape::EntityRef,
        )
        .with_capabilities([ComponentCapability::EntityRef])
    }

    fn binding() -> PrefabReferenceBinding {
        PrefabReferenceBinding {
            slot: ComponentFieldPath::empty(),
            expected: id("socket"),
            target: PrefabEntityTarget::source(id("socket")),
        }
    }

    #[test]
    fn scalar_slots_require_one_matching_binding_and_reject_unused_bindings() {
        let field = field();
        let binding = binding();
        let targets = BTreeMap::from([(binding.target.clone(), id("output"))]);
        let bind = |value: &ComponentValue, bindings: &[PrefabReferenceBinding]| {
            bind_property_references(
                &field,
                field.shape(),
                value,
                bindings,
                &targets,
                Default::default(),
            )
        };
        assert_eq!(
            bind(&local("socket"), std::slice::from_ref(&binding)),
            Ok(local("output"))
        );
        assert_eq!(
            bind(&local("socket"), &[]),
            Err(PrefabOverlayConflict::Reference)
        );
        assert_eq!(
            bind(&local("stale"), std::slice::from_ref(&binding)),
            Err(PrefabOverlayConflict::Reference)
        );
        assert_eq!(
            bind(&local("socket"), &[binding.clone(), binding.clone()]),
            Err(PrefabOverlayConflict::Reference)
        );
        assert_eq!(
            bind(&ComponentValue::Null, &[binding]),
            Err(PrefabOverlayConflict::Reference)
        );
    }

    #[test]
    fn resolved_reference_growth_obeys_the_compilers_value_budget() {
        let field = field();
        let binding = binding();
        let targets = BTreeMap::from([(binding.target.clone(), id(&"x".repeat(512)))]);
        let limits = ComponentValueMaterializationLimits::new(
            ItemLimit::new(128).unwrap(),
            ByteLimit::new(128).unwrap(),
            DepthLimit::new(64).unwrap(),
        );
        assert_eq!(
            bind_property_references(
                &field,
                field.shape(),
                &local("socket"),
                &[binding],
                &targets,
                limits
            ),
            Err(PrefabOverlayConflict::WorkLimit),
        );
    }
}
