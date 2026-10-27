//! Schema-relative property pins evaluated against one immutable inherited value.

use std::collections::BTreeMap;

use nara_reflect::{
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema,
    ComponentPropertyError, ComponentPropertyPath, ComponentPropertyStep, ComponentSchema,
    ComponentSchemaVersion, ComponentValue, ComponentValueMaterializationLimits,
    ComponentValueShape, effective_component_value_owned, resolve_component_property_with_limits,
};

use crate::SceneEntityId;

use super::components::{
    check_value_cost, codec_failure, effective_value, reserve_cost, shape_failure, validate_value,
};
use super::references::{bind_property_references, binding_growth};
use super::{
    PrefabEntityTarget, PrefabExpansionFailureKind, PrefabPropertyClaim, PrefabPropertyIntent,
};

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
) -> Result<ComponentValue, PrefabExpansionFailureKind> {
    if version != schema.version() {
        return Err(PrefabExpansionFailureKind::Component);
    }
    let (mut nodes, mut bytes) = claim_cost(claims, limits)?;
    let fields = claims
        .iter()
        .map(|claim| {
            if schema.field_tombstones().contains(&claim.path.root) {
                return Err(PrefabExpansionFailureKind::Component);
            }
            schema
                .fields()
                .iter()
                .find(|field| field.id() == &claim.path.root)
                .ok_or(PrefabExpansionFailureKind::Component)
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
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
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
                    return Err(PrefabExpansionFailureKind::Component);
                }
                if field.default_value().is_none() && !claim.bindings.is_empty() {
                    return Err(PrefabExpansionFailureKind::Reference);
                }
                require_map_parent(original, field, true)?;
            }
            PrefabPropertyIntent::Set(value) => {
                if root_missing(original, field)? {
                    require_map_parent(original, field, false)?;
                    if claim.path.steps.is_empty() {
                        if field.required && field.default_value().is_none() {
                            return Err(PrefabExpansionFailureKind::Component);
                        }
                        field.shape().validate_value(value).map_err(shape_failure)?;
                        continue;
                    }
                    if field.default_value().is_none() {
                        return Err(PrefabExpansionFailureKind::Component);
                    }
                }
                let selected = resolve_component_property_with_limits(
                    schema,
                    version,
                    &base,
                    &claim.path,
                    base_limits,
                )
                .map_err(property_failure)?;
                selected
                    .shape
                    .validate_value(value)
                    .map_err(shape_failure)?;
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
                        .map_err(|_| PrefabExpansionFailureKind::Component)?;
                }
            }
            PrefabPropertyIntent::Set(value) => {
                let shape = if claim.path.steps.is_empty() {
                    field.shape()
                } else {
                    resolve_component_property_with_limits(
                        schema,
                        version,
                        &base,
                        &claim.path,
                        base_limits,
                    )
                    .map_err(property_failure)?
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
                            .ok_or(PrefabExpansionFailureKind::Component)?
                    };
                    candidate
                        .set_path(field.path(), initial.clone())
                        .map_err(|_| PrefabExpansionFailureKind::Component)?;
                    if claim.path.steps.is_empty() {
                        continue;
                    }
                }
                // This candidate is isolated from the source; consume it on failure instead of
                // allocating another whole-component rollback copy for every property claim.
                // Complete validation and enclosing-key preservation follow all replacements.
                candidate = claim
                    .path
                    .replace_in_owned_value_with_limits(schema, version, candidate, bound, limits)
                    .map_err(property_failure)?;
            }
        }
    }
    let effective = effective_value(schema, &candidate, limits)?;
    for claim in claims {
        preserve_enclosing_keys(schema, version, &base, &effective, &claim.path, limits)?;
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
                .map_err(codec_failure)?;
            candidate
                .set_path(field.path(), bound)
                .map_err(|_| PrefabExpansionFailureKind::Component)?;
        }
    }
    validate_value(schema, &candidate, limits)?;
    Ok(candidate)
}

fn property_failure(error: ComponentPropertyError) -> PrefabExpansionFailureKind {
    match error {
        ComponentPropertyError::InvalidSchema(_) => PrefabExpansionFailureKind::OwnerAdmission,
        ComponentPropertyError::ReadBudgetExceeded
        | ComponentPropertyError::PathDepthLimit
        | ComponentPropertyError::PathByteLimit
        | ComponentPropertyError::StorageIndexOverflow => PrefabExpansionFailureKind::WorkLimit,
        ComponentPropertyError::ValueBudget(error)
        | ComponentPropertyError::InvalidReplacementShape(error) => shape_failure(error),
        ComponentPropertyError::InvalidValue(error)
        | ComponentPropertyError::InvalidReplacement(error) => codec_failure(error),
        _ => PrefabExpansionFailureKind::Component,
    }
}

/// Declaration conflicts survive even when a later complete component discards the values.
/// The caller admits this pointer index before allocation.
pub(super) fn validate_claim_paths(
    claims: &[PrefabPropertyClaim],
) -> Result<(), PrefabExpansionFailureKind> {
    let mut paths = claims.iter().map(|claim| &claim.path).collect::<Vec<_>>();
    paths.sort_unstable();
    if paths
        .windows(2)
        .any(|pair| pair[0].root == pair[1].root && pair[1].steps.starts_with(&pair[0].steps))
    {
        return Err(PrefabExpansionFailureKind::DuplicateIntent);
    }
    Ok(())
}

fn claim_cost(
    claims: &[PrefabPropertyClaim],
    limits: ComponentValueMaterializationLimits,
) -> Result<(usize, usize), PrefabExpansionFailureKind> {
    if claims.len() > limits.nodes().get() {
        return Err(PrefabExpansionFailureKind::WorkLimit);
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for claim in claims {
        if claim.path.steps.len() > limits.depth().get() {
            return Err(PrefabExpansionFailureKind::WorkLimit);
        }
        bytes = bytes.saturating_add(claim.path.logical_bytes().map_err(property_failure)?);
        if bytes > limits.bytes().get() {
            return Err(PrefabExpansionFailureKind::WorkLimit);
        }
        if let PrefabPropertyIntent::Set(value) = &claim.intent {
            check_value_cost(value, limits)?;
            let cost = value
                .cost_up_to(limits.nodes().get() - nodes, limits.bytes().get() - bytes)
                .ok_or(PrefabExpansionFailureKind::WorkLimit)?;
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
) -> Result<bool, PrefabExpansionFailureKind> {
    match value.get_path(field.path()) {
        Ok(_) => Ok(false),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) => Ok(true),
        Err(_) => Err(PrefabExpansionFailureKind::Component),
    }
}

fn require_map_parent(
    value: &ComponentValue,
    field: &ComponentFieldSchema,
    allow_absent_parent: bool,
) -> Result<(), PrefabExpansionFailureKind> {
    let Some((ComponentFieldPathSegment::Field(_), ancestors)) =
        field.path().segments().split_last()
    else {
        return Err(PrefabExpansionFailureKind::Component);
    };
    let parent = ComponentFieldPath::new(ancestors.iter().cloned());
    match value.get_path(&parent) {
        Ok(ComponentValue::Map(_)) => Ok(()),
        Err(
            ComponentFieldPathError::MissingField { .. }
            | ComponentFieldPathError::IndexOutOfBounds { .. },
        ) if allow_absent_parent => Ok(()),
        _ => Err(PrefabExpansionFailureKind::Component),
    }
}

fn preserve_enclosing_keys(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    original: &ComponentValue,
    candidate: &ComponentValue,
    path: &ComponentPropertyPath,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), PrefabExpansionFailureKind> {
    for (index, step) in path.steps.iter().enumerate() {
        let ComponentPropertyStep::ListItem { by, .. } = step else {
            continue;
        };
        let item = ComponentPropertyPath::new(path.root.clone())
            .with_steps(path.steps[..=index].iter().cloned());
        let before =
            resolve_component_property_with_limits(schema, version, original, &item, limits)
                .map_err(property_failure)?;
        let after =
            resolve_component_property_with_limits(schema, version, candidate, &item, limits)
                .map_err(property_failure)?;
        let ComponentValueShape::Record { members } = before.shape else {
            return Err(PrefabExpansionFailureKind::Component);
        };
        let member = members
            .iter()
            .find(|member| member.id.as_ref() == Some(by))
            .ok_or(PrefabExpansionFailureKind::Component)?;
        let key = before
            .value
            .get(&member.name)
            .ok_or(PrefabExpansionFailureKind::Component)?;
        if after.value.get(&member.name) != Some(key) {
            return Err(PrefabExpansionFailureKind::Component);
        }
    }
    Ok(())
}

#[cfg(test)]
mod failure_mapping_tests {
    use super::*;
    use nara_core::{ByteLimit, DepthLimit, ItemLimit};
    use nara_reflect::{
        ComponentCodecError, ComponentFieldId, ComponentShapeError, ComponentShapeErrorKind,
        ComponentTypeId, ComponentValueKind,
    };

    #[test]
    fn property_budget_and_authority_errors_remain_distinct_from_source_intent_errors() {
        for kind in [
            ComponentShapeErrorKind::NodeLimit,
            ComponentShapeErrorKind::ByteLimit,
            ComponentShapeErrorKind::DepthLimit,
        ] {
            let error = ComponentShapeError {
                path: ComponentFieldPath::default(),
                kind,
            };
            assert_eq!(
                property_failure(ComponentPropertyError::ValueBudget(error.clone())),
                PrefabExpansionFailureKind::WorkLimit
            );
            assert_eq!(
                property_failure(ComponentPropertyError::InvalidSchema(error)),
                PrefabExpansionFailureKind::OwnerAdmission
            );
        }
        for error in [
            ComponentPropertyError::ReadBudgetExceeded,
            ComponentPropertyError::PathDepthLimit,
            ComponentPropertyError::PathByteLimit,
            ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationNodeLimit {
                observed: 3,
                maximum: 2,
            }),
            ComponentPropertyError::InvalidReplacement(
                ComponentCodecError::MaterializationByteLimit {
                    observed: 3,
                    maximum: 2,
                },
            ),
            ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2,
            }),
        ] {
            assert_eq!(
                property_failure(error),
                PrefabExpansionFailureKind::WorkLimit
            );
        }
        assert_eq!(
            property_failure(ComponentPropertyError::UnknownRoot(ComponentFieldId::new(
                "missing"
            ))),
            PrefabExpansionFailureKind::Component
        );
    }

    #[test]
    fn effective_component_helper_preserves_semantic_and_materialization_budget_failures() {
        let schema = ComponentSchema::new(
            ComponentTypeId::new("test.PropertyDefaultFailure"),
            "Property default failure",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("x"),
            "X",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueShape::list(ComponentValueKind::U64.into()),
            ComponentValue::List(vec![ComponentValue::U64(0)]),
        )]);
        let limits = ComponentValueMaterializationLimits::new(
            ItemLimit::new(2).unwrap(),
            ByteLimit::new(128).unwrap(),
            DepthLimit::new(64).unwrap(),
        );
        assert_eq!(
            effective_value(
                &schema,
                &ComponentValue::map([] as [(&str, ComponentValue); 0]),
                limits
            ),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
        assert_eq!(
            effective_value(&schema, &ComponentValue::Bool(false), limits),
            Err(PrefabExpansionFailureKind::Component)
        );
    }
}
