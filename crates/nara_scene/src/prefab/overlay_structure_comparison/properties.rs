//! Bounded property claims for the test-private overlay comparison.
#![cfg(test)]

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaVersion,
    ComponentValue, ComponentValueMaterializationLimits, ComponentValueShape,
    effective_component_value, replace_component_property, resolve_component_property,
    validate_component_value,
};
use serde::{Deserialize, Serialize};

use super::Conflict;

mod tests;

const MAX_CLAIMS: usize = 64;
const MAX_CLAIM_NODES: usize = 4_096;
const MAX_CLAIM_BYTES: usize = 64 * 1024;
const MAX_VALUE_NODES: usize = 16_384;
const MAX_VALUE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PropertyClaim {
    pub(super) path: ComponentPropertyPath,
    pub(super) intent: PropertyIntent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum PropertyIntent {
    Set(ComponentValue),
    Absent,
}

/// Admits borrowed raw declarations before the parent clones or binds replacement payloads.
/// Schema-relative selection and complete-value admission remain in `apply_claims`.
pub(super) fn admit_claims(claims: &[&PropertyClaim]) -> Result<(), Conflict> {
    claim_cost(claims).map(|_| ())
}

/// Normalizes retained claims against one admitted current Schema and immutable base.
///
/// The result is authored storage, so omitted defaults remain omitted. An equal Set remains
/// in the caller's claims, and Revert means removing that claim, never inferring inheritance
/// from result equality. Payload reference binding and document admission belong to the parent.
/// These small comparison limits are not production file or authoring policy.
pub(super) fn apply_claims(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    original: &ComponentValue,
    claims: &[PropertyClaim],
) -> Result<ComponentValue, Conflict> {
    if claims.len() > MAX_CLAIMS {
        return Err(Conflict::WorkLimit);
    }
    if version != schema.version() {
        return Err(Conflict::Component);
    }

    let borrowed = claims.iter().collect::<Vec<_>>();
    let (nodes, mut bytes) = claim_cost(&borrowed)?;

    let fields = claims
        .iter()
        .map(|claim| {
            if schema.field_tombstones().contains(&claim.path.root) {
                return Err(Conflict::Component);
            }
            schema
                .fields()
                .iter()
                .find(|field| field.id() == &claim.path.root)
                .ok_or(Conflict::Component)
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Root insertion also copies its wire member name, which need not match the selector ID.
    for (claim, field) in claims.iter().zip(&fields) {
        if matches!(&claim.intent, PropertyIntent::Set(_))
            && let Some(ComponentFieldPathSegment::Field(name)) = field.path().segments().last()
        {
            bytes = bytes.saturating_add(name.len());
        }
    }
    let max_bytes = MAX_VALUE_BYTES
        .checked_sub(bytes)
        .ok_or(Conflict::WorkLimit)?;
    let max_nodes = MAX_VALUE_NODES - nodes;
    original
        .cost_up_to(max_nodes, max_bytes)
        .ok_or(Conflict::WorkLimit)?;
    let limits = ComponentValueMaterializationLimits::new(
        ItemLimit::new(max_nodes).ok_or(Conflict::WorkLimit)?,
        ByteLimit::new(max_bytes).ok_or(Conflict::WorkLimit)?,
        DepthLimit::new(64).ok_or(Conflict::WorkLimit)?,
    );
    let base =
        effective_component_value(schema, original, limits).map_err(|_| Conflict::Component)?;
    base.cost_up_to(max_nodes, max_bytes)
        .ok_or(Conflict::WorkLimit)?;
    validate_component_value(schema, base.as_ref()).map_err(|_| Conflict::Component)?;

    // Admission always observes the same base. Later Set claims cannot manufacture a target
    // for another claim, including a selector reached through a changed list identity.
    for (claim, field) in claims.iter().zip(&fields) {
        match &claim.intent {
            PropertyIntent::Absent => {
                if !claim.path.steps.is_empty()
                    || (field.required && field.default_value().is_none())
                {
                    return Err(Conflict::Component);
                }
                require_map_parent(original, field, true)?;
            }
            PropertyIntent::Set(value) => {
                let missing = root_missing(original, field)?;
                if missing {
                    require_map_parent(original, field, false)?;
                    if claim.path.steps.is_empty() {
                        // Like source authoring, a missing root can be inserted directly;
                        // no anonymous storage ancestor is reconstructed.
                        if field.required && field.default_value().is_none() {
                            return Err(Conflict::Component);
                        }
                        field
                            .shape()
                            .validate_value(value)
                            .map_err(|_| Conflict::Component)?;
                        continue;
                    }
                    if field.default_value().is_none() {
                        return Err(Conflict::Component);
                    }
                }
                resolve_component_property(schema, version, base.as_ref(), &claim.path)
                    .map_err(|_| Conflict::Component)?;
            }
        }
    }

    let mut candidate = original.clone();
    for (claim, field) in claims.iter().zip(&fields) {
        match &claim.intent {
            PropertyIntent::Absent => {
                if !root_missing(&candidate, field)? {
                    candidate
                        .remove_path(field.path())
                        .map_err(|_| Conflict::Component)?;
                }
            }
            PropertyIntent::Set(value) => {
                if root_missing(&candidate, field)? {
                    let initial = if claim.path.steps.is_empty() {
                        value
                    } else {
                        field.default_value().ok_or(Conflict::Component)?
                    };
                    candidate
                        .set_path(field.path(), initial.clone())
                        .map_err(|_| Conflict::Component)?;
                    if claim.path.steps.is_empty() {
                        continue;
                    }
                }
                replace_component_property(
                    schema,
                    version,
                    &mut candidate,
                    &claim.path,
                    value.clone(),
                )
                .map_err(|_| Conflict::Component)?;
            }
        }
    }
    candidate
        .cost_up_to(MAX_VALUE_NODES, MAX_VALUE_BYTES)
        .ok_or(Conflict::WorkLimit)?;
    let effective = effective_component_value(
        schema,
        &candidate,
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(MAX_VALUE_NODES).ok_or(Conflict::WorkLimit)?,
            ByteLimit::new(MAX_VALUE_BYTES).ok_or(Conflict::WorkLimit)?,
            DepthLimit::new(64).ok_or(Conflict::WorkLimit)?,
        ),
    )
    .map_err(|_| Conflict::Component)?;
    validate_component_value(schema, effective.as_ref()).map_err(|_| Conflict::Component)?;
    for claim in claims {
        preserve_enclosing_keys(
            schema,
            version,
            base.as_ref(),
            effective.as_ref(),
            &claim.path,
        )?;
    }
    Ok(candidate)
}

fn claim_cost(claims: &[&PropertyClaim]) -> Result<(usize, usize), Conflict> {
    if claims.len() > MAX_CLAIMS {
        return Err(Conflict::WorkLimit);
    }
    // Bound all caller selectors and payloads before cloning a value, even if an earlier
    // claim would fail. Reserve their complete cost, rather than estimating the net delta.
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for claim in claims {
        bytes = bytes.saturating_add(
            claim
                .path
                .logical_bytes()
                .map_err(|_| Conflict::Component)?,
        );
        if bytes > MAX_CLAIM_BYTES {
            return Err(Conflict::WorkLimit);
        }
        if let PropertyIntent::Set(value) = &claim.intent {
            let cost = value
                .cost_up_to(MAX_CLAIM_NODES - nodes, MAX_CLAIM_BYTES - bytes)
                .ok_or(Conflict::WorkLimit)?;
            nodes += cost.nodes();
            bytes += cost.logical_bytes();
        }
    }
    for (index, claim) in claims.iter().enumerate() {
        if claims[..index].iter().any(|other| {
            claim.path.root == other.path.root
                && (claim.path.steps.starts_with(&other.path.steps)
                    || other.path.steps.starts_with(&claim.path.steps))
        }) {
            return Err(Conflict::DuplicateIntent);
        }
    }
    Ok((nodes, bytes))
}

fn root_missing(value: &ComponentValue, field: &ComponentFieldSchema) -> Result<bool, Conflict> {
    match value.get_path(field.path()) {
        Ok(_) => Ok(false),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) => Ok(true),
        Err(_) => Err(Conflict::Component),
    }
}

fn require_map_parent(
    value: &ComponentValue,
    field: &ComponentFieldSchema,
    allow_absent_parent: bool,
) -> Result<(), Conflict> {
    let Some((ComponentFieldPathSegment::Field(_), ancestors)) =
        field.path().segments().split_last()
    else {
        return Err(Conflict::Component);
    };
    let parent = ComponentFieldPath::new(ancestors.iter().cloned());
    match value.get_path(&parent) {
        Ok(ComponentValue::Map(_)) => Ok(()),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) if allow_absent_parent => Ok(()),
        _ => Err(Conflict::Component),
    }
}

fn preserve_enclosing_keys(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    original: &ComponentValue,
    candidate: &ComponentValue,
    path: &ComponentPropertyPath,
) -> Result<(), Conflict> {
    for (index, step) in path.steps.iter().enumerate() {
        let ComponentPropertyStep::ListItem { by, .. } = step else {
            continue;
        };
        let item = ComponentPropertyPath::new(path.root.clone())
            .with_steps(path.steps[..=index].iter().cloned());
        let before = resolve_component_property(schema, version, original, &item)
            .map_err(|_| Conflict::Component)?;
        let after = resolve_component_property(schema, version, candidate, &item)
            .map_err(|_| Conflict::Component)?;
        let ComponentValueShape::Record { members } = before.shape else {
            return Err(Conflict::Component);
        };
        let member = members
            .iter()
            .find(|member| member.id.as_ref() == Some(by))
            .ok_or(Conflict::Component)?;
        let key = before.value.get(&member.name).ok_or(Conflict::Component)?;
        if after.value.get(&member.name) != Some(key) {
            return Err(Conflict::Component);
        }
    }
    Ok(())
}
