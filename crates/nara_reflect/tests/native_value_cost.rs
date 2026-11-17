use std::{collections::BTreeMap, convert::Infallible};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{
    AuthoredEntityReference, EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
    PersistentRuntimeReference, ReferenceInputId, SceneEntityId,
};
use nara_reflect::{
    __private::{
        EffectiveValueAdmissionError, FrozenComponentDefaults,
        effective_component_value_with_default_overrides, freeze_component_defaults,
        validate_native_component_value_references_with_admission,
        validate_value_materialization_budget,
    },
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment as Segment, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueCost,
    ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueRef,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn persistent() -> EntityReference {
    EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    }
}

fn frozen_values(
    default: EntityReference,
) -> (ComponentValue, ComponentValue, FrozenComponentDefaults) {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("nara.test.NativeValueCost"),
        "Native value cost",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("fallback_target"),
        "Fallback target",
        ComponentFieldPath::new([
            Segment::field("rows"),
            Segment::index(1),
            Segment::field("target"),
        ]),
        ComponentValueKind::EntityRef,
        ComponentValue::EntityReference(default),
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef])]);
    let source = ComponentValue::map([
        (
            "opaque",
            ComponentValue::map([(
                "flags",
                ComponentValue::List(vec![
                    ComponentValue::Null,
                    ComponentValue::Bool(true),
                    ComponentValue::I64(-7),
                    ComponentValue::U64(8),
                    ComponentValue::f64(1.5).unwrap(),
                    ComponentValue::String("\u{96ea}".into()),
                ]),
            )]),
        ),
        (
            "rows",
            ComponentValue::List(vec![ComponentValue::map([
                ("label", ComponentValue::String("kept".into())),
                (
                    "target",
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: SceneEntityId::new("local").unwrap(),
                    }),
                ),
            ])]),
        ),
    ]);
    let allowance = ComponentValueMaterializationLimits::default();
    let overrides = BTreeMap::new();
    let (explicit, _) =
        effective_component_value_with_default_overrides(&schema, &source, &overrides, allowance)
            .unwrap();
    let explicit = explicit.into_owned();
    let frozen = freeze_component_defaults(&schema, &source, &overrides, allowance)
        .unwrap()
        .unwrap();
    (source, explicit, frozen)
}

#[test]
fn accepted_native_values_return_complete_ordinary_cost() {
    let (source, explicit, frozen) = frozen_values(persistent());
    let original_source = source.clone();
    let original_explicit = explicit.clone();
    let expected = explicit.cost();
    assert_eq!(expected.nodes(), 15);
    assert_eq!(expected.logical_bytes(), 89);
    let exact = limits(expected.nodes(), expected.logical_bytes(), 4);

    for value in [ComponentValueRef::from(&explicit), frozen.read(&source)] {
        let mut nodes = 0;
        let mut bytes = 0;
        let mut allocations = 0;
        let metrics = validate_native_component_value_references_with_admission(
            value,
            exact,
            |delta_nodes, delta_bytes, allocated| {
                nodes += delta_nodes;
                bytes += delta_bytes;
                allocations += usize::from(allocated > 0);
                Ok::<(), Infallible>(())
            },
        )
        .unwrap();
        let cost: ComponentValueCost = metrics.cost();
        assert_eq!(cost, expected);
        assert_eq!(metrics.depth(), 4);
        assert_eq!(cost.nodes(), nodes);
        assert_eq!(cost.logical_bytes(), bytes);
        assert!(allocations > 0);
    }
    assert_eq!(source, original_source);
    assert_eq!(explicit, original_explicit);
}

#[test]
fn native_cost_retains_strict_limits_and_exact_caller_refusals() {
    let (source, explicit, frozen) = frozen_values(persistent());
    let original_source = source.clone();
    let original_explicit = explicit.clone();
    let expected = explicit.cost();
    let exact = limits(expected.nodes(), expected.logical_bytes(), 4);

    for value in [ComponentValueRef::from(&explicit), frozen.read(&source)] {
        for (allowance, error) in [
            (
                limits(14, 89, 4),
                ComponentCodecError::MaterializationNodeLimit {
                    observed: 15,
                    maximum: 14,
                },
            ),
            (
                limits(15, 88, 4),
                ComponentCodecError::MaterializationByteLimit {
                    observed: 89,
                    maximum: 88,
                },
            ),
            (
                limits(15, 89, 3),
                ComponentCodecError::MaterializationDepthLimit {
                    observed: 4,
                    maximum: 3,
                },
            ),
        ] {
            assert_eq!(
                validate_native_component_value_references_with_admission(
                    value,
                    allowance,
                    |_, _, _| Ok::<(), Infallible>(()),
                ),
                Err(EffectiveValueAdmissionError::Value(error)),
            );
        }

        let mut requests = Vec::new();
        let cost = validate_native_component_value_references_with_admission(
            value,
            exact,
            |nodes, bytes, allocated| {
                requests.push((nodes, bytes, allocated));
                Ok::<(), Infallible>(())
            },
        )
        .unwrap()
        .cost();
        assert_eq!(cost, expected);
        assert!(matches!(requests.first(), Some((0, 0, allocated)) if *allocated > 0));

        for boundary in 0..requests.len() {
            let refusal = Box::new(std::io::Error::other("native admission refused"));
            let pointer = std::ptr::from_ref(refusal.as_ref());
            let mut refusal = Some(refusal);
            let mut calls = 0;
            let result = validate_native_component_value_references_with_admission(
                value,
                exact,
                |nodes, bytes, allocated| {
                    assert_eq!((nodes, bytes, allocated), requests[calls]);
                    let current = calls;
                    calls += 1;
                    if current == boundary {
                        Err(refusal.take().unwrap())
                    } else {
                        Ok(())
                    }
                },
            );
            let Err(EffectiveValueAdmissionError::Admission(actual)) = result else {
                panic!("native validation must retain the caller refusal at boundary {boundary}");
            };
            assert!(std::ptr::eq(actual.as_ref(), pointer));
            assert_eq!(calls, boundary + 1);
        }
    }
    assert_eq!(source, original_source);
    assert_eq!(explicit, original_explicit);
}

#[test]
fn native_cost_still_rejects_nested_authored_references() {
    let (source, explicit, frozen) = frozen_values(EntityReference::Authored {
        reference: AuthoredEntityReference::input(ReferenceInputId::new("socket").unwrap()),
    });
    let original_source = source.clone();
    let original_explicit = explicit.clone();
    let expected = explicit.cost();
    let exact = limits(expected.nodes(), expected.logical_bytes(), 4);

    for value in [ComponentValueRef::from(&explicit), frozen.read(&source)] {
        assert_eq!(
            validate_value_materialization_budget(value, exact),
            Ok(expected),
        );
        assert_eq!(
            validate_native_component_value_references_with_admission(value, exact, |_, _, _| {
                Ok::<(), Infallible>(())
            }),
            Err(EffectiveValueAdmissionError::Value(
                ComponentCodecError::UnresolvedAuthoredReference,
            )),
        );
    }
    assert_eq!(source, original_source);
    assert_eq!(explicit, original_explicit);
}
