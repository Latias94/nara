//! Temporary default materialization for admitted component schemas.

use std::{borrow::Cow, collections::BTreeMap, convert::Infallible, mem::size_of};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};

use crate::{
    ComponentCodecError, ComponentFieldPathError, ComponentFieldPathSegment, ComponentSchema,
    ComponentValue, ComponentValueCost, EntityReference, schema::MAX_SHAPE_DEPTH,
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

/// Separates interpretation failure from the owning caller's preparation allowance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveValueAdmissionError<E> {
    Value(ComponentCodecError),
    Admission(E),
}

impl<E> From<ComponentCodecError> for EffectiveValueAdmissionError<E> {
    fn from(error: ComponentCodecError) -> Self {
        Self::Value(error)
    }
}

/// Supplies defaults through the same plan and materializer, admitting temporary ownership first.
///
/// The schema and raw source must already be admitted. Callback deltas describe visited work,
/// logical bytes and modeled allocation, excluding allocator capacity and shared schema storage.
/// No insertion retains the borrowed source. A denied callback returns its exact error before
/// the next lookup scratch, plan growth or complete effective-value copy.
pub fn effective_component_value_with_admission<'value, E>(
    schema: &ComponentSchema,
    value: &'value ComponentValue,
    limits: ComponentValueMaterializationLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Cow<'value, ComponentValue>, EffectiveValueAdmissionError<E>> {
    let Some(plan) = plan_defaults_with_admission(schema, value, true, &mut admit)? else {
        return Ok(Cow::Borrowed(value));
    };
    let mut refusal = None;
    let cost = {
        let mut callback = |nodes, bytes, allocated| {
            admit(nodes, bytes, allocated).map_err(|error| {
                refusal = Some(error);
                // Allocation-free carrier; the exact caller error is returned below.
                ComponentCodecError::MaterializationNodeLimit {
                    observed: 0,
                    maximum: 0,
                }
            })
        };
        preflight_materialization_cost(&plan, value, limits, Some(&mut callback))
    };
    if let Some(error) = refusal {
        return Err(EffectiveValueAdmissionError::Admission(error));
    }
    let cost = cost?;
    admit(
        0,
        0,
        cost.nodes()
            .saturating_mul(96)
            .saturating_add(cost.logical_bytes()),
    )
    .map_err(EffectiveValueAdmissionError::Admission)?;
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

/// Performs one effective-value admission against raw residual ceilings and returns the exact
/// retained logical cost. This is the composition seam for callers that admit many component
/// values under one aggregate allowance. The caller must admit any independently retained source
/// clone before calling this function.
pub fn effective_component_value_with_budget<'value>(
    schema: &ComponentSchema,
    value: &'value ComponentValue,
    remaining_nodes: usize,
    remaining_bytes: usize,
    maximum_depth: DepthLimit,
) -> Result<(Cow<'value, ComponentValue>, ComponentValueCost), ComponentCodecError> {
    let mut budget = MaterializationBudget {
        maximum_nodes: remaining_nodes,
        maximum_bytes: remaining_bytes,
        maximum_depth,
        nodes: 0,
        bytes: 0,
        admission: None,
    };
    budget.observe_tree(value, 1)?;
    let plan = plan_defaults(schema, value)?;
    if let Some(plan) = &plan {
        preflight_plan(plan, Some(value), 1, &mut budget)?;
    }
    let cost = ComponentValueCost::from_parts(budget.nodes, budget.bytes);
    match plan {
        Some(plan) => {
            let mut effective = value.clone();
            apply_plan(&plan, &mut effective)?;
            Ok((Cow::Owned(effective), cost))
        }
        None => Ok((Cow::Borrowed(value), cost)),
    }
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
    match plan_defaults_with_admission(
        schema,
        value,
        false,
        &mut |_, _, _| Ok::<(), Infallible>(()),
    ) {
        Ok(plan) => Ok(plan),
        Err(EffectiveValueAdmissionError::Value(error)) => Err(error),
        Err(EffectiveValueAdmissionError::Admission(error)) => match error {},
    }
}

fn plan_defaults_with_admission<'schema, E>(
    schema: &'schema ComponentSchema,
    value: &ComponentValue,
    account: bool,
    admit: &mut impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Option<DefaultPlan<'schema>>, EffectiveValueAdmissionError<E>> {
    let mut missing = Vec::new();
    for field in schema.fields() {
        if account {
            admit(1, 0, 0).map_err(EffectiveValueAdmissionError::Admission)?;
        }
        let Some(default) = field.default_value() else {
            continue;
        };
        if field.path().segments().len() > MAX_SHAPE_DEPTH {
            return Err(materialization_error("default path exceeds admitted schema depth").into());
        }
        // get_path owns prefix paths, including on a missing-field return. Admit those
        // bounded copies before lookup rather than constructing an error to measure it.
        if account {
            let path_bytes = field
                .path()
                .segments()
                .iter()
                .fold(0usize, |bytes, segment| {
                    bytes
                        .saturating_add(size_of::<ComponentFieldPathSegment>())
                        .saturating_add(match segment {
                            ComponentFieldPathSegment::Field(name) => name.len(),
                            ComponentFieldPathSegment::Index(_) => 0,
                        })
                });
            admit(
                field.path().segments().len(),
                path_bytes,
                path_bytes.saturating_mul(field.path().segments().len().saturating_add(3)),
            )
            .map_err(EffectiveValueAdmissionError::Admission)?;
        }
        match value.get_path(field.path()) {
            Ok(_) => {}
            Err(ComponentFieldPathError::MissingField { .. })
            | Err(ComponentFieldPathError::IndexOutOfBounds { .. }) => {
                if account && missing.len() == missing.capacity() {
                    let capacity = missing
                        .len()
                        .saturating_add(1)
                        .max(missing.capacity().saturating_mul(2))
                        .max(4);
                    admit(
                        0,
                        0,
                        capacity
                            .saturating_mul(
                                size_of::<(&crate::ComponentFieldPath, &ComponentValue)>(),
                            ),
                    )
                    .map_err(EffectiveValueAdmissionError::Admission)?;
                    missing.reserve_exact(capacity - missing.len());
                }
                missing.push((field.path(), default));
            }
            Err(error) => {
                return Err(ComponentCodecError::invalid_field(
                    field.path().to_string(),
                    error.to_string(),
                )
                .into());
            }
        }
    }
    if missing.is_empty() {
        return Ok(None);
    }
    missing.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    let mut plan = DefaultPlan::default();
    for (path, default) in missing {
        let mut current = &mut plan;
        for segment in path.segments() {
            if current.value.is_some() {
                return Err(materialization_error("default paths overlap").into());
            }
            if account && !current.children.contains_key(segment) {
                admit(
                    1,
                    0,
                    size_of::<(&ComponentFieldPathSegment, DefaultPlan<'_>)>()
                        .saturating_add(6 * size_of::<usize>()),
                )
                .map_err(EffectiveValueAdmissionError::Admission)?;
            }
            current = current.children.entry(segment).or_default();
        }
        if current.value.is_some() || !current.children.is_empty() {
            return Err(materialization_error("default paths overlap").into());
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
    preflight_materialization_cost(plan, value, limits, None).map(|_| ())
}

fn preflight_materialization_cost<'admit>(
    plan: &DefaultPlan<'_>,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
    admission: Option<&'admit mut MaterializationAdmission<'admit>>,
) -> Result<ComponentValueCost, ComponentCodecError> {
    let mut budget = MaterializationBudget {
        maximum_nodes: limits.nodes().get(),
        maximum_bytes: limits.bytes().get(),
        maximum_depth: limits.depth(),
        nodes: 0,
        bytes: 0,
        admission,
    };
    budget.observe_tree(value, 1)?;
    preflight_plan(plan, Some(value), 1, &mut budget)?;
    Ok(ComponentValueCost::from_parts(budget.nodes, budget.bytes))
}

fn preflight_plan(
    plan: &DefaultPlan<'_>,
    existing: Option<&ComponentValue>,
    depth: usize,
    budget: &mut MaterializationBudget<'_>,
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
    let child_depth = depth
        .checked_add(1)
        .ok_or_else(|| depth_overflow(budget.maximum_depth.get()))?;
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

type MaterializationAdmission<'a> =
    dyn FnMut(usize, usize, usize) -> Result<(), ComponentCodecError> + 'a;

struct MaterializationBudget<'a> {
    maximum_nodes: usize,
    maximum_bytes: usize,
    maximum_depth: DepthLimit,
    nodes: usize,
    bytes: usize,
    admission: Option<&'a mut MaterializationAdmission<'a>>,
}

pub(crate) fn validate_value_materialization_budget(
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), ComponentCodecError> {
    MaterializationBudget {
        maximum_nodes: limits.nodes().get(),
        maximum_bytes: limits.bytes().get(),
        maximum_depth: limits.depth(),
        nodes: 0,
        bytes: 0,
        admission: None,
    }
    .observe_tree(value, 1)
}

enum ValueFrame<'value> {
    Value(&'value ComponentValue, usize),
    List(std::slice::Iter<'value, ComponentValue>, usize),
    Map(
        std::collections::btree_map::Iter<'value, String, ComponentValue>,
        usize,
    ),
}

impl MaterializationBudget<'_> {
    fn push_frame<'value>(
        &mut self,
        pending: &mut Vec<ValueFrame<'value>>,
        frame: ValueFrame<'value>,
    ) -> Result<(), ComponentCodecError> {
        if let Some(admit) = self.admission.as_deref_mut()
            && pending.len() == pending.capacity()
        {
            let capacity = pending
                .len()
                .saturating_add(1)
                .max(pending.capacity().saturating_mul(2))
                .max(4);
            admit(0, 0, capacity.saturating_mul(size_of::<ValueFrame<'_>>()))?;
            pending.reserve_exact(capacity - pending.len());
        }
        pending.push(frame);
        Ok(())
    }

    fn observe_tree(
        &mut self,
        value: &ComponentValue,
        depth: usize,
    ) -> Result<(), ComponentCodecError> {
        let mut pending = Vec::new();
        self.push_frame(&mut pending, ValueFrame::Value(value, depth))?;
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
                            self.push_frame(
                                &mut pending,
                                ValueFrame::List(
                                    values.iter(),
                                    depth
                                        .checked_add(1)
                                        .ok_or_else(|| depth_overflow(self.maximum_depth.get()))?,
                                ),
                            )?;
                        }
                        ComponentValue::Map(values) if !values.is_empty() => {
                            self.push_frame(
                                &mut pending,
                                ValueFrame::Map(
                                    values.iter(),
                                    depth
                                        .checked_add(1)
                                        .ok_or_else(|| depth_overflow(self.maximum_depth.get()))?,
                                ),
                            )?;
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
                        self.push_frame(&mut pending, ValueFrame::List(values, depth))?;
                        self.push_frame(&mut pending, ValueFrame::Value(value, depth))?;
                    }
                }
                ValueFrame::Map(mut values, depth) => {
                    if let Some((key, value)) = values.next() {
                        self.observe_bytes(key.len())?;
                        self.push_frame(&mut pending, ValueFrame::Map(values, depth))?;
                        self.push_frame(&mut pending, ValueFrame::Value(value, depth))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn observe_node(&mut self, depth: usize) -> Result<(), ComponentCodecError> {
        if let Some(admit) = self.admission.as_deref_mut() {
            admit(1, 0, 0)?;
        }
        if depth > self.maximum_depth.get() {
            return Err(ComponentCodecError::MaterializationDepthLimit {
                observed: depth,
                maximum: self.maximum_depth.get(),
            });
        }
        self.nodes =
            self.nodes
                .checked_add(1)
                .ok_or(ComponentCodecError::MaterializationNodeLimit {
                    observed: usize::MAX,
                    maximum: self.maximum_nodes,
                })?;
        if self.nodes > self.maximum_nodes {
            return Err(ComponentCodecError::MaterializationNodeLimit {
                observed: self.nodes,
                maximum: self.maximum_nodes,
            });
        }
        Ok(())
    }

    fn observe_bytes(&mut self, bytes: usize) -> Result<(), ComponentCodecError> {
        if let Some(admit) = self.admission.as_deref_mut() {
            admit(0, bytes, 0)?;
        }
        self.bytes =
            self.bytes
                .checked_add(bytes)
                .ok_or(ComponentCodecError::MaterializationByteLimit {
                    observed: usize::MAX,
                    maximum: self.maximum_bytes,
                })?;
        if self.bytes > self.maximum_bytes {
            return Err(ComponentCodecError::MaterializationByteLimit {
                observed: self.bytes,
                maximum: self.maximum_bytes,
            });
        }
        Ok(())
    }
}

fn depth_overflow(maximum: usize) -> ComponentCodecError {
    ComponentCodecError::MaterializationDepthLimit {
        observed: usize::MAX,
        maximum,
    }
}

fn materialization_error(message: impl std::fmt::Display) -> ComponentCodecError {
    ComponentCodecError::Message(format!(
        "effective component value materialization: {message}"
    ))
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use crate::{
        ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaVersion,
        ComponentTypeId, ComponentValueKind,
    };

    fn defaulted(value: ComponentValue) -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new("test.Data"),
            "Data",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::String,
            value,
        )])
    }

    #[test]
    fn aggregate_zero_bytes_accepts_empty_values_but_refuses_default_growth() {
        let schema = ComponentSchema::new(
            ComponentTypeId::new("test.Empty"),
            "Empty",
            ComponentSchemaVersion::ONE,
        );
        let value = ComponentValue::Null;
        let (effective, cost) = effective_component_value_with_budget(
            &schema,
            &value,
            1,
            0,
            DepthLimit::new(2).unwrap(),
        )
        .unwrap();
        assert!(matches!(effective, Cow::Borrowed(_)));
        assert_eq!(cost, value.cost());
        let schema = defaulted(ComponentValue::String(String::new()));
        assert!(matches!(
            effective_component_value_with_budget(
                &schema,
                &ComponentValue::map([] as [(String, ComponentValue); 0]),
                2,
                0,
                DepthLimit::new(2).unwrap()
            ),
            Err(ComponentCodecError::MaterializationByteLimit { .. })
        ));
    }

    #[test]
    fn aggregate_default_cost_is_exact_and_depth_is_checked_without_insertions() {
        let schema = defaulted(ComponentValue::String("yes".into()));
        let value = ComponentValue::map([] as [(String, ComponentValue); 0]);
        let (effective, cost) = effective_component_value_with_budget(
            &schema,
            &value,
            2,
            8,
            DepthLimit::new(2).unwrap(),
        )
        .unwrap();
        assert!(matches!(effective, Cow::Owned(_)));
        assert_eq!(cost, effective.cost());
        assert_eq!(cost.nodes(), 2);
        assert_eq!(cost.logical_bytes(), 8);
        assert!(matches!(
            effective_component_value_with_budget(
                &schema,
                &value,
                2,
                7,
                DepthLimit::new(2).unwrap(),
            ),
            Err(ComponentCodecError::MaterializationByteLimit { .. })
        ));
        assert!(matches!(
            effective_component_value_with_budget(&schema, &effective, 2, 8, DepthLimit::ONE,),
            Err(ComponentCodecError::MaterializationDepthLimit { .. })
        ));
    }

    #[test]
    fn admission_refuses_large_default_before_effective_tree_copy() {
        let schema = defaulted(ComponentValue::String("x".repeat(64 * 1024)));
        let source = ComponentValue::map([] as [(String, ComponentValue); 0]);
        let mut observed_copy = false;
        let result = effective_component_value_with_admission(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
            |_, _, allocated| {
                if allocated >= 64 * 1024 {
                    observed_copy = true;
                    Err("copy refused")
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            result,
            Err(EffectiveValueAdmissionError::Admission("copy refused"))
        );
        assert!(observed_copy);
        assert!(source.get("value").is_none());
    }

    #[test]
    fn admission_keeps_no_insertion_borrowed_and_preserves_interpretation() {
        let schema = defaulted(ComponentValue::String("fallback".into()));
        let source = ComponentValue::map([("value", ComponentValue::String("explicit".into()))]);
        let effective = effective_component_value_with_admission(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
            |_, _, _| Ok::<(), Infallible>(()),
        )
        .unwrap();
        assert!(matches!(effective, Cow::Borrowed(_)));
        let source = ComponentValue::map([] as [(String, ComponentValue); 0]);
        let expected = effective_component_value(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap();
        let actual = effective_component_value_with_admission(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
            |_, _, _| Ok::<(), Infallible>(()),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn admission_refusal_precedes_default_path_error_construction() {
        let schema = defaulted(ComponentValue::String("fallback".into()));
        let source = ComponentValue::Null;
        let result = effective_component_value_with_admission(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
            |_, _, _| Err("lookup refused"),
        );
        assert_eq!(
            result,
            Err(EffectiveValueAdmissionError::Admission("lookup refused"))
        );
        let result = effective_component_value_with_admission(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
            |_, _, _| Ok::<(), Infallible>(()),
        );
        assert_eq!(
            result.unwrap_err(),
            EffectiveValueAdmissionError::Value(
                effective_component_value(
                    &schema,
                    &source,
                    ComponentValueMaterializationLimits::default(),
                )
                .unwrap_err(),
            )
        );
    }

    #[test]
    fn admission_preserves_borrowed_refusal_at_every_planning_and_preflight_boundary() {
        #[derive(Debug)]
        struct Refusal<'a>(&'a String);

        let schema = defaulted(ComponentValue::String("default".into()));
        let source = ComponentValue::map([(
            "explicit",
            ComponentValue::List(vec![
                ComponentValue::String("retained".into()),
                ComponentValue::Bool(true),
            ]),
        )]);
        let mut observations = Vec::new();
        effective_component_value_with_admission(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
            |nodes, bytes, allocated| {
                observations.push((nodes, bytes, allocated));
                Ok::<(), Infallible>(())
            },
        )
        .unwrap();
        assert!(
            observations
                .iter()
                .any(|(nodes, _, allocated)| *nodes == 1 && *allocated == 0)
        );
        assert!(
            observations
                .iter()
                .any(|(_, bytes, allocated)| *bytes > 0 && *allocated == 0)
        );
        assert!(
            observations
                .iter()
                .any(|(nodes, bytes, allocated)| *nodes == 0 && *bytes == 0 && *allocated > 0)
        );
        let marker = "borrowed refusal".to_owned();
        for denied in 0..observations.len() {
            let mut calls = 0;
            let result = effective_component_value_with_admission(
                &schema,
                &source,
                ComponentValueMaterializationLimits::default(),
                |_, _, _| {
                    let current = calls;
                    calls += 1;
                    if current == denied {
                        Err(Refusal(&marker))
                    } else {
                        Ok(())
                    }
                },
            );
            let Err(EffectiveValueAdmissionError::Admission(Refusal(actual))) = result else {
                panic!("boundary {denied} must retain the exact caller refusal");
            };
            assert!(std::ptr::eq(actual, &marker));
            assert_eq!(calls, denied + 1);
            assert!(source.get("value").is_none());
        }
    }
}
