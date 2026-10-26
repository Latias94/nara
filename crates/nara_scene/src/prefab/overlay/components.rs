//! Bounded component value helpers shared by property and staged value compilation.

use std::borrow::Cow;

use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    ComponentSchema, ComponentValue, ComponentValueMaterializationLimits,
    effective_component_value, validate_component_value_with_limits,
};

use super::PrefabOverlayConflict;

pub(super) fn reserve_cost(
    limits: ComponentValueMaterializationLimits,
    nodes: usize,
    bytes: usize,
) -> Result<ComponentValueMaterializationLimits, PrefabOverlayConflict> {
    let nodes = limits
        .nodes()
        .get()
        .checked_sub(nodes)
        .and_then(ItemLimit::new)
        .ok_or(PrefabOverlayConflict::WorkLimit)?;
    let bytes = limits
        .bytes()
        .get()
        .checked_sub(bytes)
        .and_then(ByteLimit::new)
        .ok_or(PrefabOverlayConflict::WorkLimit)?;
    Ok(ComponentValueMaterializationLimits::new(
        nodes,
        bytes,
        limits.depth(),
    ))
}

pub(super) fn effective_value<'value>(
    schema: &ComponentSchema,
    value: &'value ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<Cow<'value, ComponentValue>, PrefabOverlayConflict> {
    check_value_cost(value, limits)?;
    let effective = effective_component_value(schema, value, limits)
        .map_err(|_| PrefabOverlayConflict::WorkLimit)?;
    validate_value(schema, &effective, limits)?;
    Ok(effective)
}

pub(super) fn check_value_cost(
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), PrefabOverlayConflict> {
    value
        .cost_up_to(limits.nodes().get(), limits.bytes().get())
        .ok_or(PrefabOverlayConflict::WorkLimit)?;
    Ok(())
}

pub(super) fn validate_value(
    schema: &ComponentSchema,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), PrefabOverlayConflict> {
    check_value_cost(value, limits)?;
    validate_component_value_with_limits(schema, value, limits)
        .map_err(|_| PrefabOverlayConflict::Component)
}
