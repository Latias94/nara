//! Schema-relative property pins evaluated against one immutable inherited value.

use std::collections::BTreeMap;

use nara_reflect::{
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaVersion,
    ComponentValue, ComponentValueMaterializationLimits, ComponentValueShape,
    effective_component_value_owned, resolve_component_property,
};

use crate::SceneEntityId;

use super::components::{check_value_cost, effective_value, reserve_cost, validate_value};
use super::references::{bind_property_references, binding_growth};
use super::{PrefabEntityTarget, PrefabOverlayConflict, PrefabPropertyClaim, PrefabPropertyIntent};

#[cfg(test)]
#[path = "properties/owned_candidate_tests.rs"]
mod owned_candidate_tests;

pub(super) fn apply_claims(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    original: &ComponentValue,
    claims: &[PrefabPropertyClaim],
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValue, PrefabOverlayConflict> {
    if version != schema.version() {
        return Err(PrefabOverlayConflict::Component);
    }
    let (mut nodes, mut bytes) = claim_cost(claims, limits)?;
    let fields = claims
        .iter()
        .map(|claim| {
            if schema.field_tombstones().contains(&claim.path.root) {
                return Err(PrefabOverlayConflict::Component);
            }
            schema
                .fields()
                .iter()
                .find(|field| field.id() == &claim.path.root)
                .ok_or(PrefabOverlayConflict::Component)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (claim, field) in claims.iter().zip(&fields) {
        // Reserve every introduced subtree and binding's positive growth before cloning
        // the inherited value. This intentionally does not credit removed subtrees.
        bytes = bytes.saturating_add(binding_growth(&claim.bindings, targets, limits)?);
        if matches!(&claim.intent, PrefabPropertyIntent::Absent)
            && let Some(default) = field.default_value()
        {
            let cost = default
                .cost_up_to(limits.nodes().get(), limits.bytes().get())
                .ok_or(PrefabOverlayConflict::WorkLimit)?;
            nodes = nodes.saturating_add(cost.nodes());
            bytes = bytes.saturating_add(cost.logical_bytes());
        }
        if (matches!(&claim.intent, PrefabPropertyIntent::Set(_))
            || field.default_value().is_some())
            && let Some(ComponentFieldPathSegment::Field(name)) = field.path().segments().last()
        {
            bytes = bytes.saturating_add(name.len());
        }
    }
    let base_limits = reserve_cost(limits, nodes, bytes)?;
    let base = effective_value(schema, original, base_limits)?;

    // Every selector observes the same base. A Set cannot manufacture another claim's target.
    for (claim, field) in claims.iter().zip(&fields) {
        match &claim.intent {
            PrefabPropertyIntent::Absent => {
                if !claim.path.steps.is_empty()
                    || (field.required && field.default_value().is_none())
                {
                    return Err(PrefabOverlayConflict::Component);
                }
                if field.default_value().is_none() && !claim.bindings.is_empty() {
                    return Err(PrefabOverlayConflict::Reference);
                }
                require_map_parent(original, field, true)?;
            }
            PrefabPropertyIntent::Set(value) => {
                if root_missing(original, field)? {
                    require_map_parent(original, field, false)?;
                    if claim.path.steps.is_empty() {
                        if field.required && field.default_value().is_none() {
                            return Err(PrefabOverlayConflict::Component);
                        }
                        field
                            .shape()
                            .validate_value(value)
                            .map_err(|_| PrefabOverlayConflict::Component)?;
                        continue;
                    }
                    if field.default_value().is_none() {
                        return Err(PrefabOverlayConflict::Component);
                    }
                }
                let selected = resolve_component_property(schema, version, &base, &claim.path)
                    .map_err(|_| PrefabOverlayConflict::Component)?;
                selected
                    .shape
                    .validate_value(value)
                    .map_err(|_| PrefabOverlayConflict::Component)?;
            }
        }
    }

    let mut candidate = original.clone();
    for (claim, field) in claims.iter().zip(&fields) {
        match &claim.intent {
            PrefabPropertyIntent::Absent => {
                if !root_missing(&candidate, field)? {
                    candidate
                        .remove_path(field.path())
                        .map_err(|_| PrefabOverlayConflict::Component)?;
                }
            }
            PrefabPropertyIntent::Set(value) => {
                let shape = if claim.path.steps.is_empty() {
                    field.shape()
                } else {
                    resolve_component_property(schema, version, &base, &claim.path)
                        .map_err(|_| PrefabOverlayConflict::Component)?
                        .shape
                };
                let bound = bind_property_references(
                    field,
                    shape,
                    value,
                    &claim.bindings,
                    targets,
                    limits,
                )?;
                if root_missing(&candidate, field)? {
                    let initial = if claim.path.steps.is_empty() {
                        &bound
                    } else {
                        field
                            .default_value()
                            .ok_or(PrefabOverlayConflict::Component)?
                    };
                    candidate
                        .set_path(field.path(), initial.clone())
                        .map_err(|_| PrefabOverlayConflict::Component)?;
                    if claim.path.steps.is_empty() {
                        continue;
                    }
                }
                // This candidate is isolated from the source; consume it on failure instead of
                // allocating another whole-component rollback copy for every property claim.
                // Complete validation and enclosing-key preservation follow all replacements.
                candidate = claim
                    .path
                    .replace_in_owned_value(schema, version, candidate, bound)
                    .map_err(|_| PrefabOverlayConflict::Component)?;
            }
        }
    }
    let effective = effective_value(schema, &candidate, limits)?;
    for claim in claims {
        preserve_enclosing_keys(schema, version, &base, &effective, &claim.path)?;
    }
    drop(effective);

    // Absent may expose a default with local references. Only that newly exposed payload
    // consumes these bindings; inherited references do not change namespace again.
    for (claim, field) in claims.iter().zip(&fields) {
        if matches!(&claim.intent, PrefabPropertyIntent::Absent)
            && let Some(default) = field.default_value()
        {
            let bound = bind_property_references(
                field,
                field.shape(),
                default,
                &claim.bindings,
                targets,
                limits,
            )?;
            candidate = effective_component_value_owned(schema, candidate, limits)
                .map_err(|_| PrefabOverlayConflict::Component)?;
            candidate
                .set_path(field.path(), bound)
                .map_err(|_| PrefabOverlayConflict::Component)?;
        }
    }
    validate_value(schema, &candidate, limits)?;
    Ok(candidate)
}

/// Declaration conflicts survive even when a later complete component discards the values.
/// The caller admits this pointer index before allocation.
pub(super) fn validate_claim_paths(
    claims: &[PrefabPropertyClaim],
) -> Result<(), PrefabOverlayConflict> {
    let mut paths = claims.iter().map(|claim| &claim.path).collect::<Vec<_>>();
    paths.sort_unstable();
    if paths
        .windows(2)
        .any(|pair| pair[0].root == pair[1].root && pair[1].steps.starts_with(&pair[0].steps))
    {
        return Err(PrefabOverlayConflict::DuplicateIntent);
    }
    Ok(())
}

fn claim_cost(
    claims: &[PrefabPropertyClaim],
    limits: ComponentValueMaterializationLimits,
) -> Result<(usize, usize), PrefabOverlayConflict> {
    if claims.len() > limits.nodes().get() {
        return Err(PrefabOverlayConflict::WorkLimit);
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for claim in claims {
        if claim.path.steps.len() > limits.depth().get() {
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        bytes = bytes.saturating_add(
            claim
                .path
                .logical_bytes()
                .map_err(|_| PrefabOverlayConflict::Component)?,
        );
        if bytes > limits.bytes().get() {
            return Err(PrefabOverlayConflict::WorkLimit);
        }
        if let PrefabPropertyIntent::Set(value) = &claim.intent {
            check_value_cost(value, limits)?;
            let cost = value
                .cost_up_to(limits.nodes().get() - nodes, limits.bytes().get() - bytes)
                .ok_or(PrefabOverlayConflict::WorkLimit)?;
            nodes += cost.nodes();
            bytes += cost.logical_bytes();
        }
    }
    validate_claim_paths(claims)?;
    Ok((nodes, bytes))
}

fn root_missing(
    value: &ComponentValue,
    field: &ComponentFieldSchema,
) -> Result<bool, PrefabOverlayConflict> {
    match value.get_path(field.path()) {
        Ok(_) => Ok(false),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) => Ok(true),
        Err(_) => Err(PrefabOverlayConflict::Component),
    }
}

fn require_map_parent(
    value: &ComponentValue,
    field: &ComponentFieldSchema,
    allow_absent_parent: bool,
) -> Result<(), PrefabOverlayConflict> {
    let Some((ComponentFieldPathSegment::Field(_), ancestors)) =
        field.path().segments().split_last()
    else {
        return Err(PrefabOverlayConflict::Component);
    };
    let parent = ComponentFieldPath::new(ancestors.iter().cloned());
    match value.get_path(&parent) {
        Ok(ComponentValue::Map(_)) => Ok(()),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) if allow_absent_parent => Ok(()),
        _ => Err(PrefabOverlayConflict::Component),
    }
}

fn preserve_enclosing_keys(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    original: &ComponentValue,
    candidate: &ComponentValue,
    path: &ComponentPropertyPath,
) -> Result<(), PrefabOverlayConflict> {
    for (index, step) in path.steps.iter().enumerate() {
        let ComponentPropertyStep::ListItem { by, .. } = step else {
            continue;
        };
        let item = ComponentPropertyPath::new(path.root.clone())
            .with_steps(path.steps[..=index].iter().cloned());
        let before = resolve_component_property(schema, version, original, &item)
            .map_err(|_| PrefabOverlayConflict::Component)?;
        let after = resolve_component_property(schema, version, candidate, &item)
            .map_err(|_| PrefabOverlayConflict::Component)?;
        let ComponentValueShape::Record { members } = before.shape else {
            return Err(PrefabOverlayConflict::Component);
        };
        let member = members
            .iter()
            .find(|member| member.id.as_ref() == Some(by))
            .ok_or(PrefabOverlayConflict::Component)?;
        let key = before
            .value
            .get(&member.name)
            .ok_or(PrefabOverlayConflict::Component)?;
        if after.value.get(&member.name) != Some(key) {
            return Err(PrefabOverlayConflict::Component);
        }
    }
    Ok(())
}
