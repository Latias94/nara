use std::convert::Infallible;

use nara_core::{ByteLimit, ItemLimit};
use nara_identity::{AuthoredEntityReference, ReferenceInputId};
use nara_reflect::__private::{
    EffectiveValueAdmissionError, freeze_component_defaults,
    validate_native_component_value_references_with_admission,
};
use nara_reflect::{
    ComponentFieldPath, ComponentFieldSchema, ComponentSchemaVersion, ComponentValueKind,
};

use super::*;

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

#[test]
fn accepted_frozen_values_use_only_the_native_walk_and_consume_its_complete_cost() {
    let source = ComponentValue::map([(
        "cells",
        ComponentValue::List(vec![ComponentValue::Null; 4_096]),
    )]);
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.NativeCost"),
        "Native cost",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("visible"),
        "Visible",
        ComponentFieldPath::from_fields(["visible"]),
        ComponentValueKind::Bool,
        ComponentValue::Bool(true),
    )]);
    let defaults =
        freeze_component_defaults(&schema, &source, &BTreeMap::new(), Default::default())
            .unwrap()
            .unwrap();
    let value = defaults.read(&source);
    let cost = defaults.cost();
    let exact = limits(cost.nodes(), cost.logical_bytes(), 3);
    let mut expected_work = Vec::new();
    assert_eq!(
        validate_native_component_value_references_with_admission(
            value,
            exact,
            |nodes, bytes, scratch| {
                expected_work.push((nodes, bytes, scratch));
                Ok::<(), Infallible>(())
            },
        )
        .unwrap(),
        cost,
    );
    let mut budget = SceneValueBudget::new(exact);
    let mut actual_work = Vec::new();
    budget
        .admit_native(value, &mut |nodes, bytes, scratch| {
            actual_work.push((nodes, bytes, scratch));
            Ok::<(), Infallible>(())
        })
        .unwrap();
    assert_eq!(actual_work, expected_work);
    assert_eq!((budget.nodes, budget.bytes), (0, 0));
    assert!(std::ptr::eq(
        value.get("cells").unwrap().as_value().unwrap(),
        source.get("cells").unwrap(),
    ));
    assert!(source.get("visible").is_none());
}

fn rejected_value() -> ComponentValue {
    ComponentValue::List(vec![
        ComponentValue::EntityReference(EntityReference::Authored {
            reference: AuthoredEntityReference::input(ReferenceInputId::new("target").unwrap()),
        }),
        ComponentValue::String("later complete payload".repeat(32)),
    ])
}

#[test]
fn native_failures_still_charge_complete_values_and_preserve_aggregate_limit_diagnostics() {
    let value = rejected_value();
    let original = value.clone();
    let cost = value.cost();
    for (depth, expected) in [
        (
            1,
            ComponentCodecError::MaterializationDepthLimit {
                observed: 2,
                maximum: 1,
            },
        ),
        (4, ComponentCodecError::UnresolvedAuthoredReference),
    ] {
        let mut budget =
            SceneValueBudget::new(limits(cost.nodes() + 2, cost.logical_bytes() + 1, depth));
        let mut admit = |_, _, _| Ok::<(), Infallible>(());
        assert_eq!(
            budget.admit_native((&value).into(), &mut admit),
            Err(EffectiveValueAdmissionError::Value(expected)),
        );
        assert_eq!((budget.nodes, budget.bytes), (2, 1));
        budget
            .admit_native((&ComponentValue::Bool(true)).into(), &mut admit)
            .unwrap();
        assert_eq!((budget.nodes, budget.bytes), (1, 0));
        budget
            .admit_native((&ComponentValue::Null).into(), &mut admit)
            .unwrap();
        assert_eq!((budget.nodes, budget.bytes), (0, 0));
        assert_eq!(
            budget.admit_native((&ComponentValue::Null).into(), &mut admit),
            Err(EffectiveValueAdmissionError::Value(
                ComponentCodecError::MaterializationNodeLimit {
                    observed: 1,
                    maximum: 0,
                },
            )),
        );
    }
    for (nodes, bytes, expected) in [
        (
            cost.nodes() - 1,
            cost.logical_bytes() - 1,
            ComponentCodecError::MaterializationNodeLimit {
                observed: cost.nodes(),
                maximum: cost.nodes() - 1,
            },
        ),
        (
            cost.nodes(),
            cost.logical_bytes() - 1,
            ComponentCodecError::MaterializationByteLimit {
                observed: cost.logical_bytes(),
                maximum: cost.logical_bytes() - 1,
            },
        ),
    ] {
        let mut budget = SceneValueBudget::new(limits(nodes, bytes, 1));
        assert_eq!(
            budget.admit_native((&value).into(), &mut |_, _, _| Ok::<(), Infallible>(())),
            Err(EffectiveValueAdmissionError::Value(expected)),
        );
        assert_eq!((budget.nodes, budget.bytes), (nodes, bytes));
    }
    assert_eq!(value, original);
}

#[test]
fn an_owner_refusal_stops_immediately_in_native_and_failed_value_cost_admission() {
    let value = rejected_value();
    let mut native_calls = 0;
    assert!(matches!(
        validate_native_component_value_references_with_admission(
            &value,
            Default::default(),
            |_, _, _| {
                native_calls += 1;
                Ok::<(), Infallible>(())
            },
        ),
        Err(EffectiveValueAdmissionError::Value(
            ComponentCodecError::UnresolvedAuthoredReference,
        )),
    ));
    for stop in [1, native_calls + 1] {
        let mut budget = SceneValueBudget::new(Default::default());
        let remaining = (budget.nodes, budget.bytes);
        let marker = String::from("original receiving refusal");
        let mut calls = 0;
        let result = budget.admit_native((&value).into(), &mut |_, _, _| {
            calls += 1;
            if calls == stop { Err(&marker) } else { Ok(()) }
        });
        let Err(EffectiveValueAdmissionError::Admission(returned)) = result else {
            panic!("the original owner refusal must be returned at {stop}");
        };
        assert!(std::ptr::eq(returned, &marker));
        assert_eq!(calls, stop);
        assert_eq!((budget.nodes, budget.bytes), remaining);
    }
}
