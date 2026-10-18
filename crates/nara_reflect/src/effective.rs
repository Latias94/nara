//! Temporary default materialization for admitted component schemas.

use std::{borrow::Cow, collections::BTreeMap};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};

use crate::{
    ComponentCodecError, ComponentFieldPathError, ComponentFieldPathSegment, ComponentSchema,
    ComponentValue, EntityReference, schema::MAX_SHAPE_DEPTH,
};

/// Bounds one expanded effective value using logical value cost rather than wire size.
///
/// The caller owns admission of the source value. These limits apply when omitted defaults
/// require materialization; a value needing no insertions is returned without scanning it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentValueMaterializationLimits {
    nodes: ItemLimit,
    bytes: ByteLimit,
    depth: DepthLimit,
}

impl ComponentValueMaterializationLimits {
    #[must_use]
    pub const fn new(nodes: ItemLimit, bytes: ByteLimit, depth: DepthLimit) -> Self {
        Self {
            nodes,
            bytes,
            depth,
        }
    }

    #[must_use]
    pub const fn nodes(self) -> ItemLimit {
        self.nodes
    }

    #[must_use]
    pub const fn bytes(self) -> ByteLimit {
        self.bytes
    }

    #[must_use]
    pub const fn depth(self) -> DepthLimit {
        self.depth
    }
}

impl Default for ComponentValueMaterializationLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(5_000_000).expect("native materialization node limit is non-zero"),
            ByteLimit::new(64 * 1024 * 1024)
                .expect("native materialization byte limit is non-zero"),
            DepthLimit::new(64).expect("native materialization depth limit is non-zero"),
        )
    }
}

/// Supplies missing declared defaults without changing the authored source.
///
/// The schema must already be admitted by its catalog owner. This only materializes defaults:
/// required-field, shape, native, capability and reference-target validation remain the caller's
/// responsibility. Explicit values, including Null, are retained. Missing Map ancestors and
/// contiguous List suffixes can be constructed, but explicit ancestors and sparse holes cannot
/// be replaced. The complete expanded cost is checked before cloning any value subtree.
pub fn effective_component_value<'value>(
    schema: &ComponentSchema,
    value: &'value ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<Cow<'value, ComponentValue>, ComponentCodecError> {
    let Some(plan) = plan_defaults(schema, value)? else {
        return Ok(Cow::Borrowed(value));
    };
    preflight_materialization(&plan, value, limits)?;
    let mut effective = value.clone();
    apply_plan(&plan, &mut effective)?;
    Ok(Cow::Owned(effective))
}

/// Supplies defaults to an owned candidate without cloning its existing value tree.
///
/// This has the same interpretation and prior-admission requirements as
/// [`effective_component_value`]. It consumes the candidate on either success or failure.
pub fn effective_component_value_owned(
    schema: &ComponentSchema,
    mut value: ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<ComponentValue, ComponentCodecError> {
    if let Some(plan) = plan_defaults(schema, &value)? {
        preflight_materialization(&plan, &value, limits)?;
        apply_plan(&plan, &mut value)?;
    }
    Ok(value)
}

#[derive(Default)]
struct DefaultPlan<'schema> {
    value: Option<&'schema ComponentValue>,
    children: BTreeMap<&'schema ComponentFieldPathSegment, Self>,
}

fn plan_defaults<'schema>(
    schema: &'schema ComponentSchema,
    value: &ComponentValue,
) -> Result<Option<DefaultPlan<'schema>>, ComponentCodecError> {
    let mut missing = Vec::new();
    for field in schema.fields() {
        let Some(default) = field.default_value() else {
            continue;
        };
        if field.path().segments().len() > MAX_SHAPE_DEPTH {
            return Err(materialization_error(
                "default path exceeds admitted schema depth",
            ));
        }
        match value.get_path(field.path()) {
            Ok(_) => {}
            Err(ComponentFieldPathError::MissingField { .. })
            | Err(ComponentFieldPathError::IndexOutOfBounds { .. }) => {
                missing.push((field.path(), default));
            }
            Err(error) => {
                return Err(ComponentCodecError::invalid_field(
                    field.path().to_string(),
                    error.to_string(),
                ));
            }
        }
    }
    if missing.is_empty() {
        return Ok(None);
    }
    missing.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut plan = DefaultPlan::default();
    for (path, default) in missing {
        let mut current = &mut plan;
        for segment in path.segments() {
            if current.value.is_some() {
                return Err(materialization_error("default paths overlap"));
            }
            current = current.children.entry(segment).or_default();
        }
        if current.value.is_some() || !current.children.is_empty() {
            return Err(materialization_error("default paths overlap"));
        }
        current.value = Some(default);
    }
    Ok(Some(plan))
}

fn preflight_materialization(
    plan: &DefaultPlan<'_>,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), ComponentCodecError> {
    let mut budget = MaterializationBudget {
        limits,
        nodes: 0,
        bytes: 0,
    };
    budget.observe_tree(value, 1)?;
    preflight_plan(plan, Some(value), 1, &mut budget)
}

fn preflight_plan(
    plan: &DefaultPlan<'_>,
    existing: Option<&ComponentValue>,
    depth: usize,
    budget: &mut MaterializationBudget,
) -> Result<(), ComponentCodecError> {
    if let Some(default) = plan.value {
        if existing.is_some() {
            return Err(materialization_error(
                "default would replace an explicit value",
            ));
        }
        return budget.observe_tree(default, depth);
    }
    let Some((first, _)) = plan.children.first_key_value() else {
        return Ok(());
    };
    if existing.is_none() {
        budget.observe_node(depth)?;
    }
    let child_depth = depth.checked_add(1).ok_or_else(depth_overflow)?;
    match first {
        ComponentFieldPathSegment::Field(_) => {
            let fields = match existing {
                Some(ComponentValue::Map(fields)) => Some(fields),
                None => None,
                Some(_) => return Err(materialization_error("default parent must be a Map")),
            };
            for (segment, child) in &plan.children {
                let ComponentFieldPathSegment::Field(name) = segment else {
                    return Err(materialization_error(
                        "default parent has incompatible locators",
                    ));
                };
                let existing = fields.and_then(|fields| fields.get(name));
                if existing.is_none() {
                    budget.observe_bytes(name.len())?;
                }
                preflight_plan(child, existing, child_depth, budget)?;
            }
        }
        ComponentFieldPathSegment::Index(_) => {
            let items = match existing {
                Some(ComponentValue::List(items)) => Some(items),
                None => None,
                Some(_) => return Err(materialization_error("default parent must be a List")),
            };
            let mut next_index = items.map_or(0, Vec::len);
            for (segment, child) in &plan.children {
                let ComponentFieldPathSegment::Index(index) = segment else {
                    return Err(materialization_error(
                        "default parent has incompatible locators",
                    ));
                };
                let index = usize::try_from(*index).map_err(|_| {
                    materialization_error("default List index is not representable")
                })?;
                let existing = items.and_then(|items| items.get(index));
                if existing.is_none() {
                    if index != next_index {
                        return Err(materialization_error(
                            "default List insertion would create a sparse hole",
                        ));
                    }
                    next_index = next_index
                        .checked_add(1)
                        .ok_or_else(|| materialization_error("default List length overflow"))?;
                }
                preflight_plan(child, existing, child_depth, budget)?;
            }
        }
    }
    Ok(())
}

fn apply_plan(
    plan: &DefaultPlan<'_>,
    value: &mut ComponentValue,
) -> Result<(), ComponentCodecError> {
    match value {
        ComponentValue::Map(fields) => {
            for (segment, child) in &plan.children {
                let ComponentFieldPathSegment::Field(name) = segment else {
                    return Err(materialization_error("default parent must be a List"));
                };
                if let Some(existing) = fields.get_mut(name) {
                    apply_plan(child, existing)?;
                } else {
                    fields.insert(name.clone(), materialize_branch(child)?);
                }
            }
        }
        ComponentValue::List(items) => {
            for (segment, child) in &plan.children {
                let ComponentFieldPathSegment::Index(index) = segment else {
                    return Err(materialization_error("default parent must be a Map"));
                };
                let index = usize::try_from(*index).map_err(|_| {
                    materialization_error("default List index is not representable")
                })?;
                if let Some(existing) = items.get_mut(index) {
                    apply_plan(child, existing)?;
                } else if index == items.len() {
                    items.push(materialize_branch(child)?);
                } else {
                    return Err(materialization_error(
                        "default List insertion would create a sparse hole",
                    ));
                }
            }
        }
        _ => return Err(materialization_error("default parent must be a container")),
    }
    Ok(())
}

fn materialize_branch(plan: &DefaultPlan<'_>) -> Result<ComponentValue, ComponentCodecError> {
    if let Some(value) = plan.value {
        return Ok(value.clone());
    }
    let mut value = match plan.children.first_key_value() {
        Some((ComponentFieldPathSegment::Field(_), _)) => ComponentValue::Map(BTreeMap::new()),
        Some((ComponentFieldPathSegment::Index(_), _)) => ComponentValue::List(Vec::new()),
        None => return Err(materialization_error("default insertion plan is empty")),
    };
    apply_plan(plan, &mut value)?;
    Ok(value)
}

struct MaterializationBudget {
    limits: ComponentValueMaterializationLimits,
    nodes: usize,
    bytes: usize,
}

enum ValueFrame<'value> {
    Value(&'value ComponentValue, usize),
    List(std::slice::Iter<'value, ComponentValue>, usize),
    Map(
        std::collections::btree_map::Iter<'value, String, ComponentValue>,
        usize,
    ),
}

impl MaterializationBudget {
    fn observe_tree(
        &mut self,
        value: &ComponentValue,
        depth: usize,
    ) -> Result<(), ComponentCodecError> {
        let mut pending = vec![ValueFrame::Value(value, depth)];
        while let Some(frame) = pending.pop() {
            match frame {
                ValueFrame::Value(value, depth) => {
                    self.observe_node(depth)?;
                    match value {
                        ComponentValue::Null => {}
                        ComponentValue::Bool(_) => self.observe_bytes(1)?,
                        ComponentValue::I64(_)
                        | ComponentValue::U64(_)
                        | ComponentValue::F64(_) => {
                            self.observe_bytes(8)?;
                        }
                        ComponentValue::String(value) => self.observe_bytes(value.len())?,
                        ComponentValue::List(values) if !values.is_empty() => {
                            pending.push(ValueFrame::List(
                                values.iter(),
                                depth.checked_add(1).ok_or_else(depth_overflow)?,
                            ));
                        }
                        ComponentValue::Map(values) if !values.is_empty() => {
                            pending.push(ValueFrame::Map(
                                values.iter(),
                                depth.checked_add(1).ok_or_else(depth_overflow)?,
                            ));
                        }
                        ComponentValue::List(_) | ComponentValue::Map(_) => {}
                        ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) => {
                            self.observe_bytes(entity.as_str().len())?;
                        }
                        ComponentValue::EntityReference(EntityReference::Persistent { entity }) => {
                            self.observe_bytes(entity.namespace.as_str().len())?;
                            self.observe_bytes(16)?;
                        }
                    }
                }
                ValueFrame::List(mut values, depth) => {
                    if let Some(value) = values.next() {
                        pending.push(ValueFrame::List(values, depth));
                        pending.push(ValueFrame::Value(value, depth));
                    }
                }
                ValueFrame::Map(mut values, depth) => {
                    if let Some((key, value)) = values.next() {
                        self.observe_bytes(key.len())?;
                        pending.push(ValueFrame::Map(values, depth));
                        pending.push(ValueFrame::Value(value, depth));
                    }
                }
            }
        }
        Ok(())
    }

    fn observe_node(&mut self, depth: usize) -> Result<(), ComponentCodecError> {
        if depth > self.limits.depth().get() {
            return Err(materialization_error(format!(
                "depth limit {} exceeded",
                self.limits.depth().get()
            )));
        }
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| materialization_error("node count overflow"))?;
        if self.nodes > self.limits.nodes().get() {
            return Err(materialization_error(format!(
                "node limit {} exceeded",
                self.limits.nodes().get()
            )));
        }
        Ok(())
    }

    fn observe_bytes(&mut self, bytes: usize) -> Result<(), ComponentCodecError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| materialization_error("logical byte count overflow"))?;
        if self.bytes > self.limits.bytes().get() {
            return Err(materialization_error(format!(
                "logical byte limit {} exceeded",
                self.limits.bytes().get()
            )));
        }
        Ok(())
    }
}

fn depth_overflow() -> ComponentCodecError {
    materialization_error("depth overflow")
}

fn materialization_error(message: impl std::fmt::Display) -> ComponentCodecError {
    ComponentCodecError::Message(format!(
        "effective component value materialization: {message}"
    ))
}
