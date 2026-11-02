//! Bounded component value helpers shared by property and staged value compilation.

use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    ComponentCodecError, ComponentSchema, ComponentShapeError, ComponentShapeErrorKind,
    ComponentValue, ComponentValueMaterializationLimits, validate_component_value_with_limits,
};

use super::SceneCompilationFailureKind;

pub(super) fn reserve_cost(
    limits: ComponentValueMaterializationLimits,
    nodes: usize,
    bytes: usize,
) -> Result<ComponentValueMaterializationLimits, SceneCompilationFailureKind> {
    let nodes = limits
        .nodes()
        .get()
        .checked_sub(nodes)
        .and_then(ItemLimit::new)
        .ok_or(SceneCompilationFailureKind::WorkLimit)?;
    let bytes = limits
        .bytes()
        .get()
        .checked_sub(bytes)
        .and_then(ByteLimit::new)
        .ok_or(SceneCompilationFailureKind::WorkLimit)?;
    Ok(ComponentValueMaterializationLimits::new(
        nodes,
        bytes,
        limits.depth(),
    ))
}

pub(super) fn check_value_cost(
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), SceneCompilationFailureKind> {
    value
        .cost_up_to(limits.nodes().get(), limits.bytes().get())
        .ok_or(SceneCompilationFailureKind::WorkLimit)?;
    Ok(())
}

pub(super) fn validate_value(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), SceneCompilationFailureKind> {
    check_value_cost(value, limits)?;
    validate_component_value_with_limits(schema, value, limits).map_err(codec_failure)
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
