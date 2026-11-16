//! Bounded component value helpers shared by property and staged value compilation.

use nara_reflect::{
    ComponentCodecError, ComponentSchema, ComponentShapeError, ComponentShapeErrorKind,
    ComponentValue, ComponentValueCost, ComponentValueMaterializationLimits,
    validate_component_value_with_limits,
};

use super::SceneCompilationFailureKind;

pub(super) fn reserve_cost(
    limits: ComponentValueMaterializationLimits,
    nodes: usize,
    bytes: usize,
) -> Result<ComponentValueMaterializationLimits, SceneCompilationFailureKind> {
    if nodes > limits.nodes() || bytes > limits.bytes() {
        return Err(SceneCompilationFailureKind::WorkLimit);
    }
    Ok(limits.remaining(nodes, bytes))
}

pub(super) fn check_value_cost(
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValueCost, SceneCompilationFailureKind> {
    value
        .cost_up_to(limits.nodes(), limits.bytes())
        .ok_or(SceneCompilationFailureKind::WorkLimit)
}

pub(super) fn validate_value(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValueCost, SceneCompilationFailureKind> {
    let cost = check_value_cost(value, limits)?;
    validate_component_value_with_limits(schema, value, limits).map_err(codec_failure)?;
    Ok(cost)
}

pub(super) fn codec_failure(error: ComponentCodecError) -> SceneCompilationFailureKind {
    match error {
        ComponentCodecError::MaterializationNodeLimit { .. }
        | ComponentCodecError::MaterializationByteLimit { .. }
        | ComponentCodecError::MaterializationDepthLimit { .. } => {
            SceneCompilationFailureKind::WorkLimit
        }
        _ => SceneCompilationFailureKind::Component,
    }
}

pub(super) fn shape_failure(error: ComponentShapeError) -> SceneCompilationFailureKind {
    match error.kind {
        ComponentShapeErrorKind::NodeLimit
        | ComponentShapeErrorKind::ByteLimit
        | ComponentShapeErrorKind::DepthLimit => SceneCompilationFailureKind::WorkLimit,
        _ => SceneCompilationFailureKind::Component,
    }
}
