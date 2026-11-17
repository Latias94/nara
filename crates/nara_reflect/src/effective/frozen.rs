use super::{
    DefaultPlan, DefaultSelection, EffectiveValueAdmissionError, plan_defaults_with_admission,
    preflight_materialization_cost_with_admission,
};
use crate::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPathSegment, ComponentSchema,
    ComponentValue, ComponentValueCost, ComponentValueMaterializationLimits, ComponentValueRef,
};
use std::{collections::BTreeMap, convert::Infallible, mem::size_of};

/// Closed default insertions for one privately retained, immutable source value.
///
/// This owns only selected defaults and their locators. It retains neither source nor Schema.
#[doc(hidden)]
#[derive(Debug)]
pub struct FrozenComponentDefaults {
    pub(crate) root: FrozenDefaultNode,
    cost: ComponentValueCost,
    retained_bytes: usize,
}

#[derive(Debug)]
pub(crate) enum FrozenDefaultNode {
    Value(ComponentValue),
    Map {
        children: BTreeMap<String, FrozenDefaultNode>,
        len: usize,
    },
    List {
        children: BTreeMap<u32, FrozenDefaultNode>,
        len: usize,
    },
}

impl FrozenComponentDefaults {
    /// Borrows the closed effective value over the exact original, unchanged source.
    ///
    /// The private source owner must keep that source paired with these insertions. This does
    /// not interpret defaults, inspect a receiving Schema, or retain independent ownership.
    #[must_use]
    pub fn read<'a>(&'a self, source: &'a ComponentValue) -> ComponentValueRef<'a> {
        ComponentValueRef::with_defaults(Some(source), &self.root)
    }

    /// Returns the complete effective logical cost checked before freezing.
    #[must_use]
    pub const fn cost(&self) -> ComponentValueCost {
        self.cost
    }

    /// Models this wrapper, occupied tree entries, locator names and selected default storage.
    ///
    /// Tree entries include key/value layouts and six pointer-sized bookkeeping slots. Value
    /// storage includes occupied List slots, Map entries, strings and logical reference payloads
    /// plus authored hop records. Allocator capacity and privately retained source are excluded.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

/// Freezes selected defaults using the ordinary planner and complete effective preflight.
///
/// Source, Schema and the entire override map require prior owner admission. Explicit values,
/// including Null, retain their original backing. No insertion returns `None` without scanning
/// the already admitted source. Only selected defaults and locator names are cloned.
#[doc(hidden)]
pub fn freeze_component_defaults(
    schema: &ComponentSchema,
    source: &ComponentValue,
    overrides: &BTreeMap<ComponentFieldId, ComponentValue>,
    limits: ComponentValueMaterializationLimits,
) -> Result<Option<FrozenComponentDefaults>, ComponentCodecError> {
    match freeze_component_defaults_with_admission(schema, source, overrides, limits, |_, _, _| {
        Ok::<(), Infallible>(())
    }) {
        Ok(frozen) => Ok(frozen),
        Err(EffectiveValueAdmissionError::Value(error)) => Err(error),
        Err(EffectiveValueAdmissionError::Admission(error)) => match error {},
    }
}

/// Freezes the same insertions with admission before planner, traversal and frozen allocations.
///
/// Callback deltas retain the ordinary work, logical-byte and modeled-allocation policy. Frozen
/// construction admits every occupied entry/name and the complete selected default branch before
/// cloning it. Refusal preserves the original caller error and never mutates the source.
#[doc(hidden)]
pub fn freeze_component_defaults_with_admission<E>(
    schema: &ComponentSchema,
    source: &ComponentValue,
    overrides: &BTreeMap<ComponentFieldId, ComponentValue>,
    limits: ComponentValueMaterializationLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Option<FrozenComponentDefaults>, EffectiveValueAdmissionError<E>> {
    let Some(mut plan) = plan_defaults_with_admission(
        schema,
        source,
        DefaultSelection::Overrides(overrides),
        true,
        &mut admit,
    )?
    else {
        return Ok(None);
    };
    let cost =
        preflight_materialization_cost_with_admission(&mut plan, source, limits, &mut admit)?;
    let mut retained_bytes = size_of::<FrozenComponentDefaults>();
    admit(1, 0, retained_bytes).map_err(EffectiveValueAdmissionError::Admission)?;
    let root = freeze_plan(&plan, &mut retained_bytes, &mut admit)?;
    Ok(Some(FrozenComponentDefaults {
        root,
        cost,
        retained_bytes,
    }))
}

pub(super) const fn btree_entry_bytes<K, V>() -> usize {
    size_of::<(K, V)>() + 6 * size_of::<usize>()
}

fn freeze_plan<E>(
    plan: &DefaultPlan<'_>,
    retained_bytes: &mut usize,
    admit: &mut impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<FrozenDefaultNode, EffectiveValueAdmissionError<E>> {
    if let Some(value) = plan.value {
        admit_frozen_storage(
            plan.value_cost.nodes(),
            plan.value_cost.logical_bytes(),
            plan.value_retained_bytes,
            retained_bytes,
            admit,
        )?;
        return Ok(FrozenDefaultNode::Value(value.clone()));
    }
    match plan.children.first_key_value() {
        Some((ComponentFieldPathSegment::Field(_), _)) => {
            let mut children = BTreeMap::new();
            for (segment, child) in &plan.children {
                let ComponentFieldPathSegment::Field(name) = segment else {
                    return Err(super::materialization_error(
                        "default parent has incompatible locators",
                    )
                    .into());
                };
                admit_frozen_storage(
                    1,
                    name.len(),
                    btree_entry_bytes::<String, FrozenDefaultNode>().saturating_add(name.len()),
                    retained_bytes,
                    admit,
                )?;
                let child = freeze_plan(child, retained_bytes, admit)?;
                children.insert(name.clone(), child);
            }
            Ok(FrozenDefaultNode::Map {
                children,
                len: plan.len,
            })
        }
        Some((ComponentFieldPathSegment::Index(_), _)) => {
            let mut children = BTreeMap::new();
            for (segment, child) in &plan.children {
                let ComponentFieldPathSegment::Index(index) = segment else {
                    return Err(super::materialization_error(
                        "default parent has incompatible locators",
                    )
                    .into());
                };
                admit_frozen_storage(
                    1,
                    0,
                    btree_entry_bytes::<u32, FrozenDefaultNode>(),
                    retained_bytes,
                    admit,
                )?;
                let child = freeze_plan(child, retained_bytes, admit)?;
                children.insert(*index, child);
            }
            Ok(FrozenDefaultNode::List {
                children,
                len: plan.len,
            })
        }
        None => Err(super::materialization_error("default insertion plan is empty").into()),
    }
}

fn admit_frozen_storage<E>(
    nodes: usize,
    bytes: usize,
    allocated: usize,
    retained_bytes: &mut usize,
    admit: &mut impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<(), EffectiveValueAdmissionError<E>> {
    let retained = retained_bytes
        .checked_add(allocated)
        .ok_or_else(|| super::materialization_error("frozen default storage overflow"))?;
    admit(nodes, bytes, allocated).map_err(EffectiveValueAdmissionError::Admission)?;
    *retained_bytes = retained;
    Ok(())
}

#[cfg(test)]
mod tests;
